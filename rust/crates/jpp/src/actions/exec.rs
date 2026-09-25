//! 执行器动作：`exec_py`、`check_tests`、`exec_sql`（B150；R2a 施工，`exec_py`/`check_tests`
//! 挪自 `cli/actions_r2a.rs`；`exec_sql` 是 24e-1 同一块的补齐项，见
//! `地基/过程记录/工程-比赛R2a-exec_sql.md`）。三者的 `reversible: true` 依赖
//! `sandbox.rs` 的操作系统级隔离才成立（PR #36 复核 P1/严重项，见
//! `地基/过程记录/工程-执行器动作安全修补.md`）。
//!
//! 依据：`地基/比赛/附注-2026-09-25-搭配层设计与36小时施工.md` §三·3.2、§五、§八；
//! `12-IR与类契约-v0.1.md` B150（画像 `sandbox: "subprocess"`）。
//! 过程记录：`地基/过程记录/工程-比赛R2a.md`、`工程-比赛R2a-exec_sql.md`、
//! `工程-执行器动作安全修补.md`。
//!
//! **三层防线（从弱到强）**：(1) 静态拒绝表——对 `code`/`tests` 字符串做启发式扫描，挡明显的
//! 逃逸手法，但挡不住不含扫描关键字的写法（例如 `pathlib.Path("x").write_text(...)` 不触发
//! `open(` 扫描）；(2) 运行期网络补丁——子进程执行前打补丁禁用 Python `socket` 模块；
//! (3) **操作系统级沙箱**（`sandbox.rs`）——真正让「写限定在临时工作目录、网络一律拒绝」
//! 成立的那一层，没有它两者一律拒绝执行，不在沙箱外跑。三层都挡不住 `ctypes` 直接调 libc、
//! CPU/内存/进程数耗用——如实写在 `sandbox.rs` 模块头。
//!
//! `exec_sql` 走的是 SQLite 自身的只读打开位（`file:...?mode=ro`）+ `PRAGMA query_only` +
//! 授权回调（只放行 `SELECT`/`READ`/`FUNCTION`/`RECURSIVE`，挡住 `VACUUM INTO`/
//! `ATTACH DATABASE` 这类不经过「写原库文件」检查的逃逸路径）+ 同一套操作系统沙箱，四层。
//! 不套静态拒绝表（`sql` 参数不会被 `exec()`），但同样跑网络禁用补丁（防御性）。

use super::Ctx;
use super::sandbox;
use super::subprocess_util::{run_subprocess, truncate};
use super::values_util::{value_number, value_text, value_to_assertions};
use crate::value::Value;
use std::time::Duration;

/* ============================== 静态拒绝表 ============================== */

/// 静态拒绝表命中的四个模块名（任务书点名的最小集）。
const REJECTED_IMPORTS: &[&str] = &["os", "subprocess", "socket", "shutil"];

/// 静态拒绝扫描：四个模块 import，加三条覆盖动态绕过（`__import__`/`eval(`/`exec(`/`compile(`），
/// 加 `open()` 写模式启发式扫描。命中即返回拒绝理由；不是解析器，是字符串扫描
/// （无资源隔离时的第一道防线，见模块头注）。
pub(super) fn static_reject(code: &str) -> Option<String> {
    for (i, raw) in code.lines().enumerate() {
        let line = raw.trim_start();
        for m in REJECTED_IMPORTS {
            let hit = line == format!("import {m}")
                || line.starts_with(&format!("import {m} "))
                || line.starts_with(&format!("import {m},"))
                || line.starts_with(&format!("import {m}."))
                || line.starts_with(&format!("import {m} as "))
                || line.starts_with(&format!("from {m} "))
                || line.starts_with(&format!("from {m}."));
            if hit {
                return Some(format!("第 {} 行：禁止 `import {m}`（静态拒绝表）", i + 1));
            }
        }
    }
    if code.contains("__import__") {
        return Some("禁止 `__import__`（静态拒绝表：动态导入可绕过 import 扫描）".into());
    }
    for kw in ["eval(", "exec(", "compile("] {
        if code.contains(kw) {
            return Some(format!("禁止 `{kw}`（静态拒绝表：可执行任意字符串）"));
        }
    }
    reject_write_open(code)
}

/// `open(...)` 写模式启发式：扫 `open(` 到最近 `)` 之间的引号短串，长度 ≤3 且字符集合
/// ⊆ `{r,w,a,x,b,t,+}` 视为模式参数，含 w/a/x/+ 即拒。字符串扫描，可被拼接手法绕过。
fn reject_write_open(code: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(rel) = code[search_from..].find("open(") {
        let start = search_from + rel + "open(".len();
        let end = code[start..]
            .find(')')
            .map(|e| start + e)
            .unwrap_or(code.len());
        let args = &code[start..end];
        let mut i = 0usize;
        let bytes: Vec<char> = args.chars().collect();
        while i < bytes.len() {
            let qc = bytes[i];
            if (qc == '\'' || qc == '"')
                && let Some(close_off) = bytes[i + 1..].iter().position(|c| *c == qc)
            {
                let mode: String = bytes[i + 1..i + 1 + close_off].iter().collect();
                if !mode.is_empty()
                    && mode.len() <= 3
                    && mode.chars().all(|c| "rwaxbt+".contains(c))
                    && mode.chars().any(|c| "wax+".contains(c))
                {
                    return Some(format!("`open(…, \"{mode}\")` 写模式禁止（静态拒绝表）"));
                }
                i += close_off + 2;
                continue;
            }
            i += 1;
        }
        if end >= code.len() {
            break;
        }
        search_from = end + 1;
    }
    None
}

fn clamp_timeout(s: f64) -> Duration {
    let s = if s.is_finite() && s > 0.0 { s } else { 5.0 };
    Duration::from_secs_f64(s.min(120.0))
}

/// 网络禁用前奏：打补丁 `socket.socket.connect`/`connect_ex`/`create_connection`/
/// `getaddrinfo`，让通过标准库 `socket`（因此也包括建在它之上的 `urllib`/`http.client`/
/// `ftplib`/`smtplib`/第三方 `requests` 等）发起的网络访问在拿到地址或连接那一步就抛异常。
/// **这不是内核级隔离**：`ctypes` 直接调 libc、或任何绕过 Python `socket` 模块的手法不受影响——
/// `sandbox: "subprocess"` 的含义就是「只隔离到子进程这一层」，不做资源隔离（B150、附注 §八）。
/// 挡得住：`import urllib.request; urllib.request.urlopen(...)`、`requests.get(...)`、
/// 裸 `socket.socket().connect(...)`（即便静态拒绝表放过了没写 `import socket` 的路径）。
/// 挡不住：`ctypes.CDLL("libc...").connect(...)` 之类绕过 Python socket 层的手法、
/// 通过已放行的 `os`/`subprocess` 之外的其它系统调用面。
const NETWORK_DISABLE_PRELUDE: &str = r#"
import socket as _jpp_socket


class _JppNetworkDisabled(OSError):
    pass


def _jpp_blocked(*_a, **_k):
    raise _JppNetworkDisabled(
        "network access disabled (jpp exec_py/check_tests sandbox: subprocess "
        "isolation only, no OS-level sandbox -- this patches Python's socket "
        "module; it does not block raw syscalls via ctypes or similar)"
    )


_jpp_socket.socket.connect = _jpp_blocked
_jpp_socket.socket.connect_ex = _jpp_blocked
_jpp_socket.create_connection = _jpp_blocked
_jpp_socket.getaddrinfo = _jpp_blocked
"#;

fn exec_python_path() -> String {
    std::env::var("JPP_EXEC_PYTHON").unwrap_or_else(|_| "python3".to_string())
}

/* ============================== exec_py ============================== */

#[derive(Debug, Clone)]
struct ExecResult {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
}

/// 固定的 exec_py 执行外壳：先跑网络禁用前奏，再从 `JPP_EXEC_PY_USER_FILE` 指的文件读用户
/// 代码并 `exec`。不直接把用户代码当成被执行的脚本本身，是为了让外壳（前奏部分）总是先跑
/// 到——用户代码若本身有语法错误也不影响前奏已经生效。
const EXEC_PY_HARNESS_HEADER: &str = r#"
import os as _jpp_os
"#;
const EXEC_PY_HARNESS_FOOTER: &str = r#"
with open(_jpp_os.environ["JPP_EXEC_PY_USER_FILE"], "r", encoding="utf-8") as _jpp_f:
    _jpp_src = _jpp_f.read()
exec(compile(_jpp_src, "<exec_py>", "exec"))
"#;

/// `exec_py(code, stdin, timeout_s)`：子进程 `python3 -I <固定外壳>`，跑在操作系统沙箱里
/// （`sandbox.rs`；没有沙箱工具直接拒绝，不在沙箱外跑）；外壳先打网络禁用补丁再执行用户代码
/// （见 [`NETWORK_DISABLE_PRELUDE`] 的能挡/不能挡范围，沙箱的 `deny network*` 是第二道网络
/// 防线）；静态拒绝表是最外层、最快的一道，挡明显的文件系统/进程逃逸手法；超时是结构化失败
/// （`timed_out: true`），不是 `Err`——只有静态拒绝命中、或找不到沙箱时才 `Err`
/// （J++ 侧变失败值，J-12）。
fn exec_py_core(code: &str, stdin: &str, timeout_s: f64) -> Result<ExecResult, String> {
    let tool = sandbox::tool().ok_or_else(|| sandbox::missing_message("exec_py"))?;
    if let Some(reason) = static_reject(code) {
        return Err(format!("exec_py 拒绝执行：{reason}"));
    }
    let timeout = clamp_timeout(timeout_s);
    let call_dir = sandbox::new_call_dir("exec-py")?;
    let harness_path = call_dir.join("harness.py");
    let user_path = call_dir.join("user.py");
    let harness =
        format!("{EXEC_PY_HARNESS_HEADER}{NETWORK_DISABLE_PRELUDE}{EXEC_PY_HARNESS_FOOTER}");
    std::fs::write(&harness_path, &harness).map_err(|e| format!("exec_py: 写临时外壳失败：{e}"))?;
    std::fs::write(&user_path, code).map_err(|e| format!("exec_py: 写临时脚本失败：{e}"))?;
    let python = exec_python_path();
    let sandboxed_args = vec![
        "-I".to_string(),
        harness_path.to_string_lossy().into_owned(),
    ];
    let mut cmd = sandbox::wrap(tool, &python, &sandboxed_args, &call_dir)?;
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    cmd.env("JPP_EXEC_PY_USER_FILE", &user_path);
    let res = run_subprocess(cmd, stdin, timeout);
    let _ = std::fs::remove_dir_all(&call_dir);
    let r = res.map_err(|e| {
        format!(
            "exec_py: {e}（解释器：{python}；沙箱：{}）",
            sandbox::describe(tool)
        )
    })?;
    Ok(ExecResult {
        stdout: r.stdout,
        stderr: r.stderr,
        exit_code: r.exit_code,
        timed_out: r.timed_out,
    })
}

/// `do("exec_py", [code, stdin, timeout_s], seq)`。
pub(super) fn exec_py(_ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [code, stdin, timeout] = args else {
        return Err("exec_py expects (code, stdin, timeout_s)".into());
    };
    let code = value_text(code).ok_or("exec_py: code 必须是文本")?;
    let stdin = value_text(stdin).ok_or("exec_py: stdin 必须是文本")?;
    let timeout_s = value_number(timeout).ok_or("exec_py: timeout_s 必须是数字")?;
    let r = exec_py_core(&code, &stdin, timeout_s)?;
    Ok(Value::record(vec![
        ("stdout".to_string(), Value::text(&r.stdout)),
        ("stderr".to_string(), Value::text(&r.stderr)),
        (
            "exit_code".to_string(),
            match r.exit_code {
                Some(c) => Value::int(c as i64),
                None => Value::Unit,
            },
        ),
        ("timed_out".to_string(), Value::bool(r.timed_out)),
    ]))
}

/* ============================== check_tests ============================== */

/// 固定驱动脚本：`{code, tests}` 整份从 stdin 以 JSON 喂入，不插值用户文本
/// （避免用户代码里的引号/三引号破坏脚本拼接）。前半是 [`NETWORK_DISABLE_PRELUDE`]
/// （先于任何用户代码生效）；`exec(` 在余下部分里是驱动脚本自己的、用来跑已过静态拒绝表
/// 的用户代码，不受静态拒绝表约束（拒绝表管的是用户 `code`/`tests` 字符串本身，不是我们
/// 自己的固定驱动脚本）。用拼接而不是 `format!`——脚本体里全是 Python 花括号，拼接不用
/// 逐个转义。
const CHECK_TESTS_DRIVER_BODY: &str = r#"
import sys, json
payload = json.load(sys.stdin)
code = payload["code"]
tests = payload["tests"]
ns = {}
log = []
try:
    exec(code, ns)
except Exception as e:
    log.append({"stage": "code", "ok": False, "error": f"{type(e).__name__}: {e}"})
    print(json.dumps({"passed": 0, "failed": max(len(tests), 1), "log": log}))
    sys.exit(0)
passed = 0
failed = 0
for t in tests:
    try:
        exec(t, ns)
        passed += 1
        log.append({"assertion": t, "ok": True})
    except Exception as e:
        failed += 1
        log.append({"assertion": t, "ok": False, "error": f"{type(e).__name__}: {e}"})
print(json.dumps({"passed": passed, "failed": failed, "log": log}))
"#;

fn check_tests_driver() -> String {
    format!("{NETWORK_DISABLE_PRELUDE}{CHECK_TESTS_DRIVER_BODY}")
}

struct CheckTestsResult {
    passed: i64,
    failed: i64,
    log: serde_json::Value,
}

/// `check_tests(code, tests, timeout_s)`：先 `exec(code)` 建命名空间，再逐条 `tests`
/// 断言/语句在同一命名空间跑，记 `passed`/`failed`/`log`。复用 `exec_py` 的子进程与
/// 静态拒绝表（对 `code`+`tests` 合并文本扫）。
fn check_tests_core(
    code: &str,
    tests: &[String],
    timeout_s: f64,
) -> Result<CheckTestsResult, String> {
    let tool = sandbox::tool().ok_or_else(|| sandbox::missing_message("check_tests"))?;
    let combined = format!("{code}\n{}", tests.join("\n"));
    if let Some(reason) = static_reject(&combined) {
        return Err(format!("check_tests 拒绝执行：{reason}"));
    }
    let timeout = clamp_timeout(timeout_s);
    let call_dir = sandbox::new_call_dir("check-tests")?;
    let script_path = call_dir.join("driver.py");
    std::fs::write(&script_path, check_tests_driver())
        .map_err(|e| format!("check_tests: 写临时脚本失败：{e}"))?;
    let python = exec_python_path();
    let sandboxed_args = vec!["-I".to_string(), script_path.to_string_lossy().into_owned()];
    let mut cmd = sandbox::wrap(tool, &python, &sandboxed_args, &call_dir)?;
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    let payload = serde_json::json!({"code": code, "tests": tests}).to_string();
    let res = run_subprocess(cmd, &payload, timeout);
    let _ = std::fs::remove_dir_all(&call_dir);
    let r = res.map_err(|e| {
        format!(
            "check_tests: {e}（解释器：{python}；沙箱：{}）",
            sandbox::describe(tool)
        )
    })?;
    if r.timed_out {
        return Err(format!(
            "check_tests: 超过 {:.1} 秒超时",
            timeout.as_secs_f64()
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(r.stdout.trim()).map_err(|e| {
        format!(
            "check_tests: 解析子进程输出失败：{e}；stderr: {}",
            truncate(&r.stderr, 500)
        )
    })?;
    Ok(CheckTestsResult {
        passed: parsed.get("passed").and_then(|v| v.as_i64()).unwrap_or(0),
        failed: parsed.get("failed").and_then(|v| v.as_i64()).unwrap_or(0),
        log: parsed
            .get("log")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    })
}

/// `do("check_tests", [code, tests, timeout_s], seq)`。
pub(super) fn check_tests(_ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [code, tests, timeout] = args else {
        return Err("check_tests expects (code, tests, timeout_s)".into());
    };
    let code = value_text(code).ok_or("check_tests: code 必须是文本")?;
    let tests = value_to_assertions(tests).map_err(|e| format!("check_tests: {e}"))?;
    let timeout_s = value_number(timeout).ok_or("check_tests: timeout_s 必须是数字")?;
    let r = check_tests_core(&code, &tests, timeout_s)?;
    Ok(Value::record(vec![
        ("passed".to_string(), Value::int(r.passed)),
        ("failed".to_string(), Value::int(r.failed)),
        ("log".to_string(), crate::interp::json_to_value(&r.log)),
    ]))
}

/* ============================== exec_sql ============================== */

/// 固定 3 秒（`地基/比赛/旗舰演示方案.md`「只读连接，超时 3 秒」）；不是 J++ 侧可调参数——
/// `exec_sql(db, sql)` 只有两个位置参数，签名由依据文本（B150）定死。
const EXEC_SQL_TIMEOUT_S: f64 = 3.0;

/// 固定驱动脚本：`{db, sql}` 从 stdin 以 JSON 喂入，不插值。先跑 [`NETWORK_DISABLE_PRELUDE`]
/// （防御性：`sqlite3` 本身不发网络请求，但代价接近零，照 `exec_py` 的口径加上）。
/// `db` 转成 SQLite 只读 URI（`file:<abspath>?mode=ro`）——**只读打开位单独不够**：
/// `VACUUM INTO`、`ATTACH DATABASE` 都不写「原库文件」，只读打开位管不到它们，会在磁盘上
/// 生成新文件（PR #36 复核，主会话实测）。修法是 `PRAGMA query_only = ON` 加授权回调
/// `set_authorizer`，只放行 `SQLITE_SELECT`/`SQLITE_READ`/`SQLITE_FUNCTION`/`SQLITE_RECURSIVE`，
/// 其余一律 `SQLITE_DENY`——**顺序要紧**：`PRAGMA query_only` 必须在 `set_authorizer` 之前跑，
/// 因为 `PRAGMA` 本身有自己的动作码、不在放行集合里，装完授权器再跑这条 PRAGMA 会被自己挡下
/// （写代码前用真实脚本核实过，见 `地基/过程记录/工程-执行器动作安全修补.md` §二）。
///
/// **B164 改判**：授权回调拒绝要报 `Fail(Denied)`，不能落进 `error` 字段（那是给「语法错、库
/// 不存在」这类内容性错误用的，授权拒绝是安全边界，性质不同）。驱动脚本自己不知道进程要不要
/// `sys.exit` 非零，用一个 `_jpp_denied` 标记记下授权器有没有真的返回过 `SQLITE_DENY`
/// 且这次异常确实源自它（不是靠解析英文报文，跨 SQLite 版本/locale 更稳）；命中就打印
/// `{"denied": true, ...}` 并以非零码退出，Rust 侧据此转成 `Err`，不当成结构化 `error`。
/// 其余异常（坏 SQL、库文件不存在）仍然落 `error`，不 panic。行内非 JSON 原生类型
/// （`bytes`/`BLOB`）转 `{"__blob_b64__": <base64>}`，其余（`int`/`float`/`str`/`None`）原样。
const EXEC_SQL_DRIVER_BODY: &str = r#"
import sys, json, sqlite3, os
from urllib.parse import quote

payload = json.load(sys.stdin)
db = payload["db"]
sql = payload["sql"]

_jpp_denied = False


def _to_jsonable(v):
    if v is None or isinstance(v, (int, float, str)):
        return v
    if isinstance(v, bytes):
        import base64
        return {"__blob_b64__": base64.b64encode(v).decode("ascii")}
    return str(v)


def _jpp_sql_authorizer(action, arg1, arg2, db_name, trigger_name):
    global _jpp_denied
    allowed = {
        sqlite3.SQLITE_SELECT,
        sqlite3.SQLITE_READ,
        sqlite3.SQLITE_FUNCTION,
        sqlite3.SQLITE_RECURSIVE,
    }
    if action in allowed:
        return sqlite3.SQLITE_OK
    _jpp_denied = True
    return sqlite3.SQLITE_DENY


try:
    uri = "file:" + quote(os.path.abspath(db)) + "?mode=ro"
    conn = sqlite3.connect(uri, uri=True, timeout=1)
    try:
        # 顺序要紧：先 PRAGMA 后装授权器（PRAGMA 有自己的动作码，装完授权器再跑会被拒）。
        conn.execute("PRAGMA query_only = ON")
        conn.set_authorizer(_jpp_sql_authorizer)
        cur = conn.cursor()
        cur.execute(sql)
        columns = [d[0] for d in cur.description] if cur.description else []
        rows = [[_to_jsonable(v) for v in row] for row in cur.fetchall()]
        print(json.dumps({"columns": columns, "rows": rows, "error": None, "denied": False}))
    finally:
        conn.close()
except Exception as e:
    if _jpp_denied:
        print(json.dumps({"columns": [], "rows": [], "error": None, "denied": True,
                           "detail": f"{type(e).__name__}: {e}"}))
        sys.exit(1)
    print(json.dumps({"columns": [], "rows": [], "error": f"{type(e).__name__}: {e}", "denied": False}))
"#;

fn exec_sql_driver() -> String {
    format!("{NETWORK_DISABLE_PRELUDE}{EXEC_SQL_DRIVER_BODY}")
}

#[derive(Debug)]
struct ExecSqlResult {
    columns: Vec<String>,
    rows: serde_json::Value,
    error: Option<String>,
}

/// `exec_sql(db, sql)`：子进程 Python 标准库 `sqlite3`（不加新 cargo 依赖），跑在操作系统
/// 沙箱里（`sandbox.rs`；没有沙箱工具直接拒绝，与 `exec_py`/`check_tests` 同一套，防御性——
/// SQLite 层面的只读打开位 + `query_only` + 授权回调已经挡住已知的逃逸路径，沙箱是万一还有
/// 遗漏路径时的最后一层，不依赖它们互相知道对方存在）。固定 3 秒超时。超时不是 `Err`
/// （`exec_sql` 的契约形状是文档定死的三字段，没有 `exec_py` 那样单独的 `timed_out` 字段），
/// 并入 `error`，`columns`/`rows` 为空——与写语句被拒、坏 SQL、库不存在走同一条
/// 「结构化失败落进 error」的路。
fn exec_sql_core(db: &str, sql: &str) -> Result<ExecSqlResult, String> {
    let tool = sandbox::tool().ok_or_else(|| sandbox::missing_message("exec_sql"))?;
    let timeout = Duration::from_secs_f64(EXEC_SQL_TIMEOUT_S);
    let call_dir = sandbox::new_call_dir("exec-sql")?;
    let script_path = call_dir.join("driver.py");
    std::fs::write(&script_path, exec_sql_driver())
        .map_err(|e| format!("exec_sql: 写临时脚本失败：{e}"))?;
    let python = exec_python_path();
    let sandboxed_args = vec!["-I".to_string(), script_path.to_string_lossy().into_owned()];
    let mut cmd = sandbox::wrap(tool, &python, &sandboxed_args, &call_dir)?;
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    let payload = serde_json::json!({"db": db, "sql": sql}).to_string();
    let res = run_subprocess(cmd, &payload, timeout);
    let _ = std::fs::remove_dir_all(&call_dir);
    let r = res.map_err(|e| {
        format!(
            "exec_sql: {e}（解释器：{python}；沙箱：{}）",
            sandbox::describe(tool)
        )
    })?;
    if r.timed_out {
        return Ok(ExecSqlResult {
            columns: Vec::new(),
            rows: serde_json::json!([]),
            error: Some(format!("超过 {EXEC_SQL_TIMEOUT_S:.1} 秒超时")),
        });
    }
    let parsed: serde_json::Value = serde_json::from_str(r.stdout.trim()).map_err(|e| {
        format!(
            "exec_sql: 解析子进程输出失败：{e}；stderr: {}",
            truncate(&r.stderr, 500)
        )
    })?;
    // B164：授权回调拒绝要报 Fail(Denied)，不落 error 字段（安全边界，不是内容性错误）。
    if parsed.get("denied").and_then(|v| v.as_bool()) == Some(true) {
        let detail = parsed
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("SQL 被授权回调拒绝");
        return Err(format!(
            "exec_sql: Denied: 只放行 SELECT/READ/FUNCTION/RECURSIVE，其余一律拒绝\
             （VACUUM INTO/ATTACH DATABASE/写语句等）：{detail}"
        ));
    }
    let columns = parsed
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let rows = parsed
        .get("rows")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let error = parsed
        .get("error")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(ExecSqlResult {
        columns,
        rows,
        error,
    })
}

/// `do("exec_sql", [db, sql], seq)`。
pub(super) fn exec_sql(_ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [db, sql] = args else {
        return Err("exec_sql expects (db, sql)".into());
    };
    let db = value_text(db).ok_or("exec_sql: db 必须是文本")?;
    let sql = value_text(sql).ok_or("exec_sql: sql 必须是文本")?;
    let r = exec_sql_core(&db, &sql)?;
    Ok(Value::record(vec![
        (
            "columns".to_string(),
            Value::list(r.columns.iter().map(|c| Value::text(c)).collect()),
        ),
        ("rows".to_string(), crate::interp::json_to_value(&r.rows)),
        (
            "error".to_string(),
            match r.error {
                Some(e) => Value::text(&e),
                None => Value::Unit,
            },
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_import_os() {
        let r = static_reject("import os\nprint(1)");
        assert!(
            r.is_some_and(|m| m.contains("import os")),
            "应拒绝 import os"
        );
    }

    #[test]
    fn reject_import_subprocess_variants() {
        assert!(static_reject("import subprocess").is_some());
        assert!(static_reject("import subprocess as sp").is_some());
        assert!(static_reject("from subprocess import run").is_some());
    }

    #[test]
    fn reject_import_socket() {
        assert!(static_reject("import socket").is_some());
    }

    #[test]
    fn reject_import_shutil() {
        assert!(static_reject("import shutil").is_some());
        assert!(static_reject("from shutil import rmtree").is_some());
    }

    #[test]
    fn reject_dunder_import() {
        let r = static_reject("__import__('os').system('echo hi')");
        assert!(r.is_some_and(|m| m.contains("__import__")));
    }

    #[test]
    fn reject_eval_exec_compile() {
        assert!(static_reject("eval('1+1')").is_some());
        assert!(static_reject("exec('print(1)')").is_some());
        assert!(static_reject("compile('1', '<s>', 'eval')").is_some());
    }

    #[test]
    fn reject_open_write_mode() {
        let r = static_reject("open('x.txt', 'w').write('boom')");
        assert!(r.is_some_and(|m| m.contains("写模式")));
        assert!(static_reject("open('x.txt', 'a')").is_some());
        assert!(static_reject("open('x.txt', 'wb')").is_some());
    }

    #[test]
    fn allow_open_read_mode_and_normal_code() {
        assert!(static_reject("x = 1 + 2\nprint(x)").is_none());
        assert!(static_reject("with open('x.txt', 'r') as f:\n    print(f.read())").is_none());
        assert!(static_reject("open('x.txt').read()").is_none());
    }

    #[test]
    fn exec_py_core_runs_and_captures_stdout_stderr_exit_code() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_py_core_runs_and_captures_stdout_stderr_exit_code：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let r = exec_py_core(
            "import sys\nprint('hi')\nprint('err', file=sys.stderr)\nsys.exit(3)",
            "",
            5.0,
        )
        .expect("应能执行");
        assert_eq!(r.stdout.trim(), "hi");
        assert_eq!(r.stderr.trim(), "err");
        assert_eq!(r.exit_code, Some(3));
        assert!(!r.timed_out);
    }

    #[test]
    fn exec_py_core_stdin_roundtrip() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_py_core_stdin_roundtrip：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let r = exec_py_core(
            "import sys\nprint(sys.stdin.read().strip().upper())",
            "hello",
            5.0,
        )
        .expect("应能执行");
        assert_eq!(r.stdout.trim(), "HELLO");
    }

    #[test]
    fn exec_py_core_times_out() {
        if sandbox::tool().is_none() {
            eprintln!("跳过 exec_py_core_times_out：本机没有可用的沙箱工具（环境依赖，非失败）");
            return;
        }
        let r = exec_py_core("import time\ntime.sleep(2)", "", 0.2).expect("应返回结构化结果");
        assert!(r.timed_out, "应标记超时");
    }

    #[test]
    fn exec_py_core_rejects_without_spawning() {
        let r = exec_py_core("import os\nos.system('echo boom')", "", 5.0);
        assert!(r.is_err(), "应拒绝，不落盘不起子进程");
    }

    /// PR #36 复核 P1 的核实：`pathlib.Path(...).write_text(...)` 不含任何一条静态拒绝表的
    /// 关键字（不是 `import os/subprocess/socket/shutil`，不含 `open(`），能穿过静态扫描，
    /// 真正到子进程里执行——挡它的必须是操作系统沙箱，不是静态表。本机（macOS）有
    /// `sandbox-exec`，跑真实断言；没有沙箱工具的平台跳过并注明原因（不是假跳过，
    /// `sandbox::detect()` 探测不到时就没有别的办法验证这条）。
    #[test]
    fn exec_py_core_sandbox_blocks_write_outside_call_dir() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_py_core_sandbox_blocks_write_outside_call_dir：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let target = std::env::temp_dir().join(format!(
            "jpp-sandbox-escape-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&target);
        let code = format!(
            "import pathlib\np = pathlib.Path({target:?})\n\
             try:\n    p.write_text('escaped')\n    print('WROTE')\n\
             except Exception as e:\n    print(f'blocked: {{type(e).__name__}}: {{e}}')\n",
        );
        let r = exec_py_core(&code, "", 5.0).expect("静态拒绝表不拦 pathlib 写，应能起子进程");
        assert!(
            !target.exists(),
            "沙箱外的写不该成功：{target:?}；stdout={}",
            r.stdout
        );
        assert!(
            !r.stdout.contains("WROTE"),
            "不该看到写成功的输出：{}",
            r.stdout
        );
        let _ = std::fs::remove_file(&target);
    }

    /// 无网络的核实：`urllib` 不在静态拒绝表的模块名单里（只有 os/subprocess/socket/shutil），
    /// 所以这条能穿过静态扫描，真正到子进程里执行——网络禁用补丁要在这一层挡住它。
    /// 这是对「B150 要求 check_tests/exec_py 无网络」的实测核实，不是复述文档。
    #[test]
    fn exec_py_core_blocks_network_not_caught_by_static_reject() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_py_core_blocks_network_not_caught_by_static_reject：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let r = exec_py_core(
            "import urllib.request\nurllib.request.urlopen('http://169.254.169.254/', timeout=2)",
            "",
            10.0,
        )
        .expect("静态拒绝表不拦 urllib，应能起子进程（由运行期网络补丁挡）");
        assert_ne!(
            r.exit_code,
            Some(0),
            "urllib 发起的连接应被网络补丁挡住而失败：{r:?}"
        );
        assert!(
            r.stderr.contains("_JppNetworkDisabled")
                || r.stderr.contains("network access disabled"),
            "stderr 应能看到网络补丁抛出的异常：{}",
            r.stderr
        );
    }

    /// 第二条独立路径：`http.client` 直接走 `socket.create_connection`（不经 `urllib`），
    /// 同样不在静态拒绝表的模块名单里，核实补丁挡的是 `socket` 这一层、不是只挡 `urllib` 一家。
    #[test]
    fn exec_py_core_blocks_network_via_http_client_too() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_py_core_blocks_network_via_http_client_too：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let r = exec_py_core(
            "import http.client\nc = http.client.HTTPConnection('169.254.169.254', timeout=2)\nc.connect()",
            "",
            10.0,
        )
        .expect("静态拒绝表不拦 http.client，应能起子进程（由运行期网络补丁挡）");
        assert_ne!(r.exit_code, Some(0), "{r:?}");
        assert!(
            r.stderr.contains("_JppNetworkDisabled")
                || r.stderr.contains("network access disabled"),
            "{}",
            r.stderr
        );
    }

    #[test]
    fn check_tests_core_blocks_network_not_caught_by_static_reject() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 check_tests_core_blocks_network_not_caught_by_static_reject：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let r = check_tests_core(
            "import urllib.request\nurllib.request.urlopen('http://169.254.169.254/', timeout=2)",
            &["assert True".to_string()],
            10.0,
        )
        .expect("静态拒绝表不拦 urllib，应能起子进程");
        // check_tests 的驱动脚本把 code 阶段的异常记进 log，passed/failed 不会双双是 0/0
        assert_eq!(r.passed, 0);
        assert!(r.failed >= 1);
        let log_text = r.log.to_string();
        assert!(
            log_text.contains("_JppNetworkDisabled")
                || log_text.contains("network access disabled"),
            "{log_text}"
        );
    }

    #[test]
    fn check_tests_core_counts_pass_and_fail() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 check_tests_core_counts_pass_and_fail：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let r = check_tests_core(
            "x = 2",
            &["assert x == 2".to_string(), "assert x == 3".to_string()],
            5.0,
        )
        .expect("应能执行");
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 1);
    }

    #[test]
    fn check_tests_core_rejects_dangerous_code() {
        assert!(check_tests_core("import socket", &["assert True".to_string()], 5.0).is_err());
    }

    /// 建一个带两行数据的临时 sqlite 库，供 `exec_sql_core` 测试用。借 `exec_py_core`
    /// 起 Python 建库，不新增依赖、不手写 SQLite 文件格式。
    /// 建测试用的 sqlite 库：直接用系统 `python3`（不经 `exec_py_core`——`exec_py` 现在把写
    /// 限定在它自己每次调用新建的沙箱工作目录里，没法用来在任意测试目录建库；这里只是测试
    /// 夹具准备，不测 `exec_py` 本身，用未沙箱化的 `Command` 反而是对的）。
    fn make_test_db(tag: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("jpp-exec-sql-test-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_str = dir.join("t.db").to_string_lossy().to_string();
        let code = format!(
            "import sqlite3\n\
             conn = sqlite3.connect({db_str:?})\n\
             conn.execute('create table t (id integer, name text)')\n\
             conn.execute('insert into t values (1, \"a\")')\n\
             conn.execute('insert into t values (2, \"b\")')\n\
             conn.commit()\n\
             conn.close()\n"
        );
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(&code)
            .output()
            .expect("python3 应可用");
        assert!(
            out.status.success(),
            "建库失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        db_str
    }

    #[test]
    fn exec_sql_core_reads_rows() {
        if sandbox::tool().is_none() {
            eprintln!("跳过 exec_sql_core_reads_rows：本机没有可用的沙箱工具（环境依赖，非失败）");
            return;
        }
        let db = make_test_db("read");
        let r = exec_sql_core(&db, "select id, name from t order by id").expect("应能执行");
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.columns, vec!["id".to_string(), "name".to_string()]);
        assert_eq!(
            r.rows,
            serde_json::json!([[1, "a"], [2, "b"]]),
            "columns={:?}",
            r.columns
        );
    }

    /// 核实「无资源隔离」以外的那一层——SQLite 自己的只读打开位，不是应用层假装拒绝写：
    /// 只读连接上跑写语句要被 SQLite 拒绝，异常落进 `error`，不 panic。
    #[test]
    fn exec_sql_core_rejects_write_on_readonly_connection() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_sql_core_rejects_write_on_readonly_connection：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let db = make_test_db("write-reject");
        // B164：授权回调拒绝是 Fail(Denied)，不是 Ok(结构化 error)——安全边界，不是内容性错误。
        let err = exec_sql_core(&db, "insert into t values (3, 'c')")
            .expect_err("只读连接上的写语句应报 Err（Fail(Denied)）");
        assert!(err.contains("Denied"), "{err}");
        // 真的没写进去：再读一次应仍只有两行
        let check = exec_sql_core(&db, "select count(*) from t").expect("应能执行");
        assert_eq!(check.rows, serde_json::json!([[2]]), "写语句不该生效");
    }

    #[test]
    fn exec_sql_core_bad_sql_reports_error_not_panic() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_sql_core_bad_sql_reports_error_not_panic：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let db = make_test_db("bad-sql");
        let r =
            exec_sql_core(&db, "select * from 不存在的表 where").expect("应能执行（结构化失败）");
        assert!(r.error.is_some());
    }

    #[test]
    fn exec_sql_core_missing_db_reports_error_not_panic() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_sql_core_missing_db_reports_error_not_panic：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let missing = std::env::temp_dir()
            .join(format!("jpp-exec-sql-missing-{}.db", std::process::id()))
            .to_string_lossy()
            .to_string();
        let r = exec_sql_core(&missing, "select 1").expect("应能执行（结构化失败）");
        assert!(r.error.is_some());
    }

    /// 严重项的直接复现与核实：`mode=ro` 只读打开位挡不住 `VACUUM INTO` 在磁盘生成新文件——
    /// 授权回调（只放行 SELECT/READ/FUNCTION/RECURSIVE）修好之后，这条应该被拒、不产生目标文件。
    #[test]
    fn exec_sql_core_rejects_vacuum_into_and_leaves_no_file() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_sql_core_rejects_vacuum_into_and_leaves_no_file：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let db = make_test_db("vacuum-into");
        let out = std::env::temp_dir()
            .join(format!("jpp-exec-sql-vacuum-out-{}.db", std::process::id()))
            .to_string_lossy()
            .to_string();
        let _ = std::fs::remove_file(&out);
        let sql = format!("VACUUM INTO '{out}'");
        let err = exec_sql_core(&db, &sql).expect_err("VACUUM INTO 应报 Err（Fail(Denied)）");
        assert!(err.contains("Denied"), "{err}");
        assert!(
            !std::path::Path::new(&out).exists(),
            "VACUUM INTO 不该在磁盘上留下文件：{out}"
        );
    }

    /// 同上，`ATTACH DATABASE` 那条路径：`ATTACH` 本身不写「原库文件」，`mode=ro` 管不到，
    /// 授权回调修好之后应该在 `ATTACH` 这一步本身就被拒（`exec_sql` 一次只执行一条语句，
    /// 「ATTACH 后再 CREATE TABLE」两步攻击在这个接口下传不进一次调用，见过程记录 §二）。
    #[test]
    fn exec_sql_core_rejects_attach_database_and_leaves_no_file() {
        if sandbox::tool().is_none() {
            eprintln!(
                "跳过 exec_sql_core_rejects_attach_database_and_leaves_no_file：本机没有可用的沙箱工具（环境依赖，非失败）"
            );
            return;
        }
        let db = make_test_db("attach");
        let att = std::env::temp_dir()
            .join(format!("jpp-exec-sql-attach-out-{}.db", std::process::id()))
            .to_string_lossy()
            .to_string();
        let _ = std::fs::remove_file(&att);
        let sql = format!("ATTACH DATABASE '{att}' AS a");
        let err = exec_sql_core(&db, &sql).expect_err("ATTACH DATABASE 应报 Err（Fail(Denied)）");
        assert!(err.contains("Denied"), "{err}");
        assert!(
            !std::path::Path::new(&att).exists(),
            "ATTACH DATABASE 不该在磁盘上留下文件：{att}"
        );
    }
}

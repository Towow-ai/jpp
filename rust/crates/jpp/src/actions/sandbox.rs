//! 操作系统级沙箱：`exec_py`、`check_tests`、`exec_sql` 唯一让 `BUILTIN_ACTIONS` 表里的
//! `reversible: true` 成立的机制——静态拒绝表（`exec.rs::static_reject`）与网络补丁
//! （`exec.rs::NETWORK_DISABLE_PRELUDE`）只是辅助层，挡不住不落在它们扫描规则里的写法
//! （例如 `pathlib.Path("x").write_text(...)` 不含任何一条静态拒绝表的关键字）。真正的
//! 强制隔离在这里：macOS 用 `sandbox-exec`，Linux 用 `bwrap`；两者都没有就拒绝执行，
//! **绝不在沙箱外跑**（PR #36 复核 P1；过程记录 `地基/过程记录/工程-执行器动作安全修补.md`）。
//!
//! **隔离范围（如实写，不夸大）**：
//! - 挡得住：往每次调用新建的工作目录以外的任何路径写文件；发起网络连接。
//! - 挡不住：读宿主机任意可读文件（`(allow default)`/`--ro-bind / /` 打底，明确不隔离读，
//!   见 `08` 定律「判断只对字面材料」不适用于执行器本身就要读写文件系统这件事）；CPU、内存、
//!   进程数不限（fork 炸弹类未防）。
//!
//! **排错记录**：macOS 的 `subpath` 规则按内核解析后的真实路径匹配，`/tmp`→`/private/tmp`、
//! `/var`→`/private/var` 的符号链接若不先解析，规则会悄悄不生效（写代码前用真实脚本核实过，
//! 见过程记录）。因此 [`new_call_dir`] 一定返回 `canonicalize()` 过的路径。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub(super) enum Tool {
    MacSandboxExec,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    LinuxBwrap(PathBuf),
}

fn tool_name(tool: &Tool) -> &'static str {
    match tool {
        Tool::MacSandboxExec => "sandbox-exec",
        Tool::LinuxBwrap(_) => "bwrap",
    }
}

/// 画像 `actions` 分表里 `sandbox.kind` 的三个取值（B164）；`reversible` 由
/// `kind != SandboxKind::None` 派生，不再单独声明。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SandboxKind {
    SandboxExec,
    Bwrap,
    None,
}

impl SandboxKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            SandboxKind::SandboxExec => "sandbox-exec",
            SandboxKind::Bwrap => "bwrap",
            SandboxKind::None => "none",
        }
    }
}

/// 宿主启动时探测一次（B164：不是每次调用都重探），结果缓存进程生命周期内不变。
/// `JPP_FORCE_NO_SANDBOX`（任意非空值）强制探测结果为「没有」——只给测试用（`Command::env`
/// 只作用于被测的子进程，不影响当前测试进程自己的并发测试），只能让沙箱「更严格（拒绝执行）」，
/// 不能绕过任何限制，不是安全后门。
///
/// **存在不等于能用**（主会话复核追加）：公开仓库 CI（GitHub ubuntu runner）上 `bwrap` 即便
/// 装了，也可能因非特权用户命名空间受限跑不起来——只查文件在不在/在不在 PATH 里不够，找到
/// 候选工具后要真跑一次冒烟测试（[`smoke_test`]），跑不通就当没有这个工具，继续探测下一个
/// 候选。这一位不是测试专用旁路：生产路径（`reversible`/`E-action-no-sandbox` 的判据）与
/// 测试路径（`crate::actions::sandbox_available`）读的是同一个探测结果。
static PROBED: OnceLock<Option<Tool>> = OnceLock::new();

fn probe() -> &'static Option<Tool> {
    PROBED.get_or_init(|| {
        if std::env::var_os("JPP_FORCE_NO_SANDBOX").is_some() {
            return None;
        }
        #[cfg(target_os = "macos")]
        {
            if Path::new("/usr/bin/sandbox-exec").is_file() && smoke_test(&Tool::MacSandboxExec) {
                return Some(Tool::MacSandboxExec);
            }
        }
        #[cfg(target_os = "linux")]
        {
            if let Some(p) = find_in_path("bwrap") {
                let candidate = Tool::LinuxBwrap(p);
                if smoke_test(&candidate) {
                    return Some(candidate);
                }
            }
        }
        None
    })
}

/// 真跑一次最小沙箱化子进程（`/bin/sh -c "exit 0"`：POSIX shell，macOS/Linux 都有，不依赖
/// Python 等额外前提），退出码为 0 才算这个工具真的能用。用完即弃的工作目录跑完就删。
fn smoke_test(tool: &Tool) -> bool {
    let Ok(dir) = new_call_dir("probe") else {
        return false;
    };
    let args = vec!["-c".to_string(), "exit 0".to_string()];
    let ok = match wrap(tool, "/bin/sh", &args, &dir) {
        Ok(mut cmd) => cmd.status().map(|s| s.success()).unwrap_or(false),
        Err(_) => false,
    };
    let _ = std::fs::remove_dir_all(&dir);
    ok
}

/// 宿主启动时探测到的沙箱工具（缓存值，见 [`probe`]）；`None` 表示两者都没有。
pub(super) fn tool() -> Option<&'static Tool> {
    probe().as_ref()
}

/// 探测到的沙箱种类，供画像 `sandbox.kind` 与 `reversible` 派生用。
pub(super) fn kind() -> SandboxKind {
    match tool() {
        Some(Tool::MacSandboxExec) => SandboxKind::SandboxExec,
        Some(Tool::LinuxBwrap(_)) => SandboxKind::Bwrap,
        None => SandboxKind::None,
    }
}

#[cfg(target_os = "linux")]
fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

/// 找不到沙箱工具时统一的拒绝理由：写清缺什么、怎么装。`NoSandbox` 标签跟这个代码库既有的
/// Fail 报文惯例（`<动作>: <类别>: <细节>`，见 `mat_shape` 的 `ShapeMismatch`）一致，
/// 供调用方/测试按前缀识别（B164：`Fail(NoSandbox)`）。
pub(super) fn missing_message(action: &str) -> String {
    format!(
        "{action}: NoSandbox: 宿主启动时没有探测到操作系统级沙箱，出于安全考虑拒绝在沙箱外执行\
         任意代码。macOS 需要 /usr/bin/sandbox-exec（系统自带，不需要另装）；Linux 需要 \
         bubblewrap 的 bwrap 在 PATH 里，例如 `apt install bubblewrap` 或 \
         `dnf install bubblewrap`。"
    )
}

/// 新建一个只属于本次调用的工作目录，返回其真实路径（已 `canonicalize()`，见模块头「排错
/// 记录」）。调用方负责事后 `remove_dir_all` 清理。
pub(super) fn new_call_dir(tag: &str) -> Result<PathBuf, String> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "jpp-sbx-{tag}-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("建沙箱工作目录失败：{e}"))?;
    std::fs::canonicalize(&dir).map_err(|e| format!("解析沙箱工作目录真实路径失败：{e}"))
}

/// 把「本来要跑的 `program args...`」包一层沙箱。写操作限定在 `writable_dir`（必须是
/// [`new_call_dir`] 给的真实路径）与 `/dev/null`；网络一律拒绝；读默认放行（见模块头注）。
pub(super) fn wrap(
    tool: &Tool,
    program: &str,
    args: &[String],
    writable_dir: &Path,
) -> Result<Command, String> {
    match tool {
        Tool::MacSandboxExec => {
            let profile_path = writable_dir.join(".jpp-sandbox.sb");
            let profile = format!(
                "(version 1)\n(allow default)\n(deny file-write*)\n\
                 (allow file-write* (subpath \"{}\"))\n\
                 (allow file-write* (literal \"/dev/null\"))\n\
                 (deny network*)\n",
                sb_escape(&writable_dir.display().to_string()),
            );
            std::fs::write(&profile_path, profile)
                .map_err(|e| format!("写沙箱 profile 失败：{e}"))?;
            let mut cmd = Command::new("/usr/bin/sandbox-exec");
            cmd.arg("-f").arg(&profile_path).arg(program).args(args);
            Ok(cmd)
        }
        Tool::LinuxBwrap(bwrap) => {
            let mut cmd = Command::new(bwrap);
            cmd.arg("--ro-bind")
                .arg("/")
                .arg("/")
                .arg("--dev")
                .arg("/dev")
                .arg("--proc")
                .arg("/proc")
                .arg("--bind")
                .arg(writable_dir)
                .arg(writable_dir)
                .arg("--unshare-net")
                .arg("--die-with-parent")
                .arg("--")
                .arg(program)
                .args(args);
            Ok(cmd)
        }
    }
}

fn sb_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 供报错信息用：这次实际用了哪个沙箱工具。
pub(super) fn describe(tool: &Tool) -> &'static str {
    tool_name(tool)
}

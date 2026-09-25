//! 子进程小工具，`exec.rs`（`exec_py`、`check_tests`）与 `retrieval.rs`（`embed_topk`）共用。
//! 从 `cli/actions_r2a.rs`（R2a）原样搬来，签名不变。

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(super) struct SubprocessResult {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: Option<i32>,
    pub(super) timed_out: bool,
}

/// 起子进程、边跑边用独立线程收 `stdout`/`stderr`（避免管道满了子进程写阻塞、
/// 主线程 `try_wait` 轮询卡死），超时后 `kill()`。`cmd` 由调用方配置好程序名、参数、环境。
pub(super) fn run_subprocess(
    mut cmd: Command,
    stdin_data: &str,
    timeout: Duration,
) -> Result<SubprocessResult, String> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("子进程启动失败：{e}"))?;
    let mut stdin_pipe = child.stdin.take().expect("piped stdin");
    let data = stdin_data.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin_pipe.write_all(data.as_bytes());
        // drop 关闭写端，子进程读到 EOF
    });
    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait().ok();
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return Err(format!("等待子进程失败：{e}")),
        }
    };
    let _ = writer.join();
    let stdout_bytes = stdout_reader.join().unwrap_or_default();
    let stderr_bytes = stderr_reader.join().unwrap_or_default();
    Ok(SubprocessResult {
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        exit_code: status.and_then(|s| s.code()),
        timed_out,
    })
}

pub(super) fn temp_script_path(tag: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "jpp-{tag}-{}-{}-{}.py",
        std::process::id(),
        nanos,
        n
    ))
}

/// 按字节数截断到 `n` 附近，安全跨多字节 UTF-8 字符（PR #36 复核 P2：`&s[..n]` 在
/// 多字节字符中间切会 panic——退到不超过 `n` 的最近一个字符边界，绝不切在字符中间）。
pub(super) fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…（截断，原长 {} 字节）", &s[..end], s.len())
}

#[cfg(test)]
mod tests {
    use super::truncate;

    /// PR #36 复核 P2：一段含中文（每字符 3 字节）的长 stderr，截断点 `n` 故意落在
    /// 某个字符的 UTF-8 编码中间（100 个「中」= 300 字节，第 34 个字符占字节 99..102，
    /// `n=101` 正好切在它中间）——修前 `&s[..101]` 会 panic。
    #[test]
    fn truncate_splits_at_char_boundary_not_mid_multibyte_char() {
        let s: String = "中".repeat(100);
        assert_eq!(s.len(), 300);
        assert!(!s.is_char_boundary(101), "构造前提：101 本该落在字符中间");
        let out = truncate(&s, 101); // 不应 panic
        assert!(out.starts_with("中"));
        assert!(out.contains("（截断，原长 300 字节）"));
        // 实际保留的字节数应退到不超过 101 的最近字符边界（99）
        assert_eq!(&out[..99], &s[..99]);
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn truncate_ascii_exact_boundary() {
        let out = truncate("hello world", 5);
        assert!(out.starts_with("hello"));
        assert!(out.contains("原长 11 字节"));
    }
}

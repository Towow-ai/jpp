//! 出料：把一趟运行的证据折进校准记录（步 14a 自 CLI `run_io.rs` 原样搬来，`21` 步 14a「出料进
//! `jpp-calib`」；`Session::feed_calib` 调它）。不读写文件：落盘仍由宿主做。
//!
//! **折进去的是无标注观察**：`absorb` 不让记录上岗（`n` 不动、冷记录推到「待真值」），
//! **上岗仍要 `commission`，而认证是校准过程不是程序行为。**

use crate::calib::CalibStore;
use jpp_effects::views::Sample;

/// 以 `store` 为底折进 `evidence`，并把本趟漂移信号标出的键（`suspend_candidates`）中处于「上岗」的
/// 记录改为「停岗候选」（B25：正式停岗由人确认）。返回新的记录集与被改为候选的键（按给出的顺序），
/// 报文由宿主打印。
pub fn feed(
    store: &CalibStore,
    evidence: &[(String, Sample)],
    suspend_candidates: &[&str],
) -> Result<(CalibStore, Vec<String>), String> {
    let mut 出 = store.clone();
    for (k, sm) in evidence {
        出.absorb(k, sm.clone())
            .map_err(|e| format!("折证据进 {k} 失败：{e}"))?;
    }
    let mut 标出 = vec![];
    for k in suspend_candidates {
        if let Some(r) = 出.records.get_mut(*k) {
            if r.status == "上岗" {
                r.status = "停岗候选".into();
                标出.push(k.to_string());
            }
        }
    }
    Ok((出, 标出))
}

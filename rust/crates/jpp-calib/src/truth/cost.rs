//! 代价线导入的前置（B129，步 20a-2a；从 `truth.rs` 拆出，控制单文件行数）。
//!
//! 两道检查都在写任何记录之前做，任一不过即整份拒收，不留半份导入：
//! 1. 只收 test 行：select / measure 的代价线未定（`commission_costed` 也拒）。
//! 2. 键上已有非代价证书或下侧证书即拒：代价线把记录的 lo 置 0，已有的两侧线的 Ignore 出口从此不可达——
//!    「一个改动若使某个出口种类变得不可达，它就不是收紧」（`CalibStore::put` 的判据），不静默做。
//!    只带代价证书的记录可以再加一张不同代价的证书（证书按地址并存）。

use super::{LabelRow, row_op};
use crate::calib::CalibStore;

pub(super) fn 代价前置<'a>(
    rows: &[LabelRow],
    keys: impl Iterator<Item = &'a String>,
    store: &CalibStore,
) -> Result<(), String> {
    for (i, r) in rows.iter().enumerate() {
        let op = row_op(r).map_err(|e| format!("第 {} 行：{e}", i + 1))?;
        if op != "test" {
            // 依据：B129（代价线只对 test 题有定义）
            return Err(format!(
                "第 {} 行：--cost 只收 test 行，收到 {op}（B129；select / measure 的代价线未定）",
                i + 1
            ));
        }
    }
    for key in keys {
        if let Some(rec) = store.records.get(key) {
            let 非代价 = rec.certs.values().filter(|c| c.cost.is_none()).count();
            if 非代价 > 0 || rec.lower.is_some() {
                // 依据：B129；`CalibStore::put` 的「出口种类不可达不是收紧」判据
                return Err(format!(
                    "键 {key} 已有 {非代价} 张不按代价认证的证书{}：代价线会把记录的 lo 置 0，让已有线的 Ignore 出口不可达（B129）。修法：在不带 --calib 的新目录里做代价认证，或先把这条记录移走",
                    if rec.lower.is_some() {
                        "与下侧证书"
                    } else {
                        ""
                    }
                ));
            }
        }
    }
    Ok(())
}

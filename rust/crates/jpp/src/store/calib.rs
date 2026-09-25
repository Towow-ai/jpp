//! 校准记录的装载、合并策略与写回（`20` v2 §2.3 `jpp::store::calib_open`/`calib_save`；
//! `jpp::cli`「隐藏的决定」：`run_io.rs` 的合并逻辑搬到这里，CLI 只传路径）。步 18-0 自 `run_io.rs` 原样搬来。

use std::path::Path;

use jpp_calib::CalibStore;

/// 装载一个校准目录。装载即按证书重跑认证（步 20c，B117），结论在返回值的 `load_report` 里，
/// 由调用者打印。
pub fn open(dir: &Path) -> Result<CalibStore, String> {
    CalibStore::load(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// 合并策略：**夹具优先**。夹具是这一次跑的显式布置，目录是常备资产；两边都给同一个键时
/// 夹具的记录覆盖目录的。返回被覆盖的键（已排序），谁赢要说得出来，由调用者报出。
pub fn merge_fixture(dir: &mut CalibStore, fixture: &CalibStore) -> Vec<String> {
    let mut 被覆盖: Vec<String> = vec![];
    for (k, rec) in &fixture.records {
        if dir.records.contains_key(k) {
            被覆盖.push(k.clone());
        }
        dir.records.insert(k.clone(), rec.clone());
    }
    被覆盖.sort();
    被覆盖
}

/// 写回（`--calib-out`）：只写记录，不写画像（画像是输入）。
pub fn save(store: &CalibStore, dir: &Path) -> Result<(), String> {
    store
        .save(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))
}

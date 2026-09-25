//! 画像装载（`20` v2 §2.3 `jpp::store::ProfileLoader`）：只收字节，路径解析在 CLI（B73）。
//! 按 `jpp-effects::profile_schema` 核字段：未知字段报错并指名，非 δ 字段缺则未测（步 15d）。

use jpp_effects::profile::Profile;
use serde_json::Value as Json;

pub struct ProfileLoader;

impl ProfileLoader {
    /// 从画像文件的字节装载。报文与 `Profile::load` 相同。
    pub fn load(bytes: &[u8]) -> Result<Profile, String> {
        let j: Json =
            serde_json::from_slice(bytes).map_err(|e| format!("档案不是合法 JSON：{e}"))?;
        Profile::from_json(&j)
    }
}

//! 反序列化的第二条构造路径（`20` A3：反序列化走 `Raw` 与 `TryFrom`，重新经过构造函数）。
//!
//! 步 8a-4：`Mat` 的反序列化先读成 [`MatRaw`]，再经 `Mat::new` 重算哈希；文件里写的哈希
//! 与内容不符即拒（I6：材料带来源链与 taint，哈希由内容定，不信文件里的）。
//! `State`、`Question`、`Form` 的同类收紧随步 8b（读数与责任改句柄）一起做。

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::Value as Json;

use crate::value::{Mat, Taint};

/// ```
/// // 依据：20 §2.4 `bypass/i6_raw`：改过内容的材料，反序列化被拒
/// use jpp_value::value::Mat;
/// let good = serde_json::to_value(Mat::literal(serde_json::json!("原文"))).unwrap();
/// assert!(serde_json::from_value::<Mat>(good.clone()).is_ok());
/// let mut bad = good;
/// bad["content"] = serde_json::json!("改过的内容");
/// let e = serde_json::from_value::<Mat>(bad).unwrap_err().to_string();
/// assert!(e.contains("I6"), "{e}");
/// ```
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatRaw {
    pub content: Json,
    pub addr: String,
    pub modality: String,
    pub origin: Vec<String>,
    pub taint: Taint,
    pub derived_from: BTreeSet<String>,
    pub hash: String,
    /// 来源出口键（B59，步 17a）；不进哈希，缺省为空。
    #[serde(default)]
    pub from_key: BTreeSet<String>,
    /// 值依赖边（B92，步 18c）；缺省为空。
    #[serde(default)]
    pub value_q: std::collections::BTreeMap<String, String>,
}

impl TryFrom<MatRaw> for Mat {
    type Error = String;
    fn try_from(r: MatRaw) -> Result<Mat, String> {
        let mut m = Mat::new(r.content, &r.addr, r.origin, r.taint, r.derived_from)
            .with_from_key(r.from_key);
        m.value_q = r.value_q;
        // 依据：I6（20 §2.4 `bypass/i6_raw`）：哈希只由内容与地址算，文件里的不作数
        if m.hash != r.hash {
            return Err(format!(
                "I6: 材料哈希与内容不符（文件里 {}，按内容算 {}）",
                r.hash, m.hash
            ));
        }
        if r.modality != m.modality {
            return Err(format!(
                "I6: 材料模态 {} 不是本版支持的 {}",
                r.modality, m.modality
            ));
        }
        Ok(m)
    }
}

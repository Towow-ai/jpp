//! 宿主入口参数 `EntryArgs`（`20` §2.3；B105、B106；`21` 步 14b）。取代步 14b-0 的 `HostInput`。
//!
//! 两种条目（B105-1）：**值条目**按 B33 第 2 条读出规则绑定（与 `content()` 读出的值同形，叶子 taint =
//! 宿主声明、缺省 Untrusted，sources ∅，不带 origin；程序直接取字段）；**材料条目**绑定为 `Value::Mat`，
//! 运行入口盖 `origin = ["input"]`、清空 `from_key` 与 `derived_from`，taint 取 `Mat` 上宿主声明的位。
//! `purpose` 有则进 `entry_hash`（B105-3），并以名字 `purpose` 绑定为不可信 Text（步 17b 随 B58 落地；
//! 14b 时推迟，因为 B105-3 的放行论证以题面 taint 已生效为前提，主会话 2026-09-25）。
//! 依据：B105 / B106（地基/附注/2026-09-25-B105-B106裁定.md）

use jpp_ir::ir::{EntryDecl, EntryKind, EntryParam, EntryTaint};
use jpp_ir::key::{canon, hash_of};
use jpp_value::value::{Mat, Taint};
use serde_json::Value as Json;

/// 一条值条目：宿主给的 JSON，按读出规则绑定
#[derive(Clone, Debug)]
pub struct EntryValue {
    pub name: String,
    pub value: Json,
    /// 宿主声明的 taint；缺省 Untrusted（[`EntryValue::new`]）
    pub taint: Taint,
}

impl EntryValue {
    /// 缺省不可信的值条目（CLI `--input` 走这里）
    pub fn new(name: &str, value: Json) -> EntryValue {
        EntryValue {
            name: name.to_string(),
            value,
            taint: Taint::Untrusted,
        }
    }
    /// 宿主声明 taint（放行方向的杠杆：进 `entry_hash`，CLI 不暴露）
    pub fn with_taint(mut self, taint: Taint) -> EntryValue {
        self.taint = taint;
        self
    }
}

/// 一条材料条目：宿主给的 `Mat`，整份绑定；origin 与来源键由运行入口盖，不由宿主写
#[derive(Clone, Debug)]
pub struct EntryMat {
    pub name: String,
    pub mat: Mat,
}

impl EntryMat {
    /// 用宿主造好的材料（taint 取材料上声明的位）
    pub fn new(name: &str, mat: Mat) -> EntryMat {
        EntryMat {
            name: name.to_string(),
            mat,
        }
    }
    /// 缺省不可信的材料条目
    pub fn untrusted(name: &str, content: Json) -> EntryMat {
        let mut m = Mat::literal(content);
        m.taint = Taint::Untrusted;
        EntryMat::new(name, m)
    }
    /// 绑定进程序的材料：`origin = ["input"]`，`from_key`、`derived_from` 清空，taint 与哈希不变
    pub fn bound_mat(&self) -> Mat {
        let mut m = self.mat.clone();
        m.origin = vec!["input".into()];
        m.from_key.clear();
        m.derived_from.clear();
        m
    }
}

/// 宿主交给程序的目的与入口条目（`20` §2.3 `EntryArgs`，B105）
#[derive(Clone, Debug, Default)]
pub struct EntryArgs {
    pub purpose: Option<String>,
    pub values: Vec<EntryValue>,
    pub materials: Vec<EntryMat>,
}

fn taint_word(t: Taint) -> &'static str {
    match t {
        Taint::Trusted => "trusted",
        Taint::Untrusted => "untrusted",
    }
}

fn entry_taint(t: Taint) -> EntryTaint {
    match t {
        Taint::Trusted => EntryTaint::Trusted,
        Taint::Untrusted => EntryTaint::Untrusted,
    }
}

impl EntryArgs {
    /// 只有一条值条目（CLI `--input`）
    pub fn value(name: &str, value: Json) -> EntryArgs {
        EntryArgs {
            values: vec![EntryValue::new(name, value)],
            ..EntryArgs::default()
        }
    }
    pub fn is_empty(&self) -> bool {
        self.purpose.is_none() && self.values.is_empty() && self.materials.is_empty()
    }
    /// 入口声明（B106）：`Session::compile` 把它写进 `Program.entry`。顺序：`purpose`、值条目、材料条目
    /// （与绑定顺序相同）。`purpose` 记作名为 `purpose` 的不可信值条目（B105-3；步 17b 随 B58 绑定），
    /// 所以与同名条目撞名时 `compile` 报 `E-entry-dup`。
    pub fn decl(&self) -> EntryDecl {
        let mut params = vec![];
        if self.purpose.is_some() {
            params.push(EntryParam {
                name: "purpose".into(),
                kind: EntryKind::Value,
                taint: entry_taint(Taint::Untrusted),
            });
        }
        for v in &self.values {
            params.push(EntryParam {
                name: v.name.clone(),
                kind: EntryKind::Value,
                taint: entry_taint(v.taint),
            });
        }
        for m in &self.materials {
            params.push(EntryParam {
                name: m.name.clone(),
                kind: EntryKind::Mat,
                taint: entry_taint(m.mat.taint),
            });
        }
        EntryDecl { params }
    }
    /// 账本头 `entry_hash`（B105-2）：`hash_of(["entry", purpose 或 "", 每条目按名字排序依次:
    /// 种类、名字、canon(JSON) 或 Mat.hash、taint])`；无条目且无 `purpose` 为 `None`。一份算法。
    /// 依据：B105（地基/附注/2026-09-25-B105-B106裁定.md §二）
    pub fn hash(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut items: Vec<(String, [String; 4])> = vec![];
        for v in &self.values {
            items.push((
                v.name.clone(),
                [
                    "value".into(),
                    v.name.clone(),
                    canon(&v.value),
                    taint_word(v.taint).into(),
                ],
            ));
        }
        for m in &self.materials {
            items.push((
                m.name.clone(),
                [
                    "mat".into(),
                    m.name.clone(),
                    m.mat.hash.clone(),
                    taint_word(m.mat.taint).into(),
                ],
            ));
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let mut parts: Vec<&str> = vec!["entry", self.purpose.as_deref().unwrap_or("")];
        for (_, f) in &items {
            parts.extend(f.iter().map(|s| s.as_str()));
        }
        Some(hash_of(&parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 哈希与条目顺序无关_空入口为none() {
        assert_eq!(EntryArgs::default().hash(), None);
        let a = EntryArgs {
            values: vec![
                EntryValue::new("a", json!({"x": 1})),
                EntryValue::new("b", json!(2)),
            ],
            ..Default::default()
        };
        let b = EntryArgs {
            values: vec![
                EntryValue::new("b", json!(2)),
                EntryValue::new("a", json!({"x": 1})),
            ],
            ..Default::default()
        };
        assert_eq!(a.hash(), b.hash());
        assert!(a.hash().is_some());
        let only_purpose = EntryArgs {
            purpose: Some("找出要退款的对话".into()),
            ..Default::default()
        };
        assert!(only_purpose.hash().is_some());
    }

    #[test]
    fn 声明的taint与种类进哈希() {
        let v = EntryArgs::value("input", json!({"x": 1}));
        let mut t = v.clone();
        t.values[0].taint = Taint::Trusted;
        assert_ne!(v.hash(), t.hash());
        let m = EntryArgs {
            materials: vec![EntryMat::untrusted("input", json!({"x": 1}))],
            ..Default::default()
        };
        assert_ne!(v.hash(), m.hash());
    }
}

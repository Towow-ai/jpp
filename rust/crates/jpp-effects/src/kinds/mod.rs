//! 效应注册表（S2）：每种效应一文件，`spec(id)` 是唯一查表处。`EffectId` 的变体名只许出现在这个目录
//! 与内置端口里（A2，`scripts/grep_effect_names.py`）。

use jpp_ir::key::EffectId;

use crate::spec::EffectSpec;

mod ask;
mod r#do;
mod r#gen;
mod judge;
mod transform;

/// 五种效应，按注册顺序。
pub const ALL: [EffectId; 5] = [
    EffectId::Judge,
    EffectId::Gen,
    EffectId::Do,
    EffectId::Ask,
    EffectId::Transform,
];

/// 经端口的效应（步 15c 由 `CLIENT_SERVED` 改名）：宿主为它们注册端口。`do` 走动作登记处，
/// `transform` 是宿主函数，都不经端口。
pub const PORTED: [EffectId; 3] = [EffectId::Judge, EffectId::Gen, EffectId::Ask];

/// 按源码名查效应（步 15a）：名字表、检查器、规划与运行时的内置分派都经它取 `EffectSpec`，再按字段
/// 分派，不按效应名分支（`20` A2）。不是效应名时为 `None`。
pub fn by_name(name: &str) -> Option<&'static EffectSpec> {
    ALL.into_iter().map(spec).find(|s| s.name == name)
}

/// 第一个满足字段条件的效应（步 15b）：注册表外的代码按字段找效应，例如后端取「产出读数的效应」，
/// 不写效应变体名（`20` A2）。
pub fn find(pred: impl Fn(&EffectSpec) -> bool) -> Option<EffectId> {
    ALL.into_iter().find(|id| pred(spec(*id)))
}

/// 唯一注册表。
pub fn spec(id: EffectId) -> &'static EffectSpec {
    match id {
        EffectId::Judge => &judge::SPEC,
        EffectId::Gen => &r#gen::SPEC,
        EffectId::Do => &r#do::SPEC,
        EffectId::Ask => &ask::SPEC,
        EffectId::Transform => &transform::SPEC,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 注册表按id取回自己() {
        for id in ALL {
            assert_eq!(spec(id).id, id);
        }
    }

    #[test]
    fn 名字与序列化名一致() {
        for id in ALL {
            let ser = serde_json::to_value(id).unwrap();
            assert_eq!(ser.as_str(), Some(spec(id).name));
        }
    }

    /// B37：类假设只挂在产出读数的效应上，而今天只有 judge 产出读数
    #[test]
    fn 只有判断产出读数() {
        let v: Vec<_> = ALL
            .into_iter()
            .filter(|i| spec(*i).produces_reading)
            .collect();
        assert_eq!(v, vec![EffectId::Judge]);
        for id in ALL {
            let s = spec(id);
            assert_eq!(
                s.produces_reading,
                s.profile_schema == crate::spec::ProfileSchema::Reading
            );
        }
    }

    #[test]
    fn 只有do触世界() {
        let v: Vec<_> = ALL
            .into_iter()
            .filter(|i| spec(*i).side_effecting)
            .collect();
        assert_eq!(v, vec![EffectId::Do]);
    }

    /// 效应行只收作者声明的效应形式；记账变换不进（`12` §2.8）
    #[test]
    fn 只有transform不进效应行() {
        let v: Vec<_> = ALL
            .into_iter()
            .filter(|i| !spec(*i).in_effect_row)
            .collect();
        assert_eq!(v, vec![EffectId::Transform]);
    }

    /// 键里的轮次序号分量必须有同名输入槽（步 12a/12b 查出 gen 缺 retry_seq、do 第 3 槽误登记为 cost）
    #[test]
    fn 轮次序号分量都有输入槽() {
        use crate::spec::KeyPart;
        for id in ALL {
            let s = spec(id);
            for (part, slot) in [
                (KeyPart::IterSeq, "iter_seq"),
                (KeyPart::RetrySeq, "retry_seq"),
            ] {
                if s.key_parts.contains(&part) {
                    assert!(
                        s.input_schema.iter().any(|d| d.name == slot),
                        "{} 的键含 {slot} 却没有这一输入槽",
                        s.name
                    );
                }
            }
        }
    }

    /// `12` §2.11：cut 与 gen 继承，do 与 transform 由声明给出，ask 可信
    #[test]
    fn taint规则按12表() {
        use crate::spec::TaintRule::*;
        let t: Vec<_> = ALL.into_iter().map(|i| spec(i).taint_rule).collect();
        assert_eq!(t, vec![Inherit, Inherit, Declared, Trusted, Declared]);
    }

    /// 判断键的分量逐项对应 `JudgeKey` 的字段（除去 digest 不含的无）
    #[test]
    fn 判断键分量对应judge_key字段() {
        let k = jpp_ir::key::JudgeKey::new("m", "s", "q", "noul", 0, 0, 0);
        let fields = serde_json::to_value(&k).unwrap().as_object().unwrap().len();
        assert_eq!(spec(EffectId::Judge).key_parts.len(), fields);
    }
}

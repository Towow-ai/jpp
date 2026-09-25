//! 出口合成构造 `compose`（B131，步 25-2b）：出口的第二个也是最后一个来源。
//!
//! 从若干分量出口按**封闭规则集**（`any`、`all`、`{first: k}`、`{sup: 格}`、`min`）得一个出口：种类由
//! 内核按规则算出（[`合成种类`]，纯函数），调用者只选规则、给不出种类；taint 取分量之 ∨；分量记进
//! `Exit.parts`（谱系入口）；结果未决时把给定的未决分量的责任并入合成出口。规则的未决取法是强 Kleene
//! 在格上的推广：结果只在未决分量的一切解析下都相同时才已决（最坏元素已出现即最坏，否则任一分量
//! 未决即未决；B51-R1）。
//!
//! 本步它是**内部构造**：只经 `caps.rs` 的入口 `调合成` 被 `tally`、`first_k` 调用；开放成 `.jpp` 名字在
//! 步 25-8（`examples/composition.jpp` 自定义了 `compose`，开放会改该金样，见 `过程记录/工程-步25-2b.md`
//! Q16）。合成出口的放行派生（全部分量放行之合取、谱系穿 `parts`）在步 25-9 落；此前合成出口按冷线记，
//! 不作放行证据（步 25-1）。
//!
//! 依据：B131（地基/附注/2026-09-25-库层出口合成与待补批3裁定.md §一；12 §2.3 出口合成条）；B51-R1；B3

use crate::caps::Caps;
use crate::*;

/// 合成规则（B131 表；封闭，加规则须先改 `12` §2.3 B131 条）。
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum 规则 {
    /// 是非题：任一 Act 即 Act；全部 Ignore 即 Ignore；否则未决
    Any,
    /// 是非题：任一 Ignore 即 Ignore；全部 Act 即 Act；否则未决
    All,
    /// 是非题，按调用者给的顺序读：凑够 k 个 Act 即 Act；读到未决而未凑够即未决；读完不足即 Ignore
    First(usize),
    /// K 选一：声明格上的上确界。格是候选下标从低到高的一条链（施工解读，见过程记录）。
    /// 本步只有单元测试用它；步 25-8 开放 `compose` 后由 `.jpp` 选用（`worst_index`）
    #[allow(dead_code)]
    Sup(Vec<usize>),
    /// 打分：档取最低。步 25-8 起由 `.jpp` 选用（`worst_grade`）
    #[allow(dead_code)]
    Min,
}

impl 规则 {
    fn 名(&self) -> &'static str {
        match self {
            规则::Any => "any",
            规则::All => "all",
            规则::First(_) => "first",
            规则::Sup(_) => "sup",
            规则::Min => "min",
        }
    }
}

/// 一个分量：它的出口种类，与（若有）它的出口。`tally` 数的元素不一定带出口（调用者用 `outcome()` 造的
/// 契约值），种类照 `tally` 的列表成员定；有出口的进 `parts`（`过程记录/工程-步25-2b.md` Q14）。
pub(crate) struct 分量 {
    pub 种类: ExitKind,
    pub 出口: Option<Rc<Exit>>,
}

/// 一次合成的请求（内部入口 `调合成` 收它）。
pub(crate) struct 合成请求<'r> {
    pub 规则: 规则,
    pub 分量: Vec<分量>,
    /// 结果未决时并入合成出口的未决责任（B131 (3)；调用者给，`first_k` 现行不吸收，见 Q15）
    pub 吸收: &'r [Rc<Exit>],
    pub op: Op,
    /// 合成出口的题键（报告里出口的 `q`）
    pub 题键: &'r str,
    /// 结果已决时合成出口自身的消费标签
    pub 已决标签: &'r str,
    /// 被吸收的分量的消费标签
    pub 吸收标签: &'r str,
}

fn 未决原因(分量: &[ExitKind]) -> Option<String> {
    let us: Vec<&String> = 分量
        .iter()
        .filter_map(|k| match k {
            ExitKind::Unsure(c) => Some(c),
            _ => None,
        })
        .collect();
    // B131 (4)：缺席类（没观察到）优先，否则第一个未决分量的原因
    us.iter()
        .find(|c| 缺席类原因.contains(&c.as_str()))
        .or(us.first())
        .map(|c| (*c).clone())
}

/// 合成的种类（纯函数，B131 表）。分量类型不符规则时报错。
pub(crate) fn 合成种类(r: &规则, 分量: &[ExitKind]) -> Result<ExitKind, String> {
    let 类型不符 = |want: &str| {
        Err(format!(
            "compose 的规则 {} 只收{want}的出口（或未决）",
            r.名()
        ))
    };
    match r {
        规则::Any | 规则::All | 规则::First(_) => {
            if 分量
                .iter()
                .any(|k| !matches!(k, ExitKind::Act | ExitKind::Ignore | ExitKind::Unsure(_)))
            {
                return 类型不符("是非题");
            }
        }
        规则::Sup(_) => {
            if 分量
                .iter()
                .any(|k| !matches!(k, ExitKind::Pick(_) | ExitKind::Unsure(_)))
            {
                return 类型不符("K 选一");
            }
        }
        规则::Min => {
            if 分量
                .iter()
                .any(|k| !matches!(k, ExitKind::At(_) | ExitKind::Unsure(_)))
            {
                return 类型不符("打分");
            }
        }
    }
    match r {
        规则::Any | 规则::All => {
            let (胜, 负) = if *r == 规则::Any {
                (ExitKind::Act, ExitKind::Ignore)
            } else {
                (ExitKind::Ignore, ExitKind::Act)
            };
            if 分量.contains(&胜) {
                Ok(胜)
            } else if let Some(c) = 未决原因(分量) {
                Ok(ExitKind::Unsure(c))
            } else {
                Ok(负)
            }
        }
        规则::First(k) => {
            if *k == 0 {
                return Err("compose 的规则 first 要 k ≥ 1".into());
            }
            // B3：没有候选 → no_candidate；有候选、全部否定 → rejected_all
            if 分量.is_empty() {
                return Ok(ExitKind::Unsure("no_candidate".into()));
            }
            if 分量.iter().all(|x| *x == ExitKind::Ignore) {
                return Ok(ExitKind::Unsure("rejected_all".into()));
            }
            let mut n = 0;
            for x in 分量 {
                match x {
                    ExitKind::Act => {
                        n += 1;
                        if n == *k {
                            return Ok(ExitKind::Act);
                        }
                    }
                    // 挡路的未决：原因取这个分量的
                    ExitKind::Unsure(c) => return Ok(ExitKind::Unsure(c.clone())),
                    _ => {}
                }
            }
            Ok(ExitKind::Ignore)
        }
        规则::Sup(格) => {
            let 位 = |p: usize| 格.iter().position(|x| *x == p);
            let mut 最高: Option<(usize, usize)> = None;
            for x in 分量 {
                if let ExitKind::Pick(p) = x {
                    let Some(i) = 位(*p) else {
                        return Err(format!("compose 的规则 sup：候选 {p} 不在声明的格里"));
                    };
                    if 最高.is_none_or(|(j, _)| i > j) {
                        最高 = Some((i, *p));
                    }
                }
            }
            if 分量.is_empty() {
                return Err("compose 的规则 sup 至少要一个分量".into());
            }
            match 最高 {
                // 格顶已出现即格顶（任一触发即升级）
                Some((i, p)) if i + 1 == 格.len() => Ok(ExitKind::Pick(p)),
                _ => match 未决原因(分量) {
                    Some(c) => Ok(ExitKind::Unsure(c)),
                    None => Ok(ExitKind::Pick(最高.expect("非空且无未决").1)),
                },
            }
        }
        规则::Min => {
            if 分量.is_empty() {
                return Err("compose 的规则 min 至少要一个分量".into());
            }
            let 最低 = 分量
                .iter()
                .filter_map(|x| match x {
                    ExitKind::At(l) => Some(*l),
                    _ => None,
                })
                .min();
            match 最低 {
                // 最低档（0）已出现即最低
                Some(0) => Ok(ExitKind::At(0)),
                _ => match 未决原因(分量) {
                    Some(c) => Ok(ExitKind::Unsure(c)),
                    None => Ok(ExitKind::At(最低.expect("非空且无未决"))),
                },
            }
        }
    }
}

impl<'a> Interp<'a> {
    /// 合成构造的实现（只经 `caps.rs::调合成` 进来，令牌按 `compose` 的声明发放）。
    pub(crate) fn 合成(&mut self, caps: &Caps, 请求: 合成请求, sp: Span) -> R<Value> {
        let x =
            match caps
                .issue_composite()
                .issue(self, &请求.规则, &请求.分量, 请求.op, 请求.题键, sp)
            {
                Ok(x) => x,
                Err(m) => return err(Some("E-rt-arg"), m, sp),
            };
        if let Value::Exit(e) = &x {
            if e.is_unsure() {
                // 分量的未决责任并入合成出口；合成出口自己进调用者的未决清单
                for p in 请求.吸收 {
                    caps.duty().settle(p, 请求.吸收标签);
                }
            } else {
                // 已决的合成出口不带责任
                caps.duty().settle(e, 请求.已决标签);
            }
        }
        Ok(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(c: &str) -> ExitKind {
        ExitKind::Unsure(c.into())
    }
    use ExitKind::{Act as A, At, Ignore as I, Pick};

    #[test]
    fn any_真值表() {
        assert_eq!(合成种类(&规则::Any, &[A, I]), Ok(A));
        assert_eq!(合成种类(&规则::Any, &[I, I]), Ok(I));
        assert_eq!(合成种类(&规则::Any, &[]), Ok(I));
        assert_eq!(合成种类(&规则::Any, &[I, u("band")]), Ok(u("band")));
        // 最坏元素与未决并存：已触发即定
        assert_eq!(合成种类(&规则::Any, &[u("band"), A]), Ok(A));
    }

    #[test]
    fn all_真值表() {
        assert_eq!(合成种类(&规则::All, &[A, A]), Ok(A));
        assert_eq!(合成种类(&规则::All, &[A, I]), Ok(I));
        assert_eq!(合成种类(&规则::All, &[]), Ok(A));
        assert_eq!(合成种类(&规则::All, &[A, u("cold")]), Ok(u("cold")));
        assert_eq!(合成种类(&规则::All, &[u("cold"), I]), Ok(I));
    }

    #[test]
    fn first_按序读() {
        assert_eq!(合成种类(&规则::First(1), &[I, A, u("band")]), Ok(A));
        assert_eq!(合成种类(&规则::First(1), &[I, u("band"), A]), Ok(u("band")));
        assert_eq!(合成种类(&规则::First(2), &[A, I]), Ok(I));
        assert_eq!(合成种类(&规则::First(1), &[]), Ok(u("no_candidate")));
        assert_eq!(合成种类(&规则::First(1), &[I, I]), Ok(u("rejected_all")));
        // 挡路分量的原因，不按缺席类优先
        assert_eq!(
            合成种类(&规则::First(2), &[A, u("band"), u("budget")]),
            Ok(u("band"))
        );
        assert!(合成种类(&规则::First(0), &[A]).is_err());
    }

    #[test]
    fn sup_格上确界() {
        let 格 = 规则::Sup(vec![2, 0, 1]); // 1 最高
        assert_eq!(合成种类(&格, &[Pick(2), Pick(0)]), Ok(Pick(0)));
        assert_eq!(合成种类(&格, &[Pick(2), u("tie"), Pick(1)]), Ok(Pick(1)));
        assert_eq!(合成种类(&格, &[Pick(2), u("tie")]), Ok(u("tie")));
        assert!(合成种类(&格, &[Pick(7)]).is_err());
        assert!(合成种类(&格, &[]).is_err());
    }

    #[test]
    fn min_档取最低() {
        assert_eq!(合成种类(&规则::Min, &[At(2), At(1)]), Ok(At(1)));
        assert_eq!(合成种类(&规则::Min, &[At(2), u("band"), At(0)]), Ok(At(0)));
        assert_eq!(合成种类(&规则::Min, &[At(2), u("band")]), Ok(u("band")));
        assert!(合成种类(&规则::Min, &[]).is_err());
    }

    #[test]
    fn 未决原因_缺席类优先() {
        assert_eq!(
            合成种类(&规则::Any, &[u("band"), u("absent"), u("budget")]),
            Ok(u("absent"))
        );
        assert_eq!(合成种类(&规则::All, &[u("cold"), u("band")]), Ok(u("cold")));
    }

    #[test]
    fn 类型不符报错() {
        assert!(合成种类(&规则::Any, &[Pick(0)]).is_err());
        assert!(合成种类(&规则::Sup(vec![0]), &[A]).is_err());
        assert!(合成种类(&规则::Min, &[Pick(0)]).is_err());
    }
}

//! 值级来源标签（B84，步 17c）：`Provenance = (taint, sources)`。
//!
//! taint 是 B33 的内容来源属性（有人为内容担保吗），sources 是直接来源读数的账本键集合（B59 的跳、
//! B45 派生题、谱系放行都读它）。两者是同一标签的两个分量，沿同一张边界表（`20` §3.10）传播，
//! 合并只在 [`join`] 一处：taint 取 ∨，sources 取 ∪。只有「按计算键取下标或字段」一行两者不同：
//! sources 并入键的 sources，taint 取元素自身的位（B33 第 3 条）。控制流两者都不传播。
//!
//! sources 只记直接来源（保一跳，不做闭包）；更早的祖先经账本 `Entry::Judge.parents` 链按需算。
//! 依据：B84（`地基/附注/2026-09-24-B83续接缺口裁定.md` §二）。

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::value::Taint;

/// 来源边的种类（B92，步 18c）。**值依赖**：内容由读数的值算出（`pick`/`at` 的 k、出口读出、
/// `as_mat(Exit)`、`literalize`、由这些拼接或计算出的文本与材料）；**选择依赖**：内容先于读数存在，
/// 读数只决定选了哪个（`sieve`/`pair` 元素的 `item`、按计算键取的下标与字段，如 `over[k]`）。
/// 三个消费者三种投影：J-02 的 `derived_from` 只取值依赖边的题哈希；跳数（B59）与谱系放行（B72-4）计全部边。
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Value,
    Select,
}

/// 一条来源边：种类，与产生它的出口的题哈希（值依赖边的 J-02 投影用；选择边可为空）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub kind: EdgeKind,
    pub q: String,
}

/// 直接来源读数：账本键 → 边。`None` = 空集（不分配）。按键排序（D13.5：传播确定，重放逐字节）。
/// 同一键两种边并存按值边计（B92）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources(Option<Rc<BTreeMap<String, Edge>>>);

impl Sources {
    pub fn empty() -> Sources {
        Sources(None)
    }
    /// 一条只有键的边（选择依赖、无题哈希）：结构通道上不经出口值的边与测试夹具用。空键即空集。
    pub fn from_key(k: &str) -> Sources {
        Sources::one(k, EdgeKind::Select, "")
    }
    /// 值依赖边：由出口 `k`（题哈希 `q`）的值算出（B92）。
    pub fn value(k: &str, q: &str) -> Sources {
        Sources::one(k, EdgeKind::Value, q)
    }
    /// 选择依赖边：由出口 `k` 选出（B92）。
    pub fn select(k: &str, q: &str) -> Sources {
        Sources::one(k, EdgeKind::Select, q)
    }
    fn one(k: &str, kind: EdgeKind, q: &str) -> Sources {
        if k.is_empty() {
            return Sources(None);
        }
        Sources(Some(Rc::new(BTreeMap::from([(
            k.to_string(),
            Edge {
                kind,
                q: q.to_string(),
            },
        )]))))
    }
    /// 一组只有键的边（选择依赖）。
    pub fn from_set(s: BTreeSet<String>) -> Sources {
        Sources::from_map(
            s.into_iter()
                .map(|k| {
                    (
                        k,
                        Edge {
                            kind: EdgeKind::Select,
                            q: String::new(),
                        },
                    )
                })
                .collect(),
        )
    }
    pub fn from_map(m: BTreeMap<String, Edge>) -> Sources {
        let m: BTreeMap<String, Edge> = m.into_iter().filter(|(k, _)| !k.is_empty()).collect();
        if m.is_empty() {
            Sources(None)
        } else {
            Sources(Some(Rc::new(m)))
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    /// 全部边的键（跳数、谱系放行、账本 `parents` 用）。
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter().flat_map(|s| s.keys())
    }
    pub fn edges(&self) -> impl Iterator<Item = (&String, &Edge)> {
        self.0.iter().flat_map(|s| s.iter())
    }
    pub fn to_set(&self) -> BTreeSet<String> {
        self.iter().cloned().collect()
    }
    /// 值依赖边的题哈希（J-02 的 `derived_from` 投影，B92）。
    pub fn value_q(&self) -> impl Iterator<Item = &String> {
        self.edges()
            .filter(|(_, e)| e.kind == EdgeKind::Value && !e.q.is_empty())
            .map(|(_, e)| &e.q)
    }
    /// 值依赖边：键 → 题哈希。
    pub fn value_edges(&self) -> BTreeMap<String, String> {
        self.edges()
            .filter(|(_, e)| e.kind == EdgeKind::Value)
            .map(|(k, e)| (k.clone(), e.q.clone()))
            .collect()
    }
    /// 全部改为选择依赖（按计算键取下标或字段时，键的来源作为选择边并入元素，B92）。
    pub fn as_select(&self) -> Sources {
        match &self.0 {
            None => Sources(None),
            Some(m) if m.values().all(|e| e.kind == EdgeKind::Select) => self.clone(),
            Some(m) => Sources(Some(Rc::new(
                m.iter()
                    .map(|(k, e)| {
                        (
                            k.clone(),
                            Edge {
                                kind: EdgeKind::Select,
                                q: e.q.clone(),
                            },
                        )
                    })
                    .collect(),
            ))),
        }
    }
    /// ∪。任一侧为空时共享另一侧，不分配。同键两种边并存取值边（B92）。
    pub fn union(&self, other: &Sources) -> Sources {
        match (&self.0, &other.0) {
            (None, _) => other.clone(),
            (_, None) => self.clone(),
            (Some(a), Some(b)) => {
                if Rc::ptr_eq(a, b) {
                    return self.clone();
                }
                let mut m = (**a).clone();
                let mut changed = false;
                for (k, e) in b.iter() {
                    match m.get(k) {
                        None => {
                            m.insert(k.clone(), e.clone());
                            changed = true;
                        }
                        Some(old) if old.kind == EdgeKind::Select && e.kind == EdgeKind::Value => {
                            m.insert(k.clone(), e.clone());
                            changed = true;
                        }
                        Some(old) if old.q.is_empty() && !e.q.is_empty() && old.kind == e.kind => {
                            m.insert(k.clone(), e.clone());
                            changed = true;
                        }
                        _ => {}
                    }
                }
                if changed {
                    Sources(Some(Rc::new(m)))
                } else {
                    self.clone()
                }
            }
        }
    }
}

/// 值级来源标签：B33 的 taint 与 B59/B84 的 sources。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub taint: Taint,
    pub sources: Sources,
}

impl Provenance {
    /// 程序自己造的值：trusted、无来源读数。
    pub fn trusted() -> Provenance {
        Provenance {
            taint: Taint::Trusted,
            sources: Sources::empty(),
        }
    }
    pub fn new(taint: Taint, sources: Sources) -> Provenance {
        Provenance { taint, sources }
    }
    /// 只有来源、taint 取 trusted（∨ 的单位元）：用于「sources 并入、taint 不并」的边。
    pub fn sources_only(sources: Sources) -> Provenance {
        Provenance {
            taint: Taint::Trusted,
            sources,
        }
    }
    /// join 的单位元吗（trusted 且无来源）：是则 `with_prov` 原样返回。
    pub fn is_unit(&self) -> bool {
        self.taint == Taint::Trusted && self.sources.is_empty()
    }
}

impl From<Taint> for Provenance {
    fn from(t: Taint) -> Provenance {
        Provenance {
            taint: t,
            sources: Sources::empty(),
        }
    }
}

/// 唯一合并处：taint ∨、sources ∪。
pub fn join(a: &Provenance, b: &Provenance) -> Provenance {
    Provenance {
        taint: Taint::join(a.taint, b.taint),
        sources: a.sources.union(&b.sources),
    }
}

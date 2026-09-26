//! J++ 校准（`20` §2.3 `jpp-calib`，L3）：校准记录与证书、校准库（`commission` 是唯一产线）、
//! `fit` 注册表、真值通道。强度估计（`strength`）步 14a 搬进 `jpp-runtime`（`allocate` 构造是唯一调用方，运行时不依赖本 crate）。只依赖 `jpp-ir`、`jpp-value`、`jpp-effects`。
//!
//! 运行时与检查器经 `jpp_effects::views::CalibView` 读线，不直接依赖这里的类型（`20` §2.2 第 4 条）；
//! 步 11 起 `CalibStore` 实现 `CalibView`。`jpp-core` 在原路径重导出本 crate 的公共项。

pub mod calib;
pub mod feed;
pub mod fit;
pub mod truth;

pub use calib::*;
pub use fit::*;

use serde_json::Value as Json;

/// **校准库的哈希**（进账本头，J-18）。（步 11 自 `jpp-core/src/effects/profile.rs` 搬入，代码不变；文档里「反面教材就在下面几行」改为直接点名 `behavior_hash`，因为它不再在同一文件）
///
/// 为什么要它：**出口 = f(读数, 线)。读数进了账本，线没有。** 头里的 `profile_hash`
/// 记的是**档案里的缺省线**，而 `cut` 用的是**按键的线**——同一份账本换一批校准记录重放，
/// 读数一样、出口可以不一样，**而账本上看不出**。运行期写入口接上之后，
/// 校准记录在两次运行之间会长，所以这不再是理论上的可变。
///
/// **用排除法，不用列举法。** 覆盖整条记录，只减去一张封闭的「说明字段」清单。
/// 反面教材是 `behavior_hash`：它是列举法，**它自己的注释承认了失效方式**
/// ——「加字段时要同步这里——漏加的后果是『行为变了但摘要没变』」。列举法在有人加新字段时
/// **会说出一个假的「相同」**，而假的「相同」倒向放行那一侧。
///
/// **空库也有哈希，不是 `None`。** 头里的 `None` 只该有一个意思：**这份账本早于这个字段**。
/// 让空库也占 `None`，两种情形在账本上就分不开。
pub fn calib_hash(store: &CalibStore) -> String {
    /// 说明性字段：改了不影响执行，所以不进哈希。
    /// 步 20a-1 之前这张表是空的——`CalibRecord` 每个字段都承载行为（`hi`/`lo`/`n`/`status` 进 `cut`，
    /// `delta` 进 `delta_for`，`unsure_rate` 进 J-10，`set_id` 进 J-16，`samples` 是证据本身）。
    /// **空表不是摆设**：它是加 `note` 那类字段时该动的那一处，
    /// 有了它，新字段的默认归宿是「进哈希」而不是「被忘掉」。
    /// 步 20a-1 起有一项：`kind`（B76 题类）是 `op`、`request`、槽声明的函数，不携带新信息，
    /// 按 B76 不进任何哈希。
    const 说明字段: &[&str] = &["kind"];

    // BTreeMap：哈希不随写入顺序变，否则同一批线会因写入顺序不同报假 W-header
    let mut sorted: std::collections::BTreeMap<&str, Json> = Default::default();
    for (k, rec) in &store.records {
        let mut j = serde_json::to_value(rec).unwrap_or(Json::Null);
        if let Json::Object(m) = &mut j {
            m.retain(|f, _| !说明字段.contains(&f.as_str()));
        }
        sorted.insert(k.as_str(), j);
    }
    jpp_effects::profile::hash16(&Json::Array(sorted.into_values().collect()))
}

fn cert_view(c: &Cert, alpha_eff: f64) -> jpp_effects::views::CertView {
    // 依据：B89 解读 (b)（地基/附注/2026-09-25-批量裁定.md §五）：试用上限取导入时记下的试用 α，旧证书按缺省
    let 试用上限 = c
        .eff
        .as_ref()
        .and_then(|e| e.trial_alpha)
        .unwrap_or(jpp_value::stat::ALPHA_TRIAL_DEFAULT);
    let 临时 = alpha_eff > 试用上限;
    jpp_effects::views::CertView {
        alpha: c.alpha,
        hi: c.hi,
        cost: c.cost,
        label_source_suspicious: c.label_source.可疑(),
        label_source: format!("{:?}", c.label_source),
        // B89：有效 α 超过证书自己的 α（模型真值、复核没有全覆盖）→ 按试用线，不放行不可逆 do；
        // 解读 (b)（步 20c）：超过试用 α 为临时上岗，不是试用
        trial: !临时 && (c.grade == CertGrade::Trial || alpha_eff > c.alpha),
        provisional: 临时,
        n_accepted: c.n_accepted,
        delta: c.selection.as_ref().and_then(|s| s.delta),
        delta_shifted: c.selection.is_some(),
        alpha_eff,
        label_fp: c.label_fp.clone(),
    }
}

impl CalibStore {
    /// **证书的有效 α**（B89）。证书写了 `eff` 用它；真值账没有 `model:` 来源时等于 `alpha`；
    /// 旧的模型真值记录（证书无 `eff`）按 `α + (1 − a_lb) / c` 补算：a_lb 取抽检账的下界（旧账没有时按
    /// 一致数补算），c 为带标注样本落在这条线已决区（p ≥ hi + δ 或 p ≤ lo − δ，K 元只有上侧）的占比。
    /// 没有复核账时为 1（不放行）。
    pub fn alpha_eff_of(&self, rec: &CalibRecord, c: &Cert) -> f64 {
        if let Some(e) = &c.eff {
            return e.alpha_eff;
        }
        let Some(t) = &rec.truth else { return c.alpha };
        if !t.sources.keys().any(|k| k.starts_with("model:")) {
            return c.alpha;
        }
        let Some(sc) = &t.spot_check else {
            return jpp_value::stat::ALPHA_EFF_NONE;
        };
        let a_lb = sc.lower.unwrap_or_else(|| {
            jpp_value::stat::agreement_lower(
                sc.agree,
                sc.n,
                sc.conf.unwrap_or(jpp_value::stat::REVIEW_CONF_DEFAULT),
            )
        });
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        let op = 带标注
            .first()
            .and_then(|s| calib::反查题型_pub(&s.phys))
            .unwrap_or(jpp_value::value::Op::Test);
        let _ = op;
        // 与 cut 同一个 δ 与同一边界比较（步 15d-2）；取不到 δ 时已决为空（不放行）
        let 已决 = match CalibStore::cert_delta(rec, c) {
            Some(delta) => 带标注
                .iter()
                .filter(|s| {
                    let p = s.p.expect("已滤");
                    jpp_value::stat::decided_up(p, rec.hi, delta)
                        || (rec.lower.is_some() && jpp_value::stat::decided_down(p, rec.lo, delta))
                })
                .count(),
            None => 0,
        };
        let cov = if 带标注.is_empty() {
            None
        } else {
            Some(已决 as f64 / 带标注.len() as f64)
        };
        jpp_value::stat::alpha_eff(c.alpha, a_lb, Some(cov.unwrap_or_default()))
    }

    /// 记录 → 只读视图（运行时只见这个形状）。
    pub fn lookup_of(&self, r: &CalibRecord) -> jpp_effects::views::Lookup {
        let cv = |c: &Cert| cert_view(c, self.alpha_eff_of(r, c));
        jpp_effects::views::Lookup {
            key: r.key.clone(),
            hi: r.hi,
            lo: r.lo,
            n: r.n,
            status: r.status.clone(),
            delta: r.delta,
            fixture: r.fixture,
            set_id: r.set_id.clone(),
            truth_gate: r.truth.as_ref().map(|t| t.gate.clone()),
            certs: r.certs.values().map(cv).collect(),
            selected: r.选中的证书().map(cv),
            lower: r.lower.as_ref().map(cv),
            rerun_independent: r.rerun_independent,
            scope: r.scope.as_ref().and_then(|s| s.fingerprint.clone()),
            scope_n_text: r.scope.as_ref().and_then(|s| s.n_text),
            // B91：扩展 α 大于记录选中证书的 α 即试用级
            scope_extensions: r
                .scope
                .as_ref()
                .map(|s| {
                    let a = r.选中的证书().map(|c| c.alpha).unwrap_or(f64::INFINITY);
                    s.extensions
                        .iter()
                        .map(|e| (e.fingerprint.clone(), e.alpha > a))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// 校准库的只读视图（`20` §2.3）。步 11 起由 `CalibStore` 实现；步 11b 起运行时与 `strength` 只经它读校准。
impl jpp_effects::views::CalibView for CalibStore {
    fn lookup(&self, key: &str) -> Option<jpp_effects::views::Lookup> {
        self.records.get(key).map(|r| self.lookup_of(r))
    }
    fn line(&self, key: &str) -> jpp_effects::views::Lookup {
        self.lookup_of(&self.get(key))
    }
    fn hash(&self) -> String {
        calib_hash(self)
    }
    fn unsure_rate(&self, key: &str) -> Option<f64> {
        self.records
            .get(key)
            .and_then(|r| self.usable_unsure_rate(r))
    }
    fn profile(&self) -> &jpp_effects::Profile {
        &self.profile
    }
    fn drift(&self, key: &str) -> Option<jpp_value::stat::DriftReport> {
        self.drift_of(key)
    }
    fn record_json(&self, key: &str) -> Option<serde_json::Value> {
        self.records
            .get(key)
            .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
    }
    fn chain(&self, key: &str, form_hash: Option<&str>) -> jpp_effects::views::Chain {
        let link = |k: String| jpp_effects::views::Link {
            rec: self.lookup_of(&self.get(&k)),
            key: k,
        };
        jpp_effects::views::Chain {
            question: link(key.to_string()),
            form: form_hash.map(|h| link(CalibStore::form_key(h))),
            // 类别标签是作者写的键；`fit:` 键与保留命名空间里的键不是类别标签（B34）
            class: (!key.starts_with("fit:") && !key.starts_with('\u{1f}'))
                .then(|| link(CalibStore::class_key(key))),
        }
    }
    fn labelled(&self, key: &str) -> Vec<(f64, bool)> {
        // 样本的 `label` 已是「对错位」：test 是标签本身，K 元划分是 argmax 等于真值（`truth.rs::correct`，B63）
        self.records
            .get(key)
            .map(|r| {
                r.samples
                    .iter()
                    .filter_map(|s| Some((s.p?, s.label? == 1)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use jpp_effects::views::CalibView;

    /// 视图与记录一致：有键时逐字段相同，无键时 `None`（调用方按冷处理）；哈希即 `calib_hash`。
    #[test]
    fn calib_view_matches_the_record() {
        let mut s = CalibStore::new();
        assert!(s.lookup("k").is_none());
        let before = calib_hash(&s);
        let mut r = s.get("k");
        r.hi = 0.7;
        r.lo = 0.3;
        r.n = 40;
        r.status = "上岗".into();
        s.records.insert("k".into(), r);
        let l = s.lookup("k").unwrap();
        assert_eq!((l.hi, l.lo, l.n, l.status.as_str()), (0.7, 0.3, 40, "上岗"));
        assert_eq!(CalibView::hash(&s), calib_hash(&s));
        assert_ne!(CalibView::hash(&s), before);
    }

    /// B34：查找链的类级只给作者写的类别标签；`fit:` 键与保留命名空间里的键没有类级（步 20b）。
    #[test]
    fn chain_has_class_link_only_for_author_labels() {
        let s = CalibStore::new();
        let c = s.chain("k", None);
        assert_eq!(c.class.map(|l| l.key), Some(CalibStore::class_key("k")));
        assert!(s.chain("fit:f", None).class.is_none());
        assert!(s.chain(&CalibStore::form_key("h"), None).class.is_none());
    }
}

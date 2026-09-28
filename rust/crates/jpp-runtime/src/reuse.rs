//! 按缓存键复用（步 19；B40、B151 两段式；B20 与 jev-ca 提醒 1）。
//!
//! 判断、生成、变换各有一把不含调用位置的缓存键（`jpp_ir::key::CacheKey`、`EffectKey::cache_digest`）。
//! 登记时账本按自己的账本键没有命中，就先查本运行已有的同缓存键结果，再查宿主交进来的跨运行缓存
//! （`CacheLookup`）。命中即当场给出结果，并在本账本写一条复用条目：`reused_from` 记来源（本运行第一次那条的
//! 账本键，或跨运行的 `ext:<来源>#<序号>`），`cost` 0、`call` 0、`layer` 0。复用条目有自己的账本键，
//! 所以只凭本账本审计重放照样命中（账本自足），审计重放不把它计入调用与费用。
//!
//! 不复用：`do`（要等动作能声明「纯」，主会话 2026-09-26）、`ask`；同一次刷新里还没回答的同键判断之间、
//! 还在飞的惰性生成之间（过程记录 `工程-步19.md`「不做」第 3 条）。
//! 依据：B40（`20` v2 附录）；B151（`21` 步 19 追加项、§四·14）

use super::*;
use jpp_effects::ReuseRule;
use jpp_effects::spec::EffectSpec;
use jpp_effects::views::{CacheLookup, CachedReading};
use jpp_ir::key::CacheKey;
use jpp_ledger::PermMeasure;
use serde::Serialize;
use std::collections::BTreeMap;

/// 复用条目的费用：复用不发请求，费用恒为零（B40「复用条目 `cost: 0`」）
pub(crate) const 零费用: f64 = 0.0;

/// 报告 `cache` 一节（步 19）：给了跨运行缓存、或本趟有命中时才出（B151 的偏离，主会话 2026-09-26 交 Fable 追认）。
/// 按效应分列的两张表以效应名为键，从注册表建（`20` A2：注册表外不按效应名分支）。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CacheStats {
    /// 命中数，按效应（注册表里 `reuse` 不是 `Never` 的：`gen`、`judge`、`transform`）
    pub hits: BTreeMap<String, u64>,
    /// 其中本运行内的命中
    pub same_run: u64,
    /// 其中跨运行缓存的命中
    pub cross_run: u64,
    /// 省下的请求数（判断与生成每次命中省一次请求；判断合批时实际省下的调用可能更少；变换不是请求）
    pub saved_calls: u64,
    /// 本趟实际发出的请求，按效应（注册表里能写进效应行的：`ask`、`do`、`gen`、`judge`）
    pub requests: BTreeMap<String, u64>,
}

impl Default for CacheStats {
    fn default() -> CacheStats {
        let specs = || jpp_effects::ALL.into_iter().map(jpp_effects::spec);
        CacheStats {
            hits: specs()
                .filter(|s| s.reuse != ReuseRule::Never)
                .map(|s| (s.name.to_string(), 0))
                .collect(),
            same_run: 0,
            cross_run: 0,
            saved_calls: 0,
            requests: specs()
                .filter(|s| s.in_effect_row)
                .map(|s| (s.name.to_string(), 0))
                .collect(),
        }
    }
}

/// 复用状态：本运行的缓存键表、跨运行缓存、生成器身份、计数。
#[derive(Default)]
pub(crate) struct ReuseState<'a> {
    /// 判断：缓存键摘要 → （第一次那条的账本键、答案、置换测量、自报置信度）
    judges: HashMap<String, (String, Answer, Option<PermMeasure>, Option<f64>)>,
    /// 生成与变换：缓存键摘要 → 第一次那条的账本键
    effects: HashMap<String, String>,
    pub(crate) cross: Option<&'a dyn CacheLookup>,
    /// 生成器模型与画像哈希（宿主给了真实生成器时才有；头定稿时写入）
    pub(crate) gen_model: Option<String>,
    pub(crate) gen_profile_hash: Option<String>,
    pub(crate) stats: CacheStats,
}

/// 复用来的一条效应结果。
pub(crate) struct 复用效应 {
    pub output: Json,
    pub output_mat: Option<Box<MatMeta>>,
    pub reused_from: String,
}

impl<'a> Interp<'a> {
    /// 跨运行缓存（步 19，B151）：宿主从缓存目录里的账本建索引后交进来。只凭账本的审计重放不用它。
    pub fn with_cache(mut self, cache: &'a dyn CacheLookup) -> Self {
        self.复用.cross = Some(cache);
        self
    }

    /// 生成器身份（步 19）：模型与画像哈希进账本头（`gen_model`、`gen_profile_hash`），生成物的缓存键带模型。
    pub fn with_gen(mut self, model: Option<String>, profile_hash: Option<String>) -> Self {
        self.复用.gen_model = model;
        self.复用.gen_profile_hash = profile_hash;
        self
    }

    /// 生成物缓存键里的生成器模型：宿主给了取它，否则取判断模型（与账本头 `gen_model_or_default` 同口径）。
    pub(crate) fn 生成模型(&self) -> String {
        self.复用
            .gen_model
            .clone()
            .unwrap_or_else(|| self.model_id.clone())
    }

    /// 本趟的复用计数（`Outcome.cache`）：给了跨运行缓存或有命中时才有。
    pub(crate) fn 复用统计(&self) -> Option<CacheStats> {
        let s = &self.复用.stats;
        (self.复用.cross.is_some() || s.same_run + s.cross_run > 0).then(|| s.clone())
    }

    /// 本趟实际发出的一次请求（报告 `cache` 一节的 `requests`），按效应名计。
    pub(crate) fn 记请求(&mut self, spec: &EffectSpec) {
        *self
            .复用
            .stats
            .requests
            .entry(spec.name.to_string())
            .or_default() += 1;
    }

    /// 产出读数的效应（判断）的注册项：刷新发出判断时按它计请求、按它记命中
    pub(crate) fn 读数效应() -> &'static EffectSpec {
        jpp_effects::spec(
            jpp_effects::find(|s| s.produces_reading).expect("注册表里有产出读数的效应"),
        )
    }

    /// 按注册表的复用规则取效应缓存键：`Generator` 带生成器模型，`Method` 不带，其余不复用。
    fn 效应缓存键(&self, key: &str, model: &str) -> Option<(String, &'static EffectSpec)> {
        let ek = self.effect_keys.get(key)?;
        let spec = jpp_effects::by_name(&ek.kind)?;
        let d = match spec.reuse {
            ReuseRule::Generator => ek.cache_digest(Some(model))?,
            ReuseRule::Method => ek.cache_digest(None)?,
            ReuseRule::Reading | ReuseRule::Never => return None,
        };
        Some((d, spec))
    }

    fn 判断缓存键(&self, key: &str) -> Option<(String, CacheKey)> {
        let ck = self.judge_keys.get(key)?.cache_key();
        Some((ck.digest(), ck))
    }

    /// 一条判断有了答案（刷新落账、账本命中、复用命中）：按缓存键记进本运行表，先到先得。
    pub(crate) fn 记可复用判断(
        &mut self,
        key: &str,
        a: &Answer,
        perm: Option<PermMeasure>,
        confidence: Option<f64>,
    ) {
        if let Some((d, _)) = self.判断缓存键(key) {
            self.复用
                .judges
                .entry(d)
                .or_insert_with(|| (key.to_string(), a.clone(), perm, confidence));
        }
    }

    /// 这把账本键按缓存键能不能复用（只查，不填答案、不记账）。推测发出前用它：能复用的判断不推测，
    /// 等真站点走到时由 `判断复用` 当场取回——否则推测先把请求发出去，缓存命中省不下调用与费用（说话 v2 实测，2026-09-27）。
    pub(crate) fn 判断可复用(&self, key: &str) -> bool {
        if self.audit.on {
            return false;
        }
        let Some((d, ck)) = self.判断缓存键(key) else {
            return false;
        };
        self.复用.judges.contains_key(&d)
            || self
                .复用
                .cross
                .and_then(|c| c.get(&ck))
                .is_some_and(|hit| 判断记录的答案(&hit).is_some())
    }

    /// 判断登记时账本按账本键没有命中：按缓存键查本运行表与跨运行缓存。命中即填答案、写复用条目，返回 `true`。
    pub(crate) fn 判断复用(
        &mut self,
        key: &str,
        r: &Rc<Reading>,
        sp: Span,
        label: String,
    ) -> bool {
        if self.audit.on {
            return false;
        }
        let Some((d, ck)) = self.判断缓存键(key) else {
            return false;
        };
        let (answer, perm, confidence, from, 同运行) =
            if let Some((k0, a, p, c)) = self.复用.judges.get(&d) {
                (a.clone(), *p, *c, k0.clone(), true)
            } else if let Some(hit) = self.复用.cross.and_then(|c| c.get(&ck))
                && let Some((a, p, c)) = 判断记录的答案(&hit)
            {
                (a, p, c, format!("ext:{}", hit.source), false)
            } else {
                return false;
            };
        self.fill_from_record(r, answer.clone(), perm, confidence);
        let jkey = self.judge_keys.get(key).cloned();
        self.记账(
            r.id,
            Entry::Judge {
                key: key.to_string(),
                jkey,
                answer: answer.clone(),
                tokens: 0,
                cost: 零费用,
                model_id: self.model_id.clone(),
                call: 0,
                calib_ref: Some(Box::new(CalibRef::declared(&r.calib))),
                layer: 0,
                merged_by: None,
                parents: vec![],
                hop: 0,
                reused_from: Some(from),
                perm,
                confidence,
            },
        );
        self.记可复用判断(key, &answer, perm, confidence);
        let name = Self::读数效应().name;
        let s = &mut self.复用.stats;
        *s.hits.entry(name.to_string()).or_default() += 1;
        s.saved_calls += 1;
        if 同运行 {
            s.same_run += 1;
        } else {
            s.cross_run += 1;
        }
        self.trace.push("judge", key, true, 零费用, sp, label);
        true
    }

    /// 生成或变换的一条结果进了账本：按缓存键记进本运行表，先到先得。
    pub(crate) fn 记可复用效应(&mut self, key: &str, model: &str) {
        let Some((d, _)) = self.效应缓存键(key, model) else {
            return;
        };
        self.复用
            .effects
            .entry(d)
            .or_insert_with(|| key.to_string());
    }

    /// 生成或变换登记时账本按账本键没有命中：按缓存键查本运行已有的结果与跨运行缓存。命中返回要照抄的输出。
    /// 失败值与复用条目不作来源（本运行表只记成功取回的，索引建时已剔除）。
    pub(crate) fn 效应复用(&mut self, key: &str, model: &str) -> Option<复用效应> {
        if self.audit.on {
            return None;
        }
        let (d, spec) = self.效应缓存键(key, model)?;
        let (output, output_mat, from, 同运行) =
            if let Some(k0) = self.复用.effects.get(&d).cloned() {
                match self.账本查(&k0) {
                    Some(Entry::Effect {
                        output, output_mat, ..
                    }) => (output.clone(), output_mat.clone(), k0, true),
                    _ => return None,
                }
            } else {
                let hit = self.复用.cross?.get_effect(&d)?;
                let (o, m) = 效应记录的输出(&hit)?;
                (o, m, format!("ext:{}", hit.source), false)
            };
        let s = &mut self.复用.stats;
        *s.hits.entry(spec.name.to_string()).or_default() += 1;
        // 生成是请求，省下一次；变换是本地方法调用，不是请求
        s.saved_calls += u64::from(spec.reuse == ReuseRule::Generator);
        if 同运行 {
            s.same_run += 1;
        } else {
            s.cross_run += 1;
        }
        Some(复用效应 {
            output,
            output_mat,
            reused_from: from,
        })
    }
}

/// 跨运行缓存给回的判断条目里的答案、置换测量与自报置信度（B154）。
fn 判断记录的答案(
    hit: &CachedReading,
) -> Option<(Answer, Option<PermMeasure>, Option<f64>)> {
    let e: Entry = serde_json::from_value(hit.record.clone()).ok()?;
    match e {
        Entry::Judge {
            answer,
            perm,
            confidence,
            ..
        } => Some((answer, perm, confidence)),
        _ => None,
    }
}

/// 跨运行缓存给回的效应条目里的输出与材料元数据。
fn 效应记录的输出(hit: &CachedReading) -> Option<(Json, Option<Box<MatMeta>>)> {
    let e: Entry = serde_json::from_value(hit.record.clone()).ok()?;
    match e {
        Entry::Effect {
            output, output_mat, ..
        } => Some((output, output_mat)),
        _ => None,
    }
}

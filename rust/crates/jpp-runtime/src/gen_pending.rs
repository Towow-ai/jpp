//! 惰性生成值（B149、B160；`21` 步 15h 第 (4) 条由本机制替代，过程记录 `地基/过程记录/工程-步15h-2.md`）。
//!
//! B160 的四点：
//! 1. `gen` 在调用点只登记（不交端口），返回 [`Value::Gen`]；下一个刷新点把登记以来的全部生成连同待发判断
//!    作为一层一次交出（生成先 `submit`，非阻塞），程序继续。
//! 2. 读值是检视点（与 23c 的惰性出口相同）：读未交出的生成先刷新把它交出，再等它那一层收齐。
//! 3. 账本按层、按登记序一次写：交出生成的那次刷新开一个「开着的层」，这一层的判断与缺席条目先记在层里
//!    （键 = 读数号 `2r+1`），生成条目收齐时进层（键 = 登记时读数号计数 `2m`），层内全部票据收齐时按键一次
//!    写进账本。下一层开始前、层内任一生成被读取时、程序结束时收层；任何时刻至多一个开着的层。
//! 4. 层开着时，同一趟运行里按键查判断条目的几处（[`Interp::账本查`]）先查层、再查账本。
//!
//! 推测与提升期间不取回生成（[`Interp::推测中不等生成`]）。
//!
//! `--gen-cache`（B151 的过渡）：[`GenCache`] 按 `gen` 的账本键（带站点）与生成器模型名查；命中不登记、不交出、
//! 照写账本条目（费用 0）。步 19 换成不带站点的键与 `CacheLookup` 端口后退役。

use super::*;
use jpp_effects::port::{GenResult, Ticket};
use jpp_effects::spec::EffectSpec;
use jpp_ledger::Entry;
use std::cell::RefCell;
use std::collections::BTreeMap;

/// 推测与提升期间遇到未取回的生成时的内部报文：调用方据此放弃推测站点、停止提升，不外露。
pub(crate) const 不等生成报文: &str = "（推测与提升不等生成）";

/// 推测与提升期间遇到未取回的生成：以内部报文结束求值（依据：B149、B160，写账本时刻不取决于完成先后）。
pub(crate) fn 不等生成(site: Span) -> Fault {
    Fault::Error(RtError::new(Some("E-rt-client"), 不等生成报文, site))
}

/// 求值结果是不是「推测与提升不等生成」（调用方据此放弃，不外露）。
pub(crate) fn 是不等生成<T>(r: &R<T>) -> bool {
    matches!(r, Err(Fault::Error(e)) if e.message == 不等生成报文)
}

/// 生成缓存里的一条（B151 过渡，步 15h-2）：生成器模型名、输出、声明的 taint、提示（给人看）。
#[derive(Clone, Debug, PartialEq)]
pub struct GenCacheEntry {
    pub model: String,
    pub output: Json,
    pub taint: Option<Taint>,
    pub prompt: String,
}

/// 生成缓存：宿主装入（读文件），运行时查与记；新条目留在 `fresh` 里由宿主写回。
#[derive(Clone, Debug, Default)]
pub struct GenCache {
    /// 账本键 → 条目
    pub entries: BTreeMap<String, GenCacheEntry>,
    /// 本趟新生成、要写回的条目（按取回顺序）
    pub fresh: Vec<(String, GenCacheEntry)>,
    /// 本趟命中次数
    pub hits: u64,
}

/// 登记了的一次生成（交出前后都在这里，收齐后移走）。
pub(crate) struct GenJob {
    key: String,
    spec: &'static EffectSpec,
    prompt: String,
    /// 交出前的调用输入；交出后为空
    input: Option<CallInput>,
    /// 交出后的票据
    ticket: Option<Ticket>,
    /// ∨ ctx 的 taint（端口不声明时用）
    taint: Taint,
    derived: BTreeSet<String>,
    from: Sources,
    site: Span,
    handle: Rc<PendingGen>,
}

/// 开着的层：交出了生成、票据还没收齐。条目带登记序键，收齐时排序写进账本。
struct 开层 {
    gens: Vec<u64>,
    entries: Vec<((u64, u64), Entry)>,
}

/// 解释器里与生成有关的状态。
#[derive(Default)]
pub(crate) struct GenState {
    jobs: BTreeMap<u64, GenJob>,
    next: u64,
    /// 登记了、还没交出的生成数（预算核要看见它们）
    预留: u64,
    /// 推测与提升进行中（大于 0 时不取回生成）
    不等: u32,
    层: Option<开层>,
    cache: Option<Rc<RefCell<GenCache>>>,
}

/// 生成输出包成材料（`gen` 的产物，taint 与来源按调用点算好的给）。
pub(crate) fn 包材料(
    outs: &[Json],
    prompt: &str,
    key: &str,
    taint: Taint,
    derived: &BTreeSet<String>,
    from: &Sources,
) -> Value {
    Value::list(
        outs.iter()
            .map(|o| {
                Value::Mat(Rc::new(
                    Mat::new(
                        o.clone(),
                        &format!("gen:{prompt}"),
                        vec![format!("gen:{key}")],
                        taint,
                        derived.clone(),
                    )
                    .with_sources(from),
                ))
            })
            .collect(),
    )
}

impl<'a> Interp<'a> {
    /// 装入生成缓存（`--gen-cache`，步 15h-2）。
    pub fn with_gen_cache(mut self, cache: Rc<RefCell<GenCache>>) -> Self {
        self.生成.cache = Some(cache);
        self
    }

    /// 推测与提升期间调用：期间遇到未取回的生成，求值以内部报文结束，不取回（写账本时刻不取决于完成先后）。
    pub(crate) fn 推测中不等生成<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.生成.不等 += 1;
        let r = f(self);
        self.生成.不等 -= 1;
        r
    }

    /// 推测与提升进行中（此时不取回生成）。
    pub(crate) fn 生成等待中(&self) -> bool {
        self.生成.不等 > 0
    }

    /// 登记了还没交出的生成数（`budget.rs::charge` 把它算进已用调用）。
    pub(crate) fn 生成预留(&self) -> u64 {
        self.生成.预留
    }

    /// 查生成缓存：同账本键、同生成器模型才命中。
    pub(crate) fn 查生成缓存(&mut self, key: &str, model: &str) -> Option<GenCacheEntry> {
        let c = self.生成.cache.as_ref()?;
        let hit = c
            .borrow()
            .entries
            .get(key)
            .filter(|e| e.model == model)
            .cloned();
        if hit.is_some() {
            c.borrow_mut().hits += 1;
        }
        hit
    }

    /// 登记一次生成（B160：不交端口，等所在层的刷新点），返回未取回的生成值。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn 登记生成(
        &mut self,
        spec: &'static EffectSpec,
        key: String,
        prompt: &str,
        input: CallInput,
        taint: Taint,
        derived: BTreeSet<String>,
        from: Sources,
        sp: Span,
    ) -> R<Value> {
        let id = self.生成.next;
        self.生成.next += 1;
        self.生成.预留 += 1;
        let handle = Rc::new(PendingGen {
            id,
            mark: self.next_reading.get(),
            site: sp,
            resolved: RefCell::new(None),
        });
        self.生成.jobs.insert(
            id,
            GenJob {
                key,
                spec,
                prompt: prompt.to_string(),
                input: Some(input),
                ticket: None,
                taint,
                derived,
                from,
                site: sp,
                handle: handle.clone(),
            },
        );
        Ok(Value::Gen(handle))
    }

    /// 有没有登记了还没交出的生成（刷新点据此决定这一层要不要带生成）。
    pub(crate) fn 有未交生成(&self) -> bool {
        self.生成.jobs.values().any(|j| j.ticket.is_none())
    }

    /// 刷新点（B160）：把登记以来的全部生成交出去（非阻塞），开一个层。调用方先收上一层。
    pub(crate) fn 交出生成(&mut self) -> R<()> {
        let ids: Vec<u64> = self
            .生成
            .jobs
            .values()
            .filter(|j| j.ticket.is_none())
            .map(|j| j.handle.id)
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        for id in &ids {
            let job = self.生成.jobs.get_mut(id).expect("刚取的号");
            let (spec, site) = (job.spec, job.site);
            let input = job.input.take().expect("未交出的生成带输入");
            let port_err = |m: String| {
                // 依据：B149（端口报错是运行期错误；生成器报的失败才走 Fail）
                Fault::Error(RtError::new(
                    Some("E-rt-client"),
                    format!("gen 失败：{m}"),
                    site,
                ))
            };
            let Some(port) = self.ports.port(spec.id) else {
                return Err(port_err(format!(
                    "没有端口服务效应 {}：宿主没有为它注册端口",
                    spec.name
                )));
            };
            let call = jpp_effects::port::EffectCall {
                instance: port.instance(),
                input,
            };
            let ticket = port
                .submit(vec![call])
                .map_err(|e| port_err(e.0))?
                .into_iter()
                .next()
                .ok_or_else(|| port_err("端口没有给出票据".into()))?;
            if let Some(job) = self.生成.jobs.get_mut(id) {
                job.ticket = Some(ticket);
            }
            // 调用在交出时计（B38），不再算作预留
            self.生成.预留 -= 1;
            self.cost.calls += 1;
        }
        self.生成.层 = Some(开层 {
            gens: ids,
            entries: vec![],
        });
        Ok(())
    }

    /// 取回一个生成值（检视点）：未交出的先刷新交出；再等它那一层收齐。推测与提升期间不取回。
    pub(crate) fn 解析生成(&mut self, g: &Rc<PendingGen>) -> R<Value> {
        if let Some(v) = g.value() {
            return Ok(v);
        }
        if self.生成.不等 > 0 {
            return Err(不等生成(g.site));
        }
        if self
            .生成
            .jobs
            .get(&g.id)
            .is_some_and(|j| j.ticket.is_none())
        {
            // 读值本身是刷新点（B94、B160）：连同待发判断作为一层交出去
            self.flush("inspect")?;
        }
        self.收层()?;
        g.value()
            .ok_or_else(|| Fault::Error(RtError::new(Some("E-rt-client"), "生成没有取回", g.site)))
    }

    /// 收层（B160）：等开着的层里全部生成完成，层内条目按登记序一次写进账本。没有开着的层时什么都不做。
    /// 取回途中端口报错：已收的条目照写（事实要记，`13` §5），再报错。
    pub(crate) fn 收层(&mut self) -> R<()> {
        let Some(mut layer) = self.生成.层.take() else {
            return Ok(());
        };
        let mut failure = None;
        for id in &layer.gens {
            let Some(job) = self.生成.jobs.remove(id) else {
                continue;
            };
            match self.等票据(&job) {
                Ok(res) => {
                    let (v, entry) = self.生成完成(&job, res);
                    *job.handle.resolved.borrow_mut() = Some(v);
                    layer.entries.push(((2 * job.handle.mark, 0), entry));
                }
                Err(e) => {
                    failure.get_or_insert(e);
                }
            }
        }
        // 登记序：判断键 2r+1、生成键 2m；同键保持进层先后（稳定排序）
        layer.entries.sort_by_key(|(k, _)| *k);
        for (_, e) in layer.entries {
            self.账本追加(e);
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// 刷新里产生的判断与缺席条目：层开着时先记进层（带读数号），否则直接写账本。
    pub(crate) fn 记账(&mut self, reading_id: u64, e: Entry) {
        match &mut self.生成.层 {
            Some(l) => l.entries.push(((2 * reading_id + 1, 0), e)),
            None => self.账本追加(e),
        }
    }

    /// 按键查条目：先查开着的层，再查账本（层开着时，刷新写出的判断条目还在层里）。
    pub(crate) fn 账本查(&self, key: &str) -> Option<&Entry> {
        if let Some(l) = &self.生成.层
            && let Some((_, e)) = l.entries.iter().rev().find(|(_, e)| e.key() == key)
        {
            return Some(e);
        }
        self.ledger.view().get(key)
    }

    /// 等一个已交出的生成完成。
    fn 等票据(&mut self, job: &GenJob) -> R<GenResult> {
        // 依据：B160（只等已交出的；未交出的由刷新点交出）
        let Some(ticket) = job.ticket else {
            return Err(Fault::Error(RtError::new(
                Some("E-rt-client"),
                "生成没有交出",
                job.site,
            )));
        };
        let polled = loop {
            let Some(port) = self.ports.port(job.spec.id) else {
                break Err(EffectError("生成端口不见了".into()));
            };
            if let std::task::Poll::Ready(r) = port.poll(&ticket) {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        };
        // 依据：B149（端口报错是运行期错误 E-rt-client；生成器报的失败走 Fail）
        match polled {
            Ok(EffectOut::Mats(r)) => Ok(r),
            Ok(_) => Err(Fault::Error(RtError::new(
                Some("E-rt-client"),
                "gen 失败：生成端口返回的不是材料",
                job.site,
            ))),
            // 依据：B149（端口报错）
            Err(e) => Err(Fault::Error(RtError::new(
                Some("E-rt-client"),
                format!("gen 失败：{}", e.0),
                job.site,
            ))),
        }
    }

    /// 生成结果落账（`13` §5：后端已经返回 = 钱已经花了，先记事实）：费用、trace、缓存新条目；返回值与账本条目。
    fn 生成完成(&mut self, job: &GenJob, res: GenResult) -> (Value, Entry) {
        // 调用数已在交出时计；这里记 token 与费用
        self.cost.tokens += res.tokens;
        self.cost.usd += res.cost;
        let taint = res.taint_out.unwrap_or(job.taint);
        // 生成器报的失败（超时、非 JSON、空……）产出 `Fail` 值、照记账本，程序照常往下（步 15h-1，形状同 B93）
        let (value, output, output_mat) = match &res.failure {
            Some(f) => {
                let v = Value::Fail(
                    Rc::from(format!("gen {f}").as_str()),
                    Provenance::new(taint, job.from.clone()),
                );
                let (o, _) = effect_value_to_entry(&v);
                (v, o, None)
            }
            None => (
                包材料(
                    &res.outputs,
                    &job.prompt,
                    &job.key,
                    taint,
                    &job.derived,
                    &job.from,
                ),
                Json::Array(res.outputs.clone()),
                res.taint_out.map(|t| {
                    Box::new(MatMeta {
                        addr: format!("gen:{}", job.prompt),
                        origin: vec![format!("gen:{}", job.key)],
                        taint: t,
                        sources: vec![],
                    })
                }),
            ),
        };
        if res.failure.is_none()
            && let Some(c) = &self.生成.cache
        {
            let model = self
                .ports
                .instance_of(job.spec.id)
                .map(|i| i.model)
                .unwrap_or_default();
            c.borrow_mut().fresh.push((
                job.key.clone(),
                GenCacheEntry {
                    model,
                    output: output.clone(),
                    taint: res.taint_out,
                    prompt: job.prompt.clone(),
                },
            ));
        }
        self.trace.push(
            job.spec.name,
            &job.key,
            false,
            res.cost,
            job.site,
            job.prompt.clone(),
        );
        let entry = Entry::Effect {
            key: job.key.clone(),
            ekey: self.effect_keys.get(&job.key).cloned(),
            output_mat,
            kind: job.spec.name.into(),
            output,
            cost: res.cost,
        };
        (value, entry)
    }
}

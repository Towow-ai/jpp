//! `CalibStore` 本体、`Refusal`、合法状态与写入口 `put` / `absorb`。（拆 calib.rs：原第 579–800 行）

use jpp_effects::profile::Profile;
use std::collections::HashMap;

use super::*;
use jpp_value::stat::Certificate;

/// `commission` 失败的两种，**不能混成一种**。
///
/// 「认证跑完了、结论是拒绝」带着 `n_needed`——**它告诉作者这条路有终点**。
/// 「根本没跑成」（α 越界、没有标注样本、声明了簇却没有簇 id）**没有终点可报**，
/// 报一个假的 `n_needed` 比不报更糟：作者会照着那个数去凑样本，而问题根本不在样本数上。
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// 认证跑了，这批数据撑不起这个 α
    认证不过(Certificate),
    /// 认证没跑：入参或记录状态不对
    跑不成(String),
}

impl Refusal {
    /// 还差多少条零错放行才认得动。**「跑不成」没有这个数**——不编一个出来。
    pub fn n_needed(&self) -> Option<usize> {
        match self {
            Refusal::认证不过(Certificate::Refused { n_needed, .. }) => Some(*n_needed),
            _ => None,
        }
    }
}

/// 合法状态：Python `calib.py:13` 的四个，加 B25 的「停岗候选」（自动标记、待人确认）
pub const STATUSES: [&str; 5] = ["冷", "上岗", "停岗", "待真值", "停岗候选"];

#[derive(Clone, Debug)]
pub struct CalibStore {
    pub records: HashMap<String, CalibRecord>,
    pub profile: Profile,
    /// 装载时重跑认证的报告（步 20c）：降级与改写各一行，复现的记录不列。不进 `calib_hash`。
    pub load_report: Vec<String>,
}

impl Default for CalibStore {
    /// 空库；画像全部未测（步 15d-2：`Profile` 没有 `Default`，唯一默认是 `Profile::untested()`）
    fn default() -> CalibStore {
        CalibStore {
            records: HashMap::new(),
            profile: Profile::untested(),
            load_report: vec![],
        }
    }
}

impl CalibStore {
    pub fn new() -> CalibStore {
        CalibStore::default()
    }
    /// **只写夹具记录**（B29，2026-09-23 改写）：J-03 同时约束程序与宿主。
    /// 校准记录只能由认证程序（`commission*`、真值通道）从带真值样本产出；
    /// 宿主经这里写的一律是 `fixture: true` 的测试记录，凭它得到的出口带 `W-fixture-line`，
    /// 不算放行不可逆 `do` 的可信合取项。原注释「宿主能在这里写线，这是设计意图」
    /// 把 J-03 解释为只拦程序，已由 `12` J-03 B29 条取代。
    ///
    /// `delta`：这条线的 δ（步 15d-2：δ 只从记录取，夹具线要显式给；`None` 的上岗线出口 `Unsure(untested)`，
    /// 载体 `Delta`，`20` §3.9）。
    pub fn put(
        &mut self,
        key: &str,
        hi: f64,
        lo: f64,
        n: u64,
        status: &str,
        delta: Option<f64>,
    ) -> Result<(), String> {
        if delta.is_some_and(|d| !(0.0..1.0).contains(&d)) {
            return Err("δ 必须在 [0, 1) 内".into());
        }
        if !STATUSES.contains(&status) {
            return Err(format!("status 只能是 {STATUSES:?}，收到 {status:?}"));
        }
        if status == "上岗" && n == 0 {
            return Err("上岗记录必须带 n > 0（线只从标注记录来）".into());
        }
        if !(0.0..=1.0).contains(&hi) || !(0.0..=1.0).contains(&lo) || lo > hi {
            return Err("线必须满足 0 ≤ lo ≤ hi ≤ 1".into());
        }
        // **证书门**：**带标注的**证据要让键上岗，必须过认证——手写一条线不算凭据。
        //
        // **取的量是 `labeled()`，不是 `samples.len()`。** 一条只有无标注观察的记录
        // **没有积累任何「关于线的证据」，它只是判过几次**；拿它挡 `put`，是把观察当成了证据。
        // 而运行期写入口推的每一条样本 `label` 都是 `None`（真值通道还不存在），
        // 所以按 `samples.len()` 拦会**把「跑程序 → 人写线 → 上岗」这条最常规的路彻底锁死**。
        if status == "上岗" {
            if let Some(old) = self.records.get(key) {
                // **收紧永远放行，放宽才要凭据。**
                //
                // 这一格是被自己的门夹出来的：原来拦住之后记录保持原状，**旧线继续放行，
                // 而人已经不能收紧它了**——那是失败开放。我第一版改成「拦住就停岗」，
                // 结果更糟：`commission` 明写着不经由它复岗，于是那个键**永久死掉**，
                // **正是这一包在修的那一类锁**。
                //
                // 真正的判据是方向——**但「方向」要按出口种类算，不按 `Act` 一种算**。
                //
                // 原来写的是 `hi >= old.hi && lo <= old.lo`，理由是「带更宽 = `Unsure` 更多
                // = 往拒绝那边倒」。**那句把「拒绝」默认等同于「不给 `Act`」**——
                // 而 `Act` 与 `Ignore` 是**对称的两个判定**：写 `if 不安全(x) { 拦下 }` 时，
                // **`Ignore` 才是放行的那个答案**。**语言不知道哪一侧对这个程序才是安全的那一侧。**
                //
                // **一个改动若使某个出口种类变得不可达，它就不是「收紧」，不论方向。**
                // 这个活口子最难看的地方是：`lo → 0` 免凭据，**而那正是 `commission`
                // 自己做的操作**——它有证书所以照办，手写的没有。
                let 新塌: Vec<&str> = {
                    let 旧 = old.不可达出口();
                    let 新 = CalibRecord {
                        hi,
                        lo,
                        ..old.clone()
                    }
                    .不可达出口();
                    新.into_iter().filter(|k| !旧.contains(k)).collect()
                };
                let 收紧 = old.status == "上岗" && hi >= old.hi && lo <= old.lo && 新塌.is_empty();
                if old.labeled() > 0 && !收紧 {
                    let 塌 = if 新塌.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "。**这次改动会让出口 {} 变得不可达——那不是收紧，不论方向**",
                            新塌.join(" / ")
                        )
                    };
                    return Err(format!(
                        "键 {key} 上有 {} 条**带标注**的证据，手写的线不是凭据：走 commission(key, alpha, conf_delta, cluster_unit) 让证书定线。\
                         （收紧现有线——`hi` 不降、`lo` 不升、**且不让任何一种出口变得不可达**——不需要凭据，随时可写）{塌}",
                        old.labeled()
                    ));
                }
            }
        }
        // **写线不抹证据**：积累起来的观察不是这次写线的人的东西。
        // 与「`Ledger::put` 只增不改」同一条纪律——抹掉了，`provenance` 就再也分不出混合。
        let samples = self
            .records
            .get(key)
            .map(|r| r.samples.clone())
            .unwrap_or_default();
        let certs = self
            .records
            .get(key)
            .map(|r| r.certs.clone())
            .unwrap_or_default();
        let lsid = self
            .records
            .get(key)
            .map(|r| r.label_set_id.clone())
            .unwrap_or_default();
        let lsrc = self
            .records
            .get(key)
            .map(|r| r.label_source.clone())
            .unwrap_or_default();
        let lloc = self
            .records
            .get(key)
            .map(|r| r.label_locator.clone())
            .unwrap_or_default();
        let lfp = self
            .records
            .get(key)
            .map(|r| r.label_fp.clone())
            .unwrap_or_default();
        let truth = self.records.get(key).and_then(|r| r.truth.clone());
        let lower = self.records.get(key).and_then(|r| r.lower.clone());
        let old_scope = self.records.get(key).and_then(|r| r.scope.clone());
        let old_rerun = self.records.get(key).and_then(|r| r.rerun_independent);
        let old_fps = self
            .records
            .get(key)
            .map(|r| r.material_fps.clone())
            .unwrap_or_default();
        let old_sources = self
            .records
            .get(key)
            .map(|r| r.sources.clone())
            .unwrap_or_default();
        self.records.insert(
            key.to_string(),
            CalibRecord {
                key: key.into(),
                hi,
                lo,
                n,
                status: status.into(),
                delta,
                unsure_rate: None,
                unsure_rate_delta: None,
                set_id: String::new(),
                label_set_id: lsid,
                label_locator: lloc,
                label_fp: lfp,
                label_source: lsrc,
                samples,
                certs,
                truth,
                lower,
                scope: old_scope,
                fixture: true,
                rerun_independent: old_rerun,
                sources: old_sources,
                material_fps: old_fps,
                kind: self.records.get(key).and_then(|r| r.kind),
                certs_history: self
                    .records
                    .get(key)
                    .map(|r| r.certs_history.clone())
                    .unwrap_or_default(),
            },
        );
        Ok(())
    }

    /// **运行期写入口**（`12`:347 两样「未定」里的第二样）：把程序跑出来的一条观察折进记录。
    ///
    /// **它不写线，也不能让记录上岗。** 无标注的观察只把冷记录推到 `待真值`——
    /// 四个状态里本来就为这件事留了那一格。**`n` 只随带标注的样本长**，因为
    /// `put` 自己的报错写着「线只从**标注**记录来」。
    pub fn absorb(&mut self, key: &str, s: Sample) -> Result<(), String> {
        if let Some(p) = s.p {
            if !(0.0..=1.0).contains(&p) {
                return Err(format!("样本 p 必须在 0..=1，收到 {p}"));
            }
        }
        if let Some(l) = s.label {
            if l > 1 {
                return Err(format!("label ∈ {{0, 1}}，收到 {l}"));
            }
        }
        let rec = self
            .records
            .entry(key.to_string())
            .or_insert_with(|| CalibRecord {
                key: key.into(),
                hi: 0.65,
                lo: 0.35,
                n: 0,
                status: "冷".into(),
                delta: None,
                unsure_rate: None,
                unsure_rate_delta: None,
                set_id: String::new(),
                label_set_id: String::new(),
                label_locator: String::new(),
                label_fp: String::new(),
                label_source: LabelSource::default(),
                samples: vec![],
                certs: Default::default(),
                truth: None,
                lower: None,
                scope: None,
                fixture: false,
                rerun_independent: None,
                sources: Default::default(),
                material_fps: vec![],
                kind: None,
                certs_history: vec![],
            });
        let 有标注 = s.label.is_some();
        rec.samples.push(s);
        if 有标注 {
            rec.n += 1;
        }
        // **停岗不因为来了新观察就复岗**：停岗是人下的判断，不是样本数的函数。
        if rec.status == "冷" {
            rec.status = "待真值".into();
        }
        Ok(())
    }
}

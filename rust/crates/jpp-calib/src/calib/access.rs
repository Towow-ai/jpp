//! 漂移、证书查询、持久化、键构造与读线。（拆 calib.rs：原第 1388–1696 行）

use jpp_value::value::Op;
use serde_json::Value as Json;

use super::*;

impl CalibStore {
    pub fn drift_of(&self, key: &str) -> Option<jpp_value::stat::DriftReport> {
        let rec = self.records.get(key)?;
        let (参照, 近期): (Vec<f64>, Vec<f64>) = (
            rec.samples
                .iter()
                .filter(|s| s.label.is_some())
                .filter_map(|s| s.p)
                .collect(),
            rec.samples
                .iter()
                .filter(|s| s.label.is_none())
                .filter_map(|s| s.p)
                .collect(),
        );
        if 参照.is_empty() || 近期.is_empty() {
            return None;
        }
        Some(jpp_value::stat::drift(&参照, &近期, 10))
    }

    /// 这个键选中的那张证书（见 [`CalibRecord::选中的证书`]）。
    pub fn 选中的证书(&self, key: &str) -> Option<Cert> {
        self.records.get(key).and_then(|r| r.选中的证书().cloned())
    }

    /// 标注集上实测的 unsure 率（J-10）。只有上岗记录的这个值会被 `unsure_bound` 采信。
    pub fn set_unsure_rate(&mut self, key: &str, rate: f64) -> Result<(), String> {
        if !(0.0..=1.0).contains(&rate) {
            return Err("unsure_rate 必须在 0..=1".into());
        }
        let r = self
            .records
            .get_mut(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.unsure_rate = Some(rate);
        // 手填的率不知道测在哪条 δ 上：不绑 δ，照旧采信（宿主为它负责，I4）
        r.unsure_rate_delta = None;
        Ok(())
    }
    /// **从一个目录装载校准记录**（每键一个 JSON，与 Python `CalibStore(path)` 同格式）。
    ///
    /// 在这之前 `CalibStore` **没有任何落盘/装载**，CLI 也从不构造它——
    /// **所以 E-CAL 那三条真记录，内核一次也没读过。**
    ///
    /// **不许静默丢字段。** 与 `Profile::load` 缺字段就报错同一条纪律：
    /// 文件里有而内核没地方放的字段，要么报错、要么**具名地**「知道但不映射」。
    /// 无声吞掉一个字段，和把限定写进注释是同一件事——**那个数会照常丢掉**。
    ///
    /// **装载即重跑认证**（步 20c，J-03 文件面，B117）：读完后按每张证书的方法重跑（[`CalibStore::recertify_all`]），
    /// 不复现的记录降为夹具，复现的写回（旧证书进 `certs_history`），缺 δ 的旧证书由样本与线解出 δ 后写回；
    /// 结论记在 `load_report`。与画像无关：证书的 δ 取证书自己的或解出来的，画像只在 `cut` 时用。
    pub fn load(dir: &std::path::Path) -> Result<CalibStore, String> {
        let mut store = CalibStore::load_raw(dir)?;
        store.load_report = store.recertify_all();
        Ok(store)
    }

    /// 只读文件、不重跑认证（`load` 的前一半；审计与测试用，运行路径不用它）。
    pub fn load_raw(dir: &std::path::Path) -> Result<CalibStore, String> {
        /// 知道、但**故意不映射**的字段。**这张表是具名的**：
        /// `source` 是自由散文（Python 全仓零引用，E-CAL 把 `label_source` 埋在里面）；
        /// `cost_matrix` / `drift_stat` 今天 Rust 侧没有对应承载，**而留一个没有
        /// 生产者也没有消费者的字段是另一个错**（见 INTERFACE 四·十六）。
        const 知道但不映射: &[&str] = &["source", "cost_matrix", "drift_stat"];
        let mut store = CalibStore::new();
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("读不到校准目录 {}：{e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        names.sort();
        for path in names {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("读不到 {}：{e}", path.display()))?;
            let j: Json = serde_json::from_str(&text)
                .map_err(|e| format!("{} 不是合法 JSON：{e}", path.display()))?;
            let obj = j
                .as_object()
                .ok_or_else(|| format!("{} 不是对象", path.display()))?;
            // **认得的字段是算出来的，不是写出来的。**
            //
            // 这里原来是一张**手写的列举表**——我给 `calib_hash` 选了排除法、
            // 给这里留了列举法，**而它在几小时内就漂了**：
            // `label_locator` / `label_fp` 是结构体字段，**却不在那张表里**，
            // 于是 `--calib-out` 写出来的目录**喂不回 `--calib`**（实测报「不认得的字段 label_fp」）。
            // **写的和读的对不上，而两边各自都有测试。**
            //
            // 改成从**结构体自己**问：序列化一条默认记录，它的键就是认得的全集。
            // **算得出来的不许填**——这条规则我今晚用了三次，唯独在这里没用。
            let 映射: Vec<String> = match serde_json::to_value(CalibRecord {
                key: String::new(),
                hi: 0.0,
                lo: 0.0,
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
            }) {
                Ok(Json::Object(m)) => m.keys().cloned().collect(),
                _ => return Err("内部错误：CalibRecord 序列化不出对象".into()),
            };
            // `truth` 为空时不序列化（老记录哈希不变），所以上面那张表里没有它，这里补上
            let 映射: Vec<String> = 映射
                .into_iter()
                .chain([
                    "truth".to_string(),
                    "lower".to_string(),
                    "scope".to_string(),
                    "fixture".to_string(),
                    "rerun_independent".to_string(),
                    "unsure_rate_delta".to_string(),
                    "sources".to_string(),
                    // 20g-1 加的字段（为空不序列化）：漏在这张表里，带文本导入的记录喂不回 --calib（步 20h-1 发现）
                    "material_fps".to_string(),
                    // 步 20a-1 加的分类字段（B76，为空不序列化）
                    "kind".to_string(),
                    // 步 20c 加的旧证书历史（B117 (b)，为空不序列化）
                    "certs_history".to_string(),
                ])
                .collect();
            for k in obj.keys() {
                if !映射.iter().any(|m| m == k) && !知道但不映射.contains(&k.as_str()) {
                    return Err(format!(
                        "{} 有内核不认得的字段 {k:?}：装载不猜，也不无声吞掉",
                        path.display()
                    ));
                }
            }
            let rec: CalibRecord = serde_json::from_value(j.clone())
                .map_err(|e| format!("{} 读不成校准记录：{e}", path.display()))?;
            if !STATUSES.contains(&rec.status.as_str()) {
                return Err(format!(
                    "{} 的 status {:?} 不在 {STATUSES:?} 里",
                    path.display(),
                    rec.status
                ));
            }
            store.records.insert(rec.key.clone(), rec);
        }
        Ok(store)
    }

    /// **把记录落盘**（与 [`CalibStore::load`] 对称：每键一个 JSON）。
    ///
    /// **只写记录，不写 `profile`**——档案是**输入**，从 `--profile` 来，
    /// 把它写进校准目录会造出第二份真相。
    ///
    /// 文件名对键做与 Python `calib.py::_safe` 同样的转义：非字母数字与 `._-` 一律换 `_`。
    pub fn save(&self, dir: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("建不了 {}：{e}", dir.display()))?;
        for (k, rec) in &self.records {
            let safe: String = k
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || "._-".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let j =
                serde_json::to_string_pretty(rec).map_err(|e| format!("{k} 序列化不了：{e}"))?;
            std::fs::write(dir.join(format!("{safe}.json")), j)
                .map_err(|e| format!("写不了 {k}：{e}"))?;
        }
        Ok(())
    }

    /// 声明这条记录用的是哪一份标注集：`id`（J-16 用）、`locator`（给人看的去处）、
    /// `fingerprint`（**检查只信这个**）。
    pub fn declare_label_set(
        &mut self,
        key: &str,
        id: &str,
        locator: &str,
        fingerprint: &str,
    ) -> Result<(), String> {
        let r = self
            .records
            .get_mut(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.label_set_id = id.to_string();
        r.label_locator = locator.to_string();
        r.label_fp = fingerprint.to_string();
        Ok(())
    }

    /// 声明这条记录的标签是怎么选出来的（见 [`LabelSource`]）
    pub fn set_label_source(&mut self, key: &str, src: LabelSource) -> Result<(), String> {
        let r = self
            .records
            .get_mut(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.label_source = src;
        Ok(())
    }
    /// 标注集 id（J-16：代价线与保形线不能同源）
    pub fn set_label_set_id(&mut self, key: &str, id: &str) -> Result<(), String> {
        let r = self
            .records
            .get_mut(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.label_set_id = id.to_string();
        Ok(())
    }
    /// 保形集 id（J-16 用）
    pub fn set_set_id(&mut self, key: &str, id: &str) -> Result<(), String> {
        let r = self
            .records
            .get_mut(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.set_id = id.to_string();
        Ok(())
    }
    /// 这条记录自己的 δ，覆盖档案默认
    pub fn set_delta(&mut self, key: &str, delta: f64) -> Result<(), String> {
        let r = self
            .records
            .get_mut(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.delta = Some(delta);
        Ok(())
    }
    /// **模式级键**（`12`:136「题级样本不够时用模式级校准做先验收缩」）。
    ///
    /// 五元组里去掉 `q_text_hash` 就是模式键。**但不能只剩 `literal_mode`**：那样
    /// 一个模式一格，会把 noul 与 choice 的线并到一起——**不同尺不可比**是本项目自己的
    /// 判据，`delta_for` 也早就按 `op.phys()` 分。所以模式键是 `(phys, literal_mode)`。
    ///
    /// 落在 `\u{1f}` 开头的保留命名空间里，**与任何题级键名都撞不上**。
    /// **题式级键**（B2 待裁，本版只作回退层）：同一题式的所有填法共用一条线。
    /// 查找顺序是 题键 → 题式键 → 模式键；用到题式键时出口留痕，不冒充题级线。
    pub fn form_key(form_hash: &str) -> String {
        format!("\u{1f}form\u{1f}{form_hash}")
    }
    /// **类键**（B34）：作者写的校准键降为校准类别标签后，以该类别为对象、在混合样本上认证的
    /// 记录存在这里（记录种类 `class`，步 20a 的 `RecordKind::Class` 之前由命名空间承载）。
    /// 查找链 题键 → 题式键 → 类键 → 冷（B44）；借线只经这一级。
    pub fn class_key(label: &str) -> String {
        format!("\u{1f}class\u{1f}{label}")
    }
    pub fn mode_key(phys: &str, mode: LiteralMode) -> String {
        format!("\u{1f}mode\u{1f}{phys}{}", mode.suffix())
    }

    /// 带模式的键。默认档就是裸键名。
    pub fn keyed(key: &str, mode: LiteralMode) -> String {
        format!("{key}{}", mode.suffix())
    }
    /// 带模式地写一条记录（`12`:136 的第五维）
    #[allow(clippy::too_many_arguments)] // 步 15d-2：第 8 参 δ（夹具线显式声明 δ）
    pub fn put_moded(
        &mut self,
        key: &str,
        mode: LiteralMode,
        hi: f64,
        lo: f64,
        n: u64,
        status: &str,
        delta: Option<f64>,
    ) -> Result<(), String> {
        self.put(&CalibStore::keyed(key, mode), hi, lo, n, status, delta)
    }
    /// 带模式地读一条记录。**没写过的那一档仍是冷的——不继承别的模式的线。**
    pub fn get_moded(&self, key: &str, mode: LiteralMode) -> CalibRecord {
        self.get(&CalibStore::keyed(key, mode))
    }
    pub fn get(&self, key: &str) -> CalibRecord {
        self.records.get(key).cloned().unwrap_or(CalibRecord {
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
        })
    }
    /// 记录自带的 δ（步 15d-2：δ 只从记录取，没有画像或代码兜底；`op` 保留为调用口径，不参与取值）
    pub fn delta_for(&self, rec: &CalibRecord, _op: Op) -> Option<f64> {
        rec.delta
    }
    /// 某张证书的线用的 δ，与 `cut`（`CalibView::line_delta`）同一规则：证书记了认证带宽用它；证书不按 δ
    /// 平移（`certify` 线、代价线，批量裁定解读 (a)）为 0；否则记录自带；都没有为 `None`。
    pub fn cert_delta(rec: &CalibRecord, c: &super::record::Cert) -> Option<f64> {
        match c.selection.as_ref() {
            Some(s) => s.delta.or(rec.delta),
            None => Some(0.0),
        }
    }
    /// **J-10 可用的 unsure 率**：只认上岗记录；认证时绑了 δ 的，要与现在 `cut` 用的 δ 一致，
    /// 否则当未知（按 1 计）。`unsure_bound`（运行期）与 J-10 的静态那一半共用这一处，口径不会分叉。
    pub fn usable_unsure_rate(&self, rec: &CalibRecord) -> Option<f64> {
        if rec.status != "上岗" {
            return None;
        }
        let u = rec.unsure_rate?;
        match rec.unsure_rate_delta {
            // 按记录自己的题型取 δ（B63 起 K 元划分的率也绑 δ）；无样本的旧记录按 test
            // 与 `cut` 同一个 δ（选中证书的规则，步 15d-2）；取不到 δ 当未知
            Some(d)
                if rec
                    .certs
                    .values()
                    .max_by(|a, b| a.hi.total_cmp(&b.hi))
                    .and_then(|c| CalibStore::cert_delta(rec, c))
                    .or(rec.delta)
                    .is_none_or(|now| (d - now).abs() > 1e-12) =>
            {
                None
            }
            _ => Some(u),
        }
    }
    /// 判断用的线：上岗记录用自己的，其余一律用档案的保守线（与 Python `_uncertainty` 同口径）
    pub fn lines_for(&self, rec: &CalibRecord) -> Option<(f64, f64)> {
        if rec.status == "上岗" {
            Some((rec.hi, rec.lo))
        } else {
            // 画像没测保守线（步 15d-2）：没有线
            self.profile.safety.get().copied()
        }
    }
}

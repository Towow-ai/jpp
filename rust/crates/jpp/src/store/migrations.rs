//! 落盘格式的迁移（`20` v2 §2.3 `jpp::store` 的 `migrations/`、§九）。
//!
//! 只做**文本到文本**的迁移，不经运行时：旧格式读成 JSON、按条目改写、链与 `seq` 按新格式重算。
//! 旧格式的二进制在归档标签处（`21` E5）；迁移只读源文件，不删除、不就地改（改写文件由调用者决定）。

/// 账本 v2 → v3（步 18a，B124 Q3）。
///
/// 改写四处，其余逐字节保留：
/// 1. 头行：`version` 改 3；去掉 `calib_used`；`header.compared` 去掉 `calib_used_hash`。
/// 2. 头行 `calib_used` 的每一键按键序变为一条 `CalibUsed { key, hash, record }`，接在原条目之后
///    （v2 不记首次命中的位置；重放与续接按键取最后一条，位置不影响语义）。
/// 3. `Effect.output` 是 `{"__mat": 内容, "taint", "addr", "origin", "derived_from"}` 的，拆成
///    `output` = 内容、`output_mat` = `{addr, origin, taint}`；`derived_from` 丢弃（v3 由来源边重算；
///    v2 没有来源边，重放时由实参重算，同一程序同一结果）。
/// 4. 链：每条 `{seq, prev, entry}` 的 `seq` 从 1 重排，`prev` 按 v3 文本重算。
///
/// v2 末行半写的截断照 v2 的规则（只丢最后一行无换行且解析不了的），返回截断丢掉的字节数；
/// v2 的链照 v2 的规则核，断了即拒（坏账本不经迁移洗成好账本）。
pub mod ledger_v2 {
    use jpp_ledger::{Entry, Ledger, MatMeta};
    use jpp_value::value::Taint;
    use serde_json::{Value as Json, json};

    /// 迁移结果：v3 文本，与 v2 末行半写被截掉的字节数（没有即 `None`）。
    pub struct Migrated {
        pub text: String,
        pub truncated_bytes: Option<usize>,
        pub calib_used_keys: usize,
    }

    fn err(line: usize, e: impl std::fmt::Display) -> String {
        // 依据：B124 Q3（v2 账本迁移为 v3）；20 §九 账本行（链断、读不成即拒）
        format!("E-ledger-migrate: v2 账本第 {line} 行：{e}")
    }

    pub fn migrate(text: &str) -> Result<Migrated, String> {
        let mut lines: Vec<&str> = text.split('\n').collect();
        let ends_clean = text.ends_with('\n');
        if ends_clean {
            lines.pop();
        }
        // 依据：B124 Q3（只迁移 v2；空文件与非 v2 拒绝）
        let first = lines.first().ok_or("E-ledger-migrate: 账本是空的")?;
        let mut head: Json = serde_json::from_str(first).map_err(|e| err(1, e))?;
        if head.get("version").and_then(Json::as_u64) != Some(2) {
            return Err("E-ledger-migrate: 不是 v2 账本（头行 version 不是 2）".into());
        }
        let calib_used = head
            .as_object_mut()
            .and_then(|o| o.remove("calib_used"))
            .unwrap_or(json!({}));
        if let Some(c) = head
            .get_mut("header")
            .and_then(|h| h.get_mut("compared"))
            .and_then(Json::as_object_mut)
        {
            c.remove("calib_used_hash");
        }
        let header = serde_json::from_value(head["header"].clone()).map_err(|e| err(1, e))?;
        let mut l = Ledger::new();
        l.header = header;
        let n = lines.len();
        let mut truncated_bytes = None;
        // v2 的链照 v2 的规则核（`prev` = 上一行文本的哈希，`seq` 连续）：坏账本不经迁移洗成好账本
        let mut prev = jpp_value::value::hash_of(&["ledger-line", first]);
        for (i, line) in lines.iter().enumerate().skip(1) {
            let v: Json = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) if i + 1 == n && !ends_clean => {
                    truncated_bytes = Some(line.len());
                    break;
                }
                Err(e) => return Err(err(i + 1, e)),
            };
            if v.get("prev").and_then(Json::as_str) != Some(prev.as_str())
                || v.get("seq").and_then(Json::as_u64) != Some(l.entries.len() as u64 + 1)
            {
                return Err(err(i + 1, "链断（seq 或 prev 不接上一行）"));
            }
            prev = jpp_value::value::hash_of(&["ledger-line", line]);
            let mut entry = v
                .get("entry")
                .cloned()
                .ok_or_else(|| err(i + 1, "没有 entry"))?;
            if let Some(eff) = entry.get_mut("Effect").and_then(Json::as_object_mut) {
                let out = eff.get("output").cloned().unwrap_or(Json::Null);
                if let Some(content) = out.get("__mat") {
                    let taint: Taint = match out.get("taint") {
                        Some(t) => serde_json::from_value(t.clone()).unwrap_or(Taint::Untrusted),
                        None => Taint::Untrusted,
                    };
                    let meta = MatMeta {
                        addr: out
                            .get("addr")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        origin: serde_json::from_value(
                            out.get("origin").cloned().unwrap_or(json!([])),
                        )
                        .unwrap_or_default(),
                        taint,
                        sources: vec![],
                    };
                    eff.insert("output".into(), content.clone());
                    eff.insert(
                        "output_mat".into(),
                        serde_json::to_value(meta).expect("可序列化"),
                    );
                }
            }
            let e: Entry = serde_json::from_value(entry).map_err(|e| err(i + 1, e))?;
            l.entries.push(e);
        }
        let keys: Vec<(String, Json)> = calib_used
            .as_object()
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let calib_used_keys = keys.len();
        for (k, v) in keys {
            l.entries.push(Entry::CalibUsed {
                key: k,
                hash: v
                    .get("hash")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                record: v.get("record").cloned().unwrap_or(Json::Null),
            });
        }
        l.rebuild_index();
        Ok(Migrated {
            text: l.encode(),
            truncated_bytes,
            calib_used_keys,
        })
    }

    /// 读一份账本文本：v3 直接解码；v2 在内存里迁移后解码，并返回一行提示。
    pub fn read_any(
        text: &str,
    ) -> Result<(Ledger, Option<jpp_ledger::Truncated>, Option<String>), String> {
        let is_v2 = text
            .split('\n')
            .next()
            .and_then(|f| serde_json::from_str::<Json>(f).ok())
            .and_then(|j| j.get("version").and_then(Json::as_u64))
            == Some(2);
        if !is_v2 {
            let (l, t) = Ledger::decode(text)?;
            return Ok((l, t, None));
        }
        let m = migrate(text)?;
        let (l, _) = Ledger::decode(&m.text)?;
        let t = m.truncated_bytes.map(|b| jpp_ledger::Truncated {
            kept: l.entries.len() - m.calib_used_keys,
            dropped_bytes: b,
        });
        Ok((
            l,
            t,
            Some("账本 v2 已按 v3 读入（文件未改写；用 jpp ledger-migrate 改写）".into()),
        ))
    }
}

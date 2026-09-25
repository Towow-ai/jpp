//! `jpp calib-confirm <calib-dir> <key> --suspend | --keep`：人确认停岗候选（B25）。
//!
//! 系统只把记录标成「停岗候选」并停止用它放行不可逆 `do`；正式停岗由人确认。
//! `--suspend`：候选 → 停岗。`--keep`：候选 → 上岗（人判定漂移不影响这条线）。
//! 只处理状态为「停岗候选」的记录；其余状态不动并报错，免得把这条命令当成通用写线口。

use jpp::effects::CalibStore;

pub fn run(dir: &std::path::Path, key: &str, suspend: bool) -> Result<(), String> {
    // 只改一条记录的状态：按原文件读写（`load_raw`），不把装载时重跑认证的改写与降级落回目录（步 20c）
    let mut store = CalibStore::load_raw(dir)?;
    let k = resolve_key(&store, key).map_err(|e| match e {
        None => format!("{} 里没有校准记录 {key}", dir.display()),
        Some(msg) => format!("{}：{msg}", dir.display()),
    })?;
    let r = store.records.get_mut(&k).expect("刚找到");
    if r.status != "停岗候选" {
        return Err(format!(
            "{key} 的状态是「{}」，不是停岗候选；calib-confirm 只确认候选",
            r.status
        ));
    }
    r.status = if suspend {
        "停岗".into()
    } else {
        "上岗".into()
    };
    let to = r.status.clone();
    store.save(dir)?;
    eprintln!("{key}：停岗候选 → {to}（人确认，B25）");
    Ok(())
}

/// 找命令行给的键对应哪条记录（PR #30 评审 4082390451）。
///
/// `records` 是 `HashMap`，精确键与规范化后相同的别名可以并存（如字面量 `form:abc`
/// 与内部题式键 `\u{1f}form\u{1f}abc`，存盘文件名不同）。先查精确键；没有精确键才按
/// 「`\u{1f}` 写成 `:`、去掉前导 `:`」查别名，别名只有一条才用它，多于一条报错并列出候选。
/// `Err(None)` 表示一条也没有，`Err(Some(_))` 表示别名有歧义。
fn resolve_key(store: &CalibStore, key: &str) -> Result<String, Option<String>> {
    if store.records.contains_key(key) {
        return Ok(key.to_string());
    }
    let norm = |s: &str| s.replace('\u{1f}', ":").trim_start_matches(':').to_string();
    let want = norm(key);
    let mut hits: Vec<&String> = store.records.keys().filter(|k| norm(k) == want).collect();
    hits.sort();
    match hits.as_slice() {
        [] => Err(None),
        [one] => Ok((*one).clone()),
        many => Err(Some(format!(
            "{key} 对应多条校准记录（{}），请用精确键",
            many.iter()
                .map(|k| format!("{k:?}"))
                .collect::<Vec<_>>()
                .join("、")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(keys: &[&str]) -> CalibStore {
        let mut s = CalibStore::new();
        for k in keys {
            let mut r = s.get(k);
            r.status = "停岗候选".into();
            s.records.insert(k.to_string(), r);
        }
        s
    }

    #[test]
    fn exact_key_beats_alias() {
        let s = store(&["form:abc", "\u{1f}form\u{1f}abc"]);
        for _ in 0..32 {
            assert_eq!(resolve_key(&s, "form:abc").unwrap(), "form:abc");
        }
        assert_eq!(
            resolve_key(&s, "\u{1f}form\u{1f}abc").unwrap(),
            "\u{1f}form\u{1f}abc"
        );
    }

    #[test]
    fn unique_alias_resolves() {
        let s = store(&["\u{1f}form\u{1f}abc"]);
        assert_eq!(resolve_key(&s, "form:abc").unwrap(), "\u{1f}form\u{1f}abc");
        assert_eq!(resolve_key(&s, ":form:abc").unwrap(), "\u{1f}form\u{1f}abc");
    }

    #[test]
    fn ambiguous_alias_is_error() {
        let s = store(&["\u{1f}form\u{1f}abc", ":form:abc"]);
        let e = resolve_key(&s, "form:abc").unwrap_err().expect("歧义");
        assert!(e.contains("多条"), "{e}");
    }

    #[test]
    fn missing_key_keeps_old_message() {
        let s = store(&["other"]);
        assert_eq!(resolve_key(&s, "form:abc"), Err(None));
        let dir =
            std::env::temp_dir().join(format!("jpp-calib-confirm-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let e = run(&dir, "form:abc", true).unwrap_err();
        assert!(e.ends_with("里没有校准记录 form:abc"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 端到端：两条并存记录存盘再读回，`--suspend` 只动精确键那条。
    #[test]
    fn suspend_touches_only_exact_record() {
        let dir =
            std::env::temp_dir().join(format!("jpp-calib-confirm-exact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        store(&["form:abc", "\u{1f}form\u{1f}abc"])
            .save(&dir)
            .unwrap();
        run(&dir, "form:abc", true).unwrap();
        let back = CalibStore::load(&dir).unwrap();
        assert_eq!(back.records["form:abc"].status, "停岗");
        assert_eq!(back.records["\u{1f}form\u{1f}abc"].status, "停岗候选");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! 检索动作：`embed_topk`（子进程调离线 MiniLM）、`bm25_topk`（纯 Rust）（B150；R2a 施工，
//! 挪自 `cli/actions_r2a.rs`）。依据：附注 §三·3.2、§五；过程记录 `工程-比赛R2a.md`。

use super::Ctx;
use super::subprocess_util::{run_subprocess, temp_script_path, truncate};
use super::values_util::{hits_to_value, value_number, value_text, value_to_texts};
use crate::value::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/* ============================== embed_topk ============================== */

const DEFAULT_EMBED_MODEL: &str = "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2";

const EMBED_TOPK_DRIVER: &str = r#"
import sys, json, os, glob
try:
    os.environ.setdefault("HF_HUB_OFFLINE", "1")
    os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")
    payload = json.load(sys.stdin)
    model_name = payload["model"]
    corpus = payload["corpus"]
    query = payload["query"]
    k = int(payload["k"])
    cache_path = payload.get("cache_path")

    from sentence_transformers import SentenceTransformer
    import numpy as np

    # 离线解析本地缓存快照目录，绕过 huggingface_hub 对 refs/main 的依赖（有的机器缓存
    # 由外部工具灌入，没有 refs/main 文件；标准解析流程在 offline 模式下因此报「找不到文件」，
    # 即便快照本身完整）。找到就直接用目录路径实例化（天然离线，不再经 Hub 名字解析）；
    # 找不到才退回按模型名走 sentence_transformers 的常规解析（会尝试连网，报错信息更直白）。
    def local_snapshot_dir(name):
        hf_home = os.environ.get("HF_HOME") or os.path.join(os.path.expanduser("~"), ".cache", "huggingface")
        folder = "models--" + name.replace("/", "--")
        pattern = os.path.join(hf_home, "hub", folder, "snapshots", "*")
        candidates = [d for d in glob.glob(pattern) if os.path.isdir(d)]
        return candidates[0] if candidates else None

    snapshot = local_snapshot_dir(model_name)
    model = SentenceTransformer(snapshot or model_name)

    vectors = None
    if cache_path and os.path.exists(cache_path):
        try:
            with open(cache_path, "r") as f:
                cached = json.load(f)
            if cached.get("count") == len(corpus) and cached.get("model") == model_name:
                vectors = np.array(cached["vectors"], dtype="float32")
        except Exception:
            vectors = None

    if vectors is None:
        if corpus:
            vectors = model.encode(corpus, convert_to_numpy=True, normalize_embeddings=True)
        else:
            vectors = np.zeros((0, 1), dtype="float32")
        if cache_path:
            os.makedirs(os.path.dirname(cache_path), exist_ok=True)
            with open(cache_path, "w") as f:
                json.dump({"model": model_name, "count": len(corpus), "vectors": vectors.tolist()}, f)

    if len(corpus) == 0:
        print(json.dumps([]))
    else:
        qv = model.encode([query], convert_to_numpy=True, normalize_embeddings=True)[0]
        scores = vectors @ qv
        order = sorted(range(len(corpus)), key=lambda i: -float(scores[i]))[:k]
        result = [{"id": i, "score": float(scores[i])} for i in order]
        print(json.dumps(result))
except Exception as e:
    print(json.dumps({"__error__": f"{type(e).__name__}: {e}"}))
    sys.exit(1)
"#;

/// 只认 `JPP_EMBED_PYTHON`——这份代码同步到公开仓库，不能内置任何一台开发机的本地路径。
/// 没设时返回一条写清楚要装什么、模型缺省名、怎么设的错误，而不是悄悄退回某个默认值。
fn embed_python_path() -> Result<String, String> {
    std::env::var("JPP_EMBED_PYTHON").map_err(|_| {
        format!(
            "embed_topk: 未设置环境变量 JPP_EMBED_PYTHON。需要一个已安装 sentence-transformers \
             的 Python 解释器（`pip install sentence-transformers`），模型缺省 \
             `{DEFAULT_EMBED_MODEL}`（可用 JPP_EMBED_MODEL 换），首次需在联网环境下载一次到 \
             ~/.cache/huggingface（此后可离线）；设置方式：export JPP_EMBED_PYTHON=/path/to/venv/bin/python"
        )
    })
}

fn embed_model_name() -> String {
    std::env::var("JPP_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_EMBED_MODEL.to_string())
}

fn embed_cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("JPP_EMBED_CACHE_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".cache").join("jpp-embed")
}

fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `embed_topk(texts, query, k)`：子进程调离线 MiniLM。语料向量缓存到文件
/// （键 = 模型名 + 语料哈希），命中时只编码 `query`；向量不进账本，动作只返回
/// `[{id, score}]`。返回 `Err` 时不是「静态拒绝」，是环境问题（未设解释器/超时/解析失败）。
/// **已知限制**：每次调用都要重新装载模型（本机实测冷启动约 9.4 s、语料缓存命中后仍约
/// 6.7 s——缓存只省了重编码语料，没省装载模型；后续改成常驻进程才能把单次调用压到
/// 「只编码 query」的量级，见 `地基/过程记录/工程-比赛R2a.md` §六）。
fn embed_topk_core(corpus: &[String], query: &str, k: usize) -> Result<Vec<(usize, f64)>, String> {
    if corpus.is_empty() {
        return Ok(Vec::new());
    }
    let python = embed_python_path()?;
    if !Path::new(&python).is_file() {
        return Err(format!(
            "embed_topk: JPP_EMBED_PYTHON={python} 不是一个可执行文件（需已装 \
             sentence-transformers 的 Python，模型离线缓存于 ~/.cache/huggingface）"
        ));
    }
    let model = embed_model_name();
    let corpus_refs: Vec<&str> = corpus.iter().map(|s| s.as_str()).collect();
    let corpus_hash = crate::value::hash_of(&corpus_refs);
    let cache_dir = embed_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_path = cache_dir.join(format!(
        "{}-{}.json",
        sanitize_for_filename(&model),
        corpus_hash
    ));
    let payload = serde_json::json!({
        "model": model,
        "corpus": corpus,
        "query": query,
        "k": k,
        "cache_path": cache_path.to_string_lossy(),
    })
    .to_string();
    let script_path = temp_script_path("embed-topk");
    std::fs::write(&script_path, EMBED_TOPK_DRIVER)
        .map_err(|e| format!("embed_topk: 写临时脚本失败：{e}"))?;
    let mut cmd = Command::new(&python);
    cmd.arg("-I").arg(&script_path);
    cmd.env("HF_HUB_OFFLINE", "1");
    cmd.env("TRANSFORMERS_OFFLINE", "1");
    cmd.env("OMP_NUM_THREADS", "2");
    cmd.env("MKL_NUM_THREADS", "2");
    cmd.env("TOKENIZERS_PARALLELISM", "false");
    let timeout = Duration::from_secs(60);
    let res = run_subprocess(cmd, &payload, timeout);
    let _ = std::fs::remove_file(&script_path);
    let r = res.map_err(|e| format!("embed_topk: {e}（解释器：{python}）"))?;
    if r.timed_out {
        return Err("embed_topk: 60 秒超时（模型装载或编码过慢）".into());
    }
    let parsed: serde_json::Value = serde_json::from_str(r.stdout.trim()).map_err(|e| {
        format!(
            "embed_topk: 解析子进程输出失败：{e}；stderr: {}",
            truncate(&r.stderr, 800)
        )
    })?;
    if let Some(err) = parsed.get("__error__") {
        return Err(format!("embed_topk: {err}"));
    }
    let arr = parsed.as_array().ok_or_else(|| {
        format!(
            "embed_topk: 子进程输出不是数组：{}",
            truncate(&r.stdout, 300)
        )
    })?;
    let mut hits = Vec::with_capacity(arr.len());
    for item in arr {
        let id = item
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or("embed_topk: 结果项缺 id")? as usize;
        let score = item
            .get("score")
            .and_then(|v| v.as_f64())
            .ok_or("embed_topk: 结果项缺 score")?;
        hits.push((id, score));
    }
    Ok(hits)
}

/// `do("embed_topk", [texts, query, k], seq)`。
pub(super) fn embed_topk(_ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [texts, query, k] = args else {
        return Err("embed_topk expects (texts, query, k)".into());
    };
    let corpus = value_to_texts(texts).map_err(|e| format!("embed_topk: {e}"))?;
    let query_s = value_text(query).ok_or("embed_topk: query 必须是文本或材料")?;
    let k = value_number(k).ok_or("embed_topk: k 必须是数字")? as usize;
    let hits = embed_topk_core(&corpus, &query_s, k)?;
    Ok(hits_to_value(hits))
}

/* ============================== bm25_topk ============================== */

fn is_cjk(c: char) -> bool {
    let u = c as u32;
    (0x4E00..=0x9FFF).contains(&u)   // 基本汉字
        || (0x3400..=0x4DBF).contains(&u) // 扩展 A
        || (0x3040..=0x30FF).contains(&u) // 平假名/片假名
        || (0xAC00..=0xD7A3).contains(&u) // 谚文音节
}

/// 分词：ASCII 字母数字聚成词（转小写）；CJK 按单字切分（简化的字级粒度，
/// 不接分词库；够用于粗筛，不作为「已定的分词算法」写进依据文本，只是这个动作的实现细节）。
fn tokenize(s: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut buf = String::new();
    for c in s.chars() {
        if is_cjk(c) {
            if !buf.is_empty() {
                toks.push(std::mem::take(&mut buf).to_lowercase());
            }
            toks.push(c.to_string());
        } else if c.is_alphanumeric() {
            buf.push(c);
        } else if !buf.is_empty() {
            toks.push(std::mem::take(&mut buf).to_lowercase());
        }
    }
    if !buf.is_empty() {
        toks.push(buf.to_lowercase());
    }
    toks
}

/// `bm25_topk(query, corpus, k)`：经典 BM25（k1=1.5，b=0.75），纯 Rust、无新依赖。
/// 返回 `[(id, score)]` 按分数降序、同分按下标升序（稳定排序），截到前 `k`。
fn bm25_topk_core(query: &str, corpus: &[String], k: usize) -> Vec<(usize, f64)> {
    let k1 = 1.5_f64;
    let b = 0.75_f64;
    let n = corpus.len();
    if n == 0 {
        return Vec::new();
    }
    let docs: Vec<Vec<String>> = corpus.iter().map(|d| tokenize(d)).collect();
    let avgdl = docs.iter().map(|d| d.len() as f64).sum::<f64>() / n as f64;
    let mut df: HashMap<&str, usize> = HashMap::new();
    let doc_sets: Vec<HashSet<&str>> = docs
        .iter()
        .map(|d| d.iter().map(|s| s.as_str()).collect())
        .collect();
    for set in &doc_sets {
        for t in set {
            *df.entry(t).or_insert(0) += 1;
        }
    }
    let q_tokens = tokenize(query);
    let mut scores: Vec<(usize, f64)> = (0..n)
        .map(|i| {
            let dl = docs[i].len() as f64;
            let mut tf: HashMap<&str, usize> = HashMap::new();
            for t in &docs[i] {
                *tf.entry(t.as_str()).or_insert(0) += 1;
            }
            let score = q_tokens.iter().fold(0.0_f64, |acc, qt| {
                let f = *tf.get(qt.as_str()).unwrap_or(&0) as f64;
                if f == 0.0 {
                    return acc;
                }
                let n_qi = *df.get(qt.as_str()).unwrap_or(&0) as f64;
                let idf = ((n as f64 - n_qi + 0.5) / (n_qi + 0.5) + 1.0).ln();
                acc + idf * (f * (k1 + 1.0)) / (f + k1 * (1.0 - b + b * dl / avgdl))
            });
            (i, score)
        })
        .collect();
    scores.sort_by(|a, b2| {
        b2.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b2.0))
    });
    scores.truncate(k);
    scores
}

/// `do("bm25_topk", [query, corpus, k], seq)`。
pub(super) fn bm25_topk(_ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [query, corpus, k] = args else {
        return Err("bm25_topk expects (query, corpus, k)".into());
    };
    let query_s = value_text(query).ok_or("bm25_topk: query 必须是文本或材料")?;
    let corpus_texts = value_to_texts(corpus).map_err(|e| format!("bm25_topk: {e}"))?;
    let k = value_number(k).ok_or("bm25_topk: k 必须是数字")? as usize;
    let hits = bm25_topk_core(&query_s, &corpus_texts, k);
    Ok(hits_to_value(hits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_topk_core_ranks_exact_term_first() {
        let corpus = vec![
            "苹果 香蕉 橙子".to_string(),
            "今天天气很好".to_string(),
            "苹果派配咖啡".to_string(),
        ];
        let hits = bm25_topk_core("苹果", &corpus, 2);
        assert_eq!(hits.len(), 2);
        assert!(hits[0].0 == 0 || hits[0].0 == 2, "含“苹果”的文档应排前");
        assert!(hits[0].1 > 0.0);
    }

    #[test]
    fn bm25_topk_core_empty_corpus_returns_empty() {
        assert!(bm25_topk_core("query", &[], 5).is_empty());
    }

    #[test]
    fn bm25_topk_core_mixed_chinese_english() {
        let corpus = vec!["Rust 编译器 优化".to_string(), "Python 解释器".to_string()];
        let hits = bm25_topk_core("rust compiler", &corpus, 2);
        assert_eq!(hits[0].0, 0, "英文词大小写不敏感，应命中第一篇");
    }

    #[test]
    fn embed_topk_core_semantic_rank_or_skip() {
        match embed_python_path() {
            Ok(p) if Path::new(&p).is_file() => {}
            Ok(p) => {
                eprintln!(
                    "跳过 embed_topk_core_semantic_rank_or_skip：JPP_EMBED_PYTHON={p} 不是可执行文件（环境依赖，非失败）"
                );
                return;
            }
            Err(_) => {
                eprintln!(
                    "跳过 embed_topk_core_semantic_rank_or_skip：未设置 JPP_EMBED_PYTHON（环境依赖，非失败）"
                );
                return;
            }
        }
        let corpus = vec![
            "苹果".to_string(),
            "香蕉".to_string(),
            "橙子".to_string(),
            "汽车".to_string(),
        ];
        let hits = embed_topk_core(&corpus, "水果", 2).expect("应能调用离线 MiniLM");
        assert_eq!(hits.len(), 2);
        for (id, _) in &hits {
            assert!(
                *id < 3,
                "水果查询的前 2 名不该包含“汽车”（id 3），实得 {hits:?}"
            );
        }
    }
}

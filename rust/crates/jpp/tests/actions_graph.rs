//! 对拍与耗时测试：`graph:*` 六个精确图算法宿主动作（比赛 R2b）。
//! 预注册（改代码前登记的预测）见 `地基/过程记录/工程-比赛R2b.md` §五；本文件是对那份预注册的检验。
//! 不加 `rand` 依赖（本机 cargo 缓存没有，§七 R2b 明令不加新依赖）：随机数用手写 xorshift64，
//! 固定种子，任何失败都能用打印出的种子复现。
//!
//! `needless_range_loop` 全文件放行：随机图生成与暴力对拍两处大量用 `for i in 0..n` 双重下标
//! （节点对、位掩码、DP 转移），下标本身参与运算（如 `1 << i`、`(i+1)..n`），不是单纯取
//! `arr[i]`；改写成 `enumerate()` 不会更清楚，且测试文件本就不是被搬迁/复用的产物代码。
#![allow(clippy::needless_range_loop)]

use jpp::interp::{ActionRegistry, json_to_value};
use serde_json::{Value as Json, json};
use std::collections::HashMap;
use std::time::Instant;

// ---------- 测试基座 ----------

fn registry() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    a
}

/// 直接调已登记的闭包（不经完整 `.jpp` 程序/端口/账本），因为 `graph:*` 是纯函数，
/// 用 `ActionRegistry` 的公开字段够了，不需要 `Session`/`Ports`。
fn call(actions: &ActionRegistry, name: &str, input: Json) -> Result<Json, String> {
    let action = actions
        .actions
        .get(name)
        .unwrap_or_else(|| panic!("action {name} not registered"));
    let v = json_to_value(&input);
    let out = (action.f)(&[v])?;
    Ok(out.to_json())
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1) // 保证非零、为奇（xorshift64 要求种子非零）
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next_u64() as usize) % n
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() % 1_000_000) as f64 / 1_000_000.0 < p
    }
}

fn node_names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("n{i}")).collect()
}

// ---------- graph:matching（二分） ----------

fn brute_force_bipartite(n: usize, m: usize, w: &[Vec<Option<f64>>]) -> f64 {
    fn rec(i: usize, n: usize, m: usize, used: &mut [bool], w: &[Vec<Option<f64>>]) -> f64 {
        if i == n {
            return 0.0;
        }
        let mut best = rec(i + 1, n, m, used, w);
        for j in 0..m {
            if !used[j]
                && let Some(wt) = w[i][j]
            {
                used[j] = true;
                let cand = wt + rec(i + 1, n, m, used, w);
                if cand > best {
                    best = cand;
                }
                used[j] = false;
            }
        }
        best
    }
    let mut used = vec![false; m];
    rec(0, n, m, &mut used, w)
}

#[test]
fn matching_bipartite_vs_brute_force_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0x9E37_79B9 ^ trial);
        let n = 1 + rng.range(6);
        let m = 1 + rng.range(6);
        let left = node_names(n)
            .iter()
            .map(|s| format!("L{s}"))
            .collect::<Vec<_>>();
        let right = node_names(m)
            .iter()
            .map(|s| format!("R{s}"))
            .collect::<Vec<_>>();
        let mut w = vec![vec![None; m]; n];
        let mut edges_json = Vec::new();
        for i in 0..n {
            for j in 0..m {
                if rng.chance(0.5) {
                    let weight = 1 + rng.range(9);
                    w[i][j] = Some(weight as f64);
                    edges_json.push(json!({"u": left[i], "v": right[j], "w": weight}));
                }
            }
        }
        let input = json!({
            "edges": edges_json,
            "bipartite": true,
            "parts": [left, right],
        });
        let out = call(&actions, "graph:matching", input)
            .unwrap_or_else(|e| panic!("trial {trial} (n={n},m={m}): {e}"));
        let total = out["total_weight"].as_f64().unwrap();
        let expected = brute_force_bipartite(n, m, &w);
        assert_eq!(
            total, expected,
            "trial {trial} (n={n}, m={m}) seed=0x9E3779B9^{trial}: got {total}, brute force {expected}"
        );
        // 产出的 pairs 本身要是合法匹配，且权重和与 total_weight 一致（防「总权对、配对错」的空子）。
        let pairs = out["pairs"].as_array().unwrap();
        let mut seen_left = std::collections::HashSet::new();
        let mut seen_right = std::collections::HashSet::new();
        let mut sum = 0.0;
        for p in pairs {
            let u = p["u"].as_str().unwrap();
            let v = p["v"].as_str().unwrap();
            assert!(
                seen_left.insert(u.to_string()),
                "trial {trial}: node {u} matched twice"
            );
            assert!(
                seen_right.insert(v.to_string()),
                "trial {trial}: node {v} matched twice"
            );
            sum += p["w"].as_f64().unwrap();
        }
        assert_eq!(
            sum, total,
            "trial {trial}: pairs weight sum != total_weight"
        );
    }
}

#[test]
fn matching_bipartite_325_timing() {
    let actions = registry();
    let mut rng = Rng::new(325);
    let n = 162;
    let m = 163;
    let left = (0..n).map(|i| format!("L{i}")).collect::<Vec<_>>();
    let right = (0..m).map(|i| format!("R{i}")).collect::<Vec<_>>();
    let mut edges_json = Vec::new();
    for i in 0..n {
        for j in 0..m {
            if rng.chance(0.15) {
                edges_json.push(json!({"u": left[i], "v": right[j], "w": 1 + rng.range(20)}));
            }
        }
    }
    let input = json!({"edges": edges_json, "bipartite": true, "parts": [left, right]});
    let start = Instant::now();
    let out = call(&actions, "graph:matching", input).expect("325-node bipartite matching");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:matching bipartite 162x163 edges={} elapsed={:?} total_weight={}",
        out["pairs"].as_array().unwrap().len(),
        elapsed,
        out["total_weight"]
    );
    assert!(
        elapsed.as_secs_f64() < 20.0,
        "bipartite matching at 325 nodes took {elapsed:?}, predicted < 2s (debug build generous bound 20s)"
    );
}

// ---------- graph:matching（一般图） ----------

fn brute_force_general_matching(n: usize, w: &HashMap<(usize, usize), f64>) -> f64 {
    let mut memo: HashMap<usize, f64> = HashMap::new();
    fn rec(
        mask: usize,
        n: usize,
        w: &HashMap<(usize, usize), f64>,
        memo: &mut HashMap<usize, f64>,
    ) -> f64 {
        if mask == 0 {
            return 0.0;
        }
        if let Some(&v) = memo.get(&mask) {
            return v;
        }
        let i = mask.trailing_zeros() as usize;
        let without_i = mask & !(1 << i);
        let mut best = rec(without_i, n, w, memo);
        for j in 0..n {
            if j == i {
                continue;
            }
            let jb = 1usize << j;
            if mask & jb == 0 {
                continue;
            }
            if let Some(&wt) = w.get(&(i.min(j), i.max(j))) {
                let rest = without_i & !jb;
                let cand = wt + rec(rest, n, w, memo);
                if cand > best {
                    best = cand;
                }
            }
        }
        memo.insert(mask, best);
        best
    }
    rec((1 << n) - 1, n, w, &mut memo)
}

#[test]
fn matching_general_vs_brute_force_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0xC001_D00D ^ (trial << 8));
        let n = 2 + rng.range(7); // 2..=8
        let nodes = node_names(n);
        let mut w: HashMap<(usize, usize), f64> = HashMap::new();
        let mut edges_json = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.chance(0.5) {
                    let weight = 1 + rng.range(9);
                    w.insert((i, j), weight as f64);
                    edges_json.push(json!({"u": nodes[i], "v": nodes[j], "w": weight}));
                }
            }
        }
        let input = json!({"edges": edges_json, "nodes": nodes, "bipartite": false});
        let out = call(&actions, "graph:matching", input)
            .unwrap_or_else(|e| panic!("trial {trial} (n={n}): {e}"));
        let total = out["total_weight"].as_f64().unwrap();
        let expected = brute_force_general_matching(n, &w);
        assert_eq!(
            total, expected,
            "trial {trial} (n={n}): got {total}, brute force {expected}"
        );
        let pairs = out["pairs"].as_array().unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut sum = 0.0;
        for p in pairs {
            let u = p["u"].as_str().unwrap();
            let v = p["v"].as_str().unwrap();
            assert!(
                seen.insert(u.to_string()),
                "trial {trial}: node {u} matched twice"
            );
            assert!(
                seen.insert(v.to_string()),
                "trial {trial}: node {v} matched twice"
            );
            sum += p["w"].as_f64().unwrap();
        }
        assert_eq!(
            sum, total,
            "trial {trial}: pairs weight sum != total_weight"
        );
    }
}

#[test]
fn matching_general_n20_timing() {
    let actions = registry();
    let mut rng = Rng::new(20);
    let n = 20;
    let nodes = node_names(n);
    let mut edges_json = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.chance(0.6) {
                edges_json.push(json!({"u": nodes[i], "v": nodes[j], "w": 1 + rng.range(20)}));
            }
        }
    }
    let input = json!({"edges": edges_json, "nodes": nodes});
    let start = Instant::now();
    let out = call(&actions, "graph:matching", input).expect("n=20 general matching");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:matching general n=20 dense elapsed={:?} total_weight={}",
        elapsed, out["total_weight"]
    );
    assert!(
        elapsed.as_secs_f64() < 30.0,
        "general matching at n=20 took {elapsed:?}, predicted < 3s (debug build generous bound 30s)"
    );
}

#[test]
fn matching_general_over_cap_errors() {
    let actions = registry();
    let n = 21;
    let nodes = node_names(n);
    let input = json!({"edges": Json::Array(vec![]), "nodes": nodes});
    let err = call(&actions, "graph:matching", input).unwrap_err();
    assert!(
        err.contains("20"),
        "expected cap-exceeded error mentioning the limit, got: {err}"
    );
}

// ---------- graph:shortest_path ----------

fn floyd_warshall(n: usize, edges: &[(usize, usize, f64)], directed: bool) -> Vec<Vec<f64>> {
    let inf = f64::INFINITY;
    let mut d = vec![vec![inf; n]; n];
    for (i, row) in d.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    for &(u, v, w) in edges {
        if w < d[u][v] {
            d[u][v] = w;
        }
        if !directed && w < d[v][u] {
            d[v][u] = w;
        }
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if d[i][k] + d[k][j] < d[i][j] {
                    d[i][j] = d[i][k] + d[k][j];
                }
            }
        }
    }
    d
}

#[test]
fn shortest_path_vs_floyd_warshall_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0x5EED_5EED ^ (trial << 4));
        let n = 2 + rng.range(9); // 2..=10
        let nodes = node_names(n);
        let directed = rng.chance(0.5);
        let mut raw_edges = Vec::new();
        let mut edges_json = Vec::new();
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                if !directed && j < i {
                    continue;
                }
                if rng.chance(0.35) {
                    let w = 1 + rng.range(9);
                    raw_edges.push((i, j, w as f64));
                    edges_json.push(json!({"u": nodes[i], "v": nodes[j], "w": w}));
                }
            }
        }
        let s = rng.range(n);
        let mut t = rng.range(n);
        if t == s {
            t = (t + 1) % n;
        }
        let input = json!({
            "edges": edges_json, "nodes": nodes, "directed": directed,
            "source": nodes[s], "target": nodes[t],
        });
        let out = call(&actions, "graph:shortest_path", input)
            .unwrap_or_else(|e| panic!("trial {trial} (n={n}): {e}"));
        let d = floyd_warshall(n, &raw_edges, directed);
        let expected = d[s][t];
        if expected.is_infinite() {
            assert_eq!(
                out["reachable"],
                json!(false),
                "trial {trial}: expected unreachable"
            );
        } else {
            assert_eq!(
                out["reachable"],
                json!(true),
                "trial {trial}: expected reachable, dist {expected}"
            );
            let got = out["distance"].as_f64().unwrap();
            assert_eq!(
                got, expected,
                "trial {trial} (n={n}, s={s}, t={t}): got {got}, floyd {expected}"
            );
            // path 本身要是一条一致的路径，边权和等于报告的 distance
            let path = out["path"].as_array().unwrap();
            assert_eq!(path.first().unwrap().as_str().unwrap(), nodes[s]);
            assert_eq!(path.last().unwrap().as_str().unwrap(), nodes[t]);
            let path_edges = out["path_edges"].as_array().unwrap();
            let sum: f64 = path_edges.iter().map(|e| e["w"].as_f64().unwrap()).sum();
            assert_eq!(sum, got, "trial {trial}: path_edges weight sum != distance");
        }
    }
}

#[test]
fn shortest_path_325_timing() {
    let actions = registry();
    let mut rng = Rng::new(9001);
    let n = 325;
    let nodes = node_names(n);
    let mut edges_json = Vec::new();
    for i in 0..n {
        for j in 0..n {
            if i != j && rng.chance(0.015) {
                edges_json.push(json!({"u": nodes[i], "v": nodes[j], "w": 1 + rng.range(9)}));
            }
        }
    }
    let input = json!({
        "edges": edges_json, "nodes": nodes, "directed": true,
        "source": nodes[0], "target": nodes[n - 1],
    });
    let start = Instant::now();
    let out = call(&actions, "graph:shortest_path", input).expect("325-node shortest_path");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:shortest_path 325 nodes elapsed={:?} reachable={}",
        elapsed, out["reachable"]
    );
    assert!(
        elapsed.as_secs_f64() < 10.0,
        "shortest_path at 325 nodes took {elapsed:?}, predicted < 0.5s (debug build generous bound 10s)"
    );
}

// ---------- graph:max_clique ----------

fn brute_force_max_clique(n: usize, adj: &[u64]) -> u32 {
    let mut best = 0u32;
    for mask in 0u64..(1u64 << n) {
        let mut ok = true;
        'outer: for i in 0..n {
            if mask & (1 << i) == 0 {
                continue;
            }
            for j in (i + 1)..n {
                if mask & (1 << j) == 0 {
                    continue;
                }
                if adj[i] & (1 << j) == 0 {
                    ok = false;
                    break 'outer;
                }
            }
        }
        if ok {
            let sz = mask.count_ones();
            if sz > best {
                best = sz;
            }
        }
    }
    best
}

#[test]
fn max_clique_vs_brute_force_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0xC11C_0E00 ^ (trial << 3));
        let n = 1 + rng.range(13); // 1..=13
        let nodes = node_names(n);
        let mut adj = vec![0u64; n];
        let mut edges_json = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.chance(0.45) {
                    adj[i] |= 1 << j;
                    adj[j] |= 1 << i;
                    edges_json.push(json!({"u": nodes[i], "v": nodes[j]}));
                }
            }
        }
        let input = json!({"edges": edges_json, "nodes": nodes});
        let out = call(&actions, "graph:max_clique", input)
            .unwrap_or_else(|e| panic!("trial {trial} (n={n}): {e}"));
        let got = out["size"].as_u64().unwrap() as u32;
        let expected = brute_force_max_clique(n, &adj);
        assert_eq!(
            got, expected,
            "trial {trial} (n={n}): got {got}, brute force {expected}"
        );
        // 报告的团本身要真是一个团（两两相连）
        let clique = out["clique"].as_array().unwrap();
        let name_to_idx: HashMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        for a in clique {
            for b in clique {
                let ai = name_to_idx[a.as_str().unwrap()];
                let bi = name_to_idx[b.as_str().unwrap()];
                if ai != bi {
                    assert!(
                        adj[ai] & (1 << bi) != 0,
                        "trial {trial}: reported clique isn't a clique"
                    );
                }
            }
        }
    }
}

#[test]
fn max_clique_n60_sparse_timing() {
    let actions = registry();
    let mut rng = Rng::new(60);
    let n = 60;
    let nodes = node_names(n);
    let mut edges_json = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.chance(0.05) {
                edges_json.push(json!({"u": nodes[i], "v": nodes[j]}));
            }
        }
    }
    let input = json!({"edges": edges_json, "nodes": nodes});
    let start = Instant::now();
    let out = call(&actions, "graph:max_clique", input).expect("n=60 sparse max_clique");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:max_clique n=60 density~5% elapsed={:?} size={}",
        elapsed, out["size"]
    );
    assert!(
        elapsed.as_secs_f64() < 30.0,
        "max_clique at n=60 took {elapsed:?}, predicted < 5s (debug build generous bound 30s); \
         325-node timing test intentionally not run here, see 工程-比赛R2b.md §三·3.3 与问题清单 #3"
    );
}

// ---------- graph:components ----------

fn brute_force_components(n: usize, adj: &[Vec<usize>]) -> Vec<usize> {
    let mut label = vec![usize::MAX; n];
    let mut cur = 0usize;
    for start in 0..n {
        if label[start] != usize::MAX {
            continue;
        }
        let mut stack = vec![start];
        label[start] = cur;
        while let Some(u) = stack.pop() {
            for &v in &adj[u] {
                if label[v] == usize::MAX {
                    label[v] = cur;
                    stack.push(v);
                }
            }
        }
        cur += 1;
    }
    label
}

#[test]
fn components_vs_brute_force_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0xC0A1_E5CE ^ (trial << 5));
        let n = 1 + rng.range(11); // 1..=11
        let nodes = node_names(n);
        let mut adj = vec![Vec::new(); n];
        let mut edges_json = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.chance(0.2) {
                    adj[i].push(j);
                    adj[j].push(i);
                    edges_json.push(json!({"u": nodes[i], "v": nodes[j]}));
                }
            }
        }
        let input = json!({"edges": edges_json, "nodes": nodes});
        let out = call(&actions, "graph:components", input)
            .unwrap_or_else(|e| panic!("trial {trial} (n={n}): {e}"));
        let labels = brute_force_components(n, &adj);
        let comps = out["components"].as_array().unwrap();
        // 用「production 输出里同一分量的两两节点，在 brute force 里也同标签，反之亦然」核对划分相等
        let name_to_idx: HashMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let mut got_label = vec![usize::MAX; n];
        for (ci, comp) in comps.iter().enumerate() {
            for m in comp.as_array().unwrap() {
                got_label[name_to_idx[m.as_str().unwrap()]] = ci;
            }
        }
        assert!(
            got_label.iter().all(|&l| l != usize::MAX),
            "trial {trial}: every node must appear in exactly one component"
        );
        for a in 0..n {
            for b in 0..n {
                let same_brute = labels[a] == labels[b];
                let same_got = got_label[a] == got_label[b];
                assert_eq!(
                    same_brute, same_got,
                    "trial {trial} (n={n}): node {a} vs {b} component agreement mismatch"
                );
            }
        }
    }
}

#[test]
fn components_325_timing() {
    let actions = registry();
    let mut rng = Rng::new(325_002);
    let n = 325;
    let nodes = node_names(n);
    let mut edges_json = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.chance(0.02) {
                edges_json.push(json!({"u": nodes[i], "v": nodes[j]}));
            }
        }
    }
    let input = json!({"edges": edges_json, "nodes": nodes});
    let start = Instant::now();
    let out = call(&actions, "graph:components", input).expect("325-node components");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:components 325 nodes elapsed={:?} count={}",
        elapsed, out["count"]
    );
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "components at 325 nodes took {elapsed:?}, predicted < 0.1s (debug build generous bound 5s)"
    );
}

// ---------- graph:set_cover ----------

fn brute_force_set_cover(u: usize, sets: &[(u32, f64)]) -> Option<f64> {
    let full: u32 = if u == 0 { 0 } else { (1u32 << u) - 1 };
    let k = sets.len();
    let mut best: Option<f64> = None;
    for combo in 0u32..(1u32 << k) {
        let mut mask = 0u32;
        let mut cost = 0.0;
        for i in 0..k {
            if combo & (1 << i) != 0 {
                mask |= sets[i].0;
                cost += sets[i].1;
            }
        }
        if mask & full == full && (best.is_none() || cost < best.unwrap()) {
            best = Some(cost);
        }
    }
    best
}

#[test]
fn set_cover_vs_brute_force_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0x5E7C_0BE5 ^ (trial << 6));
        let u = 1 + rng.range(10); // 1..=10
        let universe: Vec<String> = (0..u).map(|i| format!("e{i}")).collect();
        let k = 2 + rng.range(7); // 2..=8
        let mut sets_bits = Vec::with_capacity(k);
        let mut sets_json = Vec::with_capacity(k);
        for si in 0..k {
            let mut mask = 0u32;
            let mut elems = Vec::new();
            for ei in 0..u {
                if rng.chance(0.4) {
                    mask |= 1 << ei;
                    elems.push(universe[ei].clone());
                }
            }
            let cost = 1 + rng.range(5);
            sets_bits.push((mask, cost as f64));
            sets_json.push(json!({"id": format!("s{si}"), "elements": elems, "cost": cost}));
        }
        let input = json!({"universe": universe, "sets": sets_json});
        let out = call(&actions, "graph:set_cover", input)
            .unwrap_or_else(|e| panic!("trial {trial} (u={u}, k={k}): {e}"));
        let expected = brute_force_set_cover(u, &sets_bits);
        match expected {
            None => {
                assert_eq!(
                    out["covers_universe"],
                    json!(false),
                    "trial {trial}: brute force says uncoverable, action disagreed"
                );
            }
            Some(exp_cost) => {
                assert_eq!(
                    out["covers_universe"],
                    json!(true),
                    "trial {trial}: expected coverable"
                );
                let got_cost = out["total_cost"].as_f64().unwrap();
                assert_eq!(
                    got_cost, exp_cost,
                    "trial {trial} (u={u}, k={k}): got {got_cost}, brute force {exp_cost}"
                );
            }
        }
    }
}

#[test]
fn set_cover_325_greedy_timing() {
    let actions = registry();
    let mut rng = Rng::new(325_003);
    let u = 325;
    let universe: Vec<String> = (0..u).map(|i| format!("e{i}")).collect();
    let k = 40;
    let mut sets_json = Vec::with_capacity(k);
    for si in 0..k {
        let mut elems = Vec::new();
        for ei in 0..u {
            if rng.chance(0.08) {
                elems.push(universe[ei].clone());
            }
        }
        sets_json
            .push(json!({"id": format!("s{si}"), "elements": elems, "cost": 1 + rng.range(5)}));
    }
    let input = json!({"universe": universe, "sets": sets_json});
    let start = Instant::now();
    let out = call(&actions, "graph:set_cover", input).expect("325-universe set_cover (greedy)");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:set_cover |U|=325 greedy elapsed={:?} covers={} cost={}",
        elapsed, out["covers_universe"], out["total_cost"]
    );
    assert_eq!(
        out["exact"],
        json!(false),
        "|U|=325 should take the greedy (non-exact) path"
    );
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "greedy set_cover at |U|=325 took {elapsed:?}, predicted < 0.2s (debug build generous bound 5s)"
    );
}

#[test]
fn set_cover_20_exact_timing() {
    let actions = registry();
    let mut rng = Rng::new(325_004);
    let u = 20;
    let universe: Vec<String> = (0..u).map(|i| format!("e{i}")).collect();
    let k = 12;
    let mut sets_json = Vec::with_capacity(k);
    for si in 0..k {
        let mut elems = Vec::new();
        for ei in 0..u {
            if rng.chance(0.3) {
                elems.push(universe[ei].clone());
            }
        }
        sets_json
            .push(json!({"id": format!("s{si}"), "elements": elems, "cost": 1 + rng.range(5)}));
    }
    let input = json!({"universe": universe, "sets": sets_json});
    let start = Instant::now();
    let out = call(&actions, "graph:set_cover", input).expect("|U|=20 exact set_cover");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:set_cover |U|=20 exact elapsed={:?} covers={} cost={}",
        elapsed, out["covers_universe"], out["total_cost"]
    );
    assert_eq!(
        out["exact"],
        json!(true),
        "|U|=20 should take the exact DP path"
    );
    assert!(
        elapsed.as_secs_f64() < 20.0,
        "exact set_cover at |U|=20 took {elapsed:?}"
    );
}

// ---------- graph:max_flow ----------

fn brute_force_max_flow(n: usize, s: usize, t: usize, edges: &[(usize, usize, f64)]) -> f64 {
    let mut best = f64::INFINITY;
    for mask in 0u32..(1u32 << n) {
        if mask & (1 << s) == 0 || mask & (1 << t) != 0 {
            continue;
        }
        let mut cut = 0.0;
        for &(u, v, cap) in edges {
            let u_in = mask & (1 << u) != 0;
            let v_in = mask & (1 << v) != 0;
            if u_in && !v_in {
                cut += cap;
            }
        }
        if cut < best {
            best = cut;
        }
    }
    best
}

#[test]
fn max_flow_vs_min_cut_brute_force_200() {
    let actions = registry();
    for trial in 0..200u64 {
        let mut rng = Rng::new(0xF10F_10F0 ^ (trial << 2));
        let n = 2 + rng.range(8); // 2..=9
        let nodes = node_names(n);
        let mut raw_edges = Vec::new();
        let mut edges_json = Vec::new();
        for i in 0..n {
            for j in 0..n {
                if i != j && rng.chance(0.3) {
                    let cap = 1 + rng.range(9);
                    raw_edges.push((i, j, cap as f64));
                    edges_json.push(json!({"u": nodes[i], "v": nodes[j], "cap": cap}));
                }
            }
        }
        let s = rng.range(n);
        let mut t = rng.range(n);
        if t == s {
            t = (t + 1) % n;
        }
        let input =
            json!({"edges": edges_json, "nodes": nodes, "source": nodes[s], "sink": nodes[t]});
        let out = call(&actions, "graph:max_flow", input)
            .unwrap_or_else(|e| panic!("trial {trial} (n={n}): {e}"));
        let got = out["max_flow"].as_f64().unwrap();
        let expected = brute_force_max_flow(n, s, t, &raw_edges);
        assert_eq!(
            got, expected,
            "trial {trial} (n={n}, s={s}, t={t}): got {got}, min-cut {expected}"
        );
    }
}

#[test]
fn max_flow_325_timing() {
    let actions = registry();
    let mut rng = Rng::new(325_005);
    let n = 325;
    let nodes = node_names(n);
    let mut edges_json = Vec::new();
    for i in 0..n {
        for j in 0..n {
            if i != j && rng.chance(0.03) {
                edges_json.push(json!({"u": nodes[i], "v": nodes[j], "cap": 1 + rng.range(20)}));
            }
        }
    }
    let input = json!({
        "edges": edges_json, "nodes": nodes, "source": nodes[0], "sink": nodes[n - 1],
    });
    let start = Instant::now();
    let out = call(&actions, "graph:max_flow", input).expect("325-node max_flow");
    let elapsed = start.elapsed();
    eprintln!(
        "[timing] graph:max_flow 325 nodes elapsed={:?} max_flow={}",
        elapsed, out["max_flow"]
    );
    assert!(
        elapsed.as_secs_f64() < 10.0,
        "max_flow at 325 nodes took {elapsed:?}, predicted < 1s (debug build generous bound 10s)"
    );
}

// ---------- 输入校验（覆盖各动作共有的校验路径，不逐条重复对拍） ----------

#[test]
fn self_loop_is_rejected() {
    let actions = registry();
    let input = json!({"edges": [{"u": "a", "v": "a", "w": 1}], "nodes": ["a"]});
    let err = call(
        &actions,
        "graph:shortest_path",
        json!({
            "edges": [{"u": "a", "v": "a", "w": 1}], "nodes": ["a"], "source": "a", "target": "a"
        }),
    )
    .unwrap_err();
    assert!(
        err.contains("self-loop"),
        "expected self-loop error, got: {err}"
    );
    let _ = input;
}

#[test]
fn negative_weight_rejected_for_shortest_path() {
    let actions = registry();
    let input = json!({
        "edges": [{"u": "a", "v": "b", "w": -1}], "nodes": ["a", "b"],
        "source": "a", "target": "b",
    });
    let err = call(&actions, "graph:shortest_path", input).unwrap_err();
    assert!(
        err.contains("negative"),
        "expected negative-weight error, got: {err}"
    );
}

#[test]
fn replay_only_rejects_all_graph_actions() {
    let mut actions = ActionRegistry::new();
    jpp::actions::register_all(&mut actions, &jpp::actions::Ctx::default(), true);
    for name in [
        "graph:matching",
        "graph:shortest_path",
        "graph:max_clique",
        "graph:components",
        "graph:set_cover",
        "graph:max_flow",
    ] {
        let err = call(&actions, name, json!({"edges": [], "nodes": []})).unwrap_err();
        assert!(
            err.contains("replay"),
            "{name}: expected replay-guard error, got: {err}"
        );
    }
}

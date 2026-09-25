//! 精确图算法宿主动作 `graph:*`（比赛 R2b，2026-09-25）。
//!
//! 六个动作：`graph:matching`（带权最大匹配，二分图 Hungarian / 一般图精确位掩码 DP）、
//! `graph:shortest_path`（Dijkstra）、`graph:max_clique`（Bron–Kerbosch + 剪枝）、
//! `graph:components`（并查集）、`graph:set_cover`（精确位掩码 DP + 贪心）、`graph:max_flow`
//! （Dinic）。全部纯函数、可逆、`taint_out: inherit`、成本 0（附注 §五）。设计决定、范围裁剪、
//! JSON 契约详见 `地基/过程记录/工程-比赛R2b.md`；六项隐性知识见 `actions/mod.rs` 头注与本文件
//! 各函数上的局部注记。
//!
//! JSON 契约要点（各函数的输入/输出细节见函数头注）：节点 id 统一转字符串（输入可为 JSON 字符串
//! 或数字）；边 `{"u", "v", "w"}`（`max_flow` 用 `"cap"`）；自环一律报错；每个动作的输出都带
//! `edge_index`（用到的边在输入 `edges` 数组里的下标），供 L2 `on_graph`/`interval` 按下标把算法
//! 产物映回边元素做 `compose(exits, "all")` 与 `differs`（附注 §3.3）。
//!
//! rebase 到 main（C-1b 合入后）接入库轨的统一动作表（`super::HostAction`/`BUILTIN_ACTIONS`）：
//! 本文件只留六个 `pub(super) fn <name>(_: &Ctx, args: &[Value]) -> Result<Value, String>` 包装
//! （与 `actions/io.rs` 同一写法），登记行搬进 `mod.rs::BUILTIN_ACTIONS`；原先本模块自带的
//! `register`/`ALGORITHMS`/过渡期常量表已删（不再需要独立注册入口）。

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use serde_json::{Value as Json, json};

use super::Ctx;
use crate::interp::json_to_value;
use crate::value::Value;

/// 六个动作共用的包装：取唯一实参、转 JSON、跑对应算法、转回 `Value`（附注 §三·3.3）。
fn wrap(
    args: &[Value],
    name: &str,
    run: fn(&Json) -> Result<Json, String>,
) -> Result<Value, String> {
    let [input] = args else {
        return Err(format!(
            "{name} expects exactly one JSON argument: do(\"{name}\", [graph], seq)"
        ));
    };
    let out = run(&input.to_json())?;
    Ok(json_to_value(&out))
}

pub(super) fn matching(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:matching", run_matching)
}
pub(super) fn shortest_path(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:shortest_path", run_shortest_path)
}
pub(super) fn max_clique(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:max_clique", run_max_clique)
}
pub(super) fn components(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:components", run_components)
}
pub(super) fn set_cover(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:set_cover", run_set_cover)
}
pub(super) fn max_flow(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:max_flow", run_max_flow)
}

// ---------- 输入解析共用 ----------

/// 节点 id 允许 JSON 字符串或数字，内部一律转字符串比较（过程记录 §四）。
fn node_id(j: &Json) -> Result<String, String> {
    match j {
        Json::String(s) => Ok(s.clone()),
        Json::Number(n) => Ok(n.to_string()),
        other => Err(format!(
            "expected a node id (string or number), got {other}"
        )),
    }
}

fn get_arr<'a>(input: &'a Json, key: &str) -> Result<&'a Vec<Json>, String> {
    input
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("missing or non-array field '{key}'"))
}

fn get_bool(input: &Json, key: &str, default: bool) -> bool {
    input.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn get_node_field(input: &Json, key: &str) -> Result<String, String> {
    match input.get(key) {
        Some(v) => node_id(v),
        None => Err(format!("missing required field '{key}'")),
    }
}

struct RawEdge {
    u: String,
    v: String,
    w: f64,
    index: usize,
}

/// 解析 `edges`（`weight_key` 是权重字段名——`matching`/`shortest_path` 用 `"w"`，`max_flow` 用
/// `"cap"`；`max_clique`/`components` 也走这条但权重被忽略）。自环一律报错（过程记录 §四）；
/// `allow_negative=false` 时负权在解析期就拒绝，不留给算法内部悄悄吞掉。
fn parse_edges(
    input: &Json,
    weight_key: &str,
    default_w: f64,
    allow_negative: bool,
) -> Result<Vec<RawEdge>, String> {
    let arr = get_arr(input, "edges")?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, e) in arr.iter().enumerate() {
        let u = node_id(
            e.get("u")
                .ok_or_else(|| format!("edges[{i}] missing 'u'"))?,
        )?;
        let v = node_id(
            e.get("v")
                .ok_or_else(|| format!("edges[{i}] missing 'v'"))?,
        )?;
        if u == v {
            return Err(format!(
                "edges[{i}] is a self-loop ('{u}' == '{v}'), which graph:* actions do not support"
            ));
        }
        let w = match e.get(weight_key) {
            Some(Json::Number(n)) => n
                .as_f64()
                .ok_or_else(|| format!("edges[{i}].{weight_key} is not a finite number"))?,
            Some(other) => {
                return Err(format!(
                    "edges[{i}].{weight_key} must be a number, got {other}"
                ));
            }
            None => default_w,
        };
        if w.is_nan() {
            return Err(format!("edges[{i}].{weight_key} is NaN"));
        }
        if !allow_negative && w < 0.0 {
            return Err(format!(
                "edges[{i}].{weight_key} = {w} is negative; this action requires non-negative values"
            ));
        }
        out.push(RawEdge { u, v, w, index: i });
    }
    Ok(out)
}

/// 节点集合：`nodes` 显式给出则校验边端点都在表里（防手误漏写），否则按边出现顺序推断。
fn collect_nodes(input: &Json, edges: &[RawEdge]) -> Result<Vec<String>, String> {
    if let Some(Json::Array(ns)) = input.get("nodes") {
        let declared: Vec<String> = ns.iter().map(node_id).collect::<Result<_, _>>()?;
        let set: HashSet<&str> = declared.iter().map(|s| s.as_str()).collect();
        for e in edges {
            if !set.contains(e.u.as_str()) {
                return Err(format!(
                    "edges[{}] references node '{}' which is not in 'nodes'",
                    e.index, e.u
                ));
            }
            if !set.contains(e.v.as_str()) {
                return Err(format!(
                    "edges[{}] references node '{}' which is not in 'nodes'",
                    e.index, e.v
                ));
            }
        }
        Ok(declared)
    } else {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for e in edges {
            if seen.insert(e.u.clone()) {
                out.push(e.u.clone());
            }
            if seen.insert(e.v.clone()) {
                out.push(e.v.clone());
            }
        }
        Ok(out)
    }
}

// ---------- graph:matching ----------
//
// 输入：`{"edges":[{"u","v","w"}], "nodes":[...]?, "bipartite": bool?(默认false),
//        "parts": [[左...],[右...]]?(bipartite=true 时必给), "size": int?(截断)}`。
// 输出（两种模式共用形状）：`{"algo":"matching","mode":"bipartite"|"general_exact",
//        "pairs":[{"u","v","w","edge_index"}],"total_weight","matched_nodes","unmatched_nodes",
//        "exact":true,"node_count"}`。
//
// 范围决定（过程记录 §三·3.1）：二分图模式用 Hungarian（0-权哑元规约），规模到 325 经过实测；
// 一般图模式用精确位掩码 DP，上限 n≤20（超界报错，不近似）。`size` 只做「按权降序截断」，不强制
// 凑够基数（问题清单 #1）。

fn run_matching(input: &Json) -> Result<Json, String> {
    if get_bool(input, "bipartite", false) {
        run_matching_bipartite(input)
    } else {
        run_matching_general(input)
    }
}

fn size_cap(input: &Json) -> Option<usize> {
    input
        .get("size")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
}

fn run_matching_bipartite(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, false)?;
    let parts = input
        .get("parts")
        .and_then(|p| p.as_array())
        .ok_or_else(|| "bipartite matching requires 'parts': [[left...],[right...]]".to_string())?;
    if parts.len() != 2 {
        return Err("'parts' must have exactly two node-id lists".into());
    }
    let left: Vec<String> = parts[0]
        .as_array()
        .ok_or_else(|| "'parts[0]' must be an array".to_string())?
        .iter()
        .map(node_id)
        .collect::<Result<_, _>>()?;
    let right: Vec<String> = parts[1]
        .as_array()
        .ok_or_else(|| "'parts[1]' must be an array".to_string())?
        .iter()
        .map(node_id)
        .collect::<Result<_, _>>()?;
    let li: HashMap<&str, usize> = left
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let ri: HashMap<&str, usize> = right
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    for l in &left {
        if ri.contains_key(l.as_str()) {
            return Err(format!("node '{l}' listed in both parts"));
        }
    }

    // 同一对 (left, right) 若有多条边只保留权最大的一条（记原下标供输出回指）。
    let mut best: HashMap<(usize, usize), (f64, usize)> = HashMap::new();
    for e in &edges {
        let pair = if let (Some(&il), Some(&ir)) = (li.get(e.u.as_str()), ri.get(e.v.as_str())) {
            (il, ir)
        } else if let (Some(&il), Some(&ir)) = (li.get(e.v.as_str()), ri.get(e.u.as_str())) {
            (il, ir)
        } else {
            return Err(format!(
                "edges[{}] ('{}' - '{}') does not connect the two parts",
                e.index, e.u, e.v
            ));
        };
        best.entry(pair)
            .and_modify(|cur| {
                if e.w > cur.0 {
                    *cur = (e.w, e.index);
                }
            })
            .or_insert((e.w, e.index));
    }

    let n = left.len();
    let m = right.len();
    let size = n.max(m);
    let mut cost = vec![vec![0.0f64; size + 1]; size + 1];
    for (&(l, r), &(w, _)) in &best {
        cost[l + 1][r + 1] = -w;
    }
    let assignment = hungarian(&cost, size);

    let mut pairs: Vec<(usize, usize, f64, usize)> = Vec::new();
    for (j, &i) in assignment.iter().enumerate().skip(1).take(size) {
        if i == 0 {
            continue;
        }
        let (row, col) = (i - 1, j - 1);
        if row < n
            && col < m
            && let Some(&(w, orig_idx)) = best.get(&(row, col))
            && w > 0.0
        {
            pairs.push((row, col, w, orig_idx));
        }
    }
    if let Some(cap) = size_cap(input) {
        pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));
        pairs.truncate(cap);
    }
    let total_weight: f64 = pairs.iter().map(|p| p.2).sum();
    let pairs_json: Vec<Json> = pairs
        .iter()
        .map(|(row, col, w, idx)| json!({"u": left[*row], "v": right[*col], "w": w, "edge_index": idx}))
        .collect();
    let (matched_nodes, unmatched_nodes) = matched_unmatched(
        left.iter().chain(right.iter()),
        pairs
            .iter()
            .flat_map(|(row, col, _, _)| [left[*row].as_str(), right[*col].as_str()]),
    );

    Ok(json!({
        "algo": "matching",
        "mode": "bipartite",
        "pairs": pairs_json,
        "total_weight": total_weight,
        "matched_nodes": matched_nodes,
        "unmatched_nodes": unmatched_nodes,
        "exact": true,
        "node_count": n + m,
    }))
}

fn matched_unmatched<'a>(
    all: impl Iterator<Item = &'a String>,
    matched: impl Iterator<Item = &'a str>,
) -> (Vec<String>, Vec<String>) {
    let matched_set: HashSet<&str> = matched.collect();
    let mut matched_nodes: Vec<String> = matched_set.iter().map(|s| s.to_string()).collect();
    matched_nodes.sort();
    let mut unmatched: Vec<String> = all
        .filter(|n| !matched_set.contains(n.as_str()))
        .cloned()
        .collect();
    unmatched.sort();
    unmatched.dedup();
    (matched_nodes, unmatched)
}

/// 方阵最小费用完美匹配（Kuhn 算法 + 势函数，O(n^3)；cp-algorithms「Assignment problem, Hungarian
/// algorithm」同型写法）。`cost` 是 (n+1)×(n+1) 的 1-索引矩阵（第 0 行/列不用）。恒返回完美匹配
/// （矩阵已经补成方阵，所有格子代价有限）：`p[j]` = 匹配到列 j 的行号（1..=n）。
fn hungarian(cost: &[Vec<f64>], n: usize) -> Vec<usize> {
    const INF: f64 = f64::INFINITY;
    let mut u = vec![0.0f64; n + 1];
    let mut v = vec![0.0f64; n + 1];
    let mut p = vec![0usize; n + 1];
    let mut way = vec![0usize; n + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![INF; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = INF;
            let mut j1 = 0usize;
            for j in 1..=n {
                if !used[j] {
                    let cur = cost[i0][j] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    p
}

const GENERAL_MATCHING_MAX_N: usize = 20;

fn run_matching_general(input: &Json) -> Result<Json, String> {
    let edges_raw = parse_edges(input, "w", 1.0, true)?;
    let nodes = collect_nodes(input, &edges_raw)?;
    let n = nodes.len();
    if n > GENERAL_MATCHING_MAX_N {
        return Err(format!(
            "graph:matching general (non-bipartite) mode supports at most {GENERAL_MATCHING_MAX_N} \
             nodes (exact bitmask DP); got {n} nodes. Use \"bipartite\": true with \"parts\" for \
             larger graphs (Hungarian; tested to 325 nodes, see 工程-比赛R2b.md §三·3.1)."
        ));
    }
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut best: HashMap<(usize, usize), (f64, usize)> = HashMap::new();
    for e in &edges_raw {
        let a = *idx
            .get(e.u.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.u))?;
        let b = *idx
            .get(e.v.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.v))?;
        let key = (a.min(b), a.max(b));
        best.entry(key)
            .and_modify(|cur| {
                if e.w > cur.0 {
                    *cur = (e.w, e.index);
                }
            })
            .or_insert((e.w, e.index));
    }

    let size = 1usize << n;
    let mut dp = vec![0.0f64; size];
    for mask in 1..size {
        let i = mask.trailing_zeros() as usize;
        let without_i = mask & !(1 << i);
        let mut best_val = dp[without_i];
        for j in 0..n {
            if j == i {
                continue;
            }
            let jb = 1usize << j;
            if mask & jb == 0 {
                continue;
            }
            if let Some(&(w, _)) = best.get(&(i.min(j), i.max(j))) {
                let rest = without_i & !jb;
                let cand = dp[rest] + w;
                if cand > best_val {
                    best_val = cand;
                }
            }
        }
        dp[mask] = best_val;
    }

    let full = size - 1;
    let mut mask = full;
    let mut pairs: Vec<(usize, usize, f64, usize)> = Vec::new();
    while mask != 0 {
        let i = mask.trailing_zeros() as usize;
        let without_i = mask & !(1 << i);
        if (dp[without_i] - dp[mask]).abs() < 1e-9 {
            mask = without_i;
            continue;
        }
        let mut found: Option<(usize, f64, usize, usize)> = None;
        for j in 0..n {
            if j == i {
                continue;
            }
            let jb = 1usize << j;
            if mask & jb == 0 {
                continue;
            }
            if let Some(&(w, orig_idx)) = best.get(&(i.min(j), i.max(j))) {
                let rest = without_i & !jb;
                if (dp[rest] + w - dp[mask]).abs() < 1e-9 {
                    found = Some((j, w, orig_idx, rest));
                    break;
                }
            }
        }
        let (j, w, orig_idx, rest) =
            found.expect("bug: general matching DP reconstruction found no valid transition");
        pairs.push((i.min(j), i.max(j), w, orig_idx));
        mask = rest;
    }

    if let Some(cap) = size_cap(input) {
        pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));
        pairs.truncate(cap);
    }
    let total_weight: f64 = pairs.iter().map(|p| p.2).sum();
    let pairs_json: Vec<Json> = pairs
        .iter()
        .map(|(a, b, w, idx)| json!({"u": nodes[*a], "v": nodes[*b], "w": w, "edge_index": idx}))
        .collect();
    let (matched_nodes, unmatched_nodes) = matched_unmatched(
        nodes.iter(),
        pairs
            .iter()
            .flat_map(|(a, b, _, _)| [nodes[*a].as_str(), nodes[*b].as_str()]),
    );

    Ok(json!({
        "algo": "matching",
        "mode": "general_exact",
        "pairs": pairs_json,
        "total_weight": total_weight,
        "matched_nodes": matched_nodes,
        "unmatched_nodes": unmatched_nodes,
        "exact": true,
        "node_count": n,
    }))
}

// ---------- graph:shortest_path ----------
//
// 输入：`{"edges":[{"u","v","w"}], "nodes":[...]?, "directed": bool?(默认false),
//        "source", "target"}`。权重非负（Dijkstra 前提），负权在解析期报错。
// 输出：`{"algo":"shortest_path","reachable":bool,"distance":f64|null,"path":[...],
//        "path_edges":[{"u","v","w","edge_index"}],"exact":true}`。

#[derive(PartialEq)]
struct OrderedF64(f64);
impl Eq for OrderedF64 {}
impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

fn run_shortest_path(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, false)?;
    let nodes = collect_nodes(input, &edges)?;
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let directed = get_bool(input, "directed", false);
    let source = get_node_field(input, "source")?;
    let target = get_node_field(input, "target")?;
    let s = *idx
        .get(source.as_str())
        .ok_or_else(|| format!("source '{source}' is not a node"))?;
    let t = *idx
        .get(target.as_str())
        .ok_or_else(|| format!("target '{target}' is not a node"))?;

    let n = nodes.len();
    let mut adj: Vec<Vec<(usize, f64, usize)>> = vec![Vec::new(); n];
    for e in &edges {
        let a = idx[e.u.as_str()];
        let b = idx[e.v.as_str()];
        adj[a].push((b, e.w, e.index));
        if !directed {
            adj[b].push((a, e.w, e.index));
        }
    }

    let mut dist = vec![f64::INFINITY; n];
    let mut prev: Vec<Option<(usize, usize)>> = vec![None; n];
    dist[s] = 0.0;
    let mut heap: BinaryHeap<std::cmp::Reverse<(OrderedF64, usize)>> = BinaryHeap::new();
    heap.push(std::cmp::Reverse((OrderedF64(0.0), s)));
    let mut visited = vec![false; n];
    while let Some(std::cmp::Reverse((OrderedF64(d), u))) = heap.pop() {
        if visited[u] {
            continue;
        }
        visited[u] = true;
        if u == t {
            break;
        }
        for &(v2, w, ei) in &adj[u] {
            let nd = d + w;
            if nd < dist[v2] - 1e-12 {
                dist[v2] = nd;
                prev[v2] = Some((u, ei));
                heap.push(std::cmp::Reverse((OrderedF64(nd), v2)));
            }
        }
    }

    if dist[t].is_infinite() {
        return Ok(json!({
            "algo": "shortest_path", "reachable": false, "distance": Json::Null,
            "path": Json::Array(vec![]), "path_edges": Json::Array(vec![]), "exact": true,
        }));
    }
    let mut path_nodes = vec![t];
    let mut path_edges_idx = Vec::new();
    let mut cur = t;
    while let Some((p, ei)) = prev[cur] {
        path_edges_idx.push(ei);
        path_nodes.push(p);
        cur = p;
    }
    path_nodes.reverse();
    path_edges_idx.reverse();
    let path_ids: Vec<String> = path_nodes.iter().map(|&i| nodes[i].clone()).collect();
    let path_edges_json: Vec<Json> = path_edges_idx
        .iter()
        .map(|&ei| {
            let e = &edges[ei];
            json!({"u": e.u, "v": e.v, "w": e.w, "edge_index": ei})
        })
        .collect();

    Ok(json!({
        "algo": "shortest_path",
        "reachable": true,
        "distance": dist[t],
        "path": path_ids,
        "path_edges": path_edges_json,
        "exact": true,
    }))
}

// ---------- graph:max_clique ----------
//
// 输入：`{"edges":[{"u","v"}], "nodes":[...]?}`（无向、不带权；`w` 若给忽略）。
// `n ≤ 60`（附注 §五原文界，超界报错）。输出：
// `{"algo":"max_clique","clique":[...],"size","edge_indices":[...],"exact":true,"node_count"}`。

const MAX_CLIQUE_MAX_N: usize = 60;

fn run_max_clique(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, true)?;
    let nodes = collect_nodes(input, &edges)?;
    let n = nodes.len();
    if n > MAX_CLIQUE_MAX_N {
        return Err(format!(
            "graph:max_clique supports at most {MAX_CLIQUE_MAX_N} nodes (branch and bound over 2^n \
             subsets); got {n}"
        ));
    }
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut adj = vec![0u64; n.max(1)];
    let mut edge_lookup: HashMap<(usize, usize), usize> = HashMap::new();
    for e in &edges {
        let a = *idx
            .get(e.u.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.u))?;
        let b = *idx
            .get(e.v.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.v))?;
        adj[a] |= 1u64 << b;
        adj[b] |= 1u64 << a;
        edge_lookup.entry((a.min(b), a.max(b))).or_insert(e.index);
    }
    let full: u64 = if n == 0 {
        0
    } else if n == 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    };
    let mut best_r: u64 = 0;
    let mut best_size: u32 = 0;
    bron_kerbosch(0, full, 0, &adj, &mut best_r, &mut best_size);

    let mut clique: Vec<usize> = (0..n).filter(|&i| best_r & (1 << i) != 0).collect();
    clique.sort_unstable();
    let mut edge_indices = Vec::new();
    for a_pos in 0..clique.len() {
        for b_pos in (a_pos + 1)..clique.len() {
            let (a, b) = (clique[a_pos], clique[b_pos]);
            if let Some(&ei) = edge_lookup.get(&(a.min(b), a.max(b))) {
                edge_indices.push(ei);
            }
        }
    }
    let clique_ids: Vec<String> = clique.iter().map(|&i| nodes[i].clone()).collect();

    Ok(json!({
        "algo": "max_clique",
        "clique": clique_ids,
        "size": clique.len(),
        "edge_indices": edge_indices,
        "exact": true,
        "node_count": n,
    }))
}

/// Bron–Kerbosch with pivoting，带「当前团 + 候选数 ≤ 已知最优即剪」的分支定界（派单要求「小规模
/// 分支定界」）。`r`/`p`/`x` 是标准三个位集：当前团、候选、已排除。
fn bron_kerbosch(r: u64, p: u64, x: u64, adj: &[u64], best_r: &mut u64, best_size: &mut u32) {
    if p == 0 && x == 0 {
        let sz = r.count_ones();
        if sz > *best_size {
            *best_size = sz;
            *best_r = r;
        }
        return;
    }
    if r.count_ones() + p.count_ones() <= *best_size {
        return;
    }
    let px = p | x;
    let mut pivot = px.trailing_zeros() as usize;
    let mut best_count = -1i32;
    let mut scan = px;
    while scan != 0 {
        let u = scan.trailing_zeros() as usize;
        let cnt = (p & adj[u]).count_ones() as i32;
        if cnt > best_count {
            best_count = cnt;
            pivot = u;
        }
        scan &= scan - 1;
    }
    let mut candidates = p & !adj[pivot];
    let mut pp = p;
    let mut xx = x;
    while candidates != 0 {
        let v = candidates.trailing_zeros() as usize;
        let vb = 1u64 << v;
        bron_kerbosch(r | vb, pp & adj[v], xx & adj[v], adj, best_r, best_size);
        pp &= !vb;
        xx |= vb;
        candidates &= !vb;
    }
}

// ---------- graph:components ----------
//
// 输入：`{"edges":[{"u","v"}], "nodes":[...]?}`（权重忽略）。输出：
// `{"algo":"components","components":[[...],[...]],"count","exact":true,"node_count"}`
// （每个分量成员排序，分量按首成员排序，确定性输出供金样比较）。

fn find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        parent[x] = find(parent, parent[x]);
    }
    parent[x]
}

fn union(parent: &mut [usize], rank: &mut [u32], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra == rb {
        return;
    }
    match rank[ra].cmp(&rank[rb]) {
        Ordering::Less => parent[ra] = rb,
        Ordering::Greater => parent[rb] = ra,
        Ordering::Equal => {
            parent[rb] = ra;
            rank[ra] += 1;
        }
    }
}

fn run_components(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, true)?;
    let nodes = collect_nodes(input, &edges)?;
    let n = nodes.len();
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank = vec![0u32; n];
    for e in &edges {
        let a = *idx
            .get(e.u.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.u))?;
        let b = *idx
            .get(e.v.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.v))?;
        union(&mut parent, &mut rank, a, b);
    }
    let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
    for (i, node) in nodes.iter().enumerate().take(n) {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(node.clone());
    }
    let mut components: Vec<Vec<String>> = groups.into_values().collect();
    for c in &mut components {
        c.sort();
    }
    components.sort_by(|a, b| a.first().cmp(&b.first()));
    let count = components.len();

    Ok(json!({
        "algo": "components",
        "components": components,
        "count": count,
        "exact": true,
        "node_count": n,
    }))
}

// ---------- graph:set_cover ----------
//
// 输入：`{"universe":[...], "sets":[{"id":Text?,"elements":[...],"cost":f64?(默认1)}]}`。
// `|universe| ≤ 20` 走精确位掩码 DP，否则贪心（附注 §五原文界）。输出：
// `{"algo":"set_cover","chosen":[...],"total_cost","exact":bool,"covers_universe":bool,"universe_size"}`。

struct SetIn {
    id: String,
    elems: Vec<usize>,
    cost: f64,
}

const SET_COVER_EXACT_MAX: usize = 20;

fn run_set_cover(input: &Json) -> Result<Json, String> {
    let universe = get_arr(input, "universe")?
        .iter()
        .map(node_id)
        .collect::<Result<Vec<_>, _>>()?;
    let u_idx: HashMap<&str, usize> = universe
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    if u_idx.len() != universe.len() {
        return Err("'universe' contains duplicate elements".into());
    }
    let sets_arr = get_arr(input, "sets")?;
    let mut sets = Vec::with_capacity(sets_arr.len());
    for (i, s) in sets_arr.iter().enumerate() {
        let id = match s.get("id") {
            Some(v) => node_id(v)?,
            None => i.to_string(),
        };
        let elems_json = s
            .get("elements")
            .and_then(|e| e.as_array())
            .ok_or_else(|| format!("sets[{i}] missing 'elements' array"))?;
        let mut elems = Vec::new();
        for e in elems_json {
            let nid = node_id(e)?;
            if let Some(&ui) = u_idx.get(nid.as_str())
                && !elems.contains(&ui)
            {
                elems.push(ui);
            }
            // 不在 universe 里的元素静默忽略：它们无助于覆盖 universe，不是错误。
        }
        let cost = match s.get("cost") {
            Some(Json::Number(n)) => n
                .as_f64()
                .ok_or_else(|| format!("sets[{i}].cost is not a finite number"))?,
            Some(other) => return Err(format!("sets[{i}].cost must be a number, got {other}")),
            None => 1.0,
        };
        if cost < 0.0 {
            return Err(format!(
                "sets[{i}].cost = {cost} is negative; set cover requires non-negative costs"
            ));
        }
        sets.push(SetIn { id, elems, cost });
    }

    if universe.len() <= SET_COVER_EXACT_MAX {
        set_cover_exact(&universe, &sets)
    } else {
        set_cover_greedy(&universe, &sets)
    }
}

fn set_cover_exact(universe: &[String], sets: &[SetIn]) -> Result<Json, String> {
    let u = universe.len();
    let full: u32 = if u == 0 { 0 } else { ((1u64 << u) - 1) as u32 };
    let masks: Vec<u32> = sets
        .iter()
        .map(|s| s.elems.iter().fold(0u32, |m, &e| m | (1 << e)))
        .collect();
    let size = 1usize << u;
    const INF: f64 = f64::INFINITY;
    let mut dp = vec![INF; size];
    dp[0] = 0.0;
    let mut choice: Vec<Option<(usize, usize)>> = vec![None; size];
    for mask in 0..size {
        if dp[mask].is_infinite() {
            continue;
        }
        let m32 = mask as u32;
        if m32 == full {
            continue;
        }
        let uncovered = (!m32) & full;
        let bit = uncovered.trailing_zeros();
        for (si, s) in sets.iter().enumerate() {
            if masks[si] & (1 << bit) == 0 {
                continue;
            }
            let nm = (m32 | masks[si]) as usize;
            let nc = dp[mask] + s.cost;
            if nc < dp[nm] - 1e-12 {
                dp[nm] = nc;
                choice[nm] = Some((mask, si));
            }
        }
    }
    let full_usize = full as usize;
    if dp[full_usize].is_infinite() {
        return Ok(json!({
            "algo": "set_cover", "chosen": Json::Array(vec![]), "total_cost": Json::Null,
            "exact": true, "covers_universe": false, "universe_size": u,
        }));
    }
    let mut chosen_idx = Vec::new();
    let mut mask = full_usize;
    while mask != 0 {
        let (prev, si) =
            choice[mask].expect("bug: set_cover_exact reconstruction reached unset choice");
        chosen_idx.push(si);
        mask = prev;
    }
    chosen_idx.sort_unstable();
    chosen_idx.dedup();
    let total_cost: f64 = chosen_idx.iter().map(|&i| sets[i].cost).sum();
    let chosen: Vec<String> = chosen_idx.iter().map(|&i| sets[i].id.clone()).collect();

    Ok(json!({
        "algo": "set_cover", "chosen": chosen, "total_cost": total_cost,
        "exact": true, "covers_universe": true, "universe_size": u,
    }))
}

fn set_cover_greedy(universe: &[String], sets: &[SetIn]) -> Result<Json, String> {
    let u = universe.len();
    let mut covered = vec![false; u];
    let mut remaining = u;
    let mut chosen = Vec::new();
    let mut total_cost = 0.0;
    let mut used = vec![false; sets.len()];
    while remaining > 0 {
        let mut best_idx: Option<usize> = None;
        let mut best_ratio = -1.0f64;
        for (i, s) in sets.iter().enumerate() {
            if used[i] {
                continue;
            }
            let new_cover = s.elems.iter().filter(|&&e| !covered[e]).count();
            if new_cover == 0 {
                continue;
            }
            let cost = if s.cost <= 0.0 { 1e-9 } else { s.cost };
            let ratio = new_cover as f64 / cost;
            if ratio > best_ratio {
                best_ratio = ratio;
                best_idx = Some(i);
            }
        }
        let Some(bi) = best_idx else {
            break;
        };
        used[bi] = true;
        for &e in &sets[bi].elems {
            if !covered[e] {
                covered[e] = true;
                remaining -= 1;
            }
        }
        chosen.push(bi);
        total_cost += sets[bi].cost;
    }
    let covers_universe = remaining == 0;
    let chosen_ids: Vec<String> = chosen.iter().map(|&i| sets[i].id.clone()).collect();

    Ok(json!({
        "algo": "set_cover", "chosen": chosen_ids, "total_cost": total_cost,
        "exact": false, "covers_universe": covers_universe, "universe_size": u,
    }))
}

// ---------- graph:max_flow ----------
//
// 输入：`{"edges":[{"u","v","cap"}], "nodes":[...]?, "source", "sink"}`（有向，容量非负）。
// 输出：`{"algo":"max_flow","max_flow":f64,"flow_edges":[{"u","v","flow","edge_index"}],
//        "exact":true,"node_count"}`。Dinic（BFS 分层 + DFS 阻塞流 + 当前弧优化）。

struct FlowEdge {
    from: usize,
    to: usize,
    cap: f64,
    flow: f64,
    orig_index: Option<usize>,
}

struct Dinic {
    graph: Vec<Vec<usize>>,
    edges: Vec<FlowEdge>,
    n: usize,
}

impl Dinic {
    fn new(n: usize) -> Self {
        Dinic {
            graph: vec![Vec::new(); n],
            edges: Vec::new(),
            n,
        }
    }

    fn add_edge(&mut self, u: usize, v: usize, cap: f64, orig_index: Option<usize>) {
        let e1 = self.edges.len();
        self.edges.push(FlowEdge {
            from: u,
            to: v,
            cap,
            flow: 0.0,
            orig_index,
        });
        self.graph[u].push(e1);
        let e2 = self.edges.len();
        self.edges.push(FlowEdge {
            from: v,
            to: u,
            cap: 0.0,
            flow: 0.0,
            orig_index: None,
        });
        self.graph[v].push(e2);
    }

    fn bfs(&self, s: usize, t: usize, level: &mut [i32]) -> bool {
        level.iter_mut().for_each(|l| *l = -1);
        level[s] = 0;
        let mut q = VecDeque::new();
        q.push_back(s);
        while let Some(u) = q.pop_front() {
            for &ei in &self.graph[u] {
                let e = &self.edges[ei];
                if level[e.to] < 0 && e.cap - e.flow > 1e-9 {
                    level[e.to] = level[u] + 1;
                    q.push_back(e.to);
                }
            }
        }
        level[t] >= 0
    }

    fn dfs(&mut self, u: usize, t: usize, f: f64, level: &[i32], it: &mut [usize]) -> f64 {
        if u == t {
            return f;
        }
        while it[u] < self.graph[u].len() {
            let ei = self.graph[u][it[u]];
            let (to, cap, flow) = {
                let e = &self.edges[ei];
                (e.to, e.cap, e.flow)
            };
            if level[to] == level[u] + 1 && cap - flow > 1e-9 {
                let d = self.dfs(to, t, f.min(cap - flow), level, it);
                if d > 1e-9 {
                    self.edges[ei].flow += d;
                    let rev = ei ^ 1;
                    self.edges[rev].flow -= d;
                    return d;
                }
            }
            it[u] += 1;
        }
        0.0
    }

    fn max_flow(&mut self, s: usize, t: usize) -> f64 {
        let mut flow = 0.0;
        let mut level = vec![-1i32; self.n];
        while self.bfs(s, t, &mut level) {
            let mut it = vec![0usize; self.n];
            loop {
                let f = self.dfs(s, t, f64::INFINITY, &level, &mut it);
                if f <= 1e-9 {
                    break;
                }
                flow += f;
            }
        }
        flow
    }
}

fn run_max_flow(input: &Json) -> Result<Json, String> {
    let edges_raw = parse_edges(input, "cap", 1.0, false)?;
    let nodes = collect_nodes(input, &edges_raw)?;
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let source = get_node_field(input, "source")?;
    let sink = get_node_field(input, "sink")?;
    let s = *idx
        .get(source.as_str())
        .ok_or_else(|| format!("source '{source}' is not a node"))?;
    let t = *idx
        .get(sink.as_str())
        .ok_or_else(|| format!("sink '{sink}' is not a node"))?;
    if s == t {
        return Err("source and sink must be different nodes".into());
    }

    let n = nodes.len();
    let mut din = Dinic::new(n);
    for e in &edges_raw {
        let a = idx[e.u.as_str()];
        let b = idx[e.v.as_str()];
        din.add_edge(a, b, e.w, Some(e.index));
    }
    let flow = din.max_flow(s, t);

    let mut flow_edges_json = Vec::new();
    for e in &din.edges {
        if let Some(orig) = e.orig_index
            && e.flow > 1e-9
        {
            flow_edges_json.push(json!({
                "u": nodes[e.from], "v": nodes[e.to], "flow": e.flow, "edge_index": orig,
            }));
        }
    }

    Ok(json!({
        "algo": "max_flow",
        "max_flow": flow,
        "flow_edges": flow_edges_json,
        "exact": true,
        "node_count": n,
    }))
}

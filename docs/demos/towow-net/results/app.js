// 结果页：计算结束后看「长出了什么」。只消费 接口.md 第二节的事件流，复用 shared/ 的归约器与播放器。
//   ?src=../run/events.jsonl       静态事件文件（默认是开发用的社会事件流）
//   ?sse=../events%3Frun%3D<名>           走 SSE，边收边更新（每秒重画一次）
//   ?ctl=<无痕迹对照的 events.jsonl>       H1 与分布图叠加对照（默认事件流自动配 results-control.jsonl）
//   ?groups=<同类意图分组 json>            事件里的 intent 不带 group 时用（默认事件流自动配 results-groups.json）
//   ?report=<metrics 的 report.json>       H1 主口径（命中预埋目标，F2 生成）；没有时退回按 discovery 事件计算并标明口径
//   #glance | #overview | #plans | #data，#plans&i=<意图>&open=<方案>（默认 #glance：方案一览）
// 页面一律显示角色（node.role），不显示名字；名字只在方案详情的成员表里小字出现一次。
// 所有数字都从事件累计，页面里没有手填的数字；没有数据的图显示「暂无数据」。

import { Store, exitClass, parseJsonl, roleOf } from '../shared/store.js';
import { Player } from '../shared/player.js';
import { createGlance, glyphSvg, glyphIcon } from './glance.js';

const qs = new URLSearchParams(location.search);
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const svgNS = 'http://www.w3.org/2000/svg';

const DEFAULT_SRC = '../run/events.jsonl';
// 静态演示包里真有哪些旁注文件（tools/build-demo 构建时写成 {ctl, groups, report}，值是相对事件文件的文件名或 null）
const BUNDLE_FILES = {"ctl": null, "groups": "events-groups.json", "report": "report.json"};
const SSE = qs.get('sse');
const SRC = SSE ? null : (qs.get('src') || DEFAULT_SRC);

const COLORS = {
  act: '#3ef0b0', ignore: '#5b6b8a', unsure: '#ffb547', pick: '#b58cff', unknown: '#6c7a96', no: '#ff5b6e',
  direct: '#57d4ff', relay: '#c38bff', config: '#ffd27a', coarse: '#7aa7ff', trace: '#ffcf6b',
};
const MEMBER_HUES = ['#57d4ff', '#ffd27a', '#c38bff', '#3ef0b0', '#ff7ad9', '#7aa7ff', '#ffb547'];
const GROUP_HUES = ['#57d4ff', '#c38bff', '#ffd27a', '#ff7ad9', '#3ef0b0', '#7aa7ff', '#ffb547'];
const EXIT_LABEL = { act: '成立', ignore: '无关', unsure: '拿不准', pick: '选项', unknown: '？' };
const RING_OK = new Set(['closed', 'cleared', 'conditional', 'ok', 'stable']);
// 接口第九节：ring.status 只取 cleared / conditional / failed
const RING_LABEL = { cleared: '闭合', conditional: '有条件闭合', failed: '未闭合', closed: '闭合', ok: '闭合', stable: '闭合' };
const ringWord = (st) => RING_LABEL[st] || String(st ?? '');
// give 超过 6 字被引擎截断时带 give_cut（CP1 修订第 4 条）：字后加省略号，悬停说明
const giveHtml = (g, cut) => (g ? `${esc(g)}${cut ? '<span class="cut" title="原句超过 6 字，已截断">…</span>' : ''}` : '—');

// ---------------- 数据源 ----------------

// 在 shared/Store 之外补几张索引：路由、按 about 的判断、构型事件、首个有效构型时的判断数
function augment(store) {
  const A = { routes: new Map(), judgesAbout: new Map(), cfgEvents: new Map(), h1: new Map(), disc: new Map(), groups: new Map(), arm: null, run: null };
  const push = (m, k, v) => { if (!m.has(k)) m.set(k, []); m.get(k).push(v); };
  store.on((ev, ctx) => {
    if (!ev || ev.type === '__reset') { for (const m of Object.values(A)) if (m instanceof Map) m.clear(); A.arm = null; A.run = null; return; }
    if (A.arm == null && ev.arm) A.arm = ev.arm; // 接口第九节：每个事件带 run、arm
    if (A.run == null && ev.run) A.run = ev.run;
    switch (ev.type) {
      case 'intent': if (ev.group) A.groups.set(ev.id, ev.group); break;
      case 'route': push(A.routes, ev.about, ev); break;
      case 'judge': push(A.judgesAbout, ev.about, ev); break;
      case 'config': {
        push(A.cfgEvents, ev.id, ev);
        const root = ctx && ctx.root;
        if (ev.status === 'stable' && root && !A.h1.has(root)) {
          const it = store.intents.get(root);
          if (it) A.h1.set(root, { judges: it.judges, ms: (ev.t ?? 0) - (it.t ?? 0), config: ev.id, via: 'stable' });
        }
        break;
      }
      case 'discovery': {
        const root = store.rootIntent(ev.about) ?? ev.about;
        if (root != null && Number.isFinite(Number(ev.judgments_at))) A.disc.set(root, ev);
        break;
      }
      default:
    }
  });
  store.aug = A;
  return store;
}

function makeSource() {
  const store = augment(new Store());
  return { store, player: new Player(store), url: null, ok: false };
}
const MAIN = makeSource();
const CTL = makeSource();
let GROUPS = new Map();
let REPORT = null; // metrics 的 report.json 里 H1 三个口径（parseReport 的结果）
let META = null; // 这次运行的 meta.json（判断线等参数），静态文件时从事件文件同目录取

// ---------------- H1 口径 ----------------
// 三个口径同等显眼（CP1 修订「主会话追加要求」）：
//   命中目标（预注册补充六，冒烟后改定）：h1_hit.per_arm.<臂>.median_by_occurrence / _capped / hit_rate_by_occurrence
//   整波、名次（预注册原定）：h1.per_arm.<臂>.median_by_occurrence.<judgments_at|judgments_rank> / _capped / found_rate_by_occurrence
// 都由 metrics/report.py 算好，页面只读不重算（命中目标要对照 stories.json，只能在评估端算）。
// 中位数两种（F2 口径裁定）：只算命中/找到的，和把没命中的按该意图全部判断数封顶计入的；图上画前者，
// 刻度下标命中率，标题旁括号里是封顶版的降幅。applicable:false（如开发意图没有组）是「不适用」，不是数据不足。
const H1_ARMS = ['trace', 'control', 'scale150'];
const H1_METRICS = [
  { key: 'hit', lab: '命中目标', tag: '*', rateWord: '命中' },
  { key: 'judgments_at', lab: '整波', tag: '', rateWord: '找到' },
  { key: 'judgments_rank', lab: '名次', tag: '', rateWord: '找到' },
];
const numOrNull = (x) => (x == null || !Number.isFinite(Number(x)) ? null : Number(x));
function parseReport(j) {
  if (!j || typeof j !== 'object') return null;
  const hh = j.h1_hit && typeof j.h1_hit === 'object' ? j.h1_hit : null;
  const h1 = j.h1 && typeof j.h1 === 'object' ? j.h1 : null;
  if (!hh && !h1 && !j.h3_h2 && !j.diag_main) return null;
  const out = { metrics: {}, runs: j.run || null, criterion: (hh && hh.criterion) || '' };
  {
    const arms = {}; const capped = {}; const rates = {};
    let na = '';
    for (const a of H1_ARMS) {
      const x = hh && hh.per_arm && hh.per_arm[a];
      if (!x) continue;
      if (a === 'trace' && x.available && x.applicable === false) na = x.reason || '口径不适用';
      if (!x.applicable || !Array.isArray(x.median_by_occurrence)) continue;
      arms[a] = x.median_by_occurrence.map(numOrNull);
      if (Array.isArray(x.median_by_occurrence_capped)) capped[a] = x.median_by_occurrence_capped.map(numOrNull);
      if (Array.isArray(x.hit_rate_by_occurrence)) rates[a] = x.hit_rate_by_occurrence;
    }
    out.metrics.hit = { arms, capped, rates, verdict: (hh && hh.verdicts) || null, na };
  }
  for (const m of ['judgments_at', 'judgments_rank']) {
    const arms = {}; const capped = {}; const rates = {};
    for (const a of H1_ARMS) {
      const x = h1 && h1.per_arm && h1.per_arm[a];
      if (!x || !x.available) continue;
      const v = x.median_by_occurrence && x.median_by_occurrence[m];
      if (Array.isArray(v)) arms[a] = v.map(numOrNull);
      const c = x.median_by_occurrence_capped && x.median_by_occurrence_capped[m];
      if (Array.isArray(c)) capped[a] = c.map(numOrNull);
      if (Array.isArray(x.found_rate_by_occurrence)) rates[a] = x.found_rate_by_occurrence;
    }
    out.metrics[m] = { arms, capped, rates, verdict: (h1 && h1.verdicts && h1.verdicts[m]) || null, na: '' };
  }
  // 三个口径都要有 trace 臂的真实数（至少一个点）才算「有 H1 数据」
  const hasPts = (arr) => Array.isArray(arr) && arr.some((v) => v != null);
  out.any = Object.values(out.metrics).some((x) => hasPts(x.arms.trace));
  const acc = j.h1_trace_accuracy && j.h1_trace_accuracy.trace;
  out.traceAcc = acc && acc.available ? acc : null;
  out.h3 = j.h3_h2 && j.h3_h2.reviewers ? j.h3_h2 : null; // H3/H2 正式盲评（baselines 产出，metrics 摘录）
  out.diag = j.diag_main && j.diag_main.member_frequency ? j.diag_main : null; // 主运行诊断（事后分析）
  return out;
}

// 第 5 次比第 1 次的降幅（预注册写法：(第 1 次 − 第 5 次) / 第 1 次）
function dropOf(meds) {
  if (!Array.isArray(meds) || !meds.length) return null;
  const a = meds[0]; const b = meds[meds.length - 1];
  if (a == null || b == null || !(a > 0)) return null;
  return (a - b) / a;
}
// 预注册判定（report.json 里的 overall）：成立 / 不成立（推翻）/ 预测未达 / 数据不足
const VERDICT_WORD = { holds: '成立', refuted: '不成立（触发推翻）', inconclusive: '不成立（预测未达，未触发推翻）', insufficient: '数据不足' };
const VERDICT_COL = { holds: '#3ef0b0', refuted: '#ff5b6e', inconclusive: '#ffb547', insufficient: '#6c7a96' };
function verdictOf(key, capped) {
  const x = REPORT && REPORT.metrics[key];
  const v = x && x.verdict;
  if (!v) return null;
  const o = key === 'hit' ? v[capped ? 'overall_capped' : 'overall'] : v[capped ? 'overall_capped' : 'overall'];
  return o && o.status ? o.status : null;
}
const pctTxt = (d) => (d == null ? '—' : `${d >= 0 ? '−' : '+'}${Math.abs(Math.round(d * 100))}%`);

// 这份运行能不能画 H1：只有 H1 的三个臂（trace / control / scale150）算，主运行（main / live）不算（接口第九节末条）
const isH1Arm = (src) => H1_ARMS.includes(src.store.aug.arm);

// 结果页上 H1 用哪一套：{kind: 'report'|'discovery'|'na'|'none', tag, …}
function h1View() {
  if (REPORT && REPORT.any) return { kind: 'report', tag: '', metrics: REPORT.metrics };
  if (REPORT && REPORT.metrics.hit.na && !REPORT.any) return { kind: 'na', tag: '命中目标', reason: REPORT.metrics.hit.na };
  if (REPORT) return { kind: 'none', tag: '' }; // 报告在，但 H1 三个臂还没有数
  if (MAIN.ok && !isH1Arm(MAIN) && MAIN.store.aug.arm) return { kind: 'na', tag: '' };
  const tr = h1Series(MAIN);
  if (!tr) return { kind: 'none', tag: '' };
  const ct = CTL.ok && isH1Arm(CTL) ? h1Series(CTL) : null;
  return { kind: 'discovery', tag: '次要口径', med: tr.med, series: tr, ctlSeries: ct };
}

// 环清算的结局：每个构型取最后一次 ring 事件（重跑的环不重复计）
function ringStats(store) {
  const st = { total: 0, cleared: 0, conditional: 0, failed: 0 };
  for (const c of store.configs.values()) {
    const r = c.ring;
    if (!r) continue;
    st.total++;
    const k = r.status === 'conditional' ? 'conditional' : (RING_OK.has(r.status) ? 'cleared' : 'failed');
    st[k]++;
  }
  return st;
}

function siblings(url, suffixes) {
  if (!url) return [];
  const m = url.match(/^(.*\/)?([^/?#]+?)(\.jsonl)?([?#].*)?$/);
  if (!m) return [];
  const dir = m[1] || '';
  const stem = m[2];
  return suffixes.map((s) => (s.startsWith('/') ? dir + s.slice(1) : dir + stem + s));
}

async function tryFetch(url, kind) {
  try {
    const res = await fetch(url, { cache: 'no-store' });
    if (!res.ok) return null;
    return kind === 'json' ? await res.json() : await res.text();
  } catch { return null; }
}

function parseGroups(j) {
  const m = new Map();
  if (!j) return m;
  const arr = Array.isArray(j) ? j : (Array.isArray(j.intents) ? j.intents : null);
  if (arr) {
    for (const x of arr) if (x && x.id != null && x.group) m.set(x.id, x.group);
  } else if (typeof j === 'object') {
    for (const [k, v] of Object.entries(j)) if (typeof v === 'string') m.set(k, v);
  }
  return m;
}

const groupOf = (src, iid) => src.store.aug.groups.get(iid) ?? GROUPS.get(iid) ?? null;

function h1Of(store, iid) {
  const d = store.aug.disc.get(iid);
  if (d) return { judges: Number(d.judgments_at), ms: Number(d.ms_at), via: 'discovery' };
  return store.aug.h1.get(iid) || null;
}

// ---------------- 小工具 ----------------

const fmtN = (n) => Number(n || 0).toLocaleString('en-US');
function fmtUsd(x) {
  x = Number(x) || 0;
  if (x >= 1) return `$${x.toFixed(2)}`;
  if (x >= 0.01) return `$${x.toFixed(3)}`;
  if (x === 0) return '$0';
  if (x < 0.001) return `$${x.toPrecision(2)}`;
  return `$${x.toFixed(4)}`;
}
function fmtDur(ms) {
  const s = Math.max(0, Number(ms) || 0) / 1000;
  if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)}s`;
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s - m * 60)).padStart(2, '0')}`;
}
function fmtMs(ms) {
  ms = Number(ms) || 0;
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return fmtDur(ms);
}
const median = (a) => {
  const v = a.filter((x) => Number.isFinite(x)).sort((x, y) => x - y);
  if (!v.length) return null;
  const k = v.length >> 1;
  return v.length % 2 ? v[k] : (v[k - 1] + v[k]) / 2;
};
const mean = (a) => { const v = a.filter((x) => Number.isFinite(x)); return v.length ? v.reduce((s, x) => s + x, 0) / v.length : null; };

function readingOf(j) {
  if (!j) return null;
  const r = j.reading || {};
  if (Number.isFinite(Number(r.p))) return { p: Number(r.p) };
  if (Array.isArray(r.probs) && r.probs.length) {
    const probs = r.probs.map(Number);
    const k = probs.indexOf(Math.max(...probs));
    // over：接口第九节是选项个数（数字）；旧数据是选项名数组。数字时按序号标 1…n
    const over = Array.isArray(r.over) ? r.over : probs.map((_, i) => `${i + 1}`);
    return { p: probs[k], probs, over, top: k };
  }
  if (Number.isFinite(Number(j.p))) return { p: Number(j.p) };
  return null;
}

function nodeOf(store, id) { return store.nodes.get(id); }
// 称呼一律用角色；缺 role 的旧数据退化为 tags[0] 加 kind（shared/store.js roleOf）
function nameOf(store, id) {
  const n = store.nodes.get(id);
  return n && (!n.placeholder || n.role) ? roleOf(n) : String(id ?? ''); // 网外的人（x:…）是占位节点，但带角色
}
function realNameOf(store, id) {
  const n = store.nodes.get(id);
  return n && !n.placeholder && n.name && n.name !== roleOf(n) ? n.name : '';
}
function kindOf(store, id) { const n = store.nodes.get(id); return n ? n.kind : 'unknown'; }
function exitChip(exit) {
  const ex = exitClass(exit);
  const lab = ex === 'pick' ? String(exit).slice(5) || '选项' : EXIT_LABEL[ex];
  return `<span class="ex" style="--c:${COLORS[ex]}">${esc(lab)}</span>`;
}

// ---------------- 提示 ----------------

const tip = $('tip');
let tipPinned = false;
function showTip(html, x, y, pin = false) {
  tip.innerHTML = html;
  tip.hidden = false;
  tipPinned = pin;
  const r = tip.getBoundingClientRect();
  const nx = Math.min(window.innerWidth - r.width - 10, x + 14);
  const ny = y + 16 + r.height > window.innerHeight ? y - r.height - 12 : y + 16;
  tip.style.left = `${Math.max(8, nx)}px`;
  tip.style.top = `${Math.max(8, ny)}px`;
}
function hideTip(force = false) { if (tipPinned && !force) return; tip.hidden = true; tipPinned = false; }
document.addEventListener('click', (e) => { if (!e.target.closest('.info')) hideTip(true); });

// ---------------- 路由状态 ----------------

const TABS = ['glance', 'overview', 'plans', 'data'];
const state = { tab: 'glance', intent: '', size: 'all', relay: false, open: null };
function readHash() {
  const h = location.hash.replace(/^#/, '');
  const parts = h.split('&');
  const tab = parts[0];
  state.tab = TABS.includes(tab) ? tab : 'glance';
  state.open = null;
  for (const p of parts.slice(1)) {
    const [k, v] = p.split('=');
    if (k === 'i') state.intent = decodeURIComponent(v || '');
    if (k === 'open') state.open = decodeURIComponent(v || '');
  }
}
function writeHash() {
  let h = `#${state.tab}`;
  if (state.tab === 'plans' && state.intent) h += `&i=${encodeURIComponent(state.intent)}`;
  if (state.open) h += `&open=${encodeURIComponent(state.open)}`;
  if (location.hash !== h) history.replaceState(null, '', h);
}

function setTab(tab) {
  state.tab = tab;
  document.body.classList.remove('g-peek');
  for (const b of document.querySelectorAll('.tab')) b.classList.toggle('on', b.dataset.tab === tab);
  for (const t of TABS) $(`tab-${t}`).hidden = t !== tab;
  writeHash();
  if (tab === 'glance') { if (MAIN.ok) GLANCE.show(); } else GLANCE.hide();
  render();
}

// ---------------- 汇总 ----------------

function perIntent(store) {
  return store.intentOrder.map((iid, k) => {
    const it = store.intents.get(iid);
    const row = { iid, k, text: it.text, from: it.from, direct: 0, relay: 0, unsure: 0, multi: 0, multiStable: 0, plans: it.plans.length, judges: it.judges, usd: it.usd };
    for (const rid of it.relations) {
      const r = store.relations.get(rid);
      if (r && row[r.kind] != null) row[r.kind]++;
    }
    for (const cid of it.configs) {
      const c = store.configs.get(cid);
      if (c && c.status !== 'dropped' && partiesOf(c) >= 3) { row.multi++; if (c.status === 'stable') row.multiStable++; }
    }
    return row;
  });
}

function h1Series(src) {
  if (!src.ok) return null;
  const occ = new Map();
  const byGroup = new Map();
  for (const iid of src.store.intentOrder) {
    const g = groupOf(src, iid);
    if (!g) continue;
    const k = (occ.get(g) || 0) + 1;
    occ.set(g, k);
    const v = h1Of(src.store, iid);
    if (!byGroup.has(g)) byGroup.set(g, []);
    byGroup.get(g).push({ k, iid, v: v ? v.judges : null });
  }
  if (!byGroup.size) return null;
  let maxK = 0;
  for (const arr of byGroup.values()) maxK = Math.max(maxK, arr.length);
  maxK = Math.min(maxK, 10);
  const med = [];
  for (let k = 1; k <= maxK; k++) {
    const vals = [];
    for (const arr of byGroup.values()) { const x = arr.find((a) => a.k === k); if (x && Number.isFinite(x.v)) vals.push(x.v); }
    med.push({ k, v: median(vals), n: vals.length });
  }
  if (!med.some((m) => m.v != null)) return null;
  return { byGroup, med, maxK };
}

// ---------------- 方案模型（依据链全部从事件推出） ----------------

function planModels(store) {
  const out = [];
  for (const plan of store.plans.values()) out.push(planModel(store, plan));
  // 兜底版（大模型出方案失败后按模板补的）排在正常方案后面
  out.sort((a, b) => (a.fallback - b.fallback) || b.size - a.size || a.seq - b.seq);
  return out;
}

// 几方：以引擎给的 config.parties 为准（网外的转介对象不单算一方，接口第九节）；旧数据没有时数网内成员
function partiesOf(cfg, ids) {
  const n = cfg && Number(cfg.parties);
  if (Number.isFinite(n) && n > 0) return n;
  return (ids || (cfg ? cfg.members : []) || []).filter((id) => !String(id).startsWith('x:')).length;
}

function planModel(store, plan) {
  const A = store.aug;
  const cfg = store.configs.get(plan.config) || null;
  const rows = Array.isArray(plan.members) ? plan.members.filter((m) => m && m.id != null) : [];
  const ids = rows.length ? rows.map((m) => m.id) : (cfg ? cfg.members : []);
  const memberSet = new Set(ids);
  const root = (cfg && cfg.root) ?? store.rootIntent(plan.config);
  const chain = [];
  for (let c = cfg; c && chain.length < 16; c = c.parent ? store.configs.get(c.parent) : null) chain.unshift(c);
  const chainIds = new Set(chain.map((c) => c.id));
  // 关系：接口第九节 config.relations 直接列出（沿构型链取并集）；旧数据没有时退回「同一根意图下、另一端（to 或 via）落在成员里的」
  const listed = new Set();
  for (const c of chain) for (const id of c.relations || []) listed.add(id);
  const rels = [];
  if (listed.size) {
    for (const id of listed) { const r = store.relations.get(id); if (r) rels.push(r); }
  } else {
    for (const r of store.relations.values()) {
      if (r.root !== root && !chainIds.has(r.about)) continue;
      if (memberSet.has(r.to) || (r.via != null && memberSet.has(r.via))) rels.push(r);
    }
  }
  rels.sort((a, b) => a.seq - b.seq);
  const ring = cfg ? cfg.ring : null;
  const cycle = ring && Array.isArray(ring.cycle) && ring.cycle.length ? ring.cycle : ids;
  const edges = ringEdgesOf(ring, cycle, rows);
  const edgeJudges = edgeJudgesOf(store, cfg, edges);
  const relJudges = [];
  for (const r of rels) for (const id of r.judges || []) { const j = store.judges.get(id); if (j) relJudges.push(j); }
  const keyJudges = [...relJudges.filter((j) => exitClass(j.exit) !== 'pick'), ...edgeJudges.filter(Boolean)]
    .filter((j) => readingOf(j))
    .sort((a, b) => a.seq - b.seq);
  const ex = { act: 0, unsure: 0, other: 0 };
  for (const e of edges) { const c = exitClass(e.exit); if (c === 'act') ex.act++; else if (c === 'unsure') ex.unsure++; else ex.other++; }
  const origin = root && store.intents.get(root) ? store.intents.get(root).from : ids[0];
  const orow = rows.find((m) => m.id === origin) || rows[0] || {};
  const missing = [...new Set(rows.map((m) => m.missing).filter((x) => x && String(x).trim()))];
  const demo = ids.some((id) => { const n = store.nodes.get(id); return n && n.demo_only; });
  return {
    id: plan.id, seq: plan.seq ?? 0, plan, fallback: plan.source === 'template' && !!plan.error, cfg, rows, ids, root, chain, rels, ring, edges, edgeJudges, cycle,
    title: String(plan.title || '').trim(), judgeN: judgesBefore(store, root, plan.seq ?? Infinity),
    keyJudges, ex, size: partiesOf(cfg, ids), members: ids.length,
    // 环上成立的边有没有经过发信人（有的环只在中间人和他自己的熟人之间闭合，不经过发信人）
    ringHasOrigin: edges.some((e) => e.from === (root && store.intents.get(root) ? store.intents.get(root).from : ids[0]) || e.to === (root && store.intents.get(root) ? store.intents.get(root).from : ids[0])),
    hasRelay: rels.some((r) => r.kind === 'relay'),
    missing, firstStep: orow.first_step || rows.map((m) => m.first_step).find(Boolean) || '', origin, demo,
    color: (id) => MEMBER_HUES[Math.max(0, ids.indexOf(id)) % MEMBER_HUES.length],
  };
}

// 这个方案花了几次判断：根意图名下、seq 早于 plan 事件的全部 judge（含对它长出的构型的判断）
let JB = { key: -1, byRoot: new Map() };
function judgesBefore(store, root, seq) {
  if (root == null) return 0;
  if (JB.store !== store || JB.key !== store.c.judgments) {
    const byRoot = new Map();
    for (const j of store.judges.values()) {
      const r = store.rootIntent(j.about);
      if (r == null) continue;
      if (!byRoot.has(r)) byRoot.set(r, []);
      byRoot.get(r).push(j.seq ?? 0);
    }
    for (const a of byRoot.values()) a.sort((x, y) => x - y);
    JB = { store, key: store.c.judgments, byRoot };
  }
  const a = JB.byRoot.get(root) || [];
  let lo = 0; let hi = a.length;
  while (lo < hi) { const mid = (lo + hi) >> 1; if (a[mid] < seq) lo = mid + 1; else hi = mid; }
  return lo;
}

// 环上的边。接口第九节：ring.edges[] = {from, to, give, exit, judge}。
// 环没闭合时（status: failed）引擎发 edges: []，但另带 all_edges（每对成员之间的判边，第九节未列，作兜底用）：
// 这时按方案的接力顺序（成员 k 给成员 k+1）从 all_edges 取每一跳的出口与判断 id，give 取方案里给方写的 give，标 derived。
function ringEdgesOf(ring, cycle, rows) {
  if (ring && Array.isArray(ring.edges) && ring.edges.length) return ring.edges;
  const all = ring && Array.isArray(ring.all_edges) ? ring.all_edges : null;
  if (!all || cycle.length < 2) return [];
  return cycle.map((from, k) => {
    const to = cycle[(k + 1) % cycle.length];
    const hit = all.find((e) => e && e.from === from && e.to === to) || {};
    const row = rows.find((r) => r.id === from) || {};
    return { from, to, give: row.give || '', give_cut: row.give_cut, exit: hit.exit ?? null, judge: hit.judge ?? null, derived: true };
  });
}

// 环上每条边对应的判断：优先 edges[].judge（第九节）；没有时退回「about 是该构型、node 是收方、在 ring 事件之前的最后一次」
function edgeJudgesOf(store, cfg, edgesIn) {
  const ring = cfg ? cfg.ring : null;
  const edges = edgesIn || (ring && Array.isArray(ring.edges) ? ring.edges : []);
  const cj = (cfg && store.aug.judgesAbout.get(cfg.id)) || [];
  const used = new Set();
  return edges.map((e) => {
    if (e && e.judge != null && store.judges.get(e.judge)) return store.judges.get(e.judge);
    let hit = null;
    for (const j of cj) if (j.node === e.to && !used.has(j.id) && (ring.seq == null || j.seq < ring.seq)) hit = j;
    if (hit) used.add(hit.id);
    return hit;
  });
}

// ---------------- 图形部件 ----------------

function ringSvg(store, m, { labels = false, cls = 'ringsvg' } = {}) {
  const ids = m.cycle.length ? m.cycle : m.ids;
  const n = ids.length;
  // 带角色字时画布放宽：角色字放在头像外侧（远离环心），不压箭头、不互相叠
  const VW = labels ? 330 : 200;
  const VH = labels ? 236 : 200;
  const cx = VW / 2;
  const R = labels ? (n <= 2 ? 72 : n >= 5 ? 66 : 62) : (n <= 2 ? 60 : n >= 5 ? 70 : 66);
  const cy = VH / 2 + (labels && n === 3 ? R * 0.22 : 0);
  const ar = n >= 5 ? 14 : 17;
  const angs = ids.map((_, i) => (n === 2 ? (i === 0 ? Math.PI : 0) : -Math.PI / 2 + (i * 2 * Math.PI) / n));
  const pos = new Map();
  ids.forEach((id, i) => pos.set(id, [cx + R * Math.cos(angs[i]), cy + R * Math.sin(angs[i])]));
  let edges = m.edges.length ? m.edges : ids.map((id, i) => ({ from: id, to: ids[(i + 1) % n], give: '', exit: null }));
  if (n < 2) edges = [];
  let paths = '';
  let dots = '';
  edges.forEach((e, k) => {
    const a = pos.get(e.from);
    const b = pos.get(e.to);
    if (!a || !b || e.from === e.to) return;
    const ex = e.exit == null ? 'none' : exitClass(e.exit);
    const col = ex === 'none' ? '#6c7a96' : ex === 'act' ? COLORS.act : ex === 'unsure' ? COLORS.unsure : COLORS.no;
    const dx = b[0] - a[0];
    const dy = b[1] - a[1];
    const L = Math.hypot(dx, dy) || 1;
    const ux = dx / L;
    const uy = dy / L;
    const x1 = a[0] + ux * (ar + 4);
    const y1 = a[1] + uy * (ar + 4);
    const x2 = b[0] - ux * (ar + 6);
    const y2 = b[1] - uy * (ar + 6);
    // 控制点：两方时上下分开，多方时向外鼓
    let mx = (x1 + x2) / 2;
    let my = (y1 + y2) / 2;
    if (n === 2) { const s = ids.indexOf(e.from) === 0 ? -1 : 1; mx += 0; my += s * 34; }
    else { const ox = mx - cx; const oy = my - cy; const ol = Math.hypot(ox, oy) || 1; mx += (ox / ol) * 22; my += (oy / ol) * 22; }
    const d = `M${x1.toFixed(1)},${y1.toFixed(1)} Q${mx.toFixed(1)},${my.toFixed(1)} ${x2.toFixed(1)},${y2.toFixed(1)}`;
    const dash = ex === 'unsure' ? ' stroke-dasharray="5 4"' : ex === 'none' ? ' stroke-dasharray="2 5"' : ex !== 'act' ? ' stroke-dasharray="2 4"' : '';
    const title = `${nameOf(store, e.from)} → ${nameOf(store, e.to)}${e.give ? `：${e.give}` : ''}${e.exit ? `（${EXIT_LABEL[exitClass(e.exit)]}）` : ''}`;
    paths += `<path d="${d}" fill="none" stroke="${col}" stroke-width="2.2" stroke-linecap="round"${dash} marker-end="url(#ar-${ex === 'act' ? 'act' : ex === 'unsure' ? 'unsure' : ex === 'none' ? 'none' : 'ignore'})" opacity="0.92"><title>${esc(title)}</title></path>`;
    paths += `<path d="${d}" fill="none" stroke="transparent" stroke-width="12"><title>${esc(title)}</title></path>`;
    if (ex === 'act') {
      dots += `<circle r="2.6" fill="#fff" filter="url(#glow)" opacity="0.9"><animateMotion dur="${(2.2 + k * 0.37).toFixed(2)}s" repeatCount="indefinite" path="${d}"/></circle>`;
    }
  });
  let avs = '';
  ids.forEach((id, i) => {
    const [x, y] = pos.get(id);
    const col = m.color(id);
    const nm = nameOf(store, id);
    const isO = id === m.origin;
    avs += `<g><title>${esc(nm)}${isO ? '（发信人）' : ''}</title>`;
    if (isO) avs += `<circle cx="${x}" cy="${y}" r="${ar + 4}" fill="none" stroke="#fff" stroke-opacity="0.55" stroke-width="1.2"/>`;
    avs += `<circle cx="${x}" cy="${y}" r="${ar}" fill="${col}" filter="url(#glow)"/>`;
    avs += glyphSvg(kindOf(store, id), x, y, ar);
    if (labels) {
      const a = angs[i];
      let lx = x; let ly; let anchor = 'middle';
      if (Math.abs(Math.cos(a)) >= 0.5) { anchor = Math.cos(a) < 0 ? 'end' : 'start'; lx = x + Math.sign(Math.cos(a)) * (ar + 6); ly = y + 4.5; }
      else if (Math.sin(a) < 0) ly = y - ar - 6;
      else ly = y + ar + 15;
      // 卡片上的角色字：网外的人只留「·」后面那段，再截到 7 个字（全名在悬停提示和详情里）
      const k = nm.lastIndexOf('·');
      const shortNm = Array.from(k > 0 && k < nm.length - 1 ? nm.slice(k + 1) : nm);
      const lab = shortNm.length > 7 ? `${shortNm.slice(0, 6).join('')}…` : shortNm.join('');
      avs += `<text x="${lx.toFixed(1)}" y="${ly.toFixed(1)}" text-anchor="${anchor}" font-size="12.5" font-weight="600" fill="#e8eefb">${esc(lab)}</text>`;
    }
    avs += '</g>';
  });
  return `<svg class="${cls}" viewBox="0 0 ${VW} ${VH}" role="img" aria-label="方案成员环">${paths}${dots}${avs}</svg>`;
}

function donutSvg(ex, size = 78) {
  const total = ex.act + ex.unsure + ex.other;
  const r = size / 2 - 7;
  const C = 2 * Math.PI * r;
  let off = 0;
  let segs = '';
  if (total) {
    for (const [k, col] of [['act', COLORS.act], ['unsure', COLORS.unsure], ['other', COLORS.no]]) {
      const v = ex[k];
      if (!v) continue;
      const len = (v / total) * C;
      segs += `<circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="${col}" stroke-width="7" stroke-dasharray="${Math.max(0, len - 2).toFixed(2)} ${(C - len + 2).toFixed(2)}" stroke-dashoffset="${(-off).toFixed(2)}" transform="rotate(-90 ${size / 2} ${size / 2})" filter="url(#glow)"/>`;
      off += len;
    }
  }
  const title = total ? `环上 ${total} 条边：成立 ${ex.act}，拿不准 ${ex.unsure}，不成立 ${ex.other}` : '没有环清算事件';
  return `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" role="img" aria-label="${esc(title)}"><title>${esc(title)}</title>
    <circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="rgba(255,255,255,0.07)" stroke-width="7"/>${segs}
    <text x="${size / 2}" y="${size / 2 + 6}" text-anchor="middle" font-family="SF Mono, Menlo, monospace" font-size="${size * 0.22}" font-weight="700" fill="#fff">${total ? `${ex.act}/${total}` : '—'}</text></svg>`;
}

function readBarsSvg(store, judges, w = 100, h = 34) {
  const js = judges.slice(-14);
  if (!js.length) return `<svg width="${w}" height="${h}"><text x="${w / 2}" y="${h / 2 + 4}" text-anchor="middle" font-size="11" fill="#6c7a96">—</text></svg>`;
  const bw = Math.min(10, (w - 2) / js.length - 2);
  let out = `<svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" role="img" aria-label="关键判断读数">`;
  out += `<line x1="0" x2="${w}" y1="${h - 0.5}" y2="${h - 0.5}" stroke="rgba(160,190,240,0.2)"/>`;
  js.forEach((j, i) => {
    const rd = readingOf(j);
    const p = rd ? rd.p : 0;
    const ex = exitClass(j.exit);
    const bh = Math.max(2, p * (h - 3));
    const x = 1 + i * (bw + 2);
    out += `<rect x="${x.toFixed(1)}" y="${(h - 1 - bh).toFixed(1)}" width="${bw.toFixed(1)}" height="${bh.toFixed(1)}" rx="1.5" fill="${COLORS[ex]}" opacity="0.9"><title>${esc(nameOf(store, j.node))}：${esc(j.q)} → ${esc(EXIT_LABEL[ex])} ${p.toFixed(2)}</title></rect>`;
  });
  return out + '</svg>';
}

function readingCell(j) {
  const rd = readingOf(j);
  const ex = exitClass(j.exit);
  if (rd && rd.probs && rd.over) {
    return `<div class="picks">${rd.probs.map((p, k) => `<div class="${k === rd.top ? 'top' : ''}"><span>${esc(rd.over[k] ?? k)}</span><span class="bar"><i style="width:${(p * 100).toFixed(0)}%"></i></span></div>`).join('')}</div>`;
  }
  const p = rd ? rd.p : null;
  return `<div class="rd">${exitChip(j.exit)}<span class="bar">${p != null ? `<i style="--c:${COLORS[ex]};width:${(p * 100).toFixed(0)}%"></i>` : ''}</span><span class="num">${p != null ? p.toFixed(2) : '—'}</span></div>`;
}

// ---------------- 总览 ----------------

function renderOverview() {
  const s = MAIN.store;
  const c = s.c;
  const plans = c.plans;
  const intents = c.intents;
  const rows = perIntent(s);
  let multiPlans = 0;
  for (const p of s.plans.values()) {
    const n = partiesOf(s.configs.get(p.config), Array.isArray(p.members) && p.members.length ? p.members.map((m) => m.id) : null);
    if (n >= 3) multiPlans++;
  }
  $('ov-line').innerHTML = multiPlans
    ? `长出 <b class="gold">${fmtN(multiPlans)}</b> 个三方以上的方案`
    : plans ? `长出 <b class="gold">${fmtN(plans)}</b> 个方案` : `长出 <b class="gold">${fmtN(s.configCount)}</b> 个构型`;
  const hv = h1View();
  let sub = '';
  // H1 的三个口径只放在数据页签，三张同样大小的图并列（同等显眼）；总览这一行要是只放一个口径就不对等，放三个又超 60 字，所以这里不放
  if (REPORT && REPORT.h3) {
    // H3 不成立和花费比并列，放在总览最显眼的一行（总控 03:55 要求：不能只挑好看的写）
    const rv = REPORT.h3.reviewers || {};
    const v = (a) => ['opus', 'sonnet'].map((r) => (rv[r] && rv[r].n_valuable ? rv[r].n_valuable[a] : null) ?? '—').join('·');
    const c = REPORT.h3.cost || {};
    const ratio = c.prediction_4_order_of_magnitude && c.prediction_4_order_of_magnitude.ratio_llm_all_over_jpp_upper_bound;
    sub = `<span title="H1 流 25 条意图的正式盲评：有价值方案数（Opus·Sonnet）。H3 预测 J++ ≥ 生成模型，不成立">有价值：生成模型 <b>${v('llm_all')}</b><i class="h1sep">·</i>J++ <b class="bad">${v('jpp')}</b></span>`
      + (ratio ? `<i class="h1sep">·</i><span title="J++ 整条主运行的 JEV 花费 / 生成模型一路 25 条按 list 价折算的花费">花费 <b>1/${Math.round(ratio)}</b></span>` : '');
  } else if (hv.kind === 'report') {
    sub = '';
  } else if (hv.med) {
    const first = hv.med[0] && hv.med[0].v;
    const lastM = [...hv.med].reverse().find((x) => x.v != null && x.k > 1);
    if (first && lastM) {
      const pct = Math.round((1 - lastM.v / first) * 100);
      sub = pct >= 0 ? `同类第 ${lastM.k} 次，判断少用 <b>${pct}%</b>` : `同类第 ${lastM.k} 次，判断多用 <b>${-pct}%</b>`;
      sub += ' <span class="tagm sec" title="没有 report.json：按 discovery 事件的 judgments_at（首个有效构型，整波）计算">次要口径</span>';
    }
  }
  $('ov-sub').innerHTML = sub;
  // 环清算结局照实显示（CP1 修订：冒烟里环一次都没闭合，这件事要写出来）
  const rs = ringStats(s);
  const tiles = [
    { k: '意图', v: fmtN(intents), c: 'rgba(87,212,255,0.55)' },
    { k: '构型', v: fmtN(s.configCount), s: `<em>稳定</em>${fmtN(s.stableConfigCount)}`, c: 'rgba(255,210,122,0.55)' },
    { k: '方案', v: fmtN(plans), s: rs.total ? `<em title="做过环清算的构型里，最后一次清算没闭合的个数 / 做过环清算的构型数（闭合 ${rs.cleared}，有条件闭合 ${rs.conditional}）">环未闭合</em>${fmtN(rs.failed)}/${fmtN(rs.total)}` : '', c: 'rgba(62,240,176,0.55)' },
    { k: '花费', v: fmtUsd(s.shownUsd), s: `<em>耗时</em>${fmtDur(c.elapsed)}`, c: 'rgba(195,139,255,0.55)' },
  ];
  $('ov-tiles').innerHTML = tiles.map((t) => `<div class="tile" style="--c:${t.c}"><div class="k">${t.k}</div><div class="v">${t.v}</div>${t.s ? `<div class="s">${t.s}</div>` : '<div class="s">&nbsp;</div>'}</div>`).join('');
  $('ov-legend').innerHTML = [
    ['直接', COLORS.direct], ['转介', COLORS.relay], ['拿不准', COLORS.unsure],
  ].map(([k, col]) => `<span><i style="--c:${col}"></i>${k}</span>`).join('') + '<span><i class="ringi"></i>多方</span>';
  drawOverviewChart(rows);
}

function drawOverviewChart(rows) {
  const box = $('ov-chart');
  const W = box.clientWidth || 800;
  const narrow = W < 700;
  if (!rows.length) { box.innerHTML = '<div class="nodata">暂无数据</div>'; return; }
  const stack = (r) => r.direct + r.relay + r.unsure + r.multi;
  const maxS = Math.max(1, ...rows.map(stack));
  const n = rows.length;
  let svg = '';
  if (!narrow) {
    const H = box.clientHeight || 300;
    const padB = 22;
    const padT = 6;
    const colW = (W - 16) / n;
    const d = Math.max(4, Math.min(colW * 0.62, 26));
    const gap = Math.max(3, Math.min(6, d * 0.2));
    const segH = Math.max(d * 0.5, Math.min(((H - padB - padT) / maxS), d * 4));
    const sh = segH - gap;
    svg += `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
    rows.forEach((r, i) => {
      const cx = 8 + colW * (i + 0.5);
      let y = H - padB - 2;
      const rx = Math.min(d / 2, sh / 2).toFixed(1);
      const put = (kind, cnt) => {
        for (let q = 0; q < cnt; q++) {
          const top = y - sh;
          if (kind === 'multi') {
            const sw = Math.max(1.6, d * 0.12);
            svg += `<rect x="${(cx - d / 2 + sw / 2).toFixed(1)}" y="${(top + sw / 2).toFixed(1)}" width="${(d - sw).toFixed(1)}" height="${(sh - sw).toFixed(1)}" rx="${rx}" fill="${q < r.multiStable ? 'rgba(255,210,122,0.25)' : 'none'}" stroke="${COLORS.config}" stroke-width="${sw.toFixed(1)}" filter="url(#glow)"/>`;
          } else {
            svg += `<rect x="${(cx - d / 2).toFixed(1)}" y="${top.toFixed(1)}" width="${d.toFixed(1)}" height="${sh.toFixed(1)}" rx="${rx}" fill="${COLORS[kind]}" opacity="${kind === 'unsure' ? 0.5 : 0.92}" filter="url(#glow)"/>`;
          }
          y -= segH;
        }
      };
      put('direct', r.direct); put('relay', r.relay); put('unsure', r.unsure); put('multi', r.multi);
      if (r.plans) svg += `<rect x="${(cx - d * 0.4).toFixed(1)}" y="${(H - padB + 1).toFixed(1)}" width="${(d * 0.8).toFixed(1)}" height="2.5" rx="1" fill="${COLORS.act}" opacity="0.85"/>`;
      if ((i + 1) % 10 === 0 || i === 0) svg += `<text x="${cx.toFixed(1)}" y="${H - 5}" text-anchor="middle" font-size="11" font-family="SF Mono, Menlo, monospace" fill="#6c7a96">${i + 1}</text>`;
      svg += `<rect class="col-hit" data-i="${esc(r.iid)}" x="${(cx - colW / 2).toFixed(1)}" y="0" width="${colW.toFixed(1)}" height="${H - 2}"/>`;
    });
    svg += '</svg>';
    box.innerHTML = svg;
  } else {
    const rowH = 18;
    const H = n * rowH + 8;
    const d = 11;
    svg += `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
    rows.forEach((r, i) => {
      const cy = 4 + rowH * (i + 0.5);
      svg += `<text x="0" y="${cy + 4}" font-size="10" font-family="SF Mono, Menlo, monospace" fill="#6c7a96">${i + 1}</text>`;
      let x = 28;
      const put = (kind, cnt) => {
        for (let q = 0; q < cnt; q++) {
          if (x > W - d) return;
          if (kind === 'multi') svg += `<circle cx="${x}" cy="${cy}" r="${d / 2 - 1}" fill="${q < r.multiStable ? 'rgba(255,210,122,0.28)' : 'none'}" stroke="${COLORS.config}" stroke-width="1.6"/>`;
          else svg += `<circle cx="${x}" cy="${cy}" r="${d / 2 - 1}" fill="${COLORS[kind]}" opacity="${kind === 'unsure' ? 0.55 : 0.95}"/>`;
          x += d + 2;
        }
      };
      put('direct', r.direct); put('relay', r.relay); put('unsure', r.unsure); put('multi', r.multi);
      svg += `<rect class="col-hit" data-i="${esc(r.iid)}" x="0" y="${cy - rowH / 2}" width="${W}" height="${rowH}"/>`;
    });
    svg += '</svg>';
    box.style.height = `${H}px`;
    box.innerHTML = svg;
  }
}

function overviewTip(iid, x, y) {
  const s = MAIN.store;
  const r = perIntent(s).find((q) => q.iid === iid);
  if (!r) return;
  showTip(`<b>「${esc(r.text)}」</b><br><span class="m">${esc(iid)} · ${esc(nameOf(s, r.from))}</span><br>
    直接 ${r.direct} · 转介 ${r.relay} · 拿不准 ${r.unsure} · 多方 ${r.multi} · 方案 ${r.plans}<br>
    <span class="m">判断 ${fmtN(r.judges)} · ${fmtUsd(r.usd)}</span>${r.plans ? '<br><span class="m">点开看方案</span>' : ''}`, x, y);
}

// ---------------- 方案 ----------------

let MODELS = [];
function renderPlans() {
  const s = MAIN.store;
  MODELS = planModels(s);
  // 意图下拉：只列有方案的意图
  const sel = $('f-intent');
  const withPlans = s.intentOrder.filter((iid) => MODELS.some((m) => m.root === iid));
  const opts = ['<option value="">全部意图</option>'].concat(withPlans.map((iid) => {
    const it = s.intents.get(iid);
    const t = Array.from(it.text || '').slice(0, 16).join('');
    return `<option value="${esc(iid)}">${esc(iid)} ${esc(t)}</option>`;
  }));
  sel.innerHTML = opts.join('');
  if (state.intent && !withPlans.includes(state.intent)) state.intent = '';
  sel.value = state.intent;
  for (const b of $('f-size').children) b.classList.toggle('on', b.dataset.size === state.size);
  $('f-relay').setAttribute('aria-pressed', String(state.relay));
  const list = MODELS.filter((m) => (!state.intent || m.root === state.intent)
    && (state.size === 'all' || (state.size === '4' ? m.size >= 4 : m.size === Number(state.size)))
    && (!state.relay || m.hasRelay));
  $('f-count').textContent = `${list.length} / ${MODELS.length}`;
  const grid = $('pl-grid');
  if (!MODELS.length) { grid.innerHTML = '<div class="empty">暂无数据</div>'; return; }
  if (!list.length) { grid.innerHTML = '<div class="empty">没有符合条件的方案</div>'; return; }
  grid.innerHTML = list.map((m) => cardHtml(s, m)).join('');
}

// 卡片只留：角色环、方案名、环上成立比例。每人的「做 / 给 / 得」点开卡片（详情）才显示。
function planTitle(s, m) {
  if (m.title) return m.title;
  const it = s.intents.get(m.root);
  const t = Array.from(it ? it.text : String(m.root || m.id));
  return t.length <= 16 ? t.join('') : `${t.slice(0, 15).join('')}…`;
}
function cardHtml(s, m) {
  const tot = m.ex.act + m.ex.unsure + m.ex.other;
  const allOk = tot > 0 && m.ex.act === tot;
  return `<article class="card${m.fallback ? ' fallback' : ''}" tabindex="0" data-plan="${esc(m.id)}" data-size="${m.size}" data-relay="${m.hasRelay ? 1 : 0}" aria-label="方案 ${esc(planTitle(s, m))}">
    ${ringSvg(s, m, { labels: true })}
    <div class="card-foot">
      <h3 class="ptitle" title="${esc(planTitle(s, m))}">${m.fallback ? '<span class="fb-tag" title="大模型出方案失败，这是按模板补的兜底版，给什么、得什么由模板拼出，部分成员为空">兜底</span>' : ''}${esc(planTitle(s, m))}</h3>
      <div class="ratio${allOk ? ' ok' : ''}" title="环上各边：成立 ${m.ex.act}，拿不准 ${m.ex.unsure}，不成立 ${m.ex.other}"><b>${tot ? m.ex.act : '—'}</b>${tot ? `<i>/${tot}</i>` : ''}</div>
    </div>
  </article>`;
}

// ---------------- 详情与依据链 ----------------

function openPlan(id) {
  const s = MAIN.store;
  const plan = s.plans.get(id);
  if (!plan) return;
  state.open = id;
  writeHash();
  const m = planModel(s, plan);
  $('drawer-body').innerHTML = drawerHtml(s, m);
  $('drawer').hidden = false;
  $('drawer').querySelector('.drawer-panel').scrollTop = 0;
}
function closePlan() {
  $('drawer').hidden = true;
  state.open = null;
  writeHash();
}

function drawerHtml(s, m) {
  const it = s.intents.get(m.root);
  const names = planTitle(s, m);
  const rows = m.rows.length ? m.rows : m.ids.map((id) => ({ id }));
  const ringLabel = m.ring ? `${esc(ringWord(m.ring.status))}${m.edges.some((e) => e.derived) ? '（各跳读数取自全部判边）' : ''}` : '无环清算';
  let h = `<h3>${esc(names)}</h3>
    <div class="intent-q">${m.ids.map((id) => esc(nameOf(s, id))).join(' · ')}<br>${it ? `${esc(it.id)}「${esc(it.text)}」· 发信人 ${esc(nameOf(s, it.from))}` : esc(m.root || '')}</div>
    <div class="dtop">
      ${ringSvg(s, m, { labels: true })}
      <div>
        <div style="display:flex;gap:18px;align-items:center;flex-wrap:wrap">
          ${donutSvg(m.ex, 96)}
          <div>
            <div><span class="ex" style="--c:${COLORS.act}">成立 ${m.ex.act}</span> <span class="ex" style="--c:${COLORS.unsure}">拿不准 ${m.ex.unsure}</span> ${m.ex.other ? `<span class="ex" style="--c:${COLORS.no}">不成立 ${m.ex.other}</span>` : ''}</div>
            <div class="fold">环清算：<span class="mono">${ringLabel}</span> · 构型 <span class="mono">${esc(m.cfg ? m.cfg.id : '—')}</span>${m.cfg && m.cfg.parent ? ` ← <span class="mono">${esc(m.cfg.parent)}</span>` : ''}</div>
          </div>
        </div>
        <div style="margin-top:12px">${readBarsSvg(s, m.keyJudges, 300, 54)}</div>
        <div class="fold">关键判断读数（关系判断 + 环上判边），颜色是出口</div>
        ${m.fallback ? '<p class="fb-note">兜底版：大模型出方案失败，这里按模板补了一份，内容由模板拼出（可能被截断），部分成员的「给什么、得什么」为空。</p>' : ''}
        ${m.plan.text ? `<p class="planline">${esc(m.plan.text)}</p>` : ''}
      </div>
    </div>
    <h4>谁做什么</h4>
    <table class="mt"><colgroup><col style="width:17%"><col><col><col><col><col></colgroup>
      <thead><tr><th>成员</th><th>做什么</th><th>给什么</th><th>得什么</th><th>还缺</th><th>第一小步</th></tr></thead>
      <tbody>${rows.map((r) => `<tr><td><span class="avw">${glyphIcon(kindOf(s, r.id), m.color(r.id), 22)}</span>${esc(nameOf(s, r.id))}${r.id === m.origin ? ' <span class="muted">发信人</span>' : ''}${realNameOf(s, r.id) ? `<div class="realname">${esc(realNameOf(s, r.id))}</div>` : ''}</td>
        <td>${esc(r.do || '—')}</td><td>${giveHtml(r.give, r.give_cut)}</td><td>${esc(r.get || '—')}</td><td>${esc(r.missing || '—')}</td><td>${esc(r.first_step || '—')}</td></tr>`).join('')}</tbody>
    </table>
    <h4>依据链</h4>
    <ol class="chain">${chainHtml(s, m)}</ol>`;
  return h;
}

function judgeRow(s, j) {
  return `<div class="jrow"><span class="who" title="${esc(nameOf(s, j.node))}">${esc(nameOf(s, j.node))}</span><span class="q">${esc(j.q || '')}</span>${readingCell(j)}<span class="key">${esc(j.id)} · ${esc(j.key || '')}${j.usd != null ? ` · ${fmtUsd(j.usd)}` : ''}</span></div>`;
}

function chainHtml(s, m) {
  const A = s.aug;
  const steps = [];
  const it = s.intents.get(m.root);
  if (it) steps.push({ seq: it.seq ?? 0, c: '#ffffff', h: `<span class="tt">意图进入</span><span class="n">${esc(it.id)}</span>`, b: `「${esc(it.text)}」· ${esc(nameOf(s, it.from))}` });
  // 路由：按 about 汇总
  const abouts = [m.root, ...m.chain.map((c) => c.id)].filter(Boolean);
  for (const ab of abouts) {
    const rs = A.routes.get(ab) || [];
    if (!rs.length) continue;
    const by = { coarse: [], fine: [], trace: [] };
    for (const r of rs) (by[r.stage] || (by[r.stage] = [])).push(r);
    const parts = [];
    if (by.trace && by.trace.length) parts.push(`痕迹召回 <b class="mono">${by.trace.reduce((a, r) => a + (r.to || []).length, 0)}</b> 人`);
    if (by.coarse.length) parts.push(`粗筛 <b class="mono">${by.coarse.reduce((a, r) => a + (r.to || []).length, 0)}</b> 人`);
    if (by.fine.length) parts.push(`细选 <b class="mono">${by.fine.length}</b> 波 <b class="mono">${by.fine.reduce((a, r) => a + (r.to || []).length, 0)}</b> 人`);
    for (const k of Object.keys(by)) if (!['coarse', 'fine', 'trace'].includes(k) && by[k].length) parts.push(`${esc(k)} ${by[k].length} 次`);
    const why = [...new Set(rs.map((r) => r.why).filter(Boolean))].slice(0, 3).map(esc).join('；');
    steps.push({ seq: rs[0].seq ?? 0, c: COLORS.coarse, h: `<span class="tt">传播</span><span class="n">${esc(ab)}</span>`, b: `${parts.join(' → ')}${why ? `<div class="fold">${why}</div>` : ''}` });
  }
  // 关系：每条带它的判断、补信息、触发的上下文
  for (const r of m.rels) {
    const js = (r.judges || []).map((id) => s.judges.get(id)).filter(Boolean);
    const en = s.enrich.filter((e) => e.about === r.about && (e.node === r.to || e.node === r.via));
    const items = [...js.map((j) => ({ seq: j.seq, html: judgeRow(s, j) })), ...en.map((e) => ({ seq: e.seq, html: `<div class="enrich">拿不准，补「${esc(e.need)}」：${esc(typeof e.got === 'object' ? (e.got.text ?? JSON.stringify(e.got)) : e.got)}</div>` }))]
      .sort((a, b) => a.seq - b.seq);
    const kindLab = { direct: '直接关系', relay: '转介', unsure: '拿不准的关系' }[r.kind] || r.kind;
    const col = r.kind === 'relay' ? COLORS.relay : r.kind === 'unsure' ? COLORS.unsure : COLORS.direct;
    steps.push({
      seq: r.seq ?? 0, c: col,
      h: `<span class="tt">${esc(kindLab)}</span><span>${esc(nameOf(s, r.from))} → ${esc(nameOf(s, r.to ?? (r.ext && r.ext.id)))}${r.via ? ` <span class="muted">经 ${esc(nameOf(s, r.via))}</span>` : ''}</span><span class="n">${esc(r.id)}</span>`,
      b: `${items.map((x) => x.html).join('')}${r.trigger ? `<div class="trigger">${esc(typeof r.trigger === 'object' ? (r.trigger.ctx ?? JSON.stringify(r.trigger)) : r.trigger)}</div>` : ''}`,
    });
  }
  // 构型：形成、复核的判断、环清算
  for (const cfg of m.chain) {
    const evs = A.cfgEvents.get(cfg.id) || [];
    const first = evs[0];
    const ej = cfg === m.cfg ? m.edgeJudges : edgeJudgesOf(s, cfg, ringEdgesOf(cfg.ring, cfg.members, []));
    const edgeSet = new Set(ej.filter(Boolean).map((j) => j.id));
    const mem = new Set(cfg.members);
    const js = (A.judgesAbout.get(cfg.id) || []).filter((j) => !edgeSet.has(j.id));
    const inside = js.filter((j) => mem.has(j.node));
    const outside = js.length - inside.length;
    steps.push({
      seq: first ? first.seq : cfg.firstSeq ?? 0, c: COLORS.config,
      h: `<span class="tt">构型${cfg.parent ? '再长' : '形成'}</span><span>${cfg.members.map((id) => esc(nameOf(s, id))).join('、')}</span><span class="n">${esc(cfg.id)}${cfg.parent ? ` ← ${esc(cfg.parent)}` : ''}</span>`,
      b: `${esc(first ? first.summary : cfg.summary)}${inside.map((j) => judgeRow(s, j)).join('')}${outside ? `<div class="fold">另问 ${outside} 人，未成立</div>` : ''}<div class="fold">状态：${evs.map((e) => esc(e.status)).join(' → ')}</div>`,
    });
    if (cfg.ring) {
      const ring = cfg.ring;
      const edges = cfg === m.cfg ? m.edges : ringEdgesOf(ring, cfg.members, []);
      steps.push({
        seq: ring.seq ?? 0, c: RING_OK.has(ring.status) ? COLORS.act : COLORS.no,
        h: `<span class="tt">环清算</span><span>${esc(ringWord(ring.status))}</span><span class="n">${edges.length} 条边</span>`,
        b: edges.map((e, k) => {
          const j = ej[k];
          return `<div class="fold">${esc(nameOf(s, e.from))} → ${esc(nameOf(s, e.to))}：${e.give ? giveHtml(e.give, e.give_cut) : ''}</div>${j ? judgeRow(s, j) : `<div class="jrow"><span class="who">${esc(nameOf(s, e.to))}</span><span class="q muted">没有找到对应的判断事件</span>${readingCell({ exit: e.exit })}</div>`}`;
        }).join(''),
      });
    }
  }
  steps.push({ seq: m.plan.seq ?? 0, c: COLORS.config, h: `<span class="tt">成方案</span><span class="n">${esc(m.id)}</span>`, b: esc(m.plan.text || '') });
  steps.sort((a, b) => a.seq - b.seq);
  return steps.map((st, k) => `<li style="--c:${st.c}"><div class="step-h"><span class="n">${k + 1}</span>${st.h}</div><div class="step-b">${st.b}</div></li>`).join('');
}

// ---------------- 数据 ----------------

function noData(box, msg = '暂无数据') { box.innerHTML = `<div class="nodata">${esc(msg)}</div>`; }

function renderData() {
  drawH1();
  drawCompare();
  drawFreq($('d-freq'));
  drawUnsure($('d-unsure'));
  // 不含知名故事（demo_only）的一行
  const s = MAIN.store;
  const demoCfg = (c) => c.members.some((id) => { const n = s.nodes.get(id); return n && n.demo_only; });
  let cfgN = 0; let planN = 0; let demoAny = false;
  for (const c of s.configs.values()) { if (c.status === 'dropped') continue; if (demoCfg(c)) { demoAny = true; continue; } cfgN++; }
  for (const p of s.plans.values()) { const c = s.configs.get(p.config); if (c && demoCfg(c)) continue; planN++; }
  // 这行说明默认不占屏（每屏汉字 ≤ 60），并进「判断」卡片的 i 里
  EXTRA.hj = (demoAny || [...s.nodes.values()].some((n) => n.demo_only) ? `不含知名故事：构型 ${fmtN(cfgN)}，方案 ${fmtN(planN)}。` : '')
    + (CTL.ok ? `对照运行：${CTL.url}。` : '');
  // 判断线照实写在页脚，和新口径一样标「冒烟后改定」（CP1 修订「主会话追加要求」）；线取自这次运行的 meta.json，不手填。
  // 为了每屏 ≤ 60 字，两处都用同一个星号，「冒烟后改定」只写一次。环清算结局在总览的「方案」大数下面。
  const foot = [];
  const rl = META && META.params && META.params.lines && META.params.lines.recv_line && META.params.lines.recv_line.declare;
  if (rl && rl.hi != null && rl.lo != null) foot.push(`<span title="接收方、组装、环清算、外部确认的判断线；原先是 0.7/0.3，看过冒烟读数之后改成窄带（预注册补充六）">判断线 ${esc(rl.hi)}/${esc(rl.lo)}<sup class="amend">*</sup></span>`);
  if (foot.length || h1View().kind === 'report') foot.push('<span><sup class="amend">*</sup>冒烟后改定</span>');
  $('d-foot').innerHTML = foot.join('<i class="h1sep">·</i>');
}

function tickCount(max) {
  for (const k of [4, 5, 3, 2]) if (Number.isInteger(max / k) || max / k >= 20) return k;
  return 4;
}
function axisY(x0, y0, h, max, fmt, ticks = tickCount(max)) {
  let o = '<g class="axis">';
  for (let i = 0; i <= ticks; i++) {
    const v = (max * i) / ticks;
    const y = y0 - (h * i) / ticks;
    o += `<line x1="${x0}" x2="${x0 - 4}" y1="${y}" y2="${y}"/><text x="${x0 - 7}" y="${y + 4}" text-anchor="end">${esc(fmt(v))}</text>`;
  }
  return o + '</g>';
}

function niceMax(v) {
  if (!(v > 0)) return 1;
  const p = 10 ** Math.floor(Math.log10(v));
  for (const k of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) if (k * p >= v) return k * p;
  return 10 * p;
}

function drawH1() {
  const box = $('d-h1');
  const hv = h1View();
  $('d-h1-tag').textContent = hv.tag;
  $('d-h1-tag').hidden = !hv.tag || hv.kind === 'report';
  $('d-h1-tag').className = `tagm${hv.kind === 'discovery' ? ' sec' : ''}`;
  EXTRA.h1 = hv.kind === 'na' && hv.reason ? `本次：${hv.reason}。` : (REPORT && REPORT.criterion ? `命中目标的口径原文：${REPORT.criterion}` : '');
  if (REPORT && REPORT.traceAcc && REPORT.traceAcc.accuracy != null) {
    const a = REPORT.traceAcc;
    EXTRA.h1 += ` 痕迹命中准确率（痕迹臂，选中的过往信号与当前意图同组）：${a.n_hit}/${a.n_applicable}；没有拾取 ${a.n_no_pick} 次。`;
  }
  if (hv.kind === 'report') {
    const vs = H1_METRICS.map((m) => `${m.lab}${m.tag ? '（冒烟后改定）' : ''}：只算${m.rateWord}的 ${VERDICT_WORD[verdictOf(m.key, false)] || '—'}，封顶 ${VERDICT_WORD[verdictOf(m.key, true)] || '—'}`);
    EXTRA.h1 = `预注册判定（第 5 次比第 1 次痕迹组降 ≥ 40%、对照组降 ≤ 10%、两组相差 ≥ 15 个百分点、规模比 300/150 ≤ 1.4；相差 < 15 或规模比 ≥ 1.8 为推翻）：${vs.join('；')}。降幅数字的颜色：绿成立，红推翻，橙预测未达。 ${EXTRA.h1}`;
  }
  if (hv.kind === 'na') return noData(box, '不适用'); // report 说口径不适用（如开发意图没有组），不是数据不足
  if (hv.kind === 'none') return noData(box);
  if (hv.kind === 'report') {
    // 三个口径三张同样大小的小图
    box.innerHTML = `<div class="h1grid">${H1_METRICS.map((m) => {
      const x = hv.metrics[m.key];
      const d = dropOf(x.arms.trace); const dc = dropOf(x.capped.trace);
      return `<div class="h1m"><div class="h1h"><b>${m.lab}</b>${m.tag ? `<sup class="amend" title="冒烟后改定：看过冒烟读数之后才改定的口径（预注册补充六），不是事先定好的主口径">${m.tag}</sup>` : ''}<span class="h1d" title="痕迹组第 5 次比第 1 次：只算${m.rateWord}的 ${pctTxt(d)}（判定：${esc(VERDICT_WORD[verdictOf(m.key, false)] || '—')}）；把没${m.rateWord}的按全部判断数封顶计入 ${pctTxt(dc)}（判定：${esc(VERDICT_WORD[verdictOf(m.key, true)] || '—')}）"><b style="color:${VERDICT_COL[verdictOf(m.key, false)] || 'inherit'}">${pctTxt(d)}</b><small style="color:${VERDICT_COL[verdictOf(m.key, true)] || 'inherit'}">(${pctTxt(dc)})</small></span></div><div class="h1c" data-m="${m.key}"></div></div>`;
    }).join('')}</div>`;
    for (const m of H1_METRICS) {
      const x = hv.metrics[m.key];
      const ser = (arr) => (Array.isArray(arr) ? arr.map((v, i) => [i + 1, v]) : []);
      const lines = [{ key: 'trace', lab: '痕迹', col: COLORS.act, w: 3, dash: '', pts: ser(x.arms.trace) }];
      if (x.arms.control) lines.push({ key: 'control', lab: '对照', col: '#a9b6cf', w: 2.2, dash: '7 6', pts: ser(x.arms.control) });
      if (x.arms.scale150) lines.push({ key: 'scale150', lab: '150', col: '#c38bff', w: 2, dash: '2 5', pts: ser(x.arms.scale150) });
      h1Chart(box.querySelector(`.h1c[data-m="${m.key}"]`), lines, { rates: x.rates.trace || null, word: m.rateWord, legend: m.key === 'hit' });
    }
    return;
  }
  // 没有 report.json：按 discovery 事件画（次要口径，只有 H1 臂的运行）
  const lines = [{ key: 'trace', lab: '痕迹', col: COLORS.act, w: 3, dash: '', pts: hv.med.map((x) => [x.k, x.v, x.n]) }];
  if (hv.ctlSeries) lines.push({ key: 'control', lab: '对照', col: '#a9b6cf', w: 2.2, dash: '7 6', pts: hv.ctlSeries.med.map((x) => [x.k, x.v, x.n]) });
  h1Chart(box, lines, { series: hv.series, legend: true });
}

// 一张 H1 小图：横轴同类第几次出现，纵轴判断次数中位数；opt.rates 在刻度下标「命中数/可判数」
function h1Chart(box, lines, opt = {}) {
  const W = box.clientWidth || 300;
  const H = box.clientHeight || 220;
  const narrow = W < 260;
  const L = narrow ? 38 : 44; const R = 12; const T = opt.legend && lines.length > 1 ? 30 : 16; const B = opt.rates ? 38 : 26;
  let maxK = 0; let maxV = 0;
  for (const ln of lines) for (const p of ln.pts) { maxK = Math.max(maxK, p[0]); if (p[1] != null && Number.isFinite(p[1])) maxV = Math.max(maxV, p[1]); }
  if (opt.series) for (const arr of opt.series.byGroup.values()) for (const a of arr) if (Number.isFinite(a.v)) maxV = Math.max(maxV, a.v);
  if (!(maxV > 0)) return noData(box);
  maxV = niceMax(maxV * 1.08);
  const X = (k) => L + (maxK <= 1 ? (W - L - R) / 2 : ((k - 1) / (maxK - 1)) * (W - L - R));
  const Y = (v) => H - B - (v / maxV) * (H - T - B);
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  o += axisY(L, H - B, H - T - B, maxV, (v) => fmtN(Math.round(v)));
  o += '<g class="axis">';
  for (let k = 1; k <= maxK; k++) o += `<line x1="${X(k)}" x2="${X(k)}" y1="${T}" y2="${H - B}" stroke-dasharray="2 6"/><text x="${X(k)}" y="${H - B + 16}" text-anchor="middle">${k}</text>`;
  o += '</g>';
  // 中位数只在命中（找到）的出现上算，命中率要一起看（metrics/README.md）：刻度下方小字标 命中数/可判数
  if (opt.rates) {
    opt.rates.forEach((r, i) => {
      if (r && Number.isFinite(Number(r.n_applicable))) o += `<text class="lab" x="${X(i + 1)}" y="${H - B + 29}" text-anchor="middle" font-family="SF Mono, Menlo, monospace" font-size="10" fill="#6c7a96"><title>痕迹组第 ${i + 1} 次：${r.n_hit} 组${opt.word || '命中'} / ${r.n_applicable} 组</title>${r.n_hit}/${r.n_applicable}</text>`;
    });
  }
  const path = (pts) => pts.filter((p) => p[1] != null && Number.isFinite(p[1])).map((p, i) => `${i ? 'L' : 'M'}${X(p[0]).toFixed(1)},${Y(p[1]).toFixed(1)}`).join(' ');
  if (opt.series) {
    let gi = 0;
    for (const [g, arr] of opt.series.byGroup) {
      const col = GROUP_HUES[gi++ % GROUP_HUES.length];
      const d = path(arr.map((a) => [a.k, Number.isFinite(a.v) ? a.v : null]));
      if (d) o += `<path d="${d}" fill="none" stroke="${col}" stroke-width="1.2" opacity="0.28"><title>${esc(g)}</title></path>`;
    }
  }
  for (const ln of lines.slice().reverse()) {
    const d = path(ln.pts);
    if (!d) continue;
    const main = ln.key === 'trace';
    o += `<path d="${d}" fill="none" stroke="${ln.col}" stroke-width="${ln.w}"${ln.dash ? ` stroke-dasharray="${ln.dash}"` : ''}${main ? ' filter="url(#glow)"' : ' opacity="0.85"'}/>`;
    for (const p of ln.pts) {
      if (p[1] == null || !Number.isFinite(p[1])) continue;
      o += `<circle cx="${X(p[0])}" cy="${Y(p[1])}" r="${main ? 4.5 : 3.5}" fill="${main ? ln.col : '#04060c'}" stroke="${ln.col}" stroke-width="2"${main ? ' filter="url(#glow)"' : ''}><title>${esc(ln.lab)} 第 ${p[0]} 次 中位数 ${p[1]}${p[2] != null ? `（${p[2]} 组）` : ''}</title></circle>`;
      if (main && !narrow) o += `<text class="labv" x="${X(p[0])}" y="${Y(p[1]) - 10}" text-anchor="middle">${fmtN(Math.round(p[1]))}</text>`;
    }
  }
  // 极小图例：只画一次（第一张小图），多于一条线时才画
  if (opt.legend && lines.length > 1) {
    const step = 62;
    let lx = W - R - lines.length * step;
    o += `<g transform="translate(0,${T - 18})">`;
    for (const ln of lines) {
      o += `<line x1="${lx}" x2="${lx + 18}" y1="0" y2="0" stroke="${ln.col}" stroke-width="${Math.min(3, ln.w)}"${ln.dash ? ` stroke-dasharray="${ln.dash}"` : ''}/><text class="lab" x="${lx + 22}" y="4">${esc(ln.lab)}</text>`;
      lx += step;
    }
    o += '</g>';
  }
  o += '</svg>';
  box.innerHTML = o;
}

// 三路对照（正式盲评，H3/H2）：有价值方案数，两位评审各一根；另写花费比。不成立的照实放在这里（总控 03:55 要求）
function drawCompareReport(box, h) {
  const rv = h.reviewers || {};
  const val = (r, a) => numOrNull(rv[r] && rv[r].n_valuable && rv[r].n_valuable[a]);
  const rows = [
    { lab: '生成', arms: ['llm_all'], col: '#a9b6cf', tip: '生成模型一次读完全部主体' },
    { lab: 'J++', arms: ['jpp'], col: COLORS.act, tip: 'J++ 网络（主运行）' },
    { lab: '检索', arms: ['bm25', 'vector'], col: '#6c7a96', tip: '关键词（BM25）与向量检索' },
  ];
  const revs = [['opus', 'O'], ['sonnet', 'S']];
  let maxV = 1;
  for (const r of rows) for (const [k] of revs) for (const a of r.arms) maxV = Math.max(maxV, val(k, a) || 0);
  const W = box.clientWidth || 400; const H = box.clientHeight || 260;
  const L = 52; const R = 44; const T = 14; const B = 44;
  const rowH = (H - T - B) / rows.length; const bh = Math.min(16, rowH * 0.28);
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  rows.forEach((r, i) => {
    const y0 = T + i * rowH + rowH / 2 - bh - 2;
    o += `<text class="lab" x="${L - 10}" y="${y0 + bh + 6}" text-anchor="end" font-size="13" fill="#e8eefb"><title>${esc(r.tip)}</title>${esc(r.lab)}</text>`;
    revs.forEach(([k, letter], q) => {
      const vals = r.arms.map((a) => val(k, a));
      const v = vals.reduce((m, x) => Math.max(m, x || 0), 0);
      const y = y0 + q * (bh + 4);
      const w = (v / maxV) * (W - L - R);
      const tip = `${r.tip}，${k === 'opus' ? 'Opus' : 'Sonnet'} 评审：有价值方案 ${r.arms.map((a, x) => `${a} ${vals[x] ?? '—'}`).join('，')}`;
      o += `<rect x="${L}" y="${y.toFixed(1)}" width="${Math.max(2, w).toFixed(1)}" height="${bh.toFixed(1)}" rx="3" fill="${r.col}" opacity="${q ? 0.55 : 0.9}"><title>${esc(tip)}</title></rect>`;
      o += `<text class="labv" x="${(L + Math.max(2, w) + 6).toFixed(1)}" y="${(y + bh - 3).toFixed(1)}" font-size="12">${v}<tspan fill="#6c7a96" font-size="10"> ${letter}</tspan></text>`;
    });
  });
  const c = h.cost || {};
  const ratio = c.prediction_4_order_of_magnitude && c.prediction_4_order_of_magnitude.ratio_llm_all_over_jpp_upper_bound;
  const h2o = rv.opus && rv.opus.h2; const h2s = rv.sonnet && rv.sonnet.h2;
  let foot = '';
  if (ratio) foot += `<tspan><title>J++ 整条主运行（59 条意图）的 JEV 花费 $${(c.cost_jpp || {}).jpp_main_run_conservative_upper_bound_usd}，生成模型一次读完 25 条按 list 价折算 $${(c.cost_llm_all || {}).usd_total}</title>花费 1/${Math.round(ratio)}</tspan>`;
  if (h2o && h2s) foot += `<tspan dx="18" fill="#ff8a96"><title>H2：有价值方案里，三方以上且缺一不可的占比（Opus ${h2o.numerator}/${h2o.denominator}，Sonnet ${h2s.numerator}/${h2s.denominator}），预测 ≥ 25%，不成立</title>H2 ${h2o.numerator}/${h2o.denominator} · ${h2s.numerator}/${h2s.denominator}</tspan>`;
  o += `<text class="lab" x="${L}" y="${H - 14}" font-size="12.5" fill="#a9b6cf">${foot}</text>`;
  o += '</svg>';
  box.innerHTML = o;
}

// 集中（事后分析）：主运行里出现在最多构型里的主体；只给第一名写角色，其余悬停看
function drawFreq(box) {
  const d = REPORT && REPORT.diag && REPORT.diag.member_frequency;
  if (!d || !Array.isArray(d.top) || !d.top.length) return noData(box);
  const top = d.top.slice(0, 10);
  const W = box.clientWidth || 400; const H = box.clientHeight || 260;
  const L = 10; const R = 40; const T = 26; const B = 8;
  const max = Math.max(...top.map((x) => x.configs));
  const rowH = (H - T - B) / top.length; const bh = Math.max(4, rowH * 0.62);
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  o += `<text class="lab" x="${W - R + 34}" y="${T - 10}" text-anchor="end" font-family="SF Mono, Menlo, monospace" font-size="11" fill="#6c7a96"><title>出现过的主体数 / 构型数</title>${fmtN(d.n_members_seen)}/${fmtN(d.n_configs)}</text>`;
  top.forEach((x, i) => {
    const y = T + i * rowH + (rowH - bh) / 2;
    const w = (x.configs / max) * (W - L - R);
    o += `<rect x="${L}" y="${y.toFixed(1)}" width="${w.toFixed(1)}" height="${bh.toFixed(1)}" rx="2" fill="${i ? '#57d4ff' : '#ffd27a'}" opacity="${i ? 0.55 : 0.9}"><title>${esc(x.role || x.id)}：出现在 ${x.configs} 个构型里，涉及 ${x.intents} 条意图</title></rect>`;
    o += `<text class="labv" x="${(L + w + 6).toFixed(1)}" y="${(y + bh - 1).toFixed(1)}" font-size="11">${x.configs}</text>`;
    if (!i) o += `<text x="${L + 8}" y="${(y + bh - 3).toFixed(1)}" font-size="12.5" font-weight="600" fill="#04060c">${esc(x.role || x.id)}</text>`;
  });
  o += '</svg>';
  box.innerHTML = o;
}

// 未决（事后分析）：主运行里 kind 为 unsure 的关系，按哪道判断落在中间带分色；原因名在悬停与「i」里
const UNSURE_COLS = ['#ffb547', '#c38bff', '#ff7ad9', '#57d4ff', '#7aa7ff', '#6c7a96'];
function drawUnsure(box) {
  const d = REPORT && REPORT.diag && REPORT.diag.unsure_relations;
  if (!d || !d.n) return noData(box);
  const W = box.clientWidth || 400; const H = box.clientHeight || 260;
  const combos = Object.entries(d.by_reason_combo || {});
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  const cx = 16;
  o += `<text x="${cx}" y="${H * 0.42}" font-family="SF Mono, Menlo, monospace" font-size="${Math.min(64, H * 0.26)}" font-weight="700" fill="#ffb547"><title>未决关系 ${d.n} 条 / 全部关系 ${d.n_relations} 条（成立的直接关系 ${(d.by_kind_total || {}).direct}、转介 ${(d.by_kind_total || {}).relay}）</title>${fmtN(d.n)}<tspan font-size="${Math.min(26, H * 0.1)}" fill="#6c7a96">/${fmtN(d.n_relations)}</tspan></text>`;
  const y = H * 0.58; const bw = W - cx * 2; const bh = Math.min(26, H * 0.11);
  let x = cx;
  combos.forEach(([k, v], i) => {
    const w = (v / d.n) * bw;
    o += `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${Math.max(1, w - 2).toFixed(1)}" height="${bh.toFixed(1)}" rx="3" fill="${UNSURE_COLS[i % UNSURE_COLS.length]}" opacity="0.85"><title>${esc(k)}：${v} 条</title></rect>`;
    if (w > 34) o += `<text class="labv" x="${(x + 6).toFixed(1)}" y="${(y + bh + 16).toFixed(1)}" font-size="11">${v}</text>`;
    x += w;
  });
  o += '</svg>';
  box.innerHTML = o;
}

function drawCompare() {
  const box = $('d-cmp');
  const s = MAIN.store;
  if (REPORT && REPORT.h3) return drawCompareReport(box, REPORT.h3);
  if (!s.baselines.length) return noData(box);
  const arms = [
    { k: 'bm25', lab: '关键词' }, { k: 'vector', lab: '向量' }, { k: 'llm_all', lab: '生成模型读全部' }, { k: 'jpp', lab: 'J++ 网络' },
  ];
  const agg = {};
  for (const a of arms.slice(0, 3)) {
    const evs = s.baselines.filter((b) => b.arm === a.k);
    agg[a.k] = {
      n: evs.length,
      usd: mean(evs.map((b) => Number(b.usd) || 0)),
      ms: mean(evs.map((b) => Number(b.ms))),
      parties: mean(evs.flatMap((b) => (Array.isArray(b.results) ? b.results : []).map((r) => (Array.isArray(r.members) ? r.members.length : 1) + 1))),
    };
  }
  const its = s.intentOrder.map((iid) => s.intents.get(iid));
  const cfgs = [...s.configs.values()].filter((c) => c.status !== 'dropped');
  agg.jpp = {
    n: its.length,
    usd: mean(its.map((it) => it.usd)),
    ms: mean(its.map((it) => { const h = h1Of(s, it.id); return h ? h.ms : NaN; })),
    parties: mean(cfgs.map((c) => partiesOf(c))),
  };
  // 字数压到最少（演示页硬标准第 7 条）：行名两个字，四路名只在顶上的极小图例出现一次，条上不再重复
  const metrics = [
    { k: 'usd', lab: '花费', fmt: fmtUsd, log: true },
    { k: 'ms', lab: '耗时', fmt: fmtMs, log: true },
    { k: 'parties', lab: '方数', fmt: (v) => (v == null ? '—' : v.toFixed(1)), log: false },
  ];
  const W = box.clientWidth || 500;
  const H = box.clientHeight || 300;
  const top = 22;
  const rowsN = metrics.length;
  const bandH = (H - top - 6) / rowsN;
  const labW = 40;
  const barX = labW + 8;
  const barW = W - barX - 70;
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  const armCol = { bm25: '#7aa7ff', vector: '#57d4ff', llm_all: '#c38bff', jpp: COLORS.act };
  const short = { bm25: '关键词', vector: '向量', llm_all: '模型', jpp: 'J++' };
  let lx = barX;
  for (const a of arms) {
    o += `<rect x="${lx}" y="4" width="10" height="10" rx="2" fill="${armCol[a.k]}"/><text class="lab" x="${lx + 14}" y="13"><title>${esc(a.lab)}</title>${esc(short[a.k])}</text>`;
    lx += 26 + String(short[a.k]).length * 12;
  }
  metrics.forEach((mt, mi) => {
    const y0 = top + mi * bandH;
    o += `<text class="lab" x="0" y="${y0 + 14}" fill="#6c7a96">${esc(mt.lab)}</text>`;
    const vals = arms.map((a) => agg[a.k][mt.k]).filter((v) => Number.isFinite(v) && v > 0);
    const vmax = Math.max(...vals, 1e-9);
    const vmin = Math.min(...vals, vmax);
    const bh = Math.min(12, (bandH - 22) / arms.length - 3);
    arms.forEach((a, ai) => {
      const v = agg[a.k][mt.k];
      const y = y0 + 20 + ai * (bh + 3);
      let f = 0;
      if (Number.isFinite(v) && v > 0) {
        if (mt.log) {
          const lo = Math.log10(vmin) - 0.5; const hi = Math.log10(vmax);
          f = hi > lo ? (Math.log10(v) - lo) / (hi - lo) : 1;
        } else f = v / vmax;
      }
      o += `<rect x="${barX}" y="${y}" width="${Math.max(1.5, f * barW).toFixed(1)}" height="${bh}" rx="2" fill="${armCol[a.k]}" opacity="${a.k === 'jpp' ? 1 : 0.7}" ${a.k === 'jpp' ? 'filter="url(#glow)"' : ''}/>`;
      o += `<text class="labv" x="${barX + Math.max(1.5, f * barW) + 6}" y="${y + bh - 1}" font-size="11">${esc(Number.isFinite(v) ? mt.fmt(v) : '—')}</text>`;
    });
  });
  // 盲评：要有评审事件才显示（接口里还没有这类事件），这里不占字
  o += '</svg>';
  box.innerHTML = o;
}

function drawHist(box, get, fmt, col) {
  const s = MAIN.store;
  const a = s.intentOrder.map((iid) => get(s.intents.get(iid))).filter((v) => Number.isFinite(v));
  const b = CTL.ok ? CTL.store.intentOrder.map((iid) => get(CTL.store.intents.get(iid))).filter((v) => Number.isFinite(v)) : [];
  if (!a.length) return noData(box);
  const W = box.clientWidth || 500;
  const H = box.clientHeight || 260;
  const L = 36; const R = 34; const T = 22; const B = 30;
  const hi = niceMax(Math.max(...a, ...b));
  const bins = 12;
  const step = hi / bins;
  const cnt = (arr) => { const c = new Array(bins).fill(0); for (const v of arr) c[Math.min(bins - 1, Math.floor(v / step))]++; return c; };
  const ca = cnt(a);
  const cb = b.length ? cnt(b) : null;
  const ymax = niceMax(Math.max(...ca, ...(cb || [0])));
  const bw = (W - L - R) / bins;
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  o += axisY(L, H - B, H - T - B, ymax, (v) => String(Math.round(v)), Math.min(tickCount(ymax), ymax));
  ca.forEach((c, i) => {
    const h = (c / ymax) * (H - T - B);
    o += `<rect x="${(L + i * bw + 2).toFixed(1)}" y="${(H - B - h).toFixed(1)}" width="${Math.max(1, bw - 4).toFixed(1)}" height="${h.toFixed(1)}" rx="2" fill="${col}" opacity="0.75"><title>${fmt(i * step)}–${fmt((i + 1) * step)}：${c} 条</title></rect>`;
  });
  if (cb) cb.forEach((c, i) => {
    const h = (c / ymax) * (H - T - B);
    if (c) o += `<rect x="${(L + i * bw + 2).toFixed(1)}" y="${(H - B - h).toFixed(1)}" width="${Math.max(1, bw - 4).toFixed(1)}" height="${h.toFixed(1)}" rx="2" fill="none" stroke="#a9b6cf" stroke-width="1.4" stroke-dasharray="3 3"><title>对照 ${fmt(i * step)}–${fmt((i + 1) * step)}：${c} 条</title></rect>`;
  });
  o += '<g class="axis">';
  for (let i = 0; i <= bins; i += 3) o += `<text x="${L + i * bw}" y="${H - B + 16}" text-anchor="middle">${esc(fmt(i * step))}</text>`;
  o += '</g>';
  o += `<text class="lab" x="${W - R}" y="${T - 8}" text-anchor="end">中位 ${esc(fmt(median(a)))}${b.length ? `<tspan fill="#a9b6cf"> · ${esc(fmt(median(b)))}</tspan>` : ''}</text>`;
  o += '</svg>';
  box.innerHTML = o;
}

const INFO = {
  method: '社会是构造的（真实新闻主体除外）。页面只显示角色，不显示名字；网外的人（转介对象）显示「中间人·关系」。方案一览右下的「判断」是这个方案的根意图名下、在方案事件之前的全部语义判断次数（含对它长出的构型的判断）；「成立」是环上成立的边数 / 边数。环没闭合时（failed），各跳的出口取自环清算的全部判边。',
  h1: '每组同类意图按出现顺序编号，纵轴是判断次数，点是 5 组的中位数。三个口径并列：「命中目标」从意图进入，数到第一个包含该组预埋目标成员（去掉发信人，评估端按 stories.json 核对）的有效构型为止，是看过冒烟读数之后才改定的口径（预注册补充六）；「整波」「名次」是预注册原定的口径，数到第一个有效构型（整波数到那一波和它的组装为止，名次数到构型里最后一个被判到的接收方）。三个口径都由 metrics 的 report.json 给出。中位数只算命中（找到）的出现，刻度下的小字是痕迹组每次出现命中（找到）的组数 / 5；标题旁是第 5 次比第 1 次的变化，括号里是把没命中的按该意图全部判断数封顶计入后的变化。实线：痕迹（300 人）；虚线：无痕迹对照（300 人）；点线：150 人（痕迹）。',
  cmp: 'H1 流 25 条意图上的正式盲评（两位评审同属一家公司，独立性有限）。条是「有价值方案」的个数，每路两根：O 是 Opus 评，S 是 Sonnet 评。生成：生成模型一次读完全部主体；J++：J++ 网络主运行；检索：关键词与向量（两者都是 0；按预注册只填成员和依据，格式对它不利）。H3 预测「J++ ≥ 生成模型」不成立，推翻条件（低于 80%）触发；H2 占比 0，不成立。花费：J++ 整条主运行（59 条意图）的 JEV 花费约为生成模型一路 25 条的 1/74；生成模型一路走 claude -p 订阅，按 list 价折算 $16.13，另有 9 次调用故障未落盘，约折算 $9.3，这笔不是实付，但按折算价已超过 Nature 批的每次 5 美元。另有一轮事后变体盲评（把依据从构型摘要换成 plan.text 重评 J++），有价值数 Opus 2、Sonnet 2，不改正式判定。没有 report.json 时退回 baseline 事件，按每条意图平均：关键词、向量、生成模型一次读完全部主体三路来自 baseline 事件；J++ 网络的花费是该意图名下全部语义判断的花费，耗时是到第一个有效构型为止。方数把发信人算一方。花费和耗时用对数刻度。盲评要有评审事件才显示。',
  freq: '事后分析（看过盲评结果之后才加）。主运行里每个网内主体出现在多少个构型里，不算构型的发信人本人、不算网外的转介对象、不算丢弃的构型；条是前 10 名，第一名写出角色，其余悬停看。右上角是出现过的主体数 / 构型数。少数主体在语义各不相同的意图里反复出现，说明路由或接收方判断偏向这些「枢纽」。',
  unsure: '事后分析（看过盲评结果之后才加）。主运行里没判成的关系（kind 为 unsure）的条数 / 全部关系条数。下面的色条按哪道判断落在中间带分开：选人题（谁最可能帮上）、转介确认（真的能帮上吗）、追问树，以及它们的组合；悬停看每段的名称和条数。',
  hj: '每条意图名下的语义判断次数，包括对它长出的构型的判断。实心：本次运行；虚框：无痕迹对照。',
  hu: '每条意图名下全部语义判断的花费之和（judge.usd）。实心：本次运行；虚框：无痕迹对照。',
};
const EXTRA = {}; // 运行时追加到 INFO 后面的说明（如不含知名故事的计数）

// ---------------- 总渲染 ----------------

function render() {
  if (!MAIN.ok) return;
  if (state.tab === 'glance') GLANCE.update();
  else if (state.tab === 'overview') renderOverview();
  else if (state.tab === 'plans') renderPlans();
  else renderData();
}

// ---------------- 方案一览 ----------------

// 顶栏里看得见的汉字数（方案一览要把整屏控制在 60 字以内）
function chromeCjk() {
  const top = document.querySelector('.top');
  if (!top) return 0;
  // 方案一览时顶栏默认收起，不在屏上（拉出来时算用户主动打开的）
  if (document.body.classList.contains('g-mode')) return 0;
  const w = document.createTreeWalker(top, NodeFilter.SHOW_TEXT);
  let n = 0;
  for (let t = w.nextNode(); t; t = w.nextNode()) {
    const p = t.parentElement;
    if (!p || p.closest('[hidden], title')) continue;
    const r = p.getBoundingClientRect();
    if (!r.width || !r.height || r.bottom <= 0) continue;
    const cs = getComputedStyle(p);
    if (cs.display === 'none' || cs.visibility === 'hidden') continue;
    n += (t.textContent.match(/[\u3400-\u9fff\uf900-\ufaff]/g) || []).length;
  }
  return n;
}

// 方案一览时顶栏收起（整屏只留画面），点右上角的按钮或把鼠标移到顶边再拉出来
{
  const top = document.querySelector('.top');
  const btn = $('g-menu');
  btn.addEventListener('click', (e) => { e.stopPropagation(); document.body.classList.toggle('g-peek'); });
  document.addEventListener('mousemove', (e) => {
    if (!document.body.classList.contains('g-mode')) return;
    if (e.clientY < 10) document.body.classList.add('g-peek');
    else if (e.clientY > top.offsetHeight + 40 && !top.contains(document.activeElement) && !btn.matches(':hover')) document.body.classList.remove('g-peek');
  });
  top.addEventListener('click', (e) => { if (e.target.closest('.tab')) document.body.classList.remove('g-peek'); });
}

const GLANCE = createGlance({
  $, esc, COLORS, readingCell, exitChip, openPlan, chromeCjk,
  isBusy: () => !$('drawer').hidden,
  getStore: () => MAIN.store,
  getModels: () => planModels(MAIN.store).filter((m) => !m.fallback),
});

// ---------------- 事件绑定 ----------------

for (const b of document.querySelectorAll('.tab')) b.addEventListener('click', () => setTab(b.dataset.tab));
$('ov-chart').addEventListener('mousemove', (e) => {
  const r = e.target.closest('.col-hit');
  if (r) overviewTip(r.dataset.i, e.clientX, e.clientY); else hideTip();
});
$('ov-chart').addEventListener('mouseleave', () => hideTip());
$('ov-chart').addEventListener('click', (e) => {
  const r = e.target.closest('.col-hit');
  if (!r) return;
  const iid = r.dataset.i;
  hideTip(true);
  state.intent = MAIN.store.plans.size && [...MAIN.store.plans.values()].some((p) => MAIN.store.rootIntent(p.config) === iid) ? iid : '';
  setTab('plans');
});
$('f-intent').addEventListener('change', (e) => { state.intent = e.target.value; writeHash(); renderPlans(); });
$('f-size').addEventListener('click', (e) => { const b = e.target.closest('[data-size]'); if (!b) return; state.size = b.dataset.size; renderPlans(); });
$('f-relay').addEventListener('click', () => { state.relay = !state.relay; renderPlans(); });
$('pl-grid').addEventListener('click', (e) => { const c = e.target.closest('.card'); if (c) openPlan(c.dataset.plan); });
$('pl-grid').addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { const c = e.target.closest('.card'); if (c) { e.preventDefault(); openPlan(c.dataset.plan); } } });
$('drawer').addEventListener('click', (e) => { if (e.target.closest('[data-close]')) closePlan(); });
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') { if (!$('drawer').hidden) closePlan(); hideTip(true); }
  if (e.target.matches('input, select, textarea')) return;
  if (e.key === '1') setTab('glance');
  if (e.key === '2') setTab('overview');
  if (e.key === '3') setTab('plans');
  if (e.key === '4') setTab('data');
});
document.addEventListener('click', (e) => {
  const b = e.target.closest('.info');
  if (!b) return;
  const r = b.getBoundingClientRect();
  const k = b.id === 'method-info' ? 'method' : b.dataset.info;
  showTip(esc(`${INFO[k] || ''}${EXTRA[k] ? ` ${EXTRA[k]}` : ''}`), r.left, r.bottom, true);
});
let rT = 0;
window.addEventListener('resize', () => { clearTimeout(rT); rT = setTimeout(() => { if (state.tab === 'glance') GLANCE.resize(); else render(); }, 150); });
window.addEventListener('hashchange', () => { readHash(); setTab(state.tab); if (state.open) openPlan(state.open); });

function showError(msg) { $('err').hidden = false; $('err').textContent = msg; }

// ---------------- 启动 ----------------

window.__rx = {
  MAIN, CTL, state, get ready() { return MAIN.ok; }, get glance() { return GLANCE; },
  counts() {
    const s = MAIN.store;
    const hv = h1View();
    return { events: s.c.events, judgments: s.c.judgments, usd: s.c.usd, plans: s.c.plans, configs: s.configCount, stable: s.stableConfigCount, intents: s.c.intents, relations: s.c.relations, ctl: CTL.ok ? CTL.store.c.events : 0, groups: GROUPS.size,
      arm: s.aug.arm, run: s.aug.run, unknownTypes: s.c.unknownTypes, tasks: s.c.tasks, h1: { kind: hv.kind, tag: hv.tag, med: hv.med ? hv.med.map((x) => x.v) : null, metrics: hv.metrics ? Object.fromEntries(Object.entries(hv.metrics).map(([k, x]) => [k, x.arms.trace || null])) : null }, rings: ringStats(s) };
  },
};

async function boot() {
  readHash();
  // 大屏入口：带上同一份数据
  const back = new URLSearchParams();
  if (SSE) back.set('sse', SSE); else back.set('src', SRC);
  for (const k of ['ctl', 'groups', 'report']) if (qs.get(k)) back.set(k, qs.get(k));
  $('to-screen').href = `../screen/?${back.toString()}`;
  const isDefault = SRC === DEFAULT_SRC;
  // 旁注文件：地址参数优先；静态演示包只取 build-demo 写明存在的文件；开发时的默认事件流按文件名猜
  const side = (param, bundleKey, suffix) => {
    const u = qs.get(param);
    if (u) return [u];
    if (!isDefault) return [];
    if (BUNDLE_FILES) return BUNDLE_FILES[bundleKey] ? siblings(SRC, [`/${BUNDLE_FILES[bundleKey]}`]) : [];
    return suffix ? siblings(SRC, [suffix]) : [];
  };
  // 同类分组
  for (const u of side('groups', 'groups', '-groups.json')) { const j = await tryFetch(u, 'json'); if (j) { GROUPS = parseGroups(j); break; } }
  // H1 主口径（metrics 的 report.json）
  for (const u of side('report', 'report', null)) { const j = await tryFetch(u, 'json'); if (j) { REPORT = parseReport(j); break; } }
  // 运行参数（判断线）：事件文件同目录的 meta.json
  if (SRC) { const j = await tryFetch(siblings(SRC, ['/meta.json'])[0], 'json'); if (j && typeof j === 'object') META = j; }
  // 对照
  const cCands = side('ctl', 'ctl', '-control.jsonl');
  for (const u of cCands) {
    const txt = await tryFetch(u, 'text');
    if (txt && txt.trim()) {
      for (const ev of parseJsonl(txt).events) CTL.store.apply(ev);
      CTL.ok = CTL.store.c.intents > 0;
      CTL.url = u;
      if (CTL.ok) break;
    }
  }
  if (SSE) {
    MAIN.url = SSE;
    MAIN.player.connectSSE(SSE);
    MAIN.player.play();
    MAIN.ok = true;
    const badge = $('src-badge');
    badge.hidden = false;
    let lastN = -1;
    let lastDraw = 0;
    const loop = (now) => {
      MAIN.player.tick(16);
      badge.textContent = { live: '直播', connecting: '连接中', reconnecting: '重连中', closed: '连接已断' }[MAIN.player.status] || MAIN.player.status;
      if (now - lastDraw > 1000 && MAIN.store.c.events !== lastN) {
        lastN = MAIN.store.c.events; lastDraw = now;
        if ($('drawer').hidden) render();
      }
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  } else {
    try {
      await MAIN.player.loadJsonl(SRC);
    } catch (e) {
      showError(`${e.message}`);
      return;
    }
    MAIN.player.seekEnd();
    MAIN.url = SRC;
    MAIN.ok = true;
  }
  // 页面里写死的「花费」与「正式结论」只属于主运行：换了事件源或报告、或默认报告没加载成功时不显示
  const isMain = !SSE && isDefault && !qs.get('report') && !!REPORT && MAIN.store.aug.run === 'main';
  for (const el of document.querySelectorAll('[data-main-only]')) el.hidden = !isMain;
  setTab(state.tab);
  if (state.open) openPlan(state.open);
}

boot().catch((e) => showError(`结果页启动失败：${e.message}`));

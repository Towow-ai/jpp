// 结果页：计算结束后看「长出了什么」。只消费 接口.md 第二节的事件流，复用 shared/ 的归约器与播放器。
//   ?src=../mock/out/results.jsonl       静态事件文件（默认是开发用的社会事件流）
//   ?sse=../events%3Frun%3D<名>           走 SSE，边收边更新（每秒重画一次）
//   ?ctl=<无痕迹对照的 events.jsonl>       H1 与分布图叠加对照（默认事件流自动配 results-control.jsonl）
//   ?groups=<同类意图分组 json>            事件里的 intent 不带 group 时用（默认事件流自动配 results-groups.json）
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

const DEFAULT_SRC = '../mock/out/results.jsonl';
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

// ---------------- 数据源 ----------------

// 在 shared/Store 之外补几张索引：路由、按 about 的判断、构型事件、首个有效构型时的判断数
function augment(store) {
  const A = { routes: new Map(), judgesAbout: new Map(), cfgEvents: new Map(), h1: new Map(), disc: new Map(), groups: new Map() };
  const push = (m, k, v) => { if (!m.has(k)) m.set(k, []); m.get(k).push(v); };
  store.on((ev, ctx) => {
    if (!ev || ev.type === '__reset') { for (const m of Object.values(A)) m.clear(); return; }
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
    return { p: probs[k], probs, over: Array.isArray(r.over) ? r.over : null, top: k };
  }
  if (Number.isFinite(Number(j.p))) return { p: Number(j.p) };
  return null;
}

function nodeOf(store, id) { return store.nodes.get(id); }
// 称呼一律用角色；缺 role 的旧数据退化为 tags[0] 加 kind（shared/store.js roleOf）
function nameOf(store, id) {
  const n = store.nodes.get(id);
  return n && !n.placeholder ? roleOf(n) : String(id ?? '');
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
      if (c && c.status !== 'dropped' && c.members.length >= 3) { row.multi++; if (c.status === 'stable') row.multiStable++; }
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
  out.sort((a, b) => b.size - a.size || a.seq - b.seq);
  return out;
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
  // 关系：同一根意图下、另一端（to 或 via）落在成员里的
  const rels = [];
  for (const r of store.relations.values()) {
    if (r.root !== root && !chainIds.has(r.about)) continue;
    if (memberSet.has(r.to) || (r.via != null && memberSet.has(r.via))) rels.push(r);
  }
  rels.sort((a, b) => a.seq - b.seq);
  const ring = cfg ? cfg.ring : null;
  const edges = ring && Array.isArray(ring.edges) ? ring.edges : [];
  const cycle = ring && Array.isArray(ring.cycle) && ring.cycle.length ? ring.cycle : ids;
  const edgeJudges = edgeJudgesOf(store, cfg);
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
    id: plan.id, seq: plan.seq ?? 0, plan, cfg, rows, ids, root, chain, rels, ring, edges, edgeJudges, cycle,
    title: String(plan.title || '').trim(), judgeN: judgesBefore(store, root, plan.seq ?? Infinity),
    keyJudges, ex, size: ids.length, hasRelay: rels.some((r) => r.kind === 'relay'),
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

// 环上每条边对应的判断：about 是该构型、node 是收方、在 ring 事件之前的最后一次（契约里 edges 不带判断 id）
function edgeJudgesOf(store, cfg) {
  const ring = cfg ? cfg.ring : null;
  const edges = ring && Array.isArray(ring.edges) ? ring.edges : [];
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
      avs += `<text x="${lx.toFixed(1)}" y="${ly.toFixed(1)}" text-anchor="${anchor}" font-size="12.5" font-weight="600" fill="#e8eefb">${esc(nm)}</text>`;
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
    const n = Array.isArray(p.members) && p.members.length ? p.members.length : ((s.configs.get(p.config) || {}).members || []).length;
    if (n >= 3) multiPlans++;
  }
  $('ov-line').innerHTML = multiPlans
    ? `长出 <b class="gold">${fmtN(multiPlans)}</b> 个三方以上的方案`
    : plans ? `长出 <b class="gold">${fmtN(plans)}</b> 个方案` : `长出 <b class="gold">${fmtN(s.configCount)}</b> 个构型`;
  const h1 = h1Series(MAIN);
  let sub = '';
  if (h1) {
    const first = h1.med[0] && h1.med[0].v;
    const lastM = [...h1.med].reverse().find((x) => x.v != null && x.k > 1);
    if (first && lastM) {
      const pct = Math.round((1 - lastM.v / first) * 100);
      sub = pct >= 0 ? `同类第 ${lastM.k} 次，判断少用 <b>${pct}%</b>` : `同类第 ${lastM.k} 次，判断多用 <b>${-pct}%</b>`;
    }
  }
  $('ov-sub').innerHTML = sub;
  const tiles = [
    { k: '意图', v: fmtN(intents), c: 'rgba(87,212,255,0.55)' },
    { k: '构型', v: fmtN(s.configCount), s: `<em>稳定</em>${fmtN(s.stableConfigCount)}`, c: 'rgba(255,210,122,0.55)' },
    { k: '方案', v: fmtN(plans), c: 'rgba(62,240,176,0.55)' },
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
  return `<article class="card" tabindex="0" data-plan="${esc(m.id)}" data-size="${m.size}" data-relay="${m.hasRelay ? 1 : 0}" aria-label="方案 ${esc(planTitle(s, m))}">
    ${ringSvg(s, m, { labels: true })}
    <div class="card-foot">
      <h3 class="ptitle" title="${esc(planTitle(s, m))}">${esc(planTitle(s, m))}</h3>
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
  const ringLabel = m.ring ? `${esc(m.ring.status)}` : '无环清算';
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
        ${m.plan.text ? `<p class="planline">${esc(m.plan.text)}</p>` : ''}
      </div>
    </div>
    <h4>谁做什么</h4>
    <table class="mt"><colgroup><col style="width:17%"><col><col><col><col><col></colgroup>
      <thead><tr><th>成员</th><th>做什么</th><th>给什么</th><th>得什么</th><th>还缺</th><th>第一小步</th></tr></thead>
      <tbody>${rows.map((r) => `<tr><td><span class="avw">${glyphIcon(kindOf(s, r.id), m.color(r.id), 22)}</span>${esc(nameOf(s, r.id))}${r.id === m.origin ? ' <span class="muted">发信人</span>' : ''}${realNameOf(s, r.id) ? `<div class="realname">${esc(realNameOf(s, r.id))}</div>` : ''}</td>
        <td>${esc(r.do || '—')}</td><td>${esc(r.give || '—')}</td><td>${esc(r.get || '—')}</td><td>${esc(r.missing || '—')}</td><td>${esc(r.first_step || '—')}</td></tr>`).join('')}</tbody>
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
      h: `<span class="tt">${esc(kindLab)}</span><span>${esc(nameOf(s, r.from))} → ${esc(nameOf(s, r.to))}${r.via ? ` <span class="muted">经 ${esc(nameOf(s, r.via))}</span>` : ''}</span><span class="n">${esc(r.id)}</span>`,
      b: `${items.map((x) => x.html).join('')}${r.trigger ? `<div class="trigger">${esc(typeof r.trigger === 'object' ? (r.trigger.ctx ?? JSON.stringify(r.trigger)) : r.trigger)}</div>` : ''}`,
    });
  }
  // 构型：形成、复核的判断、环清算
  for (const cfg of m.chain) {
    const evs = A.cfgEvents.get(cfg.id) || [];
    const first = evs[0];
    const ej = cfg === m.cfg ? m.edgeJudges : edgeJudgesOf(s, cfg);
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
      const edges = Array.isArray(ring.edges) ? ring.edges : [];
      steps.push({
        seq: ring.seq ?? 0, c: RING_OK.has(ring.status) ? COLORS.act : COLORS.no,
        h: `<span class="tt">环清算</span><span class="mono">${esc(ring.status)}</span><span class="n">${edges.length} 条边</span>`,
        b: edges.map((e, k) => {
          const j = ej[k];
          return `<div class="fold">${esc(nameOf(s, e.from))} → ${esc(nameOf(s, e.to))}：${esc(e.give || '')}</div>${j ? judgeRow(s, j) : `<div class="jrow"><span class="who">${esc(nameOf(s, e.to))}</span><span class="q muted">没有找到对应的判断事件</span>${readingCell({ exit: e.exit })}</div>`}`;
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
  drawHist($('d-hj'), (it) => it.judges, (v) => fmtN(Math.round(v)), COLORS.direct);
  drawHist($('d-hu'), (it) => it.usd, (v) => fmtUsd(v), COLORS.config);
  // 不含知名故事（demo_only）的一行
  const s = MAIN.store;
  const demoCfg = (c) => c.members.some((id) => { const n = s.nodes.get(id); return n && n.demo_only; });
  let cfgN = 0; let planN = 0; let demoAny = false;
  for (const c of s.configs.values()) { if (c.status === 'dropped') continue; if (demoCfg(c)) { demoAny = true; continue; } cfgN++; }
  for (const p of s.plans.values()) { const c = s.configs.get(p.config); if (c && demoCfg(c)) continue; planN++; }
  $('d-foot').innerHTML = demoAny || [...s.nodes.values()].some((n) => n.demo_only)
    ? `<em>不含知名故事</em>　构型 ${fmtN(cfgN)} · 方案 ${fmtN(planN)}${CTL.ok ? `　<em>对照</em> ${esc(CTL.url)}` : ''}`
    : (CTL.ok ? `<em>对照</em> ${esc(CTL.url)}` : '');
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
  const tr = h1Series(MAIN);
  const ct = CTL.ok ? h1Series(CTL) : null;
  if (!tr) return noData(box);
  const W = box.clientWidth || 600;
  const H = box.clientHeight || 300;
  const L = 48; const R = 18; const T = 40; const B = 34;
  const maxK = Math.max(tr.maxK, ct ? ct.maxK : 0);
  let maxV = 0;
  for (const arr of tr.byGroup.values()) for (const a of arr) if (Number.isFinite(a.v)) maxV = Math.max(maxV, a.v);
  if (ct) for (const x of ct.med) if (x.v != null) maxV = Math.max(maxV, x.v);
  maxV = niceMax(maxV * 1.08);
  const X = (k) => L + (maxK <= 1 ? (W - L - R) / 2 : ((k - 1) / (maxK - 1)) * (W - L - R));
  const Y = (v) => H - B - (v / maxV) * (H - T - B);
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  o += axisY(L, H - B, H - T - B, maxV, (v) => fmtN(Math.round(v)));
  o += '<g class="axis">';
  for (let k = 1; k <= maxK; k++) o += `<line x1="${X(k)}" x2="${X(k)}" y1="${T}" y2="${H - B}" stroke-dasharray="2 6"/><text x="${X(k)}" y="${H - B + 16}" text-anchor="middle">${k}</text>`;
  o += '</g>';
  o += `<text class="lab" x="${W - R}" y="${H - 6}" text-anchor="end">同类意图第几次出现</text>`;
  o += `<text class="lab" x="${L - 40}" y="${T - 22}">判断次数</text>`;
  const line = (pts) => pts.filter((p) => p[1] != null).map((p, i) => `${i ? 'L' : 'M'}${X(p[0]).toFixed(1)},${Y(p[1]).toFixed(1)}`).join(' ');
  let gi = 0;
  for (const [g, arr] of tr.byGroup) {
    const col = GROUP_HUES[gi++ % GROUP_HUES.length];
    const d = line(arr.map((a) => [a.k, Number.isFinite(a.v) ? a.v : null]));
    if (d) o += `<path d="${d}" fill="none" stroke="${col}" stroke-width="1.2" opacity="0.28"><title>${esc(g)}</title></path>`;
  }
  if (ct) {
    const d = line(ct.med.map((x) => [x.k, x.v]));
    o += `<path d="${d}" fill="none" stroke="#a9b6cf" stroke-width="2.2" stroke-dasharray="7 6" opacity="0.85"/>`;
    for (const x of ct.med) if (x.v != null) o += `<circle cx="${X(x.k)}" cy="${Y(x.v)}" r="4" fill="#04060c" stroke="#a9b6cf" stroke-width="2"><title>对照 第 ${x.k} 次 中位数 ${x.v}（${x.n} 组）</title></circle>`;
  }
  const d = line(tr.med.map((x) => [x.k, x.v]));
  o += `<path d="${d}" fill="none" stroke="${COLORS.act}" stroke-width="3" filter="url(#glow)"/>`;
  for (const x of tr.med) {
    if (x.v == null) continue;
    o += `<circle cx="${X(x.k)}" cy="${Y(x.v)}" r="5" fill="${COLORS.act}" filter="url(#glow)"><title>痕迹 第 ${x.k} 次 中位数 ${x.v}（${x.n} 组）</title></circle>`;
    o += `<text class="labv" x="${X(x.k)}" y="${Y(x.v) - 11}" text-anchor="middle">${fmtN(Math.round(x.v))}</text>`;
  }
  // 图例
  const lx = W - R - 190;
  o += `<g transform="translate(${lx},${T - 26})"><line x1="0" x2="22" y1="0" y2="0" stroke="${COLORS.act}" stroke-width="3"/><text class="lab" x="28" y="4">痕迹</text>`;
  o += ct ? `<line x1="78" x2="100" y1="0" y2="0" stroke="#a9b6cf" stroke-width="2" stroke-dasharray="6 4"/><text class="lab" x="106" y="4">无痕迹对照</text>` : `<text class="lab" x="78" y="4" fill="#6c7a96">无对照运行</text>`;
  o += '</g></svg>';
  box.innerHTML = o;
}

function drawCompare() {
  const box = $('d-cmp');
  const s = MAIN.store;
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
    parties: mean(cfgs.map((c) => c.members.length)),
  };
  const metrics = [
    { k: 'usd', lab: '每条花费', fmt: fmtUsd, log: true },
    { k: 'ms', lab: '每条耗时', fmt: fmtMs, log: true },
    { k: 'parties', lab: '平均几方', fmt: (v) => (v == null ? '—' : v.toFixed(1)), log: false },
  ];
  const W = box.clientWidth || 500;
  const H = box.clientHeight || 300;
  const rowsN = metrics.length + 1;
  const bandH = (H - 6) / rowsN;
  const labW = Math.min(118, W * 0.3);
  const barX = labW + 8;
  const barW = W - barX - 70;
  let o = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">`;
  const armCol = { bm25: '#7aa7ff', vector: '#57d4ff', llm_all: '#c38bff', jpp: COLORS.act };
  metrics.forEach((mt, mi) => {
    const y0 = mi * bandH;
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
      o += `<text x="${barX - 6}" y="${y + bh - 1}" text-anchor="end" font-size="11" fill="${a.k === 'jpp' ? '#e8eefb' : '#a9b6cf'}">${esc(a.lab)}</text>`;
      o += `<rect x="${barX}" y="${y}" width="${Math.max(1.5, f * barW).toFixed(1)}" height="${bh}" rx="2" fill="${armCol[a.k]}" opacity="${a.k === 'jpp' ? 1 : 0.7}" ${a.k === 'jpp' ? 'filter="url(#glow)"' : ''}/>`;
      o += `<text class="labv" x="${barX + Math.max(1.5, f * barW) + 6}" y="${y + bh - 1}" font-size="11">${esc(Number.isFinite(v) ? mt.fmt(v) : '—')}</text>`;
    });
  });
  const y0 = metrics.length * bandH;
  o += `<text class="lab" x="0" y="${y0 + 14}" fill="#6c7a96">盲评</text><text x="${barX}" y="${y0 + 14}" font-size="12" fill="#6c7a96">暂无数据</text>`;
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
  o += `<text class="lab" x="${W - R}" y="${T - 8}" text-anchor="end">中位数 ${esc(fmt(median(a)))}${b.length ? ` · 对照 ${esc(fmt(median(b)))}` : ''}</text>`;
  o += '</svg>';
  box.innerHTML = o;
}

const INFO = {
  method: '社会是构造的（真实新闻主体除外）。页面只显示角色，不显示名字。方案一览右下的「判断」是这个方案的根意图名下、在方案事件之前的全部语义判断次数（含对它长出的构型的判断）；「成立」是环上成立的边数 / 边数。',
  h1: '每组同类意图按出现顺序编号。纵轴：从意图进入，到它名下第一个稳定构型出现为止，归到这条意图的语义判断次数（有 discovery 事件时取其 judgments_at）。亮线：本次运行各组中位数；淡线：各组；虚线：无痕迹对照。分组取自 intent 事件的 group 字段，没有时读 groups 旁注文件。',
  cmp: '按每条意图平均。关键词、向量、生成模型一次读完全部主体三路来自 baseline 事件；J++ 网络的花费是该意图名下全部语义判断的花费，耗时是到第一个稳定构型为止。方数把发信人算一方。花费和耗时用对数刻度。盲评要有评审事件才能显示。',
  hj: '每条意图名下的语义判断次数，包括对它长出的构型的判断。实心：本次运行；虚框：无痕迹对照。',
  hu: '每条意图名下全部语义判断的花费之和（judge.usd）。实心：本次运行；虚框：无痕迹对照。',
};

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
  getModels: () => planModels(MAIN.store),
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
  showTip(esc(INFO[b.id === 'method-info' ? 'method' : b.dataset.info] || ''), r.left, r.bottom, true);
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
    return { events: s.c.events, judgments: s.c.judgments, usd: s.c.usd, plans: s.c.plans, configs: s.configCount, stable: s.stableConfigCount, intents: s.c.intents, relations: s.c.relations, ctl: CTL.ok ? CTL.store.c.events : 0, groups: GROUPS.size };
  },
};

async function boot() {
  readHash();
  // 大屏入口：带上同一份数据
  const back = new URLSearchParams();
  if (SSE) back.set('sse', SSE); else back.set('src', SRC);
  for (const k of ['ctl', 'groups']) if (qs.get(k)) back.set(k, qs.get(k));
  $('to-screen').href = `../screen/?${back.toString()}`;
  // 同类分组
  const gUrl = qs.get('groups');
  const isDefault = SRC === DEFAULT_SRC;
  const gCands = gUrl ? [gUrl] : (isDefault ? siblings(SRC, ['-groups.json']) : []);
  for (const u of gCands) { const j = await tryFetch(u, 'json'); if (j) { GROUPS = parseGroups(j); break; } }
  // 对照
  const cUrl = qs.get('ctl');
  const cCands = cUrl ? [cUrl] : (isDefault ? siblings(SRC, ['-control.jsonl']) : []);
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
  setTab(state.tab);
  if (state.open) openPlan(state.open);
}

boot().catch((e) => showError(`结果页启动失败：${e.message}`));

// 大屏入口。数据来源由地址参数决定（部署在子路径下也能用，全部是相对地址）：
//   ?src=../mock/out/events.jsonl      静态事件文件（默认）
//   ?sse=../events%3Frun%3Ddemo         SSE 直播或服务端重放（GET /events）
//   &speed=4 &autoplay=0 &compress=0 &seek=end &bloom=1 &color=kind &compare=1 &select=node:p001

import { Store, exitClass, roleOf } from '../shared/store.js';
import { Player } from '../shared/player.js';
import { NetScene, COLORS } from './scene.js';

const qs = new URLSearchParams(location.search);
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

const store = new Store();
const player = new Player(store);
let scene;
try {
  scene = new NetScene($('gl'), store, { bloom: qs.get('bloom') === '1', colorBy: qs.get('color') || 'industry' });
} catch (e) {
  showError(`无法创建 3D 画布（WebGL）：${e.message}`);
  throw e;
}

function showError(msg) {
  $('err').hidden = false;
  $('err').textContent = msg;
}

// ---------- 称呼：页面一律显示角色，名字只在节点详情里小字出现一次 ----------
function nodeName(id) {
  const n = store.nodes.get(id);
  if (n && !n.placeholder) return roleOf(n);
  return String(id ?? '');
}
function nodeLink(id) {
  const n = store.nodes.get(id);
  if (n && !n.placeholder) return `<a class="link" data-sel="node:${esc(id)}">${esc(roleOf(n))}</a>`;
  return `<span title="不在网内">${esc(id)}</span>`;
}
function aboutText(about) {
  const it = store.intents.get(about);
  if (it) return `意图 ${it.id}「${it.text}」`;
  const c = store.configs.get(about);
  if (c) return `构型 ${c.id}（${c.members.length} 方）`;
  return String(about ?? '');
}
function aboutLink(about) {
  if (store.intents.has(about)) return `<a class="link" data-sel="intent:${esc(about)}">${esc(aboutText(about))}</a>`;
  if (store.configs.has(about)) return `<a class="link" data-sel="config:${esc(about)}">${esc(aboutText(about))}</a>`;
  return esc(about);
}
function exitChip(exit) {
  const ex = exitClass(exit);
  const label = { act: '成立', ignore: '无关', unsure: '拿不准', pick: '选项', unknown: '?' }[ex];
  return `<span class="chip ex" style="--c:${COLORS[ex]}">${esc(exit)} · ${label}</span>`;
}
const fmtUsd = (x) => (x >= 1 ? `$${x.toFixed(2)}` : x >= 0.01 ? `$${x.toFixed(3)}` : `$${x.toFixed(5)}`);
function fmtTime(ms) {
  const s = Math.max(0, ms) / 1000;
  const m = Math.floor(s / 60);
  return `${m}:${(s - m * 60).toFixed(1).padStart(4, '0')}`;
}
const fmtN = (n) => n.toLocaleString('en-US');

// ---------- 播放控制 ----------
const SPEEDS = [1, 2, 4, 8, 16, 64];
function buildSpeeds() {
  const box = $('speeds');
  box.innerHTML = SPEEDS.map((s) => `<button class="btn" data-speed="${s}">${s}×</button>`).join('');
  box.addEventListener('click', (e) => {
    const b = e.target.closest('[data-speed]');
    if (b) { player.setSpeed(Number(b.dataset.speed)); }
  });
}
function renderPlayer() {
  $('btn-play').textContent = player.playing ? '暂停' : '播放';
  for (const b of $('speeds').children) b.classList.toggle('on', Number(b.dataset.speed) === player.speed);
  $('progress-bar').style.width = `${(player.progress * 100).toFixed(2)}%`;
  const live = player.mode === 'sse';
  $('progress').style.visibility = live ? 'hidden' : 'visible';
  $('btn-end').hidden = live;
  $('speeds').hidden = live;
  const badge = $('src-badge');
  badge.className = 'badge';
  if (live) {
    badge.textContent = { live: '直播 · SSE', connecting: '连接中', reconnecting: '重连中', closed: '连接已断' }[player.status] || player.status;
    if (player.status === 'live') badge.classList.add('badge-live');
  } else {
    badge.textContent = player.status === 'loading' ? '读取中' : '录制重放';
  }
  scene.speedHint = player.mode === 'sse' ? 1 : player.speed;
}
player.onStatus = renderPlayer;
player.onBulk = (start) => {
  if (start) { scene.silent = true; return; }
  scene.silent = false;
  scene.syncAll();
};
player.onSeek = () => {
  scene.syncAll();
  feedItems.length = 0;
  renderFeed();
  lastIntentShown = null;
  updateHud(true);
};

$('btn-play').addEventListener('click', () => player.toggle());
$('btn-end').addEventListener('click', () => seekSilently(player.totalT));
$('chk-compress').addEventListener('change', (e) => { player.compressIdle = e.target.checked; });
$('sel-color').addEventListener('change', (e) => { scene.setColorBy(e.target.value); renderLegend(); });
$('progress').addEventListener('click', (e) => {
  const r = e.currentTarget.getBoundingClientRect();
  const f = (e.clientX - r.left) / r.width;
  seekSilently(player.firstT + f * (player.totalT - player.firstT));
});
window.addEventListener('keydown', (e) => {
  if (e.target.matches('input, select, textarea')) return;
  if (e.code === 'Space') { e.preventDefault(); player.toggle(); }
  if (e.key === 'c' || e.key === 'C') toggleCompare();
  if (e.key === 'Escape') { closeDetail(); $('compare').hidden = true; }
  const i = '123456'.indexOf(e.key);
  if (i >= 0) player.setSpeed(SPEEDS[i]);
});

function seekSilently(t) {
  scene.silent = true;
  player.seek(t);
  scene.silent = false;
}

// ---------- 事件流（左下） ----------
const feedItems = [];
const FEED_MAX = 7;
function pushFeed(item) {
  item.fresh = true;
  feedItems.unshift(item);
  if (feedItems.length > FEED_MAX) feedItems.length = FEED_MAX;
  feedDirty = true;
}
let feedDirty = false;
function renderFeed() {
  $('feed').innerHTML = feedItems.map((f) => `<li class="${f.fresh ? 'new' : ''}" data-sel="${esc(f.sel)}"><span class="tag" style="--c:${f.color}">${esc(f.tag)}</span><span class="txt">${esc(f.text)}</span></li>`).join('');
  for (const f of feedItems) f.fresh = false;
  feedDirty = false;
}
$('feed').addEventListener('click', (e) => {
  const li = e.target.closest('[data-sel]');
  if (li) select(li.dataset.sel);
});

store.on((ev, ctx) => {
  if (scene.silent) return;
  switch (ev.type) {
    case 'intent':
      pushFeed({ tag: '意图', color: '#ffffff', text: `${nodeName(ev.from)}：「${ev.text}」`, sel: `intent:${ev.id}` });
      break;
    case 'relation': {
      const kind = { direct: '直接关系', relay: '转介', unsure: '拿不准' }[ev.kind] || ev.kind;
      const col = ev.kind === 'relay' ? COLORS.relay : ev.kind === 'unsure' ? COLORS.relUnsure : COLORS.direct;
      const via = ev.via != null ? `经 ${nodeName(ev.via)} → ` : '';
      pushFeed({ tag: kind, color: col, text: `${nodeName(ev.from)} ⇢ ${via}${nodeName(ev.to)}`, sel: `relation:${ev.id}` });
      break;
    }
    case 'config':
      if (!ctx.prevStatus || (ev.status === 'stable' && ctx.prevStatus !== 'stable')) {
        const st = { forming: '构型长出', stable: '构型稳定', dropped: '构型散了' }[ev.status] || ev.status;
        pushFeed({ tag: st, color: COLORS.config, text: `${ev.members.length} 方：${ev.members.map(nodeName).join('、')}`, sel: `config:${ev.id}` });
      }
      break;
    case 'ring':
      pushFeed({ tag: ctx.closed ? '环闭合' : '环未闭合', color: ctx.closed ? COLORS.act : COLORS.unsure, text: (ev.cycle || []).map(nodeName).join(' → '), sel: `config:${ev.config}` });
      break;
    case 'plan':
      pushFeed({ tag: '方案', color: '#ffffff', text: `${(ev.members || []).length} 方方案已翻译给每个人`, sel: `config:${ev.config}` });
      break;
    case 'node_join':
      if (ctx.node && (ctx.node.lateJoin || ctx.node.live)) {
        pushFeed({ tag: ctx.node.live ? '现场入网' : '入网', color: ctx.node.live ? COLORS.live : '#9fd8ff', text: roleOf(ctx.node), sel: `node:${ctx.node.id}` });
      }
      break;
    case 'feedback':
      pushFeed({ tag: { yes: '点头', edit: '修改', no: '拒绝' }[ev.verdict] || ev.verdict, color: COLORS[ev.verdict] || '#fff', text: `${nodeName(ev.member)}${ev.note ? '：' + ev.note : ''}`, sel: `node:${ev.member}` });
      break;
    default:
  }
});

// ---------- 当前意图横幅 ----------
let lastIntentShown = null;
function currentIntent() {
  const id = store.intentOrder[store.intentOrder.length - 1];
  return id ? store.intents.get(id) : null;
}
function renderIntent() {
  const it = currentIntent();
  if (!it) { $('intent').hidden = true; return; }
  $('intent').hidden = false;
  if (lastIntentShown !== it.id) {
    lastIntentShown = it.id;
    $('intent-id').textContent = `${it.id} · 第 ${it.index + 1} 条`;
    $('intent-from').textContent = `来自 ${nodeName(it.from)}`;
    const el = $('intent-text');
    el.textContent = `「${it.text}」`;
    el.classList.remove('pop'); void el.offsetWidth; el.classList.add('pop');
    labels.set('origin', { id: it.from, cls: 'origin', text: nodeName(it.from), role: '发信人', until: Infinity });
  }
  const dt = Math.max(0, it.lastT - it.t);
  const stable = [...it.configs].filter((c) => store.configs.get(c)?.status === 'stable').length;
  $('intent-stats').innerHTML = `<span>判断 <b>${fmtN(it.judges)}</b></span><span>关系 <b>${it.relations.length}</b></span><span>构型 <b>${it.configs.size}</b>${stable ? `（稳定 ${stable}）` : ''}</span><span>用时 <b>${(dt / 1000).toFixed(1)}s</b></span>`;
  scene.focusIntent = it.id;
}

// ---------- 标签 ----------
const labels = new Map(); // key -> {id, cls, text, role, until}
const labelEls = new Map();
function renderLabels() {
  const now = performance.now();
  const box = $('labels');
  for (const [k, l] of labels) if (l.until < now) labels.delete(k);
  for (const [k, el] of labelEls) if (!labels.has(k)) { el.remove(); labelEls.delete(k); }
  for (const [k, l] of labels) {
    let el = labelEls.get(k);
    if (!el) {
      el = document.createElement('div');
      el.className = `lbl ${l.cls || ''}`;
      box.appendChild(el);
      labelEls.set(k, el);
    }
    const html = `${esc(l.text)}${l.role ? `<span class="role">${esc(l.role)}</span>` : ''}`;
    if (el._html !== html) { el.innerHTML = html; el._html = html; }
    const p = scene.screenPosOf(l.id);
    if (!p) { el.style.opacity = 0; continue; }
    el.style.opacity = 1;
    el.style.left = `${p.x}px`;
    el.style.top = `${p.y}px`;
  }
}
scene.onJoinFx = (id, live) => {
  const n = store.nodes.get(id);
  if (!n) return;
  labels.set(`join:${id}`, { id, cls: live ? 'live' : '', text: roleOf(n), role: live ? '现场参与者' : '入网', until: performance.now() + (live ? 12000 : 5000) });
  if (live) {
    document.querySelectorAll('.toast').forEach((x) => x.remove());
    const t = document.createElement('div');
    t.className = 'toast';
    t.innerHTML = `<small>现场参与者入网</small>${esc(roleOf(n))}`;
    document.body.appendChild(t);
    setTimeout(() => t.remove(), 3300);
  }
};
scene.onConfigFx = (cfg, prev) => {
  if (cfg.status !== 'stable' || prev === 'stable') return;
  for (const k of [...labels.keys()]) if (k.startsWith('cfg:')) labels.delete(k);
  cfg.members.slice(0, 6).forEach((m) => {
    labels.set(`cfg:${m}`, { id: m, cls: 'cfg', text: nodeName(m), until: performance.now() + 5000 });
  });
};

// ---------- 计数面板 ----------
const spark = $('spark');
const sctx = spark.getContext('2d');
function drawSpark() {
  const W = spark.width, H = spark.height;
  sctx.clearRect(0, 0, W, H);
  const end = Math.floor(store.c.elapsed / 500);
  const N = 60;
  const vals = [];
  for (let i = N - 1; i >= 0; i--) vals.push((store.judgeBuckets.get(end - i) || 0) * 2);
  const max = Math.max(10, ...vals);
  const bw = W / N;
  for (let i = 0; i < N; i++) {
    const h = (vals[i] / max) * (H - 4);
    const g = sctx.createLinearGradient(0, H - h, 0, H);
    g.addColorStop(0, 'rgba(62,240,176,0.95)');
    g.addColorStop(1, 'rgba(62,240,176,0.15)');
    sctx.fillStyle = g;
    sctx.fillRect(i * bw + 1, H - h, bw - 2, h);
  }
}
function updateHud(force = false) {
  const c = store.c;
  $('c-judge').textContent = fmtN(store.shownJudgments);
  $('c-usd').textContent = fmtUsd(store.shownUsd);
  $('c-time').textContent = fmtTime(c.elapsed - (player.firstT || 0));
  $('c-cfg').textContent = fmtN(store.configCount);
  const st = store.stableConfigCount;
  $('c-cfg-stable').textContent = st ? `稳定 ${st}` : '';
  $('c-rel').textContent = fmtN(c.relations);
  $('c-ring').textContent = c.rings ? `${c.ringsClosed}/${c.rings}` : '0';
  $('c-plan').textContent = fmtN(c.plans);
  $('c-intent').textContent = fmtN(c.intents);
  $('c-alive').textContent = fmtN(store.aliveCount);
  $('c-enrich').textContent = fmtN(c.enrich);
  const fb = c.feedback;
  $('c-fb').textContent = `${fb.yes}/${fb.edit}/${fb.no}`;
  $('c-fb').title = '点头/修改/拒绝';
  $('c-gen').textContent = store.lastMetric && store.lastMetric.gen_calls != null ? fmtN(store.lastMetric.gen_calls) : '—';
  const s = scene.stats;
  const ratio = s.effectsIn ? Math.min(1, (s.effectsIn - s.effectsDropped - scene.queueLen) / s.effectsIn) : 1;
  $('c-sample').textContent = `${Math.round(ratio * 100)}%`;
  $('c-rate').textContent = fmtN(Math.round(store.judgeRate()));
  $('c-peak').textContent = fmtN(Math.round(store.peakRate()));
  const ex = c.exits;
  const tot = c.judgments || 1;
  const order = ['act', 'unsure', 'pick', 'ignore'];
  $('exit-bar').innerHTML = order.map((k) => `<span style="width:${(ex[k] / tot) * 100}%;background:${COLORS[k === 'ignore' ? 'unknown' : k]}"></span>`).join('');
  const nm = { act: '成立', unsure: '拿不准', pick: '选项', ignore: '无关' };
  $('exit-legend').innerHTML = order.map((k) => `<span><i style="--c:${COLORS[k === 'ignore' ? 'unknown' : k]}"></i>${nm[k]} ${fmtN(ex[k])}</span>`).join('');
  drawSpark();
  renderIntent();
  if (!$('compare').hidden) renderCompare(force);
  if (!$('detail').hidden && selected) renderDetail(selected, true);
}

function renderLegend() {
  // 行业多时只列人数最多的 14 个
  const cnt = new Map();
  for (const n of store.nodes.values()) if (!n.placeholder && n.alive) { const k = (n.tags && n.tags[0]) || '未标行业'; cnt.set(k, (cnt.get(k) || 0) + 1); }
  let items = scene.legend();
  if (scene.colorBy === 'industry') items = items.filter(([k]) => cnt.has(k)).sort((a, b) => cnt.get(b[0]) - cnt.get(a[0])).slice(0, 14);
  $('legend-ind').innerHTML = items.map(([k, c]) => `<span class="lg"><i style="--c:${c}"></i>${esc(k)}</span>`).join('');
}

// ---------- 选中与详情 ----------
let selected = null;
function select(key) {
  if (!key) return;
  const i = key.indexOf(':');
  const type = key.slice(0, i), id = key.slice(i + 1);
  if (type === 'intent') {
    openCompare(id);
    return;
  }
  selected = { type, id };
  scene.selected = selected;
  renderDetail(selected);
  if (type === 'node') labels.set('sel', { id, cls: '', text: nodeName(id), role: '已选中', until: Infinity });
  else labels.delete('sel');
}
function closeDetail() {
  selected = null;
  scene.selected = null;
  labels.delete('sel');
  $('detail').hidden = true;
}
$('detail-close').addEventListener('click', closeDetail);
$('detail').addEventListener('click', (e) => {
  const a = e.target.closest('[data-sel]');
  if (a) select(a.dataset.sel);
});

let downAt = null;
$('gl').addEventListener('pointerdown', (e) => { downAt = [e.clientX, e.clientY]; });
$('gl').addEventListener('pointerup', (e) => {
  if (!downAt) return;
  const moved = Math.hypot(e.clientX - downAt[0], e.clientY - downAt[1]);
  downAt = null;
  if (moved > 5) return;
  const hit = scene.pick(e.clientX, e.clientY);
  if (!hit) { closeDetail(); return; }
  if (hit.type === 'ghost') {
    const rel = [...store.relations.values()].find((r) => `ghost:${r.to}` === hit.id);
    if (rel) select(`relation:${rel.id}`);
    return;
  }
  select(`${hit.type}:${hit.id}`);
});

function judgeItem(j) {
  if (!j || typeof j !== 'object') return '';
  return `<li><div class="q">${esc(j.q)}</div><div class="chips">${exitChip(j.exit)}<span class="chip">${nodeLink(j.node)}</span></div>
    <div class="meta">关于 ${aboutLink(j.about)}<br>判断 ${esc(j.id)} · 账本键 ${esc(j.key)} · ${j.usd != null ? fmtUsd(Number(j.usd)) : ''}</div></li>`;
}

// 构型的依据：同一条根意图下、两端都在成员里的关系，各自的触发上下文与判断
function cfgBasis(c) {
  const mem = new Set(c.members);
  const rels = [...store.relations.values()].filter((r) => r.root === c.root && mem.has(r.from) && (mem.has(r.to) || mem.has(r.via)));
  if (!rels.length) return '';
  return `<h4>依据（${rels.length} 条关系）</h4><ul class="jlist">${rels.slice(0, 8).map((r) => {
    const js = (r.judges || []).map((id) => store.judges.get(id)).filter(Boolean);
    const j = js[0];
    return `<li><a class="link" data-sel="relation:${esc(r.id)}">${esc(nodeName(r.from))} ⇢ ${r.via ? esc(nodeName(r.via)) + ' → ' : ''}${esc(nodeName(r.to))}</a>
      <div class="q">${esc(r.trigger || '')}</div>
      ${j ? `<div class="meta">判的题：${esc(j.q)}</div><div class="chips">${exitChip(j.exit)}</div>` : ''}</li>`;
  }).join('')}</ul>`;
}

function renderDetail(sel, soft = false) {
  const body = $('detail-body');
  let html = '';
  if (sel.type === 'node') {
    const n = store.nodes.get(sel.id);
    if (!n) return;
    const badges = [];
    if (n.synthetic === true) badges.push('<span class="chip">合成主体</span>');
    if (n.synthetic === false) badges.push('<span class="chip warn">真实公开报道</span>');
    if (n.demo_only) badges.push('<span class="chip warn">知名故事 · 仅演示</span>');
    if (n.live) badges.push(`<span class="chip ex" style="--c:${COLORS.live}">现场参与者</span>`);
    if (!n.alive) badges.push('<span class="chip">已离网</span>');
    const kind = { person: '个人', company: '公司', org: '机构' }[n.kind] || n.kind;
    const refs = Object.entries(n.ref || {});
    const js = n.judges.slice(-8).reverse().map((id) => store.judges.get(id) || id);
    const rels = n.relations.slice(-8).reverse().map((id) => store.relations.get(id)).filter(Boolean);
    html = `<h3>${esc(roleOf(n))}</h3>${n.name && n.name !== roleOf(n) ? `<div class="realname">${esc(n.name)}</div>` : ''}<div class="chips"><span class="chip">${esc(kind)}</span>${(n.tags || []).map((t) => `<span class="chip">${esc(t)}</span>`).join('')}${badges.join('')}</div>
      <h4>材料摘要</h4><p>${esc(n.summary || '（事件里没有摘要）')}</p>
      <h4>公共参照读数（坐标第一层 · 更新 ${n.coordN || 0} 次）</h4>
      ${refs.length ? `<div class="chips">${refs.map(([q, e]) => `<span class="chip ex" style="--c:${COLORS[exitClass(e)]}" title="${esc(store.anchors.get(q) || '')}">${esc(store.anchors.get(q) || q)} · ${esc(e)}</span>`).join('')}</div>` : '<p class="muted">还没有读数</p>'}
      <h4>计算痕迹（坐标第二层）</h4>
      ${n.trace.length ? `<div class="chips">${n.trace.map((c) => `<a class="chip link" data-sel="config:${esc(c)}">${esc(c)}</a>`).join('')}</div>` : '<p class="muted">还没有痕迹</p>'}
      <h4>在这个主体上的语义判断（共 ${fmtN(n.judgeN || 0)} 次，最近 8 次）</h4>
      <ul class="jlist">${js.map(judgeItem).join('') || '<li class="muted">暂无</li>'}</ul>
      ${n.enrich.length ? `<h4>拿不准时补的信息</h4><ul class="jlist">${n.enrich.slice(-4).reverse().map((e) => `<li><div class="q">缺：${esc(e.need)}</div><div class="meta">补自：${esc(e.got)}</div></li>`).join('')}</ul>` : ''}
      <h4>关系</h4>
      <ul class="jlist">${rels.map((r) => `<li><a class="link" data-sel="relation:${esc(r.id)}">${esc(nodeName(r.from))} ⇢ ${r.via ? esc(nodeName(r.via)) + ' → ' : ''}${esc(nodeName(r.to))}</a><div class="meta">${esc(r.kind)} · ${esc(aboutText(r.about))}</div></li>`).join('') || '<li class="muted">暂无</li>'}</ul>`;
  } else if (sel.type === 'relation') {
    const r = store.relations.get(sel.id);
    if (!r) return;
    const kind = { direct: '直接关系', relay: '转介', unsure: '拿不准的关系' }[r.kind] || r.kind;
    const js = (r.judges || []).map((id) => store.judges.get(id)).filter(Boolean);
    html = `<h3>${esc(kind)}</h3>
      <dl class="kv"><dt>发起</dt><dd>${nodeLink(r.from)}</dd>${r.via != null ? `<dt>经由</dt><dd>${nodeLink(r.via)}</dd>` : ''}<dt>找到</dt><dd>${nodeLink(r.to)}</dd><dt>关于</dt><dd>${aboutLink(r.about)}</dd></dl>
      <h4>触发的上下文</h4><div class="trigger">${esc(r.trigger || '（事件里没有写）')}</div>
      <h4>依据的语义判断（${js.length} 次）</h4><ul class="jlist">${js.map(judgeItem).join('') || '<li class="muted">判断事件还没到或被省略</li>'}</ul>`;
  } else if (sel.type === 'config') {
    const c = store.configs.get(sel.id);
    if (!c) return;
    const st = { forming: '形成中', stable: '稳定', dropped: '已散' }[c.status] || c.status;
    const ring = c.ring;
    const plan = c.plan;
    const fbs = store.feedback.filter((f) => f.target === c.id || (plan && f.target === plan.id));
    html = `<h3>构型 ${esc(c.id)} · ${c.members.length} 方</h3>
      <div class="chips"><span class="chip ex" style="--c:${COLORS.config}">${esc(st)}</span>${c.parent ? `<a class="chip link" data-sel="config:${esc(c.parent)}">由 ${esc(c.parent)} 再传而来</a>` : ''}</div>
      <p>${esc(c.summary)}</p>
      <dl class="kv"><dt>成员</dt><dd>${c.members.map(nodeLink).join('、')}</dd><dt>关于</dt><dd>${aboutLink(c.about)}</dd><dt>状态变化</dt><dd>${c.history.map((h) => esc(h.status)).join(' → ')}</dd></dl>
      ${cfgBasis(c)}
      ${ring ? `<h4>环清算（${esc(ring.status)}）</h4><ul class="jlist">${(ring.edges || []).map((e) => `<li><div class="q">${esc(nodeName(e.from))} → ${esc(nodeName(e.to))}：${esc(e.give)}</div><div class="chips">${exitChip(e.exit)}</div></li>`).join('')}</ul>` : ''}
      ${plan ? `<h4>方案</h4><p>${esc(plan.text)}</p><table class="plan"><thead><tr><th>谁</th><th>做什么</th><th>给</th><th>得</th><th>还缺</th><th>第一小步</th></tr></thead><tbody>${(plan.members || []).map((m) => `<tr><td>${esc(nodeName(m.id))}</td><td>${esc(m.do)}</td><td>${esc(m.give)}</td><td>${esc(m.get)}</td><td>${esc(m.missing)}</td><td>${esc(m.first_step)}</td></tr>`).join('')}</tbody></table>` : ''}
      ${fbs.length ? `<h4>现场反馈</h4><ul class="jlist">${fbs.map((f) => `<li>${esc(nodeName(f.member))}：${esc(f.verdict)} ${esc(f.note || '')}</li>`).join('')}</ul>` : ''}`;
  }
  if (soft && body._html === html) return;
  body._html = html;
  body.innerHTML = html;
  $('detail').hidden = false;
}

// ---------- 对照区 ----------
let cmpId = null;
function toggleCompare() {
  if ($('compare').hidden) openCompare(null);
  else $('compare').hidden = true;
}
function openCompare(id) {
  if (id) { cmpId = id; $('cmp-follow').checked = false; }
  closeDetail();
  $('compare').hidden = false;
  renderCompare(true);
}
$('btn-compare').addEventListener('click', toggleCompare);
$('cmp-close').addEventListener('click', () => { $('compare').hidden = true; });
$('cmp-sel').addEventListener('change', (e) => { cmpId = e.target.value; $('cmp-follow').checked = false; renderCompare(true); });
$('cmp-prev').addEventListener('click', () => stepCompare(-1));
$('cmp-next').addEventListener('click', () => stepCompare(1));
$('cmp-follow').addEventListener('change', () => renderCompare(true));
$('compare').addEventListener('click', (e) => {
  const a = e.target.closest('[data-sel]');
  if (a) select(a.dataset.sel);
});
function stepCompare(d) {
  const ord = store.intentOrder;
  let i = ord.indexOf(cmpId);
  i = Math.max(0, Math.min(ord.length - 1, (i < 0 ? ord.length - 1 : i) + d));
  cmpId = ord[i];
  $('cmp-follow').checked = false;
  renderCompare(true);
}
function baselineItems(ev) {
  if (!ev) return '<p class="empty">这一路还没有结果</p>';
  const rs = Array.isArray(ev.results) ? ev.results : [];
  const items = rs.slice(0, 6).map((r) => {
    if (typeof r === 'string') return `<li>${esc(r)}</li>`;
    const who = Array.isArray(r.members) ? r.members.map(nodeLink).join('、') : r.id ? nodeLink(r.id) : '';
    const txt = r.text ?? r.summary ?? r.reason ?? r.name ?? '';
    return `<li>${who}${who && txt ? '：' : ''}${esc(txt)}</li>`;
  }).join('');
  return `<div class="nums"><span><b>${rs.length}</b> 条</span><span>花费 <b>${fmtUsd(Number(ev.usd) || 0)}</b></span><span>耗时 <b>${((Number(ev.ms) || 0) / 1000).toFixed(1)}s</b></span></div><ol>${items}</ol>`;
}
let cmpLastKey = '';
function renderCompare(force) {
  const ord = store.intentOrder;
  // 跟随：取最近一条已经有对照结果或构型的意图（刚进来的意图三路都还是空的）
  if ($('cmp-follow').checked || !cmpId) {
    cmpId = ord[ord.length - 1] || null;
    for (let i = ord.length - 1; i >= 0; i--) {
      const it = store.intents.get(ord[i]);
      if (Object.keys(it.baselines).length || it.configs.size) { cmpId = ord[i]; break; }
    }
  }
  const sel = $('cmp-sel');
  if (sel.options.length !== ord.length) {
    sel.innerHTML = ord.map((id) => { const it = store.intents.get(id); return `<option value="${esc(id)}">${esc(id)} · ${esc(it.text.slice(0, 18))}</option>`; }).join('');
  }
  if (cmpId) sel.value = cmpId;
  const it = cmpId ? store.intents.get(cmpId) : null;
  drawBars(it);
  if (!it) { $('cmp-intent').textContent = '还没有意图进入网络'; $('cmp-cols').innerHTML = ''; return; }
  const key = `${it.id}|${it.judges}|${it.configs.size}|${Object.keys(it.baselines).length}|${it.plans.length}`;
  if (!force && key === cmpLastKey) return;
  cmpLastKey = key;
  $('cmp-intent').innerHTML = `「${esc(it.text)}」<small>${esc(it.id)} · 来自 ${esc(nodeName(it.from))}</small>`;
  const b = it.baselines;
  const cfgs = [...it.configs].map((id) => store.configs.get(id)).filter((c) => c && c.status !== 'dropped')
    .sort((a, c) => c.members.length - a.members.length);
  const rels = it.relations.map((id) => store.relations.get(id)).filter(Boolean);
  const relay = rels.filter((r) => r.kind === 'relay').length;
  const multi = cfgs.filter((c) => c.members.length >= 3).length;
  const disc = it.discovery;
  const firstCfg = disc && disc.ms_at != null ? `${(disc.ms_at / 1000).toFixed(1)}s` : it.firstConfigT != null ? `${((it.firstConfigT - it.t) / 1000).toFixed(1)}s` : '—';
  const toFirst = disc && disc.judgments_at != null ? disc.judgments_at : it.judgesToFirstConfig;
  const jppCol = `<div class="col jpp"><h3>J++ 网络</h3><div class="sub">接收方用自己的世界判断，构型再传、环清算</div>
    <div class="nums"><span>判断 <b>${fmtN(it.judges)}</b></span><span>花费 <b>${fmtUsd(it.usd)}</b></span><span>首个构型 <b>${firstCfg}</b></span><span>到首个构型用了 <b>${toFirst ?? '—'}</b> 次判断</span></div>
    <div class="nums"><span>关系 <b>${rels.length}</b></span><span>转介 <b>${relay}</b></span><span>三方以上构型 <b>${multi}</b></span><span>方案 <b>${it.plans.length}</b></span></div>
    ${cfgs.slice(0, 5).map((c) => `<div class="cfg-item" data-sel="config:${esc(c.id)}"><span class="who">${c.members.length} 方 · ${esc(c.status)}</span>　${c.members.map((m) => esc(nodeName(m))).join('、')}</div>`).join('') || '<p class="empty">还没有构型</p>'}
    </div>`;
  $('cmp-cols').innerHTML = `
    <div class="col"><h3>检索</h3><div class="sub">BM25 关键词 / 向量相似度</div>
      <div class="arm"><div class="arm-name">BM25</div>${baselineItems(b.bm25)}</div>
      <div class="arm"><div class="arm-name">向量</div>${baselineItems(b.vector)}</div></div>
    <div class="col"><h3>生成模型一次读完</h3><div class="sub">同一份材料，把全部主体一次交给生成模型</div>${baselineItems(b.llm_all)}</div>
    ${jppCol}`;
}
const bars = $('cmp-bars');
const bctx = bars.getContext('2d');
function drawBars(cur) {
  const W = bars.width, H = bars.height;
  bctx.clearRect(0, 0, W, H);
  const ord = store.intentOrder;
  if (!ord.length) return;
  const vals = ord.map((id) => store.intents.get(id).judges);
  const max = Math.max(1, ...vals);
  const bw = W / Math.max(ord.length, 30);
  ord.forEach((id, i) => {
    const h = (vals[i] / max) * (H - 16);
    bctx.fillStyle = cur && cur.id === id ? '#3ef0b0' : 'rgba(122,167,255,0.55)';
    bctx.fillRect(i * bw + 2, H - h, bw - 4, h);
  });
  $('cmp-chart-note').textContent = `${ord.length} 条意图 · 最高 ${fmtN(max)} 次`;
}

// ---------- 主循环 ----------
let lastT = performance.now();
let hudT = 0;
let feedT = 0;
const perf = { frames: 0, frameTimes: [], fps: 0, start: performance.now() };
function loop(now) {
  const dt = now - lastT;
  lastT = now;
  const t0 = performance.now();
  player.tick(dt);
  scene.frame();
  if (feedDirty && now - feedT > 160) { feedT = now; renderFeed(); }
  renderLabels();
  if (now - hudT > 120) { hudT = now; updateHud(); renderPlayer(); }
  const ft = performance.now() - t0;
  perf.frames++;
  perf.frameTimes.push(ft);
  if (perf.frameTimes.length > 600) perf.frameTimes.shift();
  requestAnimationFrame(loop);
}

// 测试钩子：给无头浏览器读数、截图用
window.__tx = {
  store, player, scene, perf, select, openCompare,
  counters: () => ({
    judgments: store.c.judgments, usd: store.c.usd, configs: store.configCount, relations: store.c.relations,
    elapsed: store.c.elapsed, events: store.c.events, intents: store.c.intents, plans: store.c.plans, rings: store.c.rings,
    shownJudge: $('c-judge').textContent, shownCfg: $('c-cfg').textContent, shownUsd: $('c-usd').textContent,
  }),
  sampling: () => ({ ...scene.stats, queue: scene.queueLen, particles: scene.liveParticles, bulkTicks: player.bulkCount }),
  seek: (t) => seekSilently(t),
};

// fps 计数（真实 rAF 间隔）
let fpsLast = performance.now(), fpsN = 0;
function fpsLoop(now) {
  fpsN++;
  if (now - fpsLast >= 1000) { perf.fps = (fpsN * 1000) / (now - fpsLast); fpsN = 0; fpsLast = now; }
  requestAnimationFrame(fpsLoop);
}

async function start() {
  buildSpeeds();
  const sp = Number(qs.get('speed'));
  if (SPEEDS.includes(sp)) player.speed = sp;
  if (qs.get('compress') === '0') { player.compressIdle = false; $('chk-compress').checked = false; }
  if (qs.get('color') === 'kind') $('sel-color').value = 'kind';
  renderPlayer();
  try {
    if (qs.get('sse')) {
      player.connectSSE(qs.get('sse'));
    } else {
      const t0 = performance.now();
      await player.loadJsonl(qs.get('src') || '../mock/out/results.jsonl');
      perf.loadMs = performance.now() - t0;
    }
  } catch (e) {
    showError(`${e.message}。静态重放需要用本地服务打开（见 web/README.md）。`);
    return;
  }
  const seek = qs.get('seek');
  if (seek === 'end') seekSilently(player.totalT);
  else if (seek) seekSilently(Number(seek));
  if (qs.get('autoplay') !== '0') player.play();
  renderLegend();
  setInterval(renderLegend, 2000);
  if (qs.get('compare') === '1') openCompare(null);
  if (qs.get('select')) select(qs.get('select'));
  requestAnimationFrame(loop);
  requestAnimationFrame(fpsLoop);
}
start();

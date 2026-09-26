// 手机页。所有服务访问都经过下面的 api 对象，接口按 接口.md 第四节：
//   POST {api}/join      body = society/template.md 的 JSON
//   GET  {api}/me/<id>   返回 {node, relations, configs, plans, nodes, intents}（字段形状见 open_questions）
//   POST {api}/feedback  body = {target, member, verdict: yes|edit|no, note}
// 地址参数：?api=..（默认，相对本页）&id=p001 &mock=../mock/out/events.jsonl
// 没连上服务时：加入不提交；「跟我有关」用本地事件流算出的样例；反馈提示稍后再试。

import { roleOf } from '../shared/store.js';

const qs = new URLSearchParams(location.search);
const API = (qs.get('api') ?? '..').replace(/\/$/, '');
const MOCK_SRC = qs.get('mock') || '../mock/out/results.jsonl';
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const store = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* 隐私模式下可能不可用 */ } },
};

// ---------- 服务访问 ----------
const api = {
  mode: 'unknown', // server | offline
  mockServer: false,
  async probe() {
    try {
      const r = await fetch(`${API}/me/__probe__`, { cache: 'no-store' });
      const ct = r.headers.get('content-type') || '';
      if (ct.includes('application/json')) {
        const body = await r.json().catch(() => ({}));
        this.mode = 'server';
        this.mockServer = body && body.mock === true;
      } else this.mode = 'offline';
    } catch { this.mode = 'offline'; }
    return this.mode;
  },
  async post(path, body) {
    const r = await fetch(`${API}${path}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
    });
    let data = {};
    try { data = await r.json(); } catch { /* 非 JSON */ }
    if (data && data.mock === true) this.mockServer = true;
    return { ok: r.ok, status: r.status, data };
  },
  async me(id) {
    if (this.mode === 'server') {
      const r = await fetch(`${API}/me/${encodeURIComponent(id)}`, { cache: 'no-store' });
      const data = await r.json().catch(() => ({}));
      if (data && data.mock === true) this.mockServer = true;
      if (!r.ok) return { error: data.error || `服务返回 ${r.status}`, notFound: r.status === 404 };
      return normalizeMe(data, id, false);
    }
    return mockMe(id);
  },
};

function normalizeMe(d, id, fake) {
  const nodes = {};
  const src = d.nodes || {};
  if (Array.isArray(src)) for (const n of src) nodes[n.id] = n; else Object.assign(nodes, src);
  if (d.node) nodes[d.node.id ?? id] = d.node;
  const intents = {};
  const it = d.intents || {};
  if (Array.isArray(it)) for (const x of it) intents[x.id] = x; else Object.assign(intents, it);
  const configs = d.configs || [];
  const cfgById = {};
  for (const c of configs) cfgById[c.id] = c;
  return {
    id, fake, node: d.node || nodes[id] || null, nodes, intents, cfgById,
    relations: d.relations || d.candidates || [],
    configs,
    plans: (d.plans || []).filter((p) => p.grounded !== false),
  };
}

// 没有服务时：从本地事件流里算出「跟这个人有关」
let mockEventsCache = null;
async function mockMe(id) {
  if (!mockEventsCache) {
    const r = await fetch(MOCK_SRC, { cache: 'no-store' });
    if (!r.ok) return { error: '没连上服务，也没找到样例数据' };
    mockEventsCache = (await r.text()).split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l));
  }
  const nodes = {}, rels = {}, cfgs = {}, plans = {}, intents = {};
  for (const e of mockEventsCache) {
    if (e.type === 'node_join') nodes[e.node.id] = e.node;
    else if (e.type === 'relation') rels[e.id] = e;
    else if (e.type === 'config') cfgs[e.id] = e;
    else if (e.type === 'plan') plans[e.id] = e;
    else if (e.type === 'intent') intents[e.id] = e;
  }
  if (!nodes[id]) return { error: `样例数据里没有 ${id}`, notFound: true };
  const relations = Object.values(rels).filter((r) => [r.from, r.to, r.via].includes(id));
  const configs = Object.values(cfgs).filter((c) => (c.members || []).includes(id));
  const ids = new Set(configs.map((c) => c.id));
  return normalizeMe({
    node: nodes[id], nodes, intents, relations, configs,
    plans: Object.values(plans).filter((p) => ids.has(p.config)),
  }, id, true);
}

// ---------- 连接状态 ----------
function renderConn() {
  const el = $('conn');
  el.className = 'conn';
  if (api.mode === 'server') { el.textContent = '已连接'; el.classList.add('ok'); }
  else if (api.mode === 'offline') { el.textContent = '未连接服务'; el.classList.add('mock'); }
  else el.textContent = '检查连接…';
}

// ---------- 页签 ----------
function show(view) {
  for (const b of document.querySelectorAll('.tabs button')) b.setAttribute('aria-selected', String(b.dataset.view === view));
  $('view-join').hidden = view !== 'join';
  $('view-me').hidden = view !== 'me';
  if (view === 'me') loadMe();
  try { history.replaceState(null, '', `#${view}`); } catch { /* file:// 等场景 */ }
}
document.querySelector('.tabs').addEventListener('click', (e) => {
  const b = e.target.closest('[data-view]');
  if (b) show(b.dataset.view);
});

// ---------- 加入表单 ----------
const form = $('join');
const PEOPLE_MAX = 6;
function addPerson(p = {}) {
  const box = $('people');
  if (box.children.length >= PEOPLE_MAX) return;
  const d = document.createElement('div');
  d.className = 'person';
  d.innerHTML = `<input aria-label="姓名或称呼" placeholder="姓名或称呼" data-k="name" value="${esc(p.name)}">
    <input aria-label="关系" placeholder="关系，如 儿子" data-k="relation" value="${esc(p.relation)}">
    <input class="desc" aria-label="一句话" placeholder="一句话：会什么、在做什么" data-k="desc" value="${esc(p.desc)}">
    <button type="button" class="rm">删掉这个人</button>`;
  d.querySelector('.rm').addEventListener('click', () => { d.remove(); $('add-person').hidden = false; });
  box.appendChild(d);
  $('add-person').hidden = box.children.length >= PEOPLE_MAX;
}
$('add-person').addEventListener('click', () => addPerson());
addPerson();

$('consent').addEventListener('change', (e) => { $('submit').disabled = !e.target.checked; });

function collect() {
  const f = new FormData(form);
  const tags = String(f.get('tags') || '').split(/[,，、\s]+/).map((s) => s.trim()).filter(Boolean).slice(0, 5);
  const people = [...$('people').children].map((d) => {
    const o = {};
    for (const i of d.querySelectorAll('input')) o[i.dataset.k] = i.value.trim();
    return o;
  }).filter((p) => p.name || p.desc);
  const t = (k) => String(f.get(k) || '').trim();
  return {
    alias: t('alias'), kind: f.get('kind') || 'person', city: t('city'), tags,
    profile: {
      identity: t('identity'), experience: t('experience'), resources: t('resources'), people,
      recent: t('recent'), wants: t('wants'), offers: t('offers'),
    },
    consent: $('consent').checked,
  };
}

function validate(body) {
  const miss = [];
  if (!body.alias) miss.push('显示名');
  if (!body.city) miss.push('城市');
  if (!body.profile.identity) miss.push('你是谁、做什么');
  if (!body.profile.wants) miss.push('你想要什么');
  if (miss.length) return `还差：${miss.join('、')}`;
  if (!body.consent) return '需要先勾选同意';
  return null;
}

form.addEventListener('submit', async (e) => {
  e.preventDefault();
  const msg = $('join-msg');
  const body = collect();
  const bad = validate(body);
  msg.className = 'msg err';
  if (bad) { msg.textContent = bad; return; }
  if (api.mode !== 'server') {
    msg.textContent = '现在没连上服务，资料没有提交。请连上现场网络后再试。';
    return;
  }
  $('submit').disabled = true;
  msg.className = 'msg';
  msg.textContent = '提交中…';
  try {
    const r = await api.post('/join', body);
    if (!r.ok) {
      msg.className = 'msg err';
      msg.textContent = r.data.error || `提交失败（${r.status}）`;
      return;
    }
    const id = r.data.id || (r.data.node && r.data.node.id);
    if (id) store.set('towow.me', id);
    msg.className = 'msg ok';
    msg.textContent = r.data.status === 'offline' || (r.data.node && r.data.node.status === 'offline')
      ? '资料已收到。现在是录像回放，网络恢复后你会入网。'
      : '已加入，大屏上应该能看到你了。';
    renderConn();
    setTimeout(() => show('me'), 900);
  } catch {
    msg.className = 'msg err';
    msg.textContent = '网络断了，资料没有提交。';
  } finally {
    $('submit').disabled = !$('consent').checked;
  }
});

// 粘贴 agent 写好的 JSON
$('paste-apply').addEventListener('click', () => {
  const m = $('paste-msg');
  let d;
  try { d = JSON.parse($('paste').value); } catch { m.textContent = '这段不是合法的 JSON'; return; }
  const p = d.profile || {};
  const set = (name, v) => { const el = form.elements[name]; if (el && v != null) el.value = String(v); };
  set('alias', d.alias ?? d.name);
  set('city', d.city);
  set('tags', Array.isArray(d.tags) ? d.tags.join(' ') : d.tags);
  for (const k of ['identity', 'experience', 'resources', 'recent', 'wants', 'offers']) set(k, p[k]);
  if (d.kind) for (const r of form.querySelectorAll('input[name="kind"]')) r.checked = r.value === d.kind;
  if (Array.isArray(p.people) && p.people.length) {
    $('people').innerHTML = '';
    p.people.slice(0, PEOPLE_MAX).forEach(addPerson);
  }
  m.textContent = '已填进表单。请核对，再勾选同意后提交（粘贴不代表同意）。';
});

// ---------- 跟我有关 ----------
let meId = qs.get('id') || store.get('towow.me') || null;
let lastMeKey = '';

// 页面一律显示角色，不显示名字（接口.md 第一节）；名字只在自己的页头小字出现一次
function nameOf(v, id) {
  const n = v.nodes[id];
  return n ? roleOf(n) : String(id ?? '');
}
function intentLine(v, about) {
  const it = v.intents[about];
  if (it) return `因为 ${esc(nameOf(v, it.from))} 说：「${esc(it.text)}」`;
  if (v.cfgById[about] || String(about).startsWith('c')) return '由一个正在形成的构型再传过来';
  return '';
}

function actions(target) {
  const key = `towow.fb.${meId}.${target}`;
  const prev = store.get(key);
  return `<div class="acts" data-target="${esc(target)}">
      <button class="btn yes" data-v="yes" aria-pressed="${prev === 'yes'}">点头</button>
      <button class="btn edit" data-v="edit" aria-pressed="${prev === 'edit'}">修改</button>
      <button class="btn no" data-v="no" aria-pressed="${prev === 'no'}">拒绝</button>
    </div>
    <div class="editbox" hidden><textarea rows="3" placeholder="你想怎么改？比如：时间只有周末、先不带钱、换个人来谈"></textarea>
      <button class="btn ghost" data-send="edit">提交修改</button></div>
    <p class="done" role="status"></p>`;
}

function relCard(v, r) {
  const me = meId;
  let title, kindCls = r.kind, kindTxt = { direct: '直接', relay: '转介', unsure: '拿不准' }[r.kind] || r.kind;
  if (r.from === me && r.kind === 'relay' && r.via) title = `经 ${esc(nameOf(v, r.via))} 找到 ${esc(nameOf(v, r.to))}`;
  else if (r.from === me) title = esc(nameOf(v, r.to));
  else if (r.via === me) title = `你可以把 ${esc(nameOf(v, r.from))} 介绍给 ${esc(nameOf(v, r.to))}`;
  else title = `${esc(nameOf(v, r.from))} 可能需要你`;
  const other = r.from === me ? (r.kind === 'relay' ? r.to : r.to) : r.from;
  const on = v.nodes[other];
  return `<article class="card">
    <h3><span class="kind ${esc(kindCls)}">${esc(kindTxt)}</span>${title}</h3>
    ${on && on.summary ? `<p class="who">${esc(on.summary)}</p>` : ''}
    <div class="why"><small>为什么是 TA（接收方哪段情况触发的）</small>${esc(r.trigger || '（没有写明）')}</div>
    <p class="from">${intentLine(v, r.about)}</p>
    ${actions(r.id)}
  </article>`;
}

function planCard(v, p) {
  const mine = (p.members || []).find((m) => m.id === meId) || {};
  const others = (p.members || []).filter((m) => m.id !== meId);
  const withNames = (mine.with || []).map((x) => esc(nameOf(v, x))).join('、') || others.map((m) => esc(nameOf(v, m.id))).join('、');
  return `<article class="card plan">
    <h3><span class="kind cfg">${(p.members || []).length} 方方案</span>跟 ${withNames}</h3>
    <dl class="mine">
      <dt>你做什么</dt><dd>${esc(mine.do || '—')}</dd>
      <dt>你给</dt><dd>${esc(mine.give || '—')}</dd>
      <dt>你得</dt><dd>${esc(mine.get || '—')}</dd>
      <dt>还缺</dt><dd>${esc(mine.missing || '—')}</dd>
      <dt>第一小步</dt><dd>${esc(mine.first_step || '—')}</dd>
    </dl>
    ${p.text ? `<p class="plan-text">${esc(p.text)}</p>` : ''}
    <p class="others">其他人：${others.map((m) => `${esc(nameOf(v, m.id))}（${esc(m.do || '')}）`).join('；')}</p>
    <p class="hint">方案只是启发，拍板在你。点头不会替你发出任何邀请或承诺。</p>
    ${actions(p.id)}
  </article>`;
}

function cfgCard(v, c) {
  const st = { forming: '形成中', stable: '稳定' }[c.status] || c.status;
  const others = (c.members || []).filter((m) => m !== meId).map((m) => esc(nameOf(v, m)));
  return `<article class="card">
    <h3><span class="kind cfg">${(c.members || []).length} 方 · ${esc(st)}</span>你和 ${others.join('、')}</h3>
    <p class="who">${esc(c.summary || '')}</p>
    ${actions(c.id)}
  </article>`;
}

async function loadMe(force = false) {
  const head = $('me-head'), body = $('me-body');
  if (!meId) {
    head.innerHTML = '<h2>还没有加入</h2><p>先在「加入」页填好资料。</p>';
    $('me-pick').hidden = api.mode === 'server' && !api.mockServer;
    body.innerHTML = api.mode !== 'server' ? '<p class="empty">没连上服务。想先看看样子，可以在上面填一个样例 id，如 p001。</p>' : '';
    return;
  }
  if (!body.innerHTML) body.innerHTML = '<p class="empty">读取中…</p>';
  let v;
  try { v = await api.me(meId); } catch (e) { v = { error: `读取失败：${e.message}` }; }
  if (v.error) {
    head.innerHTML = `<h2>${esc(meId)}</h2>`;
    body.innerHTML = `<p class="empty">${esc(v.error)}</p>`;
    $('me-pick').hidden = false;
    return;
  }
  // 自动刷新时，如果有人正在写修改意见，就先不重画
  if (!force && document.querySelector('.editbox:not([hidden]) textarea')) return;
  const key = JSON.stringify([v.relations.length, v.configs.map((c) => c.status), v.plans.length, v.fake]);
  if (!force && key === lastMeKey) return;
  lastMeKey = key;
  $('me-pick').hidden = !(v.fake || api.mockServer);
  const n = v.node || {};
  const myRole = n.id != null || n.name ? roleOf(n) : meId;
  head.innerHTML = `<h2>${esc(myRole || meId)}</h2>${n.name && n.name !== myRole ? `<p class="realname">${esc(n.name)}</p>` : ''}<p>${esc(n.summary || '')}</p>`;
  const plans = v.plans.slice().reverse();
  const planCfg = new Set(plans.map((p) => p.config));
  const cfgs = v.configs.filter((c) => c.status !== 'dropped' && !planCfg.has(c.id)).reverse();
  const rels = v.relations.slice().sort((a, b) => (b.seq ?? 0) - (a.seq ?? 0));
  body.innerHTML = `
    <h2 class="sec">多方方案 <b>${plans.length}</b></h2>
    ${plans.map((p) => planCard(v, p)).join('') || '<p class="empty">还没有方案。网络在算，稍后刷新。</p>'}
    <h2 class="sec">候选 <b>${rels.length}</b></h2>
    ${rels.map((r) => relCard(v, r)).join('') || '<p class="empty">还没有人跟你连上。</p>'}
    ${cfgs.length ? `<h2 class="sec">正在形成的构型 <b>${cfgs.length}</b></h2>${cfgs.map((c) => cfgCard(v, c)).join('')}` : ''}
    <button class="btn ghost" id="me-refresh" style="width:100%;margin-top:8px">刷新</button>`;
  $('me-refresh').addEventListener('click', () => loadMe(true));
  body.dataset.fake = v.fake ? '1' : '';
}

$('me-body').addEventListener('click', async (e) => {
  const card = e.target.closest('.card');
  if (!card) return;
  const acts = card.querySelector('.acts');
  const target = acts && acts.dataset.target;
  const vBtn = e.target.closest('[data-v]');
  const send = e.target.closest('[data-send]');
  if (vBtn && vBtn.dataset.v === 'edit') {
    card.querySelector('.editbox').hidden = false;
    card.querySelector('.editbox textarea').focus();
    return;
  }
  let verdict = null, note = '';
  if (vBtn) verdict = vBtn.dataset.v;
  if (send) { verdict = 'edit'; note = card.querySelector('.editbox textarea').value.trim(); if (!note) { card.querySelector('.done').textContent = '写一句想怎么改'; card.querySelector('.done').className = 'done warn'; return; } }
  if (!verdict) return;
  const done = card.querySelector('.done');
  for (const b of acts.querySelectorAll('[data-v]')) b.setAttribute('aria-pressed', String(b.dataset.v === verdict));
  if ($('me-body').dataset.fake === '1' || api.mode !== 'server') {
    done.className = 'done warn';
    done.textContent = '没连上服务，稍后再试。';
    return;
  }
  done.className = 'done';
  done.textContent = '发送中…';
  try {
    const r = await api.post('/feedback', { target, member: meId, verdict, note });
    if (!r.ok) throw new Error(r.data.error || `服务返回 ${r.status}`);
    store.set(`towow.fb.${meId}.${target}`, verdict);
    done.className = 'done ok';
    done.textContent = { yes: '已点头，已记下。', edit: '修改意见已记下。', no: '已拒绝，网络会记住这一点。' }[verdict];
    card.querySelector('.editbox').hidden = true;
  } catch (err) {
    done.className = 'done err';
    done.textContent = `没发出去：${err.message}`;
  }
});

$('me-go').addEventListener('click', () => {
  const v = $('me-id').value.trim();
  if (v) { meId = v; lastMeKey = ''; $('me-body').innerHTML = ''; loadMe(true); }
});

// ---------- 启动 ----------
(async function start() {
  await api.probe();
  renderConn();
  if (api.mode !== 'server' && !meId) meId = null;
  const hash = location.hash.replace('#', '');
  show(hash === 'me' || (qs.get('id') && hash !== 'join') ? 'me' : 'join');
  setInterval(() => { if (!$('view-me').hidden && api.mode === 'server') loadMe(); }, 15000);
})();

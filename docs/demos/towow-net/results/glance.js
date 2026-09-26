// 方案一览：打开后不读字、5 秒内看出「谁和谁凑成了什么事」。
// 一屏一个方案：顶部一行方案名，中间角色环（图标 + 角色大字），环上箭头是「谁给谁什么」（6 字以内），
// 右下两个大数（环上成立几条 / 花了几次判断），下方一排无字缩略环。自动轮播，每个约 6 秒。
// 点角色或箭头弹出这一步的依据（题面、读数、出口、触发的上下文），「看依据链」进现有详情。
// 数据全部来自事件流（经 app.js 的 planModel），这里不读任何别的文件。

import { exitClass, roleOf } from '../shared/store.js';

const SHOW_MS = 6000;
const BUDGET = 60; // 每屏默认可见汉字上限（目标.md 演示页硬标准第 7 条）
const CJK = /[㐀-鿿豈-﫿]/g;
const cjkLen = (s) => (String(s || '').match(CJK) || []).length;
const chars = (s) => Array.from(String(s || ''));
const clip = (s, n) => { const a = chars(String(s || '').trim()); return a.length <= n ? a.join('') : `${a.slice(0, n - 1).join('')}…`; };

// ---------- 头像图形：按个人 / 公司 / 机构画一个图标，不用文字 ----------
const GLYPH = {
  person: 'M0,-0.62 a0.27,0.27 0 1,1 0,0.54 a0.27,0.27 0 1,1 0,-0.54 Z M-0.52,0.6 Q-0.52,0.02 0,0.02 Q0.52,0.02 0.52,0.6 Z',
  company: 'M-0.58,0.56 V-0.02 L-0.24,0.2 V-0.02 L0.1,0.2 V-0.58 H0.46 V0.56 Z',
  org: 'M-0.6,-0.16 L0,-0.6 L0.6,-0.16 Z M-0.46,-0.06 h0.2 v0.46 h-0.2 Z M-0.1,-0.06 h0.2 v0.46 h-0.2 Z M0.26,-0.06 h0.2 v0.46 h-0.2 Z M-0.6,0.44 h1.2 v0.14 h-1.2 Z',
  unknown: 'M0,-0.5 a0.5,0.5 0 1,1 0,1 a0.5,0.5 0 1,1 0,-1 Z',
};
export function glyphSvg(kind, x, y, r, fill = '#04060c') {
  const d = GLYPH[kind] || GLYPH.unknown;
  const s = r * 0.62;
  return `<path d="${d}" transform="translate(${x.toFixed(1)},${y.toFixed(1)}) scale(${s.toFixed(2)})" fill="${fill}"/>`;
}
export function glyphIcon(kind, col, size = 22) {
  const r = size / 2;
  return `<svg class="avi" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" aria-hidden="true"><circle cx="${r}" cy="${r}" r="${r - 1}" fill="${col}"/>${glyphSvg(kind, r, r, r)}</svg>`;
}

// ---------- 排序：三方以上且全部成立 → 含转介 → 其余；同样成员的只留一个 ----------
function tier(m) {
  const allAct = m.edges.length > 0 && m.ex.act === m.edges.length;
  if (m.size >= 3 && allAct) return 0;
  if (m.hasRelay) return 1;
  return 2;
}
export function glanceOrder(models) {
  const seen = new Set();
  const out = [];
  const sorted = models.slice().sort((a, b) => tier(a) - tier(b) || b.size - a.size || (b.ex.act / Math.max(1, b.edges.length)) - (a.ex.act / Math.max(1, a.edges.length)) || a.seq - b.seq);
  for (const m of sorted) {
    const k = m.ids.slice().sort().join('|');
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(m);
  }
  return out;
}

export function createGlance(env) {
  const { $, esc, COLORS, readingCell, exitChip, openPlan, getModels, getStore, chromeCjk } = env;
  const el = {
    page: $('tab-glance'), title: $('g-title'), stage: $('g-stage'), svg: $('g-svg'), nums: $('g-nums'),
    thumbs: $('g-thumbs'), pop: $('g-pop'),
  };
  const st = { list: [], sig: '', idx: 0, timer: 0, hold: false, popOpen: false, active: false, shown: null, budget: null };
  const reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  // ---- 箭头上的东西：环边的 give（6 字以内）；没有就用方案里给方的 give ----
  function edgesOf(m) {
    let edges = m.edges.length ? m.edges : [];
    const ids = m.cycle.length ? m.cycle : m.ids;
    if (!edges.length && ids.length >= 2) edges = ids.map((id, i) => ({ from: id, to: ids[(i + 1) % ids.length], give: '', exit: null }));
    return edges.map((e, k) => {
      const row = m.rows.find((r) => r.id === e.from) || {};
      return { ...e, k, label: String(e.give || row.give || '').trim() };
    });
  }

  function titleOf(m) {
    if (m.plan.title && String(m.plan.title).trim()) return clip(m.plan.title, 16);
    const s = getStore();
    const it = s.intents.get(m.root);
    return clip(it ? it.text : m.id, 16);
  }

  // 预算：标题 + 角色 + 箭头字 + 顶栏与说明文字 ≤ 60；超了先把箭头字缩到 4，再把角色缩到 5，最后收起箭头字
  function fit(m, roles, edges, title) {
    const fixed = chromeCjk() + 4; // 两个大数的说明各 2 字
    let gl = 6; let rl = 8; let showGive = true;
    const total = () => fixed + cjkLen(title) + roles.reduce((a, r) => a + cjkLen(clip(r, rl)), 0)
      + (showGive ? edges.reduce((a, e) => a + cjkLen(clip(e.label, gl)), 0) : 0);
    if (total() > BUDGET) gl = 4;
    if (total() > BUDGET) rl = 6;
    if (total() > BUDGET) showGive = false;
    if (total() > BUDGET) rl = 5;
    return { gl, rl, showGive, total: total() };
  }

  // ---------- 主画面 ----------
  function draw(m, animate = true) {
    const s = getStore();
    const box = el.svg.getBoundingClientRect();
    const W = Math.round(box.width) || el.stage.clientWidth || 800;
    const H = Math.round(box.height) || el.stage.clientHeight || 600;
    const ids = m.cycle.length ? m.cycle : m.ids;
    const n = ids.length;
    const narrow = W < 640;
    const S = Math.min(W, H);
    const roles = ids.map((id) => roleOf(s.nodes.get(id)) || String(id));
    const edges = edgesOf(m);
    const title = titleOf(m);
    const F = fit(m, roles, edges, title);
    st.budget = F;
    const shown = roles.map((r) => clip(r, F.rl));
    // 角色字放在头像外侧（远离环心）：上下的放在头像上方 / 下方，左右的放在旁边，不压箭头
    const angs = ids.map((_, i) => (n === 2 ? (i === 0 ? Math.PI : 0) : -Math.PI / 2 + (i * 2 * Math.PI) / n));
    const side = angs.map((a) => Math.abs(Math.cos(a)) >= 0.5);
    const tw = (t) => chars(t).reduce((a, c) => a + (/[\u0000-\u00ff]/.test(c) ? 0.58 : 1), 0);
    let ar = Math.max(22, Math.min(60, S * 0.085));
    let fRole = Math.max(15, Math.min(40, S * 0.05));
    const fGive = Math.max(12, Math.min(28, S * 0.034));
    let perLine = 99; // 旁边的角色字每行几个字（窄屏折成两行）
    let R = 0;
    const layout = () => {
      const lines = shown.map((t, i) => (side[i] && tw(t) > perLine ? [chars(t).slice(0, Math.ceil(chars(t).length / 2)).join(''), chars(t).slice(Math.ceil(chars(t).length / 2)).join('')] : [t]));
      const sideW = Math.max(0, ...lines.map((ls, i) => (side[i] ? Math.max(...ls.map(tw)) * fRole : 0)));
      const hasTop = angs.some((a, i) => !side[i] && Math.sin(a) < 0);
      const hasBot = angs.some((a, i) => !side[i] && Math.sin(a) > 0);
      const vPad = ar + fRole * 1.5;
      const rH = n === 2 ? W / 2 - ar - 16 - sideW - 6 : Math.min(W / 2 - ar - 16 - sideW - 6, W * 0.36);
      const rV = n === 2 ? S * 0.4 : (H - (hasTop ? vPad : ar) - (hasBot ? vPad : ar) - 12) / 2 / (n === 3 ? 0.75 : 1);
      R = Math.min(rH, rV, S * 0.42);
      return lines;
    };
    let lines = layout();
    for (let k = 0; k < 12 && R < S * 0.27; k++) {
      if (perLine > 5 && W < 700) perLine = 4;
      else if (fRole > 13) fRole = Math.max(13, fRole * 0.88);
      else ar = Math.max(18, ar * 0.9);
      lines = layout();
    }
    const cx = W / 2;
    // 三方环上下不对称：顶点在上，底边在下；把重心放到画面中间
    const cy = n === 3 ? H / 2 + R * 0.25 : H / 2;
    const pos = new Map();
    ids.forEach((id, i) => pos.set(id, [cx + R * Math.cos(angs[i]), cy + R * Math.sin(angs[i])]));

    const T0 = animate && !reduce ? 0.15 : 0; // 秒
    const nodeDur = 0.7;
    const litStart = T0 + nodeDur + 0.15;
    const litStep = animate && !reduce ? Math.min(0.55, 2.4 / Math.max(1, edges.length)) : 0;

    let defs = '<defs>';
    let back = '';
    let front = '';
    let labs = '';
    edges.forEach((e, k) => {
      const a = pos.get(e.from); const b = pos.get(e.to);
      if (!a || !b || e.from === e.to) return;
      const ex = e.exit == null ? 'none' : exitClass(e.exit);
      const col = ex === 'act' ? COLORS.act : ex === 'unsure' ? COLORS.unsure : ex === 'none' ? '#8a97b3' : COLORS.no;
      const dx = b[0] - a[0]; const dy = b[1] - a[1];
      const L = Math.hypot(dx, dy) || 1; const ux = dx / L; const uy = dy / L;
      const x1 = a[0] + ux * (ar + 8); const y1 = a[1] + uy * (ar + 8);
      const x2 = b[0] - ux * (ar + 14); const y2 = b[1] - uy * (ar + 14);
      let mx = (x1 + x2) / 2; let my = (y1 + y2) / 2;
      if (n === 2) { my += (ids.indexOf(e.from) === 0 ? -1 : 1) * R * 0.55; }
      else { const ox = mx - cx; const oy = my - cy; const ol = Math.hypot(ox, oy) || 1; const bow = R * 0.2; mx += (ox / ol) * bow; my += (oy / ol) * bow; }
      const d = `M${x1.toFixed(1)},${y1.toFixed(1)} Q${mx.toFixed(1)},${my.toFixed(1)} ${x2.toFixed(1)},${y2.toFixed(1)}`;
      const delay = litStart + k * litStep;
      const mk = `gm-${k}`;
      defs += `<marker id="${mk}" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="${(fGive * 0.62).toFixed(1)}" markerHeight="${(fGive * 0.62).toFixed(1)}" markerUnits="userSpaceOnUse" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="${col}"/></marker>`;
      const dash = ex === 'unsure' ? ` stroke-dasharray="${(fGive * 0.5).toFixed(0)} ${(fGive * 0.4).toFixed(0)}"` : ex === 'act' ? '' : ` stroke-dasharray="3 ${(fGive * 0.4).toFixed(0)}"`;
      const sw = Math.max(2.4, S * 0.006);
      back += `<path class="g-eb" d="${d}" fill="none" stroke="${col}" stroke-width="${sw.toFixed(1)}" stroke-linecap="round"${dash} marker-end="url(#${mk})" style="animation-delay:${delay.toFixed(2)}s"/>`;
      back += `<path class="g-lit" d="${d}" fill="none" stroke="${col}" stroke-width="${(sw * 2.2).toFixed(1)}" stroke-linecap="round" filter="url(#gglow)" style="animation-delay:${delay.toFixed(2)}s"/>`;
      if (ex === 'act' && !reduce) {
        front += `<circle class="g-dot" r="${(sw * 1.3).toFixed(1)}" fill="#fff" style="offset-path:path('${d}');animation-delay:${(delay + 0.5).toFixed(2)}s, ${(delay + 0.5).toFixed(2)}s;animation-duration:0.4s, ${(2.1 + k * 0.23).toFixed(2)}s"/>`;
      }
      back += `<path class="g-hit" data-e="${k}" d="${d}" fill="none" stroke="transparent" stroke-width="${Math.max(22, fGive * 1.4).toFixed(0)}"/>`;
      if (F.showGive && e.label) {
        const txt = clip(e.label, F.gl);
        const qx = 0.25 * x1 + 0.5 * mx + 0.25 * x2;
        const qy = 0.25 * y1 + 0.5 * my + 0.25 * y2;
        const w = chars(txt).reduce((a, c) => a + (/[\u0000-ÿ]/.test(c) ? 0.6 : 1), 0) * fGive + fGive * 1.1;
        const h = fGive * 1.7;
        labs += `<g class="g-lab" data-e="${k}" style="animation-delay:${(delay + 0.35).toFixed(2)}s">
          <rect x="${(qx - w / 2).toFixed(1)}" y="${(qy - h / 2).toFixed(1)}" width="${w.toFixed(1)}" height="${h.toFixed(1)}" rx="${(h / 2).toFixed(1)}" fill="rgba(6,10,20,0.88)" stroke="${col}" stroke-opacity="0.75" stroke-width="1.4"/>
          <text x="${qx.toFixed(1)}" y="${(qy + fGive * 0.36).toFixed(1)}" text-anchor="middle" font-size="${fGive.toFixed(1)}" fill="${ex === 'act' ? '#eafff7' : col}">${esc(txt)}</text></g>`;
      }
    });
    defs += `<filter id="gglow" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="${(S * 0.008).toFixed(1)}" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>`;
    defs += `<radialGradient id="gcore"><stop offset="0" stop-color="rgba(255,210,122,0.16)"/><stop offset="1" stop-color="rgba(255,210,122,0)"/></radialGradient></defs>`;

    let nodes = '';
    ids.forEach((id, i) => {
      const [x, y] = pos.get(id);
      const nd = s.nodes.get(id) || {};
      const col = m.color(id);
      const isO = id === m.origin;
      const delay = T0 + i * (animate && !reduce ? 0.08 : 0);
      const a = angs[i];
      nodes += `<g class="g-node" data-m="${esc(id)}" style="--dx:${(cx - x).toFixed(1)}px;--dy:${(cy - y).toFixed(1)}px;animation-delay:${delay.toFixed(2)}s">`;
      nodes += `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="${(ar * 1.9).toFixed(1)}" fill="${col}" opacity="0.10"/>`;
      if (isO) nodes += `<circle class="g-origin" cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="${(ar + 9).toFixed(1)}" fill="none" stroke="#fff" stroke-opacity="0.7" stroke-width="1.6" stroke-dasharray="4 7"/>`;
      nodes += `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="${ar.toFixed(1)}" fill="${col}" filter="url(#gglow)"/>`;
      nodes += glyphSvg(nd.kind, x, y, ar);
      const ls = lines[i];
      let tx = x; let ty; let anchor = 'middle';
      if (side[i]) {
        anchor = Math.cos(a) < 0 ? 'end' : 'start';
        tx = x + Math.sign(Math.cos(a)) * (ar + 14);
        ty = y + fRole * 0.36 - ((ls.length - 1) * fRole * 1.15) / 2;
      } else if (Math.sin(a) < 0) {
        ty = y - ar - fRole * 0.5 - (ls.length - 1) * fRole * 1.15;
      } else {
        ty = y + ar + fRole * 1.15;
      }
      nodes += `<text class="g-role" x="${tx.toFixed(1)}" y="${ty.toFixed(1)}" text-anchor="${anchor}" font-size="${fRole.toFixed(1)}">${ls.map((l, k) => `<tspan x="${tx.toFixed(1)}" dy="${k ? (fRole * 1.15).toFixed(1) : 0}">${esc(l)}</tspan>`).join('')}</text>`;
      nodes += '</g>';
    });

    el.svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
    el.svg.setAttribute('width', W);
    el.svg.setAttribute('height', H);
    el.svg.innerHTML = `${defs}<circle cx="${cx}" cy="${cy}" r="${(R * 1.05).toFixed(1)}" fill="url(#gcore)"/>
      <circle class="g-halo" cx="${cx}" cy="${cy}" r="${R.toFixed(1)}" fill="none" stroke="rgba(160,190,240,0.10)" stroke-width="1" stroke-dasharray="2 8"/>
      <g>${back}</g><g>${front}</g><g>${nodes}</g><g>${labs}</g>`;
    for (const p of el.svg.querySelectorAll('.g-lit')) {
      const len = p.getTotalLength ? p.getTotalLength() : 400;
      p.style.setProperty('--L', `${len.toFixed(1)}`);
    }
    el.svg.classList.toggle('still', !animate || reduce);

    // 标题与大数
    el.title.textContent = title;
    el.title.classList.remove('in'); void el.title.offsetWidth; el.title.classList.add('in');
    const total = edges.length;
    const ok = edges.filter((e) => exitClass(e.exit) === 'act').length;
    el.nums.innerHTML = `<div class="g-num ok"><b data-v="${ok}">${animate && !reduce ? 0 : ok}</b><i>/${total}</i><span>成立</span></div>
      <div class="g-num jd"><b data-v="${m.judgeN}">${animate && !reduce ? 0 : m.judgeN}</b><span>判断</span></div>`;
    if (animate && !reduce) countUp();
    void narrow;
  }

  function countUp() {
    const bs = [...el.nums.querySelectorAll('b[data-v]')];
    const t0 = performance.now();
    const dur = 1100;
    const step = (now) => {
      const f = Math.min(1, (now - t0) / dur);
      const e = 1 - (1 - f) ** 3;
      for (const b of bs) b.textContent = String(Math.round(Number(b.dataset.v) * e));
      if (f < 1 && st.active) requestAnimationFrame(step);
      else for (const b of bs) b.textContent = b.dataset.v;
    };
    requestAnimationFrame(step);
  }

  // ---------- 缩略环（不放文字） ----------
  function thumbSvg(m, size) {
    const ids = m.cycle.length ? m.cycle : m.ids;
    const n = ids.length;
    const c = size / 2; const R = size * 0.32; const r = Math.max(3.5, size * 0.085);
    const pos = new Map();
    ids.forEach((id, i) => {
      const ang = n === 2 ? (i === 0 ? Math.PI : 0) : -Math.PI / 2 + (i * 2 * Math.PI) / n;
      pos.set(id, [c + R * Math.cos(ang), c + R * Math.sin(ang)]);
    });
    let o = '';
    for (const e of edgesOf(m)) {
      const a = pos.get(e.from); const b = pos.get(e.to);
      if (!a || !b) continue;
      const ex = e.exit == null ? 'none' : exitClass(e.exit);
      const col = ex === 'act' ? COLORS.act : ex === 'unsure' ? COLORS.unsure : ex === 'none' ? '#6c7a96' : COLORS.no;
      o += `<line x1="${a[0].toFixed(1)}" y1="${a[1].toFixed(1)}" x2="${b[0].toFixed(1)}" y2="${b[1].toFixed(1)}" stroke="${col}" stroke-width="1.8"${ex === 'act' ? '' : ' stroke-dasharray="3 3"'}/>`;
    }
    for (const id of ids) { const [x, y] = pos.get(id); o += `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="${r.toFixed(1)}" fill="${m.color(id)}"/>`; }
    return `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" aria-hidden="true">${o}</svg>`;
  }

  function thumbCount() {
    const W = el.thumbs.clientWidth || el.page.clientWidth || 800;
    const narrow = W < 640;
    const size = narrow ? 56 : Math.max(56, Math.min(84, (el.page.clientHeight || 800) * 0.085));
    const per = size + (narrow ? 10 : 14);
    const n = narrow ? 12 : Math.max(3, Math.floor((W - 10) / per));
    return { n: Math.min(n, 14), size };
  }

  function buildThumbs() {
    const { size } = thumbCount();
    el.thumbs.innerHTML = st.list.map((m, i) => `<button class="g-th${i === st.idx ? ' on' : ''}" data-i="${i}" aria-label="第 ${i + 1} 个方案" title="${esc(titleOf(m))}">${thumbSvg(m, size)}<i class="g-prog"></i></button>`).join('');
  }

  function markThumb() {
    for (const b of el.thumbs.querySelectorAll('.g-th')) {
      const on = Number(b.dataset.i) === st.idx;
      b.classList.toggle('on', on);
      const pr = b.querySelector('.g-prog');
      if (pr) { pr.style.animation = 'none'; void pr.offsetWidth; pr.style.animation = on && !st.hold && !st.popOpen ? '' : 'none'; }
    }
  }

  // ---------- 轮播 ----------
  function schedule() {
    clearTimeout(st.timer);
    if (!st.active || st.hold || st.popOpen || st.list.length < 2) return;
    // 详情抽屉开着时不翻页，等它关上
    st.timer = setTimeout(() => { if (env.isBusy && env.isBusy()) schedule(); else go(st.idx + 1); }, SHOW_MS);
  }

  function go(i, animate = true) {
    if (!st.list.length) return;
    st.idx = ((i % st.list.length) + st.list.length) % st.list.length;
    const m = st.list[st.idx];
    closePop();
    if (animate && !reduce && st.shown) {
      // 旧画面淡出后整体隐藏（display:none），不与新画面同时计数
      el.stage.classList.add('out');
      clearTimeout(st.outT);
      st.outT = setTimeout(() => { el.stage.classList.remove('out'); draw(m, true); }, 260);
    } else {
      draw(m, animate);
    }
    st.shown = m.id;
    markThumb();
    schedule();
  }

  // ---------- 依据浮层 ----------
  function relFor(m, id) {
    const rs = m.rels.filter((r) => r.to === id || r.via === id);
    return rs[rs.length - 1] || null;
  }
  function lastReadJudge(s, ids) {
    const js = (ids || []).map((x) => s.judges.get(x)).filter(Boolean);
    return js.filter((j) => j.reading || j.p != null).pop() || js.pop() || null;
  }
  function trigText(r) {
    if (!r || !r.trigger) return '';
    return typeof r.trigger === 'object' ? (r.trigger.ctx ?? JSON.stringify(r.trigger)) : r.trigger;
  }
  function basisHtml(j, trig) {
    let h = '<dl class="gp-dl">';
    if (j) {
      h += `<dt>题面</dt><dd>${esc(j.q || '—')}</dd>`;
      h += `<dt>读数</dt><dd>${readingCell(j)}</dd>`;
      h += `<dt>出口</dt><dd>${exitChip(j.exit)}</dd>`;
    }
    h += `<dt>触发</dt><dd>${trig ? `<div class="gp-trig">${esc(trig)}</div>` : '<span class="muted">—</span>'}</dd>`;
    return `${h}</dl>`;
  }

  function openPop(html, x, y) {
    el.pop.innerHTML = `${html}<button class="gp-go" type="button">看依据链 →</button>`;
    el.pop.hidden = false;
    st.popOpen = true;
    clearTimeout(st.timer);
    markThumb();
    const r = el.pop.getBoundingClientRect();
    const nx = Math.min(window.innerWidth - r.width - 12, Math.max(12, x + 14));
    const ny = y + 14 + r.height > window.innerHeight - 12 ? Math.max(12, y - r.height - 14) : y + 14;
    el.pop.style.left = `${nx}px`;
    el.pop.style.top = `${ny}px`;
  }
  function closePop() {
    if (!st.popOpen) return;
    el.pop.hidden = true;
    st.popOpen = false;
    markThumb();
    schedule();
  }

  function onStageClick(ev) {
    const m = st.list[st.idx];
    if (!m) return;
    const s = getStore();
    const nodeG = ev.target.closest('[data-m]');
    const edgeG = ev.target.closest('[data-e]');
    if (!nodeG && !edgeG) { closePop(); return; }
    ev.stopPropagation();
    if (nodeG) {
      const id = nodeG.dataset.m;
      const nd = s.nodes.get(id) || {};
      const row = m.rows.find((r) => r.id === id) || {};
      let body;
      if (id === m.origin) {
        const it = s.intents.get(m.root);
        const j = (s.aug.judgesAbout.get(m.root) || []).find((x) => x.node === id && readingOfAny(x));
        body = basisHtml(j ? { ...j } : null, it ? `发出意图「${it.text}」` : '');
      } else {
        const r = relFor(m, id);
        body = basisHtml(r ? lastReadJudge(s, r.judges) : null, trigText(r));
      }
      const dgg = [row.do && `<span><i class="ic do">◆</i>${esc(row.do)}</span>`, row.give && `<span><i class="ic give">↑</i>${esc(row.give)}</span>`, row.get && `<span><i class="ic get">↓</i>${esc(row.get)}</span>`].filter(Boolean).join('');
      openPop(`<div class="gp-h">${glyphIcon(nd.kind, m.color(id), 26)}<b>${esc(roleOf(nd) || id)}</b>${nd.name && nd.name !== roleOf(nd) ? `<small>${esc(nd.name)}</small>` : ''}</div>${dgg ? `<div class="gp-dg">${dgg}</div>` : ''}${body}`, ev.clientX, ev.clientY);
    } else {
      const k = Number(edgeG.dataset.e);
      const e = edgesOf(m)[k];
      if (!e) return;
      const j = m.edgeJudges[k] || null;
      const r = relFor(m, e.to) || relFor(m, e.from);
      const fromN = s.nodes.get(e.from) || {}; const toN = s.nodes.get(e.to) || {};
      openPop(`<div class="gp-h">${glyphIcon(fromN.kind, m.color(e.from), 22)}<b>${esc(roleOf(fromN))}</b><span class="gp-arr">→</span>${glyphIcon(toN.kind, m.color(e.to), 22)}<b>${esc(roleOf(toN))}</b></div>
        ${e.give ? `<div class="gp-give">${esc(e.give)}</div>` : ''}${basisHtml(j, trigText(r))}`, ev.clientX, ev.clientY);
    }
  }
  const readingOfAny = (j) => j && (j.reading || j.p != null);

  el.svg.addEventListener('click', onStageClick);
  el.pop.addEventListener('click', (ev) => {
    if (ev.target.closest('.gp-go')) { const m = st.list[st.idx]; closePop(); if (m) openPlan(m.id); }
    ev.stopPropagation();
  });
  document.addEventListener('click', (ev) => { if (st.popOpen && !ev.target.closest('#g-pop')) closePop(); });
  el.thumbs.addEventListener('click', (ev) => { const b = ev.target.closest('.g-th'); if (b) go(Number(b.dataset.i)); });
  window.addEventListener('keydown', (ev) => {
    if (!st.active) return;
    if (ev.key === 'Escape') closePop();
    if (ev.key === 'ArrowRight') go(st.idx + 1);
    if (ev.key === 'ArrowLeft') go(st.idx - 1);
  });

  // ---------- 对外 ----------
  function update(force = false) {
    const all = glanceOrder(getModels());
    const { n } = thumbCount();
    const list = all.slice(0, n);
    const sig = list.map((m) => `${m.id}:${m.ex.act}/${m.edges.length}:${m.judgeN}`).join(',');
    if (!force && sig === st.sig) return;
    const keep = st.list[st.idx] ? st.list[st.idx].id : null;
    st.sig = sig;
    st.list = list;
    const ki = keep ? list.findIndex((m) => m.id === keep) : -1;
    if (!list.length) {
      el.title.textContent = '';
      el.svg.innerHTML = '';
      el.nums.innerHTML = '';
      el.thumbs.innerHTML = '<div class="nodata">暂无方案</div>';
      return;
    }
    if (ki >= 0) {
      st.idx = ki;
      buildThumbs();
      draw(list[ki], force);
      markThumb();
      if (force) schedule();
    } else {
      st.idx = 0;
      buildThumbs();
      go(0);
    }
  }

  return {
    show() { st.active = true; document.body.classList.add('g-mode'); update(true); },
    hide() { st.active = false; clearTimeout(st.timer); closePop(); document.body.classList.remove('g-mode'); },
    update,
    resize() { if (st.active) update(true); },
    go(i) { go(i, false); },
    hold(v) { st.hold = !!v; if (st.hold) clearTimeout(st.timer); else schedule(); markThumb(); },
    get list() { return st.list.map((m) => ({ id: m.id, size: m.size, tier: tier(m), title: titleOf(m) })); },
    get index() { return st.idx; },
    get budget() { return st.budget; },
  };
}

// 杭州聚餐演示：一次真机运行，八个镜头。所有数字取自 data.js（由 脚本/网页数据.py 从真机报告、账本、OSM 快照搬来）。
// 画面是 canvas；每一帧是 (组, 步, 进度) 的纯函数，所以能拖、能深链：#甲组/3@0.5 停在第 3 步的 50% 处。
"use strict";
const D = window.DINNER;
const G_NAMES = Object.keys(D.组);
const $ = id => document.getElementById(id);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[c]));
const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
const lerp = (a, b, t) => a + (b - a) * t;
const ease = x => x < .5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
const seg = (p, a, b) => clamp((p - a) / (b - a));
const eseg = (p, a, b) => ease(seg(p, a, b));
const hanLen = s => (String(s).match(/[㐀-鿿]/g) || []).length;
const hash = s => { let h = 2166136261; for (const c of String(s)) { h ^= c.charCodeAt(0); h = Math.imul(h, 16777619); } return (h >>> 0) / 4294967295; };
const S = {g: G_NAMES[0], t: 0, playing: true, freeze: false, last: 0, wasPlaying: false};

// ───────── 数据 ─────────
const grp = () => D.组[S.g];
const R = () => grp().结果;
const people = () => grp().人物.人;
const names = () => people().map(p => p.名);
const PAL = ["#4fd8ff", "#a98bff", "#ff6fcf", "#6f9bff", "#dfe6ff"];
const color = n => PAL[names().indexOf(n)] || "#888";
const reading = k => grp().读数[k];
const kind = e => !e ? "none" : e.startsWith("unsure") ? "maybe" : e;
// 极性：act 对这道题是好事（+1）还是坏事（-1）；只决定颜色
function tone(exit, pol) { const k = kind(exit); if (k === "maybe") return "maybe"; if (k === "act") return pol > 0 ? "good" : "bad"; if (k === "ignore") return pol > 0 ? "bad" : "good"; return "none"; }
const TC = {good: "#3ef0b0", bad: "#ff5b6e", maybe: "#ffb547", none: "#5b6b8a"};
const shortName = s => String(s).split(" / ")[0].split("：")[0];
const dishName = s => String(s).split("：")[0];
const actName = s => String(s).split("：")[0];

// 投影：经纬度 → 公里平面（经度按杭州纬度折算，与程序里通勤估算同一口径）
const P = (la, lo) => [(lo - 120.17) * 96.18, -(la - 30.26) * 111];
const REST = D.餐厅.map(r => ({id: r[0], name: r[1], la: r[2], lo: r[3], cuisine: r[4], hours: r[5], xy: P(r[2], r[3]), h: hash(r[0])}));
const RID = new Map(REST.map((r, i) => [r.id, i]));
const LAKE = D.地图.西湖.map(ch => ch.map(([a, b]) => P(a, b)));
const RIVER = D.地图.钱塘江.map(ch => ch.map(([a, b]) => P(a, b)));
const METRO = D.地图.地铁站.map(([n, a, b]) => ({n, xy: P(a, b)}));

function quant(xs, q) { const s = [...xs].sort((a, b) => a - b); return s[clamp(Math.floor(q * (s.length - 1)), 0, s.length - 1)]; }
function bbOf(pts) { let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity; for (const [x, y] of pts) { x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); } return {x0, y0, x1, y1}; }

// 每组的派生量（只做搬运和按 id 对齐，不重算程序的筛选或判断）
const PREP = {};
function prep() {
  if (PREP[S.g]) return PREP[S.g];
  const r = R(), e = r.餐厅, L = e.各层;
  const sets = ["有名字", "可去", "最远一小时内", "去重"].map(k => new Set(L[k]));
  const cand = e.候选.map(c => ({...c, i: RID.get(c.id)}));
  sets.push(new Set(cand.map(c => c.id)));
  const homes = e.家.map(h => ({n: h.名, st: h.站, xy: P(h.坐标[0], h.坐标[1])}));
  const hc = [homes.reduce((a, h) => a + h.xy[0], 0) / homes.length, homes.reduce((a, h) => a + h.xy[1], 0) / homes.length];
  // 每家店在第几轮被筛掉（5 = 留到最后）
  const out = REST.map(rs => { for (let k = 0; k < 5; k++) if (!sets[k].has(rs.id)) return k; return 5; });
  const counts = sets.map(s => s.size);
  // 轮内先后：远的先熄（最后一轮按程序排好的名次，从后往前熄）
  const rank = new Map(L.去重.map((id, k) => [id, k]));
  const far = REST.map(rs => Math.hypot(rs.xy[0] - hc[0], rs.xy[1] - hc[1]));
  const order = REST.map(() => 0);
  for (let k = 0; k < 5; k++) {
    const ids = REST.map((_, i) => i).filter(i => out[i] === k);
    if (k === 4) ids.sort((a, b) => rank.get(REST[b].id) - rank.get(REST[a].id));
    else ids.sort((a, b) => far[b] - far[a]);
    ids.forEach((i, j) => order[i] = ids.length > 1 ? j / (ids.length - 1) : 0);
  }
  // 城区取景：住处 + 20 家候选 + 「最远一小时内」那一层的 5%–95% 分位
  const near = L.最远一小时内.map(id => REST[RID.get(id)].xy);
  const qx = near.map(p => p[0]), qy = near.map(p => p[1]);
  const city = bbOf([...homes.map(h => h.xy), ...cand.map(c => REST[c.i].xy), [quant(qx, .05), quant(qy, .05)], [quant(qx, .95), quant(qy, .95)]]);
  const homeBB = bbOf(homes.map(h => h.xy));
  const chosen = cand.find(c => c.id === e.选.id);
  PREP[S.g] = {r, e, cand, homes, hc, out, order, counts, city, homeBB, chosen};
  return PREP[S.g];
}

// ───────── 画布 ─────────
const cv = $("cv"), ctx = cv.getContext("2d");
let VW = 0, VH = 0, DPR = 1, SAFE = {x: 0, y: 0, w: 1, h: 1};
function resize() {
  DPR = Math.min(2, devicePixelRatio || 1); VW = innerWidth; VH = innerHeight;
  cv.width = Math.round(VW * DPR); cv.height = Math.round(VH * DPR);
  const r = $("stage").getBoundingClientRect(); SAFE = {x: r.left, y: r.top, w: r.width, h: r.height};
  bg = null;
}
const MOBILE = () => VW < 720;
const SPR = new Map();
function sprite(col) { // 发光点：中心白热，向外按颜色衰减
  if (SPR.has(col)) return SPR.get(col);
  const c = document.createElement("canvas"); c.width = c.height = 64; const g = c.getContext("2d");
  const gr = g.createRadialGradient(32, 32, 0, 32, 32, 32);
  gr.addColorStop(0, "rgba(255,255,255,1)"); gr.addColorStop(.12, col); gr.addColorStop(.35, col + "88"); gr.addColorStop(1, col + "00");
  g.fillStyle = gr; g.fillRect(0, 0, 64, 64); SPR.set(col, c); return c;
}
function glow(x, y, r, col, a = 1) { if (a <= .003) return; ctx.globalAlpha = clamp(a); ctx.drawImage(sprite(col), x - r, y - r, 2 * r, 2 * r); ctx.globalAlpha = 1; }
function aura(x, y, r, col, a) { if (a <= .003) return; ctx.save(); ctx.globalCompositeOperation = "lighter"; const g = ctx.createRadialGradient(x, y, 0, x, y, r); g.addColorStop(0, col); g.addColorStop(1, "rgba(0,0,0,0)"); ctx.globalAlpha = clamp(a); ctx.fillStyle = g; ctx.fillRect(x - r, y - r, 2 * r, 2 * r); ctx.restore(); }
let bg = null;
function drawBg() {
  if (!bg) {
    bg = document.createElement("canvas"); bg.width = cv.width; bg.height = cv.height; const g = bg.getContext("2d");
    const gr = g.createRadialGradient(bg.width * .5, bg.height * .45, 0, bg.width * .5, bg.height * .45, Math.max(bg.width, bg.height) * .75);
    gr.addColorStop(0, "#0a1020"); gr.addColorStop(1, "#03050a"); g.fillStyle = gr; g.fillRect(0, 0, bg.width, bg.height);
    for (let i = 0; i < 260; i++) { const a = hash("s" + i), b = hash("t" + i), c = hash("u" + i); g.fillStyle = `rgba(160,190,255,${.05 + c * .18})`; g.fillRect(a * bg.width, b * bg.height, DPR * (c > .9 ? 1.6 : 1), DPR * (c > .9 ? 1.6 : 1)); }
  }
  ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.drawImage(bg, 0, 0); ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
  // 缓慢漂浮的微粒
  const t = performance.now() / 1000;
  ctx.save(); ctx.globalCompositeOperation = "lighter";
  for (let i = 0; i < 90; i++) { const a = hash("p" + i), b = hash("q" + i), c = hash("r" + i); const x = ((a * VW + t * (6 + c * 10)) % VW), y = (b * VH - t * (4 + a * 8)) % VH; const yy = y < 0 ? y + VH : y; ctx.globalAlpha = .12 + .25 * (.5 + .5 * Math.sin(t * (1 + c) + i)); ctx.fillStyle = c > .6 ? "#9fe6ff" : "#ffb58a"; ctx.fillRect(x, yy, 1.5, 1.5); }
  ctx.restore();
}

// 镜头：{cx, cy, s}（公里坐标的中心、每公里多少像素），落在 SAFE 的中心
let CAM = {cx: 0, cy: 0, s: 30};
function camFit(bb, pad = 40) {
  const w = Math.max(.5, bb.x1 - bb.x0), h = Math.max(.5, bb.y1 - bb.y0);
  const px = Math.min(pad, SAFE.w * .12), py = Math.min(pad, SAFE.h * .12);
  return {cx: (bb.x0 + bb.x1) / 2, cy: (bb.y0 + bb.y1) / 2, s: Math.min((SAFE.w - 2 * px) / w, (SAFE.h - 2 * py) / h)};
}
function camMix(a, b, t) { return {cx: lerp(a.cx, b.cx, t), cy: lerp(a.cy, b.cy, t), s: Math.exp(lerp(Math.log(a.s), Math.log(b.s), t))}; }
function camZoom(c, k) { return {...c, s: c.s * k}; }
const sx = x => SAFE.x + SAFE.w / 2 + (x - CAM.cx) * CAM.s;
const sy = y => SAFE.y + SAFE.h / 2 + (y - CAM.cy) * CAM.s;
const scr = p => [sx(p[0]), sy(p[1])];
const inSafe = (x, y, m = 0) => x >= SAFE.x - m && x <= SAFE.x + SAFE.w + m && y >= SAFE.y - m && y <= SAFE.y + SAFE.h + m;

// 每帧登记：可见文字（字数审计）、可点区域（只收当帧看得见的）、主画面内容包围盒
let FR = {texts: [], hits: [], bb: null};
function addBB(x, y, r = 0) { if (!inSafe(x, y, 2)) return; x = clamp(x, SAFE.x, SAFE.x + SAFE.w); y = clamp(y, SAFE.y, SAFE.y + SAFE.h); const b = FR.bb || (FR.bb = {x0: x, y0: y, x1: x, y1: y}); b.x0 = Math.min(b.x0, x - r); b.y0 = Math.min(b.y0, y - r); b.x1 = Math.max(b.x1, x + r); b.y1 = Math.max(b.y1, y + r); }
function hit(x, y, r, fn, a = 1) { if (a > .25) FR.hits.push({x, y, r, fn}); }
function hitRect(x, y, w, h, fn, a = 1) { if (a > .25) FR.hits.push({x: x + w / 2, y: y + h / 2, rw: w / 2, rh: h / 2, fn}); }
function text(s, x, y, o = {}) {
  const a = o.a ?? 1; if (a <= .01) return;
  ctx.save(); ctx.globalAlpha = clamp(a); ctx.font = `${o.w || 600} ${o.size || 16}px ${o.mono ? "SF Mono, Menlo, monospace" : getComputedStyle(document.body).fontFamily}`;
  ctx.textAlign = o.align || "center"; ctx.textBaseline = o.base || "middle";
  if (o.glow) { ctx.shadowColor = o.glow; ctx.shadowBlur = 14; }
  if (o.halo !== false) { ctx.lineWidth = 4; ctx.strokeStyle = "rgba(4,6,12,.85)"; ctx.lineJoin = "round"; ctx.strokeText(s, x, y); }
  ctx.fillStyle = o.color || "#e8eefb"; ctx.fillText(s, x, y); ctx.restore();
  const w = measure(s, o.size || 16, o.w || 600);
  if (a > .05) FR.texts.push({s, a, x, y, size: o.size || 16});
  addBB(x, y, 0);
  return w;
}
const MC = document.createElement("canvas").getContext("2d");
function measure(s, size, w = 600) { MC.font = `${w} ${size}px ${getComputedStyle(document.body).fontFamily}`; return MC.measureText(s).width; }
function line(x0, y0, x1, y1, col, w = 1.5, a = 1, dash = null) {
  if (a <= .01) return; ctx.save(); ctx.globalCompositeOperation = "lighter"; ctx.lineCap = "round";
  if (dash) ctx.setLineDash(dash);
  ctx.strokeStyle = col; ctx.globalAlpha = clamp(a * .18); ctx.lineWidth = w * 5; ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.stroke();
  ctx.globalCompositeOperation = "source-over"; ctx.globalAlpha = clamp(a); ctx.lineWidth = w; ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.stroke(); ctx.restore();
}
function ring(x, y, r, col, w = 2, a = 1) {
  if (a <= .01) return; ctx.save(); ctx.globalCompositeOperation = "lighter"; ctx.strokeStyle = col;
  ctx.globalAlpha = clamp(a * .22); ctx.lineWidth = w * 4; ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.stroke();
  ctx.globalCompositeOperation = "source-over"; ctx.globalAlpha = clamp(a); ctx.lineWidth = w; ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.stroke(); ctx.restore();
}
function arc(x, y, r, a0, a1, col, w, a = 1) {
  if (a <= .01) return; ctx.save(); ctx.globalCompositeOperation = "lighter"; ctx.strokeStyle = col; ctx.lineCap = "butt";
  ctx.globalAlpha = clamp(a * .25); ctx.lineWidth = w * 2.2; ctx.beginPath(); ctx.arc(x, y, r, a0, a1); ctx.stroke();
  ctx.globalCompositeOperation = "source-over"; ctx.globalAlpha = clamp(a); ctx.lineWidth = w; ctx.beginPath(); ctx.arc(x, y, r, a0, a1); ctx.stroke(); ctx.restore();
}

// 地图底：西湖、钱塘江、地铁站（都来自 OSM 快照）
function drawMap(a = 1) {
  if (a <= .01) return;
  ctx.save(); ctx.globalAlpha = a;
  ctx.lineJoin = "round";
  RIVER.forEach(ch => { ctx.beginPath(); ch.forEach((p, i) => i ? ctx.lineTo(sx(p[0]), sy(p[1])) : ctx.moveTo(sx(p[0]), sy(p[1]))); ctx.strokeStyle = "rgba(40,90,170,.22)"; ctx.lineWidth = Math.max(6, CAM.s * .9); ctx.stroke(); ctx.strokeStyle = "rgba(90,160,255,.25)"; ctx.lineWidth = 1; ctx.stroke(); });
  LAKE.forEach((ch, k) => { ctx.beginPath(); ch.forEach((p, i) => i ? ctx.lineTo(sx(p[0]), sy(p[1])) : ctx.moveTo(sx(p[0]), sy(p[1]))); ctx.closePath(); if (k === 0) { ctx.fillStyle = "rgba(40,100,190,.16)"; ctx.fill(); } ctx.strokeStyle = "rgba(100,180,255,.45)"; ctx.lineWidth = 1.2; ctx.shadowColor = "rgba(100,180,255,.6)"; ctx.shadowBlur = 8; ctx.stroke(); ctx.shadowBlur = 0; });
  ctx.fillStyle = "rgba(120,160,235,.28)";
  for (const m of METRO) { const x = sx(m.xy[0]), y = sy(m.xy[1]); if (inSafe(x, y)) ctx.fillRect(x - 1, y - 1, 2, 2); }
  ctx.restore();
}

// 住处：发光的圈，名字放在圈外，互相不压
function placeLabels(items, size, obst = [], center = null) { // items: {x,y,r,s}；obst: 额外要躲开的圆 {x,y,r}；center: 名字尽量朝外
  const boxes = [...items, ...obst].map(it => ({x0: it.x - it.r, y0: it.y - it.r, x1: it.x + it.r, y1: it.y + it.r}));
  const placed = [];
  const dirs = [[1, 0], [-1, 0], [0, -1], [0, 1], [.75, -.75], [-.75, -.75], [.75, .75], [-.75, .75]];
  return items.map(it => {
    const w = measure(it.s, size) + 6, h = size + 4;
    let best = null, bestScore = Infinity;
    for (const [dx, dy] of dirs) for (const k of [1, 1.8]) {
      const gap = it.r + 8 * k;
      const cx = it.x + dx * (gap + (dx ? w / 2 : 0)), cy = it.y + dy * (gap + (dy ? h / 2 : 0));
      const b = {x0: cx - w / 2, y0: cy - h / 2, x1: cx + w / 2, y1: cy + h / 2};
      let sc = (k - 1) * 3 + (dirs.findIndex(d => d[0] === dx && d[1] === dy)) * .1;
      if (center) { const ox = it.x - center[0], oy = it.y - center[1], L = Math.hypot(ox, oy) || 1, dl = Math.hypot(dx, dy); sc += (1 - (ox * dx + oy * dy) / (L * dl)) * 40; }
      for (const o of [...placed, ...boxes]) { const ix = Math.max(0, Math.min(b.x1, o.x1) - Math.max(b.x0, o.x0)), iy = Math.max(0, Math.min(b.y1, o.y1) - Math.max(b.y0, o.y0)); sc += ix * iy; }
      if (b.x0 < SAFE.x || b.x1 > SAFE.x + SAFE.w || b.y0 < SAFE.y || b.y1 > SAFE.y + SAFE.h) sc += 5000;
      if (sc < bestScore) { bestScore = sc; best = b; }
    }
    placed.push(best); return [(best.x0 + best.x1) / 2, (best.y0 + best.y1) / 2];
  });
}
function drawHomes(G, a = 1, opt = {}) {
  if (a <= .01) return [];
  const rr = MOBILE() ? 9 : 13, size = MOBILE() ? 14 : 17;
  const pts = G.homes.map((h, k) => { const [x, y] = scr(h.xy); const ak = a * (opt.stagger ? clamp(opt.stagger * 5 - k) : 1); return {h, x, y, ak}; });
  const t = performance.now() / 1000;
  pts.forEach(({h, x, y, ak}) => {
    const c = color(h.n), pulse = 1 + .12 * Math.sin(t * 2 + names().indexOf(h.n));
    glow(x, y, rr * 3.2 * pulse, c, ak * .55);
    ring(x, y, rr, c, 2.2, ak);
    glow(x, y, rr * .9, c, ak);
    addBB(x, y, rr * 2);
    hit(x, y, rr + 10, ev => showPop(personPop(h.n), ev), ak);
  });
  if (opt.names !== false) {
    const lab = placeLabels(pts.map(p => ({x: p.x, y: p.y, r: rr * 1.3, s: p.h.n})), size, opt.obst || []);
    pts.forEach((p, k) => text(p.h.n, lab[k][0], lab[k][1], {size, color: color(p.h.n), a: p.ak, glow: color(p.h.n)}));
  }
  return pts;
}
// 画面外的店：不逐个画，按四条边各汇总成一个朝外的小箭头和数目（点开有说明）
function edgeSide(x, y) {
  const cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2, dx = (x - cx) / SAFE.w, dy = (y - cy) / SAFE.h;
  return Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 1 : 3) : (dy > 0 ? 2 : 0);
}
function drawEdgeCounts(sides, a = 1) {
  const pos = [[SAFE.x + SAFE.w / 2, SAFE.y + 10, -Math.PI / 2], [SAFE.x + SAFE.w - 10, SAFE.y + SAFE.h / 2, 0], [SAFE.x + SAFE.w / 2, SAFE.y + SAFE.h - 10, Math.PI / 2], [SAFE.x + 10, SAFE.y + SAFE.h / 2, Math.PI]];
  sides.forEach((n, k) => {
    if (!n) return;
    const [x, y, ang] = pos[k];
    ctx.save(); ctx.translate(x, y); ctx.rotate(ang); ctx.globalAlpha = .7 * a; ctx.fillStyle = "#ff9150";
    ctx.beginPath(); ctx.moveTo(6, 0); ctx.lineTo(-4, -6); ctx.lineTo(-4, 6); ctx.closePath(); ctx.fill(); ctx.restore();
    const tx = x - Math.cos(ang) * 22, ty = y - Math.sin(ang) * 16;
    text(String(n), tx, ty, {size: 14, mono: true, color: "#ffb58a", a: .8 * a, w: 500});
    hit(x, y, 22, ev => showPop(`<h4>画面外还有 ${n} 家</h4><p>这一侧、城区取景范围以外，还有 ${n} 家还没被筛掉的店（杭州市界内的远郊）。地图只取城区，远处的店不逐个画。</p><p class="note">取景范围：这组五人的住处、最后 20 家店，以及「最远一小时内」那一层店的 5%–95% 分位。</p>`, ev), a);
  });
}
// 576 家店：暖色光点。life(i) 返回 {a, lit, flare}；flare 是熄灭前亮的那一下
function drawRests(G, life, opt = {}) {
  const sides = [0, 0, 0, 0];
  const base = MOBILE() ? 5 : 7, now = performance.now();
  ctx.save(); ctx.globalCompositeOperation = "lighter";
  REST.forEach((rs, i) => {
    const L = life(i); if (L.a <= .01) return;
    const x = sx(rs.xy[0]), y = sy(rs.xy[1]);
    if (!inSafe(x, y)) { if (L.lit) sides[edgeSide(x, y)]++; return; }
    const tw = .75 + .25 * Math.sin(now / 700 + rs.h * 40);
    const r = base * (1 + (L.flare || 0) * 1.8) * (L.lit ? (L.big || 1) : .8);
    glow(x, y, r, L.lit ? (L.col || "#ff9150") : "#7d6a78", L.a * (L.lit ? tw : 1));
    if (L.a > .1) addBB(x, y, 2);
    if (opt.hit && L.lit) hit(x, y, 7, ev => showPop(restPop(rs), ev), L.a);
  });
  ctx.restore();
  if (opt.edges !== false) drawEdgeCounts(sides, opt.edgeA ?? 1);
  return sides.reduce((a, b) => a + b, 0);
}

// ───────── 浮层（点开才有字；打开时暂停，关掉后照原样恢复） ─────────
const pop = $("pop");
function showPop(html, ev) {
  if (!pop.classList.contains("show")) S.wasPlaying = S.playing;
  pause();
  pop.innerHTML = `<button class="x" aria-label="关闭">×</button>` + html; pop.classList.add("show");
  pop.querySelector(".x").onclick = hidePop;
  const r = pop.getBoundingClientRect();
  let x = (ev?.clientX ?? VW / 2 - r.width / 2) + 14, y = (ev?.clientY ?? VH / 2 - r.height / 2) + 14;
  if (x + r.width > VW - 12) x = Math.max(12, (ev?.clientX ?? VW) - r.width - 14);
  if (y + r.height > VH - 12) y = Math.max(12, VH - r.height - 12);
  pop.style.left = clamp(x, 12, Math.max(12, VW - r.width - 12)) + "px"; pop.style.top = Math.max(12, y) + "px";
}
function hidePop() { if (!pop.classList.contains("show")) return; pop.classList.remove("show"); if (S.wasPlaying) play(); }
document.addEventListener("pointerdown", e => { if (pop.classList.contains("show") && !pop.contains(e.target) && e.target !== cv && !e.target.closest("[data-pop]")) hidePop(); });
const EXW = {act: "是", ignore: "否", maybe: "拿不准", none: "—"};
function chip(exit, pol, words) { const k = kind(exit); return `<span class="chip ${tone(exit, pol)}">${esc((words && words[k]) || EXW[k] || exit)}</span>`; }
// 一次语义判断：题面、读数条、出口；材料默认折叠
function judgeHtml(j) {
  const r = j.key ? reading(j.key) : null;
  let g = "";
  if (r && typeof r.读数 === "number") g = `<div class="gauge"><i style="left:${r.读数 * 100}%"></i></div><div class="grow"><span>0 否</span><span>读数 ${r.读数.toFixed(2)}（0.3–0.7 为拿不准）</span><span>是 1</span></div>`;
  else if (r && Array.isArray(r.读数)) { const mx = Math.max(...r.读数); g = `<div class="dist">${r.读数.map((x, k) => `<span>${esc((j.scale || [])[k] ?? "档 " + k)}</span><i style="width:${x / mx * 100}%"></i><span>${x.toFixed(2)}</span>`).join("")}</div>`; }
  return `<p class="q">「${esc(j.q)}」</p>${g}${j.exit ? `<p>回答 ${chip(j.exit, j.pol, j.words)}</p>` : ""}${j.extra ? `<p>${j.extra}</p>` : ""}${j.mat ? `<details><summary>看材料原文</summary><p>${esc(j.mat)}</p></details>` : ""}${r ? `<p class="note">${r.token} token · $${r.花费.toFixed(6)} · 账本键 ${esc(j.key)}</p>` : ""}`;
}
const selfOf = n => people().find(x => x.名 === n)?.自述 || "";
function pairSay(a, b) { const xs = grp().人物.关系.filter(s => s.涉及.length > 1 && s.涉及.every(x => x === a || x === b)); return xs.map(s => s.说法).join(" ") || "没有直接提到两人关系的说法。"; }
const fillAB = (q, a, b) => String(q).replace(/\ba\b/, a).replace(/\bb\b/, b);
function personPop(n) { const p = people().find(x => x.名 === n); return `<h4>${esc(n)} · 住${esc(p.住处站)}站附近</h4><p class="q">${esc(p.自述)}</p><p class="note">这段自述是程序的输入；每道和这个人有关的题，材料里都有它。</p>`; }
function restPop(rs) {
  const G = prep(), c = G.cand.find(x => x.id === rs.id);
  const st = ["没有名字", "是小吃店或那时打烊", "最远的人通勤超过一小时", "和排在前面的店同名", "不在离大家最近的 20 家里"][G.out[RID.get(rs.id)]];
  return `<h4>${esc(rs.name)}</h4><p class="note">菜系标签：${esc(rs.cuisine || "无")} · 营业时间：${esc(rs.hours || "未标")}</p>${c ? `<p>进了最后 20 家。通勤估算（分钟）：${c.通勤.map((m, k) => `${esc(names()[k])} ${m}`).join("，")}</p>` : `<p>在这一轮被代码筛掉：${esc(st || "")}</p>`}<p class="note">OSM ${esc(rs.id)}</p>`;
}
function infoPop() {
  const G = prep();
  return `<h4>这页在放什么</h4><p class="q">${esc(grp().人物.简介)}</p><p>程序是 <b>聚餐.jpp</b>，这里放的是它一次真机运行的结果，八个镜头自动播放，底部可以拖动、暂停。画面上的人、店、线、座位都能点开，看题面、读数和材料。</p>
  <p>颜色只表示判断结果：<span class="chip good">绿 好的一侧</span> <span class="chip maybe">琥珀 拿不准</span> <span class="chip bad">红 不好的一侧</span>。拿不准指读数落在作者写的线 0.3–0.7 之间。</p>
  <details><summary>数据来源与画法</summary><p>餐厅、地铁站、西湖与钱塘江轮廓：${esc(D.来源.OSM)}，${esc(D.来源.实例)}，查询于 ${esc(D.来源.查询时间)}。地图只取城区：范围由这组五人的住处、最后 20 家店，和「最远一小时内」那一层店的 5%–95% 分位定；更远的店在画面边缘画成朝外的小箭头。</p><p>示例菜单、饭后活动、题树由生成模型拟，不是事实。读数、回答、花费取自 真机/ 下的报告与账本，由 脚本/网页数据.py 搬进 data.js；页面不调任何接口。</p><p>键盘：← → 换步，空格暂停。链接 #甲组/3 打开某组某步。</p></details>`;
}

// ───────── 步骤 ─────────
// 每步：dur 秒；title(p)；nums(p) → [[值, 标签, 类]]（最多 3 个）；draw(p)
const STEPS = [];

// 1 开场：全城 576 家店亮起，镜头推近五个人的住处
STEPS.push({
  dur: 7, map: true,
  title: () => "帮这五个人安排一顿饭",
  nums: p => [[REST.length, "家餐厅", "warm"], [5, "个人"]],
  draw(p) {
    const G = prep();
    const city = camFit(G.city, 30), home = camFit(G.homeBB, MOBILE() ? 70 : 110);
    CAM = camMix(camZoom(city, .8), home, eseg(p, .2, .62));
    drawMap(eseg(p, 0, .25));
    const off = drawRests(G, i => ({a: clamp(p * 5 - REST[i].h * 1.5) * .95, lit: true}), {hit: true, edgeA: 1 - seg(p, .2, .4)});
    drawHomes(G, eseg(p, .3, .5), {stagger: seg(p, .3, .6)});
    FR.off = off;
  }
});

// 2 代码筛：一轮一轮熄灭，最后剩 20 家
const ROUNDS = [[.1, .24], [.26, .36], [.38, .56], [.58, .68], [.7, .84]];
const RTITLE = ["去掉没有名字的店", "去掉小吃店和那时打烊的", "有人要走一小时以上的，去掉", "同名的店只留一家", "留下离大家最近的 20 家"];
function roundAt(p) { let k = -1; ROUNDS.forEach(([a], i) => { if (p >= a) k = i; }); return k; }
function restLife(G, p) { // 第 2 步进度 p 时，第 i 家店的状态
  return i => {
    const k = G.out[i];
    if (k === 5) return {a: 1, lit: true};
    const [a, b] = ROUNDS[k], tk = a + G.order[i] * (b - a) * .75;
    if (p < tk) return {a: 1, lit: true};
    const f = seg(p, tk, tk + .03);
    return f < 1 ? {a: 1, lit: true, flare: Math.sin(f * Math.PI)} : {a: .3, lit: false};
  };
}
STEPS.push({
  dur: 9, map: true,
  title: p => { const k = roundAt(p); return k < 0 ? "先用代码筛，不花判断" : p > .86 ? "筛到 20 家，一次判断没花" : RTITLE[k]; },
  nums: p => { const G = prep(), L = restLife(G, p); let n = 0; for (let i = 0; i < REST.length; i++) if (L(i).lit) n++; return [[n, "家还亮着", "warm"], [0, "次判断", "small"]]; },
  draw(p) {
    const G = prep();
    const city = camFit(G.city, 30), home = camFit(G.homeBB, MOBILE() ? 70 : 110);
    CAM = camMix(home, city, eseg(p, 0, .1));
    drawMap(1);
    const L0 = restLife(G, p); FR.off = drawRests(G, i => { const L = L0(i); if (G.out[i] === 5 && p > .84) return {...L, big: 1 + .5 * seg(p, .84, .9)}; return L; }, {hit: true});
    drawHomes(G, 1);
  }
});

// 3 逐店判断：20 家拉到一圈，每家一个五段色环（五个人），每判一次亮一段
function orbitLayout(G) {
  const key = `${S.g}|${SAFE.w}x${SAFE.h}`; if (G._orb && G._orb.key === key) return G._orb;
  const pts = G.cand.map(c => REST[c.i].xy);
  const c0 = [pts.reduce((a, p) => a + p[0], 0) / pts.length, pts.reduce((a, p) => a + p[1], 0) / pts.length];
  const bb = bbOf(pts), span = Math.max(bb.x1 - bb.x0, bb.y1 - bb.y0, .6);
  const mob = MOBILE();
  const rx = SAFE.w * (mob ? .40 : .40), ry = SAFE.h * (mob ? .37 : .40), oy = mob ? SAFE.h * .06 : 0;
  const s0 = Math.min(rx, ry) * 1.05 / span;
  const cam = {cx: (bb.x0 + bb.x1) / 2, cy: (bb.y0 + bb.y1) / 2 - oy / s0, s: s0};
  // 椭圆按弧长取 20 个等距点
  const N = 400, samp = []; let acc = 0, prev = null;
  for (let k = 0; k <= N; k++) { const t = -Math.PI / 2 + k / N * 2 * Math.PI; const q = [Math.cos(t) * rx, Math.sin(t) * ry]; if (prev) acc += Math.hypot(q[0] - prev[0], q[1] - prev[1]); samp.push({t, q, acc}); prev = q; }
  const slots = G.cand.map((_, k) => { const target = k / G.cand.length * acc; return samp.find(s => s.acc >= target).q; });
  // 按真实位置相对中心的方位角排，引线不交叉；再整体转一个偏移，让总位移最小
  const az = G.cand.map((c, k) => ({k, a: Math.atan2(pts[k][1] - (bb.y0 + bb.y1) / 2, pts[k][0] - cam.cx)})).sort((u, v) => u.a - v.a);
  const slotAz = slots.map(q => Math.atan2(q[1], q[0]));
  const slotOrder = slots.map((_, k) => k).sort((u, v) => slotAz[u] - slotAz[v]);
  let best = null, bd = Infinity;
  for (let off = 0; off < slots.length; off++) { let d = 0; az.forEach((o, j) => { const sa = slotAz[slotOrder[(j + off) % slots.length]]; d += Math.abs(Math.atan2(Math.sin(sa - o.a), Math.cos(sa - o.a))); }); if (d < bd) { bd = d; best = off; } }
  const pos = []; az.forEach((o, j) => { pos[o.k] = slots[slotOrder[(j + best) % slots.length]]; });
  const gap = acc / G.cand.length;
  const rr = clamp(gap * .3, 13, 30);
  G._orb = {key, cam, pos, rr, c0, oy};
  return G._orb;
}
const JT = [.06, .36]; // 100 次判断在这段进度里亮完
function judgedAt(p, n) { return clamp((p - JT[0]) / (JT[1] - JT[0])) * n; }
function candTones(c) { return c.判.map(j => tone(j.出口, -1)); } // 题问「会很难避开忌口吗」：是 = 坏
function drawKey(x, y, r, opt = {}) { // 色环图例：哪一段是谁
  const ns = names(), a = opt.a ?? 1;
  ns.forEach((n, k) => { const a0 = -Math.PI / 2 + k * 2 * Math.PI / 5 + .06, a1 = a0 + 2 * Math.PI / 5 - .12; arc(x, y, r, a0, a1, "#8c9bbd", 5, a * .8); const m = (a0 + a1) / 2; text(n, x + Math.cos(m) * (r + 26), y + Math.sin(m) * (r + 14), {size: 14, color: color(n), a, w: 500}); });
  addBB(x, y, r + 30);
}
function drawToneLegend(x, y, items, a = 1, align = "left") {
  let cx = x;
  const w = items.map(([l]) => measure(l, 15, 500) + 30), tot = w.reduce((s, v) => s + v, 0);
  if (align === "center") cx = x - tot / 2;
  items.forEach(([l, c], k) => { glow(cx + 6, y, 10, c, 1 * a); text(l, cx + 16, y, {size: 15, align: "left", color: "#c8d2e8", a, w: 500}); cx += w[k]; });
}
function strikeT(G) { return .44; }
STEPS.push({
  dur: 10, map: true,
  title: p => { const G = prep(); const bad = G.cand.filter(c => c.撞.length).length; if (p < JT[1] + .04) return "每家店、每个人，判一次"; if (p < .62) return bad ? `${bad} 家有人撞忌口，划掉` : "没有店撞忌口"; const c = G.chosen; return c.撞.length ? "选中：撞忌口的人最少" : c.营业 === "营业" ? "选中：没人撞忌口、确定开门" : "选中：没人撞忌口、拿不准最少"; },
  nums: p => { const G = prep(); const n = Math.floor(judgedAt(p, G.cand.length * 5)); let bad = 0, may = 0; G.cand.forEach((c, i) => c.判.forEach((j, k) => { if (i * 5 + k < n) { const t = tone(j.出口, -1); if (t === "bad") bad++; if (t === "maybe") may++; } })); return [[n, "次判断"], [bad, "撞忌口", "bad"], [may, "拿不准", "small"]]; },
  draw(p) {
    const G = prep(), O = orbitLayout(G);
    const city = camFit(G.city, 30);
    CAM = camMix(city, O.cam, eseg(p, 0, .08));
    const fly = eseg(p, 0, .08);
    drawMap(1 - .5 * fly);
    drawRests(G, i => G.out[i] === 5 ? {a: 1, lit: true} : {a: .3 * (1 - fly * .5), lit: false}, {hit: false, edges: false});
    drawHomes(G, 1 - fly, {names: false});
    const n = judgedAt(p, G.cand.length * 5), now = performance.now() / 1000;
    const cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2;
    const chosenK = G.cand.indexOf(G.chosen);
    G.cand.forEach((c, i) => {
      const [rx, ry] = scr(REST[c.i].xy);
      const tx = cx + O.pos[i][0], ty = cy + O.oy + O.pos[i][1];
      const x = lerp(rx, tx, fly), y = lerp(ry, ty, fly);
      const struck = c.撞.length > 0, fade = struck ? 1 - .7 * eseg(p, .44, .5) : 1;
      const pick = i === chosenK ? eseg(p, .62, .7) : 0;
      const dimOthers = i !== chosenK ? 1 - .45 * eseg(p, .62, .7) : 1;
      const A = fade * dimOthers;
      line(rx, ry, x, y, "#ff9150", .8, .22 * fly * A);
      const tones = candTones(c);
      const r = O.rr * (1 + .35 * pick);
      ring(x, y, r, "#2a3656", 3, .6 * fly);
      tones.forEach((t, k) => {
        const idx = i * 5 + k, on = n > idx;
        if (!on) return;
        const f = clamp(n - idx); // 刚亮的那一下偏白
        const a0 = -Math.PI / 2 + k * 2 * Math.PI / 5 + .07, a1 = a0 + 2 * Math.PI / 5 - .14;
        arc(x, y, r, a0, a1, f < 1 ? "#ffffff" : TC[t], O.rr * .34, A * (t === "maybe" ? .85 : 1));
        if (f < 1) glow(x + Math.cos((a0 + a1) / 2) * r, y + Math.sin((a0 + a1) / 2) * r, 18, "#ffffff", 1 - f);
      });
      glow(x, y, O.rr * .6 * (1 + pick), "#ff9150", A * (.8 + .2 * Math.sin(now * 3 + i)));
      if (struck && p > .44) { const s = eseg(p, .44, .5); line(x - r * 1.2, y + r * 1.2, x - r * 1.2 + 2.4 * r * s, y + r * 1.2 - 2.4 * r * s, "#ff5b6e", 2.5, .9 * fade + .1); }
      if (pick > 0) { ring(x, y, r * 1.6 + 4 * Math.sin(now * 4), "#ffd2b0", 2, pick); text(shortName(c.名), x, y - r * 1.6 - 20, {size: MOBILE() ? 15 : 19, color: "#ffd2b0", a: pick, glow: "#ff9150"}); }
      addBB(x, y, r + 4);
      hit(x, y, r + 6, ev => showPop(candPop(c), ev), A * fly);
    });
    // 图例：哪一段是谁、颜色什么意思
    const mob = MOBILE(), la = eseg(p, .03, .1) * (1 - eseg(p, .62, .7) * .6);
    const kx = SAFE.x + (mob ? 52 : 90), ky = mob ? SAFE.y + 40 : SAFE.y + SAFE.h - 92;
    drawKey(kx, ky, mob ? 20 : 30, {a: la});
    drawToneLegend(mob ? SAFE.x + SAFE.w / 2 : SAFE.x + SAFE.w / 2, SAFE.y + SAFE.h - 14, [["能吃", TC.good], ["拿不准", TC.maybe], ["撞忌口", TC.bad]], la, "center");
  }
});
function candPop(c) {
  const q = R().餐厅.题;
  const rows = c.判.map((j, k) => { const r = reading(j.键); return `<tr><td style="color:${color(names()[k])}">${esc(names()[k])}</td><td>${chip(j.出口, -1, {act: "撞忌口", ignore: "能吃", maybe: "拿不准"})}</td><td style="font-family:var(--mono)">${r ? r.读数.toFixed(2) : "—"}</td><td style="font-family:var(--mono)">${c.通勤[k]} 分</td></tr>`; }).join("");
  return `<h4>${esc(c.名)}</h4><p class="note">菜系标签：${esc(c.菜系 || "无")} · 营业：${esc(c.营业)}${c.营业时间 ? "（" + esc(c.营业时间) + "）" : ""}</p><p class="q">「${esc(q)}」</p><table><tr><th>谁</th><th>回答</th><th>读数</th><th>通勤</th></tr>${rows}</table><p class="note">读数是「会」的概率，0.3–0.7 之间算拿不准。通勤按程序的估算：曼哈顿距离、约 23 km/h，另加 8 分钟。</p><details><summary>选店的规则</summary><p>先去掉有人撞忌口的店；剩下的按「确定开门的在前 → 拿不准的人少 → 最远那人的通勤短 → 通勤总和短」排，取第一家（聚餐.jpp 第 153–154 行）。</p></details>`;
}

// 4 选中这家：镜头推近，五个人沿地图从家里过去（折线就是程序估算通勤用的曼哈顿路径）
STEPS.push({
  dur: 8, map: true,
  title: p => { const c = prep().chosen; return p < .7 ? "五个人从家出发" : `最远的人 ${Math.max(...c.通勤)} 分钟到`; },
  nums: p => { const c = prep().chosen; const mx = Math.max(...c.通勤); return [[Math.round(mx * eseg(p, .1, .75)), "分钟 · 最远的人"], [c.撞.length, "人撞忌口", "good"]]; },
  draw(p) {
    const G = prep(), O = orbitLayout(G), c = G.chosen, rp = REST[c.i].xy;
    const tgt = camFit(bbOf([...G.homes.map(h => h.xy), rp]), MOBILE() ? 60 : 110);
    CAM = camMix(O.cam, tgt, eseg(p, 0, .14));
    drawMap(eseg(p, 0, .14) * .5 + .5);
    drawRests(G, i => G.out[i] === 5 ? {a: i === c.i ? 0 : .5, lit: true} : {a: .22, lit: false}, {edges: false});
    const [x, y] = scr(rp), now = performance.now() / 1000;
    const mx = Math.max(...c.通勤);
    G.homes.forEach((h, k) => {
      const [hx, hy] = scr(h.xy), col = color(h.n);
      const u = eseg(p, .12, .12 + .6 * c.通勤[k] / mx);
      const mid = [x, hy]; // 先横后竖
      const l1 = Math.abs(x - hx), l2 = Math.abs(y - hy), L = l1 + l2 || 1, d = u * L;
      const px = d <= l1 ? lerp(hx, x, d / (l1 || 1)) : x, py = d <= l1 ? hy : lerp(hy, y, (d - l1) / (l2 || 1));
      line(hx, hy, px, d <= l1 ? hy : hy, col, 2.2, .85);
      if (d > l1) line(x, hy, x, py, col, 2.2, .85);
      if (u > 0 && u < 1) glow(px, py, 22, col, 1);
      if (u >= 1) glow(x, y, 40 * (1 - seg(p, .12 + .6 * c.通勤[k] / mx, .2 + .6 * c.通勤[k] / mx)), col, .8);
      text(`${c.通勤[k]} 分`, hx, hy + (MOBILE() ? 26 : 32), {size: 14, color: "#c8d2e8", a: eseg(p, .12, .2), w: 500, mono: false});
      addBB(px, py, 4);
    });
    drawHomes(G, 1, {obst: [{x, y, r: 44}, {x, y: y - 58, r: 20}]});
    glow(x, y, 46 + 6 * Math.sin(now * 3), "#ff9150", 1);
    ring(x, y, 24, "#ffd2b0", 2.5, 1);
    candTones(c).forEach((t, k) => { const a0 = -Math.PI / 2 + k * 2 * Math.PI / 5 + .07, a1 = a0 + 2 * Math.PI / 5 - .14; arc(x, y, 34, a0, a1, TC[t], 6, .9); });
    text(shortName(c.名), x, y - 58, {size: MOBILE() ? 17 : 22, color: "#ffe2c4", glow: "#ff9150"});
    hit(x, y, 40, ev => showPop(candPop(c), ev));
  }
});

// 5 关系图：两两「待在一起自在吗」，拿不准的闪琥珀；补一条信息再判，照实变色
function graphPos(n, opt = {}) {
  const mob = MOBILE(), cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2 + (mob ? 0 : 6);
  const rx = SAFE.w * (mob ? .34 : .39), ry = SAFE.h * (mob ? .33 : .41);
  return Array.from({length: n}, (_, k) => { const a = -Math.PI / 2 + k * 2 * Math.PI / n; return [cx + rx * Math.cos(a), cy + ry * Math.sin(a)]; });
}
const GP = {first: [.03, .18], fill: [.22, .52]};
function groupEdges() { // 饭后分组那张「自在」图；首判、补了什么、补后出口
  const f = R().饭后, ns = names();
  return f.边.map((e, k) => {
    const b = f.补了.find(x => x.eid === k);
    return {k, a: e.对[0], b: e.对[1], ia: ns.indexOf(e.对[0]), ib: ns.indexOf(e.对[1]), first: f.首判[k], last: e.出口, key: e.键, fill: b || null};
  });
}
function fillTimes(E) { const fs = E.filter(e => e.fill); const n = fs.length; const m = new Map(); fs.forEach((e, j) => m.set(e.k, GP.fill[0] + (n > 1 ? j / (n - 1) : 0) * (GP.fill[1] - GP.fill[0] - .06))); return m; }
function edgeState(e, p, ft) { // 返回 {tone, a, flash}
  const i0 = GP.first[0] + e.k / 10 * (GP.first[1] - GP.first[0]);
  if (p < i0) return null;
  const t0 = ft.get(e.k);
  let t = tone(e.first, +1), flash = 0;
  if (t0 !== undefined && p >= t0) { const f = seg(p, t0, t0 + .06); if (f >= .6) t = tone(e.last, +1); flash = Math.sin(clamp(f) * Math.PI); }
  return {t, a: eseg(p, i0, i0 + .03), flash};
}
function flips(E) { return E.filter(e => e.fill && tone(e.first, 1) !== tone(e.last, 1)); }
function drawEdge(x0, y0, x1, y1, t, a, flash = 0, emph = 0) {
  const now = performance.now() / 1000;
  if (t === "maybe") { const fl = .45 + .35 * Math.sin(now * 5 + x0 * .01 + y1 * .013); line(x0, y0, x1, y1, TC.maybe, 2, a * fl, [7, 7]); }
  else line(x0, y0, x1, y1, TC[t], 2.6 + emph * 2, a);
  if (flash > 0) line(x0, y0, x1, y1, "#ffffff", 3.5, flash);
}
function drawNodes(pos, a = 1, alpha = null, rBase = null) {
  const r = rBase || (MOBILE() ? 13 : 20), ns = names(), size = MOBILE() ? 15 : 18;
  ns.forEach((n, k) => { const ak = alpha ? alpha[k] * a : a; const [x, y] = pos[k]; glow(x, y, r * 3, color(n), .5 * ak); ring(x, y, r, color(n), 2.4, ak); glow(x, y, r * .9, color(n), ak); addBB(x, y, r * 2); hit(x, y, r + 8, ev => showPop(personPop(n), ev), ak); });
  const lab = placeLabels(ns.map((n, k) => ({x: pos[k][0], y: pos[k][1], r: r * 1.3, s: n})), size, [], [SAFE.x + SAFE.w / 2, SAFE.y + SAFE.h / 2]);
  ns.forEach((n, k) => text(n, lab[k][0], lab[k][1], {size, color: color(n), a: alpha ? alpha[k] * a : a, glow: color(n)}));
}
function edgePop(e) {
  const f = R().饭后, q = fillAB(f.题[1], e.a, e.b);
  const fillTxt = e.fill ? (e.fill.补了 && e.fill.补了.length ? e.fill.补了.map(x => `<p>补进「${esc(x.类)}」：${esc(x.内容)}</p>`).join("") : `<p class="note">「最缺哪一类信息」这道选择题没选出来，没补成。</p>`) : "";
  return `<h4>${esc(e.a)} × ${esc(e.b)}</h4>${judgeHtml({q, key: e.key, exit: e.first, pol: 1, words: {act: "自在", ignore: "不自在", maybe: "拿不准"}, mat: `${e.a}：${selfOf(e.a)}\n${e.b}：${selfOf(e.b)}\n关系说法：${pairSay(e.a, e.b)}`})}${e.fill ? `<p><b>补信息后再判</b> → ${chip(e.last, 1, {act: "自在", ignore: "不自在", maybe: "拿不准"})}</p>${fillTxt}` : ""}<p class="note">程序原题用 a、b 代指两人。读数是首判的；「可查信息」是人物文件里写好的一张表，程序去查，不让生成模型现编。</p>`;
}
STEPS.push({
  dur: 10,
  title: p => { const E = groupEdges(), fl = flips(E); if (p < GP.first[1] + .04) return "谁和谁待在一起自在？"; if (p < GP.fill[1] + .02) return "拿不准的，补一条信息再判"; if (!fl.length) return "补完信息，仍拿不准"; if (fl.length === 1) return `补完：${fl[0].a}、${fl[0].b}${tone(fl[0].last, 1) === "bad" ? "不自在" : "自在"}`; return `补完：${fl.length} 对变了颜色`; },
  nums: p => { const E = groupEdges(), ft = fillTimes(E); const done = E.filter(e => { const t = ft.get(e.k); return t !== undefined && p >= t + .04; }); const nf = done.filter(e => tone(e.first, 1) !== tone(e.last, 1)).length; const seen = E.filter(e => p >= GP.first[0] + e.k / 10 * (GP.first[1] - GP.first[0])).length; return [[seen, "对关系"], [done.length, "对补了信息"], [nf, "对变了颜色", nf ? "bad" : "small"]]; },
  draw(p) {
    const pos = graphPos(5), E = groupEdges(), ft = fillTimes(E), now = performance.now() / 1000;
    const cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2;
    E.forEach(e => {
      const st = edgeState(e, p, ft); if (!st) return;
      const [x0, y0] = pos[e.ia], [x1, y1] = pos[e.ib];
      const fl = p > GP.fill[1] + .02 && e.fill && tone(e.first, 1) !== tone(e.last, 1);
      drawEdge(x0, y0, x1, y1, st.t, st.a, st.flash, fl ? .5 + .5 * Math.sin(now * 4) : 0);
      const mx = (x0 + x1) / 2, my = (y0 + y1) / 2;
      FR.hits.push({x: mx, y: my, r: 16, fn: ev => showPop(edgePop(e), ev)});
      // 信息包：从画面中心飞到这条边的中点
      const t0 = ft.get(e.k);
      if (t0 !== undefined && p >= t0 - .05 && p < t0) { const u = eseg(p, t0 - .05, t0); const got = e.fill.补了 && e.fill.补了.length; glow(lerp(cx, mx, u), lerp(cy, my, u), 18, got ? "#9fe6ff" : "#6c7a96", 1); }
    });
    if (p > GP.fill[0] - .06 && p < GP.fill[1] + .02) { aura(cx, cy, 70 + 10 * Math.sin(now * 5), "rgba(159,230,255,1)", .35); glow(cx, cy, 16, "#9fe6ff", .9); const la = eseg(p, GP.fill[0] - .06, GP.fill[0]) * (1 - eseg(p, GP.fill[1] - .02, GP.fill[1] + .02)); text("可查信息", cx, cy + 30, {size: 15, color: "#9fe6ff", a: la, w: 500}); hit(cx, cy, 30, ev => showPop(`<h4>可查信息</h4><p>人物文件里写好的一张表，是真实来源的替身。某条边拿不准时，语义判断模型先从这张表的类别里选「最缺哪一类」，代码查表取来，补进材料再判一次。</p>`, ev), la); }
    drawNodes(pos, 1);
    drawToneLegend(SAFE.x + SAFE.w / 2, SAFE.y + SAFE.h - 14, [["自在", TC.good], ["拿不准", TC.maybe], ["不自在", TC.bad]], 1, "center");
  }
});

// 6 圆桌：一个个入座；试遍 12 种坐法，尴尬的一对挨着就闪红；最后分开坐
function seating() {
  const G = prep(); if (G._seat) return G._seat;
  const ns = names(), N = ns.length, r = R(), st = r.座位, tr = r.题树.规则;
  const idx = n => ns.indexOf(n);
  const perms = xs => xs.length <= 1 ? [xs] : xs.flatMap((x, i) => perms([...xs.slice(0, i), ...xs.slice(i + 1)]).map(p => [x, ...p]));
  const ways = perms(Array.from({length: N - 1}, (_, k) => k + 1)).filter(p => p[0] < p[p.length - 1]).map(p => [0, ...p]);
  const adj = (s, i, j) => s.some((a, k) => { const b = s[(k + 1) % N]; return (a === i && b === j) || (a === j && b === i); });
  const edges = st.边.filter(e => kind(e.出口) === "act").map(e => e.对.map(idx));
  const pend = st.边.filter(e => kind(e.出口) === "maybe").map(e => e.对.map(idx));
  const apart = (tr.隔开 || []).map(p => p.map(idx));
  const mid = (tr.中间 || []).map(m => ({m: idx(m.中间), s: (m.两边 || []).map(idx)}));
  const score = s => 10 * (apart.filter(([i, j]) => adj(s, i, j)).length + mid.filter(o => !(adj(s, o.m, o.s[0]) && adj(s, o.m, o.s[1]))).length) + 2 * edges.filter(([i, j]) => adj(s, i, j)).length + pend.filter(([i, j]) => adj(s, i, j)).length;
  const scores = ways.map(score);
  let bi = 0; scores.forEach((v, k) => { if (v < scores[bi]) bi = k; });
  const final = st.顺时针.map(idx);
  const fi = ways.findIndex(w => w.every((v, k) => v === final[k]));
  const verified = fi === bi; // 页面按 jpp 第 286 行计分重算；与报告一致才显示分数
  const pair = apart[0] || null;
  const awkAdj = s => apart.filter(([i, j]) => adj(s, i, j)).length + edges.filter(([i, j]) => adj(s, i, j)).length;
  G._seat = {ways, scores, bi, fi: fi < 0 ? 0 : fi, verified, pair, adj, awkAdj, final, N};
  return G._seat;
}
const SP6 = {sit: [.02, .2], cyc: [.22, .47], land: [.49, .55]};
function seatPos(k, N) { const mob = MOBILE(), cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2; const rx = Math.min(SAFE.w * (mob ? .36 : .38), SAFE.h * (mob ? .42 : .8)), ry = Math.min(SAFE.h * (mob ? .36 : .41), rx * (mob ? 1.25 : .8)); const a = -Math.PI / 2 + k * 2 * Math.PI / N; return [cx + rx * Math.cos(a), cy + ry * Math.sin(a)]; }
function seatFrame(p) { // 当前每个人的屏幕位置、在第几种坐法
  const Z = seating(), N = Z.N;
  const where = (w) => { const pos = []; Z.ways[w].forEach((person, k) => pos[person] = seatPos(k, N)); return pos; };
  if (p < SP6.cyc[0]) return {pos: where(0), w: 0, tried: 1};
  if (p < SP6.cyc[1]) { const u = seg(p, SP6.cyc[0], SP6.cyc[1]) * (Z.ways.length - 1); const w = Math.floor(u), f = eseg(u - w, 0, .5); const a = where(w), b = where(Math.min(w + 1, Z.ways.length - 1)); return {pos: a.map((q, i) => [lerp(q[0], b[i][0], f), lerp(q[1], b[i][1], f)]), w: f > .5 ? w + 1 : w, tried: Math.min(Z.ways.length, w + 1 + (f > .5 ? 1 : 0)), moving: f > 0 && f < 1}; }
  const last = Z.ways.length - 1, f = eseg(p, SP6.land[0], SP6.land[1]);
  const a = where(last), b = where(Z.fi);
  return {pos: a.map((q, i) => [lerp(q[0], b[i][0], f), lerp(q[1], b[i][1], f)]), w: f > .5 ? Z.fi : last, tried: Z.ways.length, landed: f >= 1};
}
function treePop() {
  const t = R().题树, path = (t.路径[0] || []);
  return `<h4>为什么 ${esc(t.对.join("、"))} 要隔开</h4><p>生成模型先为最微妙的这一对出了一棵追问的题树，语义判断模型沿着每一步的回答往下走：</p>${path.map(s => `<p class="q">「${esc(s.q)}」 → ${chip(s.出口, 1, {act: "是", ignore: "否", maybe: "拿不准"})}</p>`).join("")}<p>走到的叶子：${esc(t.叶.说明)}</p><p class="note">红光出自这棵题树给的「隔开」规则，不是上一步那张图。排座时，犯规则记 10 分、挨着判为尴尬的一对记 2 分、挨着拿不准的一对记 1 分，12 种坐法取分最低的（聚餐.jpp 第 286 行）。</p>`;
}
function seatPop() {
  const Z = seating(), ns = names();
  const rows = Z.ways.map((w, k) => `<tr${k === Z.fi ? ' style="color:var(--good)"' : ""}><td>${k + 1}</td><td>${w.map(i => esc(ns[i])).join(" → ")}</td>${Z.verified ? `<td style="font-family:var(--mono)">${Z.scores[k]}</td>` : ""}</tr>`).join("");
  return `<h4>12 种坐法</h4><p>5 人圆桌，固定第一个人，去掉顺逆时针重复，只有 12 种本质不同的坐法，程序全部试一遍。</p><table><tr><th>#</th><th>顺时针</th>${Z.verified ? "<th>分</th>" : ""}</tr>${rows}</table><p class="note">${Z.verified ? "分数是页面按程序的计分规则重算的，最低分那种与运行报告里的座位一致。" : "页面重算的最低分与报告不一致，所以不显示分数，只以报告为准。"}</p>`;
}
STEPS.push({
  dur: 10,
  title: p => { const Z = seating(), ns = names(); const pr = Z.pair ? `${ns[Z.pair[0]]}、${ns[Z.pair[1]]}` : ""; if (p < SP6.cyc[0]) return "五个人，一个个入座"; if (p < SP6.land[1]) return pr ? `${pr}挨着会尴尬` : "试遍 12 种坐法"; return pr ? `${pr}分开坐` : "选出最好的一种坐法"; },
  nums: p => { const Z = seating(), f = seatFrame(p); const out = [[f.tried, `种坐法已试`]]; out.push([Z.awkAdj(Z.ways[p >= SP6.land[1] ? Z.fi : f.w]), "对尴尬的挨着", p >= SP6.land[1] ? "good" : "small"]); return out; },
  draw(p) {
    const Z = seating(), ns = names(), N = Z.N, f = seatFrame(p), now = performance.now() / 1000;
    const cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2;
    const [tx0] = seatPos(1, N); const trx = Math.abs(tx0 - cx) * .62, [, ty0] = seatPos(0, N), tRy = Math.abs(ty0 - cy) * .55;
    // 桌子
    ctx.save(); ctx.globalCompositeOperation = "lighter"; ctx.strokeStyle = "#57d4ff"; ctx.globalAlpha = .5; ctx.lineWidth = 2; ctx.shadowColor = "#57d4ff"; ctx.shadowBlur = 20; ctx.beginPath(); ctx.ellipse(cx, cy, trx, tRy, 0, 0, 7); ctx.stroke(); ctx.globalAlpha = .06; ctx.fillStyle = "#57d4ff"; ctx.fill(); ctx.restore();
    addBB(cx - trx, cy - tRy); addBB(cx + trx, cy + tRy);
    for (let k = 0; k < N; k++) { const [x, y] = seatPos(k, N); ring(x, y, MOBILE() ? 20 : 28, "#2a3656", 2, .8); }
    // 尴尬的一对挨着：闪红
    const w = f.w, pr = Z.pair;
    if (pr && p >= SP6.cyc[0] && p < SP6.land[1]) {
      { const [x0, y0] = f.pos[pr[0]], [x1, y1] = f.pos[pr[1]]; line(x0, y0, x1, y1, TC.bad, 1.2, .3, [4, 8]); }
      if (Z.adj(Z.ways[w], pr[0], pr[1]) && !f.moving) { const [x0, y0] = f.pos[pr[0]], [x1, y1] = f.pos[pr[1]]; const fl = .6 + .4 * Math.sin(now * 12); line(x0, y0, x1, y1, TC.bad, 4, fl); glow((x0 + x1) / 2, (y0 + y1) / 2, 60, TC.bad, .6 * fl); }
    }
    if (pr && p >= SP6.land[1]) { const [x0, y0] = f.pos[pr[0]], [x1, y1] = f.pos[pr[1]]; line(x0, y0, x1, y1, TC.bad, 1.5, .35 + .15 * Math.sin(now * 3), [4, 8]); FR.hits.push({x: (x0 + x1) / 2, y: (y0 + y1) / 2, r: 30, fn: ev => showPop(treePop(), ev)}); }
    // 人：一个个落座
    const alpha = ns.map((_, i) => { const k = Z.ways[0].indexOf(i); return eseg(p, SP6.sit[0] + k * .033, SP6.sit[0] + k * .033 + .04); });
    const pos = f.pos.map((q, i) => [q[0], q[1] - 40 * (1 - alpha[i])]);
    drawNodes(pos, 1, alpha, MOBILE() ? 14 : 20);
    // 桌心：第几种、分数（只有重算与报告一致时才显示分数）
    if (p >= SP6.cyc[0]) {
      text(`${Math.min(f.tried, Z.ways.length)} / ${Z.ways.length}`, cx, cy - (Z.verified ? 12 : 0), {size: MOBILE() ? 20 : 30, mono: true, color: "#e8eefb", glow: "#57d4ff"});
      if (Z.verified) text(`分 ${Z.scores[f.w]}`, cx, cy + (MOBILE() ? 16 : 22), {size: MOBILE() ? 14 : 16, color: Z.scores[f.w] === Z.scores[Z.bi] ? TC.good : "#a9b6cf", w: 500});
      FR.hits.push({x: cx, y: cy, r: Math.min(trx, tRy), fn: ev => showPop(seatPop(), ev)});
    }
  }
});

// 7 饭后分组；去掉关键人物，整张图的亮度跟着气氛读数变
function mood() {
  const G = prep(); if (G._mood) return G._mood;
  const m = R().气氛, ns = names();
  const E = m.键.map(k => { const r = reading(k); return r && Array.isArray(r.读数) ? r.读数.reduce((a, v, i) => a + v * i, 0) / r.读数.reduce((a, v) => a + v, 0) : null; });
  const without = n => E[1 + ns.indexOf(n)];
  const acts = [];
  (m.紧张源 || []).slice(0, 1).forEach(n => acts.push({n, kind: "tense", e: without(n)}));
  (m.粘合剂 || []).slice(0, 1).forEach(n => acts.push({n, kind: "glue", e: without(n)}));
  G._mood = {E, all: E[0], acts, keys: m.键};
  return G._mood;
}
const MP = {group: [.02, .22], acts: [[.34, .60], [.66, .92]]};
function moodAt(p) { const M = mood(); for (let k = 0; k < M.acts.length; k++) { const [a, b] = MP.acts[k]; if (p >= a && p < b) { const u = eseg(p, a, a + .06) * (1 - eseg(p, b - .05, b)); return {act: M.acts[k], u, e: lerp(M.all, M.acts[k].e, u)}; } } return {act: null, u: 0, e: M.all}; }
function groupR() { return MOBILE() ? 70 : clamp(Math.min(SAFE.w * .13, SAFE.h * .26), 100, 240); }
function groupLayout() {
  const f = R().饭后, ns = names(), mob = MOBILE();
  const groups = f.分组.map(g => g.人.map(n => ns.indexOf(n)));
  const cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2;
  const centers = groups.length === 1 ? [[cx, cy]] : mob ? [[cx, SAFE.y + SAFE.h * .2], [cx, SAFE.y + SAFE.h * .66]] : [[SAFE.x + SAFE.w * .27, cy - 20], [SAFE.x + SAFE.w * .68, cy - 20]];
  const pos = [];
  groups.forEach((g, gi) => { const R0 = g.length === 1 ? 0 : groupR() * (g.length > 3 ? 1.15 : 1); g.forEach((i, k) => { const a = -Math.PI / 2 + k * 2 * Math.PI / g.length + (gi ? .3 : 0); pos[i] = [centers[gi][0] + R0 * Math.cos(a), centers[gi][1] + R0 * Math.sin(a) * (mob ? .8 : 1)]; }); });
  return {groups, centers, pos};
}
function moodPop() {
  const M = mood(), ns = names(), sc = ["很僵", "有点别扭", "还算自在", "很融洽"];
  const rows = M.keys.map((k, i) => `<tr><td>${i ? "去掉" + esc(ns[i - 1]) : "全员"}</td><td style="font-family:var(--mono)">${M.E[i] != null ? M.E[i].toFixed(2) : "—"}</td><td style="font-family:var(--mono);color:var(--fg3)">${(reading(k)?.读数 || []).map(v => v.toFixed(2)).join(" / ")}</td></tr>`).join("");
  return `<h4>今晚的气氛，少了谁会变</h4><p class="q">「${esc(R().气氛.题)}」四档：${sc.join(" / ")}</p><p>同一道打分题问 6 次：全员一次，再逐个去掉一人（材料里删掉这个人的自述和涉及他的关系说法）。只比相对变化，不各自按线切。</p><table><tr><th></th><th>期望（0–3）</th><th>四档概率</th></tr>${rows}</table><p class="note">画面亮度跟着期望值变：比全员高就更亮，低就更暗。紧张源：${esc((R().气氛.紧张源 || []).join("、") || "无")}；粘合剂：${esc((R().气氛.粘合剂 || []).join("、") || "无")}。</p>`;
}
function groupPop(gi) {
  const f = R().饭后, g = f.分组[gi];
  return `<h4>${esc(g.人.join("、"))}</h4><p>饭后活动：${esc(g.活动)}</p><p class="note">分组依据：${esc(f.分组依据)}。在「待在一起自在」的图上跑最大团（max_clique），团里的人一组，其余的人另一组；活动是生成模型出的 6 个候选里，这组人判下来最没人不舒服的那个。${f.分组依据 !== "已决边" ? "这组的团用到了还拿不准的边（把拿不准当自在），结果是乐观的。" : ""}${f.并列团 && f.并列团.length ? ` 另有同样大的团：${f.并列团.map(x => esc(x.join("、"))).join("；")}。` : ""}</p>`;
}
STEPS.push({
  dur: 11,
  title: p => { const m = moodAt(p); if (m.act) return `去掉${m.act.n}，气氛${m.act.e > mood().all ? "变好" : "变僵"}`; if (p < MP.acts[0][0]) return R().饭后.分组.length > 1 ? "饭后分两组" : "饭后一起活动"; return "谁是今晚气氛的关键"; },
  nums: p => { const m = moodAt(p); return [[m.e.toFixed(2), "气氛读数", m.e > mood().all + .01 ? "good" : m.e < mood().all - .01 ? "bad" : ""], [R().饭后.分组.length, "个小组", "small"]]; },
  draw(p) {
    const L = groupLayout(), E = groupEdges(), M = mood(), m = moodAt(p), ns = names(), now = performance.now() / 1000;
    const from = graphPos(5), u = eseg(p, MP.group[0], MP.group[1]);
    const pos = from.map((q, i) => [lerp(q[0], L.pos[i][0], u), lerp(q[1], L.pos[i][1], u)]);
    const bright = clamp(.62 + (m.e - M.all) * 1.1, .15, 1);
    const alpha = ns.map((n, i) => m.act && m.act.n === n ? 1 - m.u : 1);
    // 气氛的底光
    const cx = SAFE.x + SAFE.w / 2, cy = SAFE.y + SAFE.h / 2;
    aura(cx, cy, Math.min(SAFE.w, SAFE.h) * .7, m.e >= M.all ? "rgba(255,145,80,1)" : "rgba(70,90,170,1)", .06 + .3 * Math.max(0, bright - .4));
    const gid = []; L.groups.forEach((g, gi) => g.forEach(i => gid[i] = gi));
    E.forEach(e => {
      const same = gid[e.ia] === gid[e.ib], t = tone(e.last, 1);
      const a = (same ? 1 : t === "bad" ? 1 : 1 - .85 * u) * bright * Math.min(alpha[e.ia], alpha[e.ib]);
      const [x0, y0] = pos[e.ia], [x1, y1] = pos[e.ib];
      drawEdge(x0, y0, x1, y1, t, a, 0, !same && t === "bad" ? .5 : 0);
      if (a > .3) FR.hits.push({x: (x0 + x1) / 2, y: (y0 + y1) / 2, r: 14, fn: ev => showPop(edgePop(e), ev)});
    });
    drawNodes(pos, bright, alpha, MOBILE() ? 13 : 20);
    // 活动名
    const f = R().饭后, la = eseg(p, MP.group[1] - .04, MP.group[1] + .04);
    L.centers.forEach((c, gi) => {
      if (!f.分组[gi]) return;
      const g = L.groups[gi], rad = g.length === 1 ? 0 : groupR() * (g.length > 3 ? 1.15 : 1);
      const y = c[1] + rad * (MOBILE() ? .8 : 1) + (MOBILE() ? 46 : 70);
      text(actName(f.分组[gi].活动), c[0], y, {size: MOBILE() ? 17 : 24, color: "#ffe2c4", a: la * (.4 + .6 * bright), glow: "#ff9150"});
      hitRect(c[0] - 120, y - 20, 240, 40, ev => showPop(groupPop(gi), ev), la);
    });
    if (f.分组依据 !== "已决边") { const yy = SAFE.y + SAFE.h - 16; text("分组含拿不准的边", SAFE.x + SAFE.w / 2, yy, {size: 15, color: TC.maybe, a: la * .9, w: 500}); hitRect(SAFE.x + SAFE.w / 2 - 80, yy - 14, 160, 28, ev => showPop(groupPop(0), ev), la); }
    // 气氛刻度：0 很僵 … 3 很融洽
    const mob = MOBILE();
    const mx = mob ? SAFE.x + SAFE.w - 18 : SAFE.x + SAFE.w - 40, y0 = SAFE.y + SAFE.h * (mob ? .12 : .18), y1 = SAFE.y + SAFE.h * (mob ? .9 : .82);
    const Y = v => lerp(y1, y0, v / 3);
    ctx.save(); ctx.globalAlpha = .9; const gr = ctx.createLinearGradient(0, y1, 0, y0); gr.addColorStop(0, "#3a4a8a"); gr.addColorStop(1, "#ff9150"); ctx.fillStyle = gr; ctx.fillRect(mx - 3, y0, 6, y1 - y0); ctx.restore();
    text("很融洽", mx - 10, y0 - 16, {size: 14, color: "#ffc49a", w: 500, align: "right"}); text("很僵", mx - 10, y1 + 16, {size: 14, color: "#8fa2d8", w: 500, align: "right"});
    line(mx - 12, Y(M.all), mx + 8, Y(M.all), "#ffffff", 1, .5);
    glow(mx, Y(m.e), 22, "#ffffff", 1); ring(mx, Y(m.e), 8, "#ffffff", 2, 1);
    addBB(mx + 10, y0); FR.hits.push({x: mx, y: (y0 + y1) / 2, rw: 24, rh: (y1 - y0) / 2, fn: ev => showPop(moodPop(), ev)});
  }
});

// 8 结果海报
function posterDishes(budget) {
  const sel = R().菜.选中.map(dishName); const out = []; let used = 0;
  for (const d of sel) { if (out.length >= 1 && used + hanLen(d) > budget) break; out.push(d); used += hanLen(d); }
  return out;
}
function timePop() {
  const t = R().时间, ns = names();
  const rows = t.各时段.map(s => `<tr${s.标签 === t.选 ? ' style="color:var(--good)"' : ""}><td>${esc(s.标签)}</td>${s.逐人.map(x => `<td>${chip(x.出口, 1, {act: "能", ignore: "不能", maybe: "？"})}</td>`).join("")}</tr>`).join("");
  return `<h4>四个候选时间 × 五个人</h4><p class="q">「${esc(t.题)}」</p><table><tr><th></th>${ns.map(n => `<th>${esc(n)}</th>`).join("")}</tr>${rows}</table><p class="note">代码挑时间：先比来不了的人数，再比能来的人数。</p>`;
}
function dishPop() {
  const c = R().菜, ns = names();
  const rows = c.逐道.map(d => `<tr${c.选中.includes(d.菜) ? "" : ' style="opacity:.5"'}><td>${esc(dishName(d.菜))}</td>${d.判.map(j => `<td>${chip(j.出口, -1, {act: "不吃", ignore: "吃", maybe: "？"})}</td>`).join("")}</tr>`).join("");
  return `<h4>示例菜单（${c.选中.length} 道上桌）</h4><p class="note">${esc(c.注)}</p><table><tr><th></th>${ns.map(n => `<th>${esc(n)}</th>`).join("")}</tr>${rows}</table><p>这桌菜够不够吃好：${c.够不够.map(x => `${esc(x.名)} ${chip(x.出口, 1, {act: "够", ignore: "不够", maybe: "拿不准"})}`).join(" ")}</p><p class="note">灰的是没选上的。代码贪心选 7 道：先照顾能吃的还不到 3 道的人。</p>`;
}
function costPop() {
  const g = grp(), r = R();
  return `<h4>这一次运行的账</h4><table><tr><td>账本里的语义判断</td><td style="font-family:var(--mono)">${g.账本.判断条数} 条</td></tr><tr><td>生成模型调用</td><td style="font-family:var(--mono)">${g.账本.生成} 次</td></tr><tr><td>报告里的调用总数</td><td style="font-family:var(--mono)">${g.花费.calls}（另有 ${g.花费.replayed} 次重放）</td></tr><tr><td>token</td><td style="font-family:var(--mono)">${g.花费.tokens}</td></tr><tr><td>花费（报告合计）</td><td style="font-family:var(--mono)">$${g.花费.usd.toFixed(6)}</td></tr></table><p>有 ${r.未决条数} 条判断拿不准，程序没有替人硬拍，随结果一起交还给人。整份安排再判一次「会让谁难受吗」：${chip(r.整体.整体.出口, -1, {act: "会", ignore: "不会", maybe: "拿不准"})}；逐人：${r.整体.逐人.map(x => `${esc(x.名)} ${chip(x.出口, -1, {act: "难受", ignore: "不难受", maybe: "拿不准"})}`).join(" ")}</p><p class="note">判断器 ${esc(g.账本.模型)}。</p>`;
}
function seatSvg() {
  const Z = seating(), ns = names(), N = Z.N;
  const pts = Z.final.map((person, k) => { const a = -Math.PI / 2 + k * 2 * Math.PI / N; return {n: ns[person], x: 100 + 64 * Math.cos(a), y: 90 + 58 * Math.sin(a)}; });
  const pr = Z.pair ? [ns[Z.pair[0]], ns[Z.pair[1]]] : null;
  const pp = pr ? pts.filter(q => pr.includes(q.n)) : [];
  return `<svg viewBox="0 0 200 180"><ellipse cx="100" cy="90" rx="38" ry="32" fill="rgba(87,212,255,.08)" stroke="#57d4ff" stroke-opacity=".6"/>${pp.length === 2 ? `<line x1="${pp[0].x}" y1="${pp[0].y}" x2="${pp[1].x}" y2="${pp[1].y}" stroke="#ff5b6e" stroke-dasharray="3 5" stroke-opacity=".7"/>` : ""}${pts.map(q => `<circle cx="${q.x}" cy="${q.y}" r="7" fill="${color(q.n)}"/><text x="${q.x}" y="${q.y + (q.y > 90 ? 22 : -12)}" text-anchor="middle" font-size="14" fill="${color(q.n)}" font-weight="600">${esc(q.n)}</text>`).join("")}</svg>`;
}
let posterBuilt = "";
function buildPoster() {
  const key = S.g + VW + "x" + VH; if (posterBuilt === key) return; posterBuilt = key;
  const r = R(), g = grp(), ns = names(), c = prep().chosen;
  const tm = String(r.时间.选).replace(/\s+/g, "");
  const fixed = hanLen("安排好了") + hanLen(groupLabel(S.g)) + hanLen(tm) + hanLen(shortName(c.名)) + ns.reduce((a, n) => a + hanLen(n), 0) + r.饭后.分组.reduce((a, x) => a + hanLen(actName(x.活动)), 0) + hanLen("示例菜单") + hanLen("次判断花费");
  const dishes = posterDishes(60 - fixed);
  const mob = VW < 720;
  $("poster").innerHTML = `<div class="pgrid">
    <div class="pcard pmain" data-pop="1" data-k="main"><div class="ptime" data-k="time">${esc(tm)}</div><div class="pplace" data-k="place">${esc(shortName(c.名))}</div><div class="pdish" data-k="dish">${dishes.map(d => `<span>${esc(d)}</span>`).join("")}<em>${dishes.length}/${r.菜.选中.length}</em></div><span class="ptag" data-k="dish">示例菜单</span></div>
    <div class="pcard pseat" data-pop="1" data-k="seat">${seatSvg()}</div>
    <div class="pcard" data-pop="1" data-k="act"><div class="pact">${r.饭后.分组.map((x, gi) => `<div data-k="act${gi}">${x.人.map(n => `<i style="color:${color(n)};background:${color(n)}"></i>`).join("")}<br>${esc(actName(x.活动))}</div>`).join("")}</div></div>
    <div class="pbig"><div class="pcard" data-pop="1" data-k="cost"><b>${g.账本.判断条数}</b><span>次判断</span></div><div class="pcard" data-pop="1" data-k="cost"><b>$${g.花费.usd.toFixed(4)}</b><span>花费</span></div></div></div>`;
  $("poster").querySelectorAll(".pcard").forEach(el => el.addEventListener("click", ev => {
    const k = (ev.target.closest("[data-k]") || el).dataset.k;
    const f = {time: timePop, place: () => candPop(c), dish: dishPop, main: timePop, seat: () => seatPop() + treePop().replace(/^<h4>.*?<\/h4>/, ""), act: () => groupPop(0), act0: () => groupPop(0), act1: () => groupPop(1), cost: costPop}[k] || costPop;
    ev.stopPropagation(); showPop(f(), ev);
  }));
}
STEPS.push({
  dur: 10, poster: true, map: true,
  title: () => "安排好了",
  nums: () => [],
  draw(p) {
    buildPoster();
    const cards = $("poster").querySelectorAll(".pcard");
    cards.forEach((el, k) => el.classList.toggle("on", p >= .02 + k * .05 || S.freeze));
    // 背景：淡地图 + 选中的店 + 五条回家的光线，慢慢呼吸
    const G = prep(), c = G.chosen, now = performance.now() / 1000;
    CAM = camFit(bbOf([...G.homes.map(h => h.xy), REST[c.i].xy]), 60);
    drawMap(.25);
    const [x, y] = scr(REST[c.i].xy);
    aura(SAFE.x + SAFE.w * .3, SAFE.y + SAFE.h * .45, Math.max(SAFE.w, SAFE.h) * .5, "rgba(255,145,80,1)", .10 + .03 * Math.sin(now));
    aura(SAFE.x + SAFE.w * .75, SAFE.y + SAFE.h * .5, Math.max(SAFE.w, SAFE.h) * .45, "rgba(87,120,255,1)", .08);
  }
});

// ───────── 时间线与控件 ─────────
let TOTAL = 0, START = [];
function retime() { START = []; TOTAL = 0; STEPS.forEach(s => { START.push(TOTAL); TOTAL += s.dur; }); }
retime();
function locate(t) { let i = STEPS.length - 1; for (let k = 0; k < STEPS.length; k++) if (t < START[k] + STEPS[k].dur) { i = k; break; } return [i, clamp((t - START[i]) / STEPS[i].dur)]; }
const groupLabel = g => (D.组[g].人物.组 || g).split("·").pop();
function renderGroups() {
  $("gbtn").textContent = groupLabel(S.g) + " ▾";
  $("gmenu").innerHTML = G_NAMES.map(g => `<button data-g="${esc(g)}" aria-pressed="${g === S.g}">${esc(groupLabel(g))}</button>`).join("");
  $("gmenu").querySelectorAll("button").forEach(b => b.onclick = () => { hidePop(); S.g = b.dataset.g; $("gmenu").hidden = true; renderGroups(); lastTitle = ""; frame(); });
}
$("gbtn").onclick = () => { const m = $("gmenu"); m.hidden = !m.hidden; $("gbtn").setAttribute("aria-expanded", String(!m.hidden)); };
document.addEventListener("pointerdown", e => { if (!e.target.closest(".gsel")) $("gmenu").hidden = true; });
let lastTitle = "", lastNums = "";
function setTitle(s) { if (s === lastTitle) return; lastTitle = s; $("title").innerHTML = `<span class="in">${esc(s)}</span>`; }
function setNums(arr) {
  const key = JSON.stringify(arr); if (key === lastNums) return; lastNums = key;
  const box = $("nums");
  if (box.children.length !== arr.length) box.innerHTML = arr.map(() => `<div class="num"><b></b><span></span></div>`).join("");
  arr.forEach(([v, l, c], i) => { const el = box.children[i]; el.className = "num " + (c || ""); el.firstChild.textContent = v; el.lastChild.textContent = l; });
}
let CUR = {i: -1, p: 0};
function frame() {
  const [i, p] = locate(S.t);
  if (i !== CUR.i) { CUR.i = i; $("stepno").textContent = `${i + 1} / ${STEPS.length}`; try { if (!S.freeze) history.replaceState(null, "", "#" + encodeURIComponent(S.g) + "/" + (i + 1)); } catch (e) {} }
  if (CUR.g !== S.g) { CUR.g = S.g; try { if (!S.freeze) history.replaceState(null, "", "#" + encodeURIComponent(S.g) + "/" + (i + 1)); } catch (e) {} }
  CUR.p = p;
  const st = STEPS[i];
  setTitle(st.title(p)); setNums(st.nums(p));
  $("credit").style.opacity = st.map ? 1 : 0;
  $("poster").classList.toggle("show", !!st.poster);
  const f = S.t / TOTAL * 100; $("fill").style.width = f + "%"; $("knob").style.left = f + "%";
}
function render() {
  { const r = $("stage").getBoundingClientRect(); if (r.width !== SAFE.w || r.height !== SAFE.h || r.top !== SAFE.y) { SAFE = {x: r.left, y: r.top, w: r.width, h: r.height}; } }
  ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
  drawBg();
  FR = {texts: [], hits: [], bb: null, off: 0};
  const st = STEPS[CUR.i];
  try { st.draw(CUR.p); } catch (e) { window.__errs.push(String(e && e.stack || e)); }
  ctx.globalAlpha = 1; ctx.globalCompositeOperation = "source-over";
}
function loop(ts) {
  const dt = S.last ? Math.min(.1, (ts - S.last) / 1000) : 0; S.last = ts;
  if (S.playing) { S.t = Math.min(TOTAL, S.t + dt); if (S.t >= TOTAL) pause(); }
  frame(); render();
  requestAnimationFrame(loop);
}
const ICON = {play: `<svg viewBox="0 0 14 14"><path d="M3 1.5v11l9-5.5z" fill="currentColor"/></svg>`, pause: `<svg viewBox="0 0 14 14"><rect x="2.5" y="1.5" width="3.2" height="11" rx="1" fill="currentColor"/><rect x="8.3" y="1.5" width="3.2" height="11" rx="1" fill="currentColor"/></svg>`};
function syncPP() { $("pp").innerHTML = S.playing ? ICON.pause : ICON.play; $("pp").setAttribute("aria-label", S.playing ? "暂停" : "播放"); }
function pause() { S.playing = false; syncPP(); }
function play() { if (S.t >= TOTAL - .01) S.t = 0; S.playing = true; syncPP(); }
$("pp").onclick = () => S.playing ? pause() : play();
$("info").onclick = ev => showPop(infoPop(), ev);
function seekStep(i) { S.t = START[clamp(i, 0, STEPS.length - 1)] + .001; lastTitle = ""; }
document.addEventListener("keydown", e => { const [i] = locate(S.t); if (e.key === "ArrowRight") seekStep(i + 1); else if (e.key === "ArrowLeft") seekStep(S.t - START[i] > 1.5 ? i : i - 1); else if (e.key === " ") { e.preventDefault(); S.playing ? pause() : play(); } else if (e.key === "Escape") hidePop(); });
(function () {
  const tr = $("track"); let drag = false;
  const at = e => { const r = tr.getBoundingClientRect(); S.t = clamp((e.clientX - r.left) / r.width) * TOTAL; if (S.t >= TOTAL) S.t = TOTAL - .001; };
  tr.addEventListener("pointerdown", e => { drag = true; tr.setPointerCapture(e.pointerId); S.dragWas = S.playing; S.playing = false; at(e); });
  tr.addEventListener("pointermove", e => { if (drag) at(e); });
  tr.addEventListener("pointerup", () => { drag = false; S.playing = S.dragWas; syncPP(); });
})();
function renderTicks() { $("ticks").innerHTML = START.slice(1).map(s => `<div class="tick" style="left:${s / TOTAL * 100}%"></div>`).join(""); }
cv.addEventListener("click", ev => {
  const x = ev.clientX, y = ev.clientY;
  let best = null, bd = Infinity;
  for (let k = FR.hits.length - 1; k >= 0; k--) {
    const h = FR.hits[k];
    const d = h.rw ? (Math.abs(x - h.x) <= h.rw && Math.abs(y - h.y) <= h.rh ? 0 : Infinity) : Math.hypot(x - h.x, y - h.y) - h.r;
    if (d <= 0 && d < bd + 1e-9) { if (!best || d < bd) { best = h; bd = d; } }
  }
  if (best) best.fn(ev); else hidePop();
});
cv.addEventListener("pointermove", ev => { const on = FR.hits.some(h => h.rw ? Math.abs(ev.clientX - h.x) <= h.rw && Math.abs(ev.clientY - h.y) <= h.rh : Math.hypot(ev.clientX - h.x, ev.clientY - h.y) <= h.r); cv.style.cursor = on ? "pointer" : "default"; });
// 画布要能接住点击，但标题、控件在它上面
cv.style.pointerEvents = "auto";
let rz; addEventListener("resize", () => { clearTimeout(rz); rz = setTimeout(() => { resize(); }, 60); });

// 审计：当帧可见的汉字（DOM + 画布）、主画面内容包围盒占比
window.__audit = function () {
  let dom = 0; const parts = [];
  const vis = el => { let o = 1, e = el; while (e && e !== document.body) { const cs = getComputedStyle(e); if (cs.display === "none" || cs.visibility === "hidden") return 0; o *= +cs.opacity; e = e.parentElement; } return o; };
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  let n; while ((n = walker.nextNode())) { const el = n.parentElement; if (!el || el.closest("script,style,#pop,#gmenu[hidden]")) continue; const t = n.textContent; const c = hanLen(t); if (!c) continue; const r = el.getBoundingClientRect(); if (r.width === 0 || r.bottom < 0 || r.top > VH || r.right < 0 || r.left > VW) continue; if (vis(el) > .05) { dom += c; parts.push(t.trim()); } }
  let can = 0, minFont = 99; FR.texts.forEach(t => { if (t.a > .05 && t.x >= 0 && t.x <= VW && t.y >= 0 && t.y <= VH) { can += hanLen(t.s); parts.push(t.s); minFont = Math.min(minFont, t.size); } });
  document.querySelectorAll('#top *, #ctrl *, #credit, #poster *').forEach(el => { if (![...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim())) return; const r = el.getBoundingClientRect(); if (!r.width || vis(el) <= .05) return; minFont = Math.min(minFont, parseFloat(getComputedStyle(el).fontSize)); });
  const b = FR.bb, pb = $("poster").classList.contains("show") ? $("poster").getBoundingClientRect() : null;
  const box = pb ? {x0: pb.left, y0: pb.top, x1: pb.right, y1: pb.bottom} : b;
  const de = document.documentElement;
  return {chars: dom + can, dom, canvas: can, parts, stageRatio: +(SAFE.w * SAFE.h / (VW * VH)).toFixed(3), contentRatio: box ? +(((box.x1 - box.x0) * (box.y1 - box.y0)) / (VW * VH)).toFixed(3) : 0, contentOfStage: box ? +(((box.x1 - box.x0) * (box.y1 - box.y0)) / (SAFE.w * SAFE.h)).toFixed(3) : 0, scrollW: de.scrollWidth, scrollH: de.scrollHeight, vw: VW, vh: VH, title: lastTitle, titleLen: hanLen(lastTitle), minFont, off: FR.off, errs: window.__errs.slice()};
};

// 深链：#甲组/3 从第 3 步播；#甲组/3@0.5 停在第 3 步的 50% 处（截图用）
function boot() {
  retime(); renderTicks();
  const raw = decodeURIComponent(location.hash.slice(1)), [gs, rest = ""] = raw.split("/"), [ns, ps] = rest.split("@");
  if (gs && D.组[gs]) S.g = gs;
  const n = clamp(parseInt(ns, 10) || 1, 1, STEPS.length);
  S.t = START[n - 1] + .001;
  if (ps !== undefined) { S.freeze = true; S.playing = false; S.t = START[n - 1] + clamp(parseFloat(ps) || 0) * STEPS[n - 1].dur * .9999; }
  renderGroups(); syncPP(); resize(); frame(); render(); if (!boot.started) { boot.started = true; requestAnimationFrame(loop); }
}
addEventListener("hashchange", () => { S.freeze = false; boot(); });
boot();

/* J++ 案例页。数据全部来自 data.js（由 构建数据.py 从真机报告与账本生成），页面不手填数字，也不请求任何接口。
 *
 * 结构：每个案例是一个「场景」，给出 steps（每步一句标题和时长）、render(第几步, 本步进度 0..1)、
 * stats(第几步, 本步进度)（右上角三个数字，随播放上涨；值只取账本累计表 tl 里的真实数）和 legend(第几步)（角落图例，只列这一步画面上有的编码）。
 * 画面只由这两个数决定，所以播放、暂停、拖动进度条、从地址栏直接跳到某一步，画面都一致。
 * 地址栏可写 #/01?c=B&s=3&t=0.6&p=1：案子 B、第 4 步、进度 60%、暂停（自查截图用）；s 为负数时从末尾数。 */
(function () {
  "use strict";
  const D = window.JPP_DATA;
  const NS = "http://www.w3.org/2000/svg";
  const REDUCE = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const main = document.getElementById("main");

  // ---------- 小工具 ----------
  function el(tag, attrs, ...kids) {
    const n = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (v == null || v === false) continue;
      if (k === "class") n.className = v;
      else if (k === "html") n.innerHTML = v;
      else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
      else n.setAttribute(k, v === true ? "" : v);
    }
    for (const c of kids.flat()) if (c != null && c !== false) n.append(c.nodeType ? c : document.createTextNode(String(c)));
    return n;
  }
  function sv(tag, attrs, ...kids) {
    const n = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (v == null) continue;
      if (k === "class") n.setAttribute("class", v);
      else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
      else n.setAttribute(k, v);
    }
    for (const c of kids.flat()) if (c != null) n.append(c.nodeType ? c : document.createTextNode(String(c)));
    return n;
  }
  function at(n, attrs) { for (const k in attrs) n.setAttribute(k, attrs[k]); }
  const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
  const seg = (t, a, b) => clamp((t - a) / (b - a));
  const ease = (t) => (t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2);
  const lerp = (a, b, t) => a + (b - a) * t;
  const kindOf = (x) => (!x ? "unsure" : x.startsWith("unsure") ? "unsure" : x.startsWith("pick") ? "pick" : x);
  const EXIT_ZH = { act: "是", ignore: "否", unsure: "拿不准", pick: "选中" };
  const fmt = (x, d = 2) => (x == null ? "—" : Number(x).toFixed(d));
  const usd = (x) => (x ? "$" + String(Number(Number(x).toPrecision(2))) : "$0");
  const pairKey = (a, b) => (a < b ? a + "-" + b : b + "-" + a);
  const short = (t) => t.replace(/（.*?）/, "");
  const setFont = (n, px) => { n.style.fontSize = px + "px"; };
  // 账本累计表里取第 idx 次判断请求之后的 [判断次数, 花费]；idx < 0 表示还没开始
  const tlAt = (tl, idx) => (idx < 0 ? [0, 0] : tl[Math.min(tl.length - 1, Math.round(idx))]);

  // 颜色：从 CSS token 读，渐变时在 RGB 里插值
  const COL = {};
  function readColors() {
    const cs = getComputedStyle(document.documentElement);
    for (const k of ["paper", "card", "ink", "ink-2", "muted", "line", "line-2", "ok", "bad", "unsure", "accent", "cell-no"]) {
      const h = cs.getPropertyValue("--" + k).trim().replace("#", "");
      COL[k] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
    }
  }
  const mix = (a, b, t) => [0, 1, 2].map((i) => Math.round(a[i] + (b[i] - a[i]) * clamp(t)));
  const rgb = (c) => `rgb(${c[0]},${c[1]},${c[2]})`;
  readColors();

  // ---------- 浮层：点画面上的元素才出现；桌面上不越过进度条上沿，超出部分在浮层里滚动 ----------
  let popEl = null, current = null;
  function closePop() { if (popEl) { popEl.remove(); popEl = null; } }
  function openPop(ev, title, ...kids) {
    closePop();
    if (current && current.player) current.player.pause();
    popEl = el("div", { class: "pop", role: "dialog", "aria-label": title },
      el("button", { class: "icon-btn x", "aria-label": "关闭", onclick: closePop }, "×"), el("h4", {}, title), ...kids);
    document.body.append(popEl);
    if (window.innerWidth > 720) {
      const bar = document.querySelector(".case .bar");
      const floor = (bar ? bar.getBoundingClientRect().top : innerHeight) - 10;
      popEl.style.maxHeight = Math.max(160, floor - 12) + "px";
      const r = popEl.getBoundingClientRect(), x = ev.clientX, y = ev.clientY;
      let L = x + 16, T = y + 12;
      if (L + r.width > innerWidth - 12) L = x - r.width - 16;
      if (T + r.height > floor) T = floor - r.height;
      popEl.style.left = Math.max(12, L) + "px";
      popEl.style.top = Math.max(12, T) + "px";
    }
  }
  document.addEventListener("pointerdown", (e) => {
    if (popEl && !popEl.contains(e.target) && !(e.target.closest && e.target.closest(".clickable,.hit,.icon-btn"))) closePop();
  });
  function pill(exit, label) { const k = kindOf(exit); return el("span", { class: "pill " + k }, label || EXIT_ZH[k]); }
  function meter(p, line) {
    const m = el("div", { class: "meter" });
    m.style.background = `linear-gradient(90deg, var(--bad) 0 ${line.lo * 100}%, var(--unsure) ${line.lo * 100}% ${line.hi * 100}%, var(--ok) ${line.hi * 100}% 100%)`;
    const x = clamp(p || 0) * 100;
    const i = el("i"); i.style.left = `calc(${x}% - 1px)`;
    const s = el("span", {}, fmt(p)); s.style.left = x + "%";
    m.append(i, s);
    return m;
  }
  function bars(rows) {
    return el("div", { class: "bars" }, rows.map((r) => {
      const i = el("i"); i.style.width = (clamp(r.v) * 100).toFixed(1) + "%";
      return [el("span", {}, r.label), el("div", { class: r.chosen ? "chosen" : "" }, i), el("span", { class: "num" }, fmt(r.v))];
    }));
  }
  const lab = (t) => el("div", { class: "lab" }, t);

  // ---------- 播放器：细进度条，可暂停、可拖动 ----------
  const ICON = {
    play: '<svg width="14" height="14" viewBox="0 0 14 14"><path d="M3 1.5v11l9.5-5.5z" fill="currentColor"/></svg>',
    pause: '<svg width="14" height="14" viewBox="0 0 14 14"><rect x="2.5" y="1.5" width="3" height="11" rx="1" fill="currentColor"/><rect x="8.5" y="1.5" width="3" height="11" rx="1" fill="currentColor"/></svg>',
    replay: '<svg width="15" height="15" viewBox="0 0 16 16"><path d="M8 2.5a5.5 5.5 0 1 1-5.2 3.7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><path d="M1.6 2.2l1.4 4.3 4.1-1.7z" fill="currentColor"/></svg>',
    info: '<svg width="18" height="18" viewBox="0 0 18 18"><circle cx="9" cy="9" r="7.5" fill="none" stroke="currentColor" stroke-width="1.4"/><circle cx="9" cy="5.4" r="1.1" fill="currentColor"/><rect x="8.2" y="7.6" width="1.6" height="5.6" rx=".8" fill="currentColor"/></svg>',
  };
  function Player(steps, api, host) {
    const starts = []; let tot = 0;
    for (const s of steps) { starts.push(tot); tot += s.dur; }
    let T = 0, playing = false, last = null, cur = -1, dirty = true, alive = true;
    const btn = el("button", { class: "play", type: "button" });
    const track = el("div", { class: "track", role: "slider", "aria-label": "播放进度", "aria-valuemin": 0, "aria-valuemax": steps.length });
    const cells = steps.map((s) => { const i = el("i", { title: s.title }, el("b")); i.style.flex = `${s.dur} 1 0`; track.append(i); return i; });
    host.append(btn, track);
    function icon() {
      btn.innerHTML = playing ? ICON.pause : T >= tot ? ICON.replay : ICON.play;
      btn.setAttribute("aria-label", playing ? "暂停" : "播放");
    }
    function locate() {
      for (let k = 0; k < steps.length; k++) if (T < starts[k] + steps[k].dur) return [k, (T - starts[k]) / steps[k].dur];
      return [steps.length - 1, 1];
    }
    function draw() {
      const [i, t] = locate();
      if (i !== cur) { cur = i; api.onStep(i); track.setAttribute("aria-valuenow", i + 1); }
      api.render(i, REDUCE ? 1 : t);
      cells.forEach((c, k) => (c.firstChild.style.width = (k < i ? 100 : k > i ? 0 : t * 100) + "%"));
    }
    function frame(now) {
      if (!alive) return;
      if (playing) {
        if (last != null) { T += now - last; if (T >= tot) { T = tot; playing = false; icon(); } }
        last = now; dirty = true;
      } else last = null;
      if (dirty) { dirty = false; draw(); }
      requestAnimationFrame(frame);
    }
    const play = () => { if (T >= tot) T = 0; playing = true; last = null; icon(); };
    const pause = () => { playing = false; icon(); };
    const seek = (x) => { T = clamp(x, 0, tot); dirty = true; icon(); };
    btn.addEventListener("click", () => (playing ? pause() : play()));
    function tAt(e) {
      for (let k = 0; k < cells.length; k++) {
        const r = cells[k].getBoundingClientRect();
        if (e.clientX <= r.right + 1.5 || k === cells.length - 1) return starts[k] + clamp((e.clientX - r.left) / r.width) * steps[k].dur;
      }
      return 0;
    }
    let drag = false, was = false;
    track.addEventListener("pointerdown", (e) => { drag = true; was = playing; playing = false; track.setPointerCapture(e.pointerId); seek(tAt(e)); });
    track.addEventListener("pointermove", (e) => { if (drag) seek(tAt(e)); });
    const up = () => { if (!drag) return; drag = false; if (was && T < tot) { playing = true; last = null; } icon(); };
    track.addEventListener("pointerup", up);
    track.addEventListener("pointercancel", up);
    icon();
    requestAnimationFrame(frame);
    return {
      play, pause, seek, redraw: () => (dirty = true), destroy: () => (alive = false),
      toggle: () => (playing ? pause() : play()),
      step: (d) => { const [i, t] = locate(); const k = d < 0 && t > 0.15 ? i : clamp(i + d, 0, steps.length - 1); pause(); seek(starts[k]); },
      jump: (s, t, paused) => { const k = clamp(s < 0 ? steps.length + s : s, 0, steps.length - 1); seek(starts[k] + Math.min(clamp(t), 0.999) * steps[k].dur); paused ? pause() : play(); },
    };
  }

  // 首页缩略图：同一个场景，时间压缩后循环放
  function Loop(scene, total) {
    const durs = scene.steps.map((s) => s.dur), sum = durs.reduce((a, b) => a + b, 0), k = total / sum;
    let alive = true, t0 = null, dirty = true;
    function frame(now) {
      if (!alive) return;
      if (t0 == null) t0 = now;
      if (REDUCE && !dirty) return requestAnimationFrame(frame);
      dirty = false;
      let T = ((now - t0) % (total + 1800)) / k;
      if (REDUCE) T = sum;
      let i = durs.length - 1, t = 1;
      for (let s = 0, acc = 0; s < durs.length; acc += durs[s], s++) if (T < acc + durs[s]) { i = s; t = (T - acc) / durs[s]; break; }
      scene.render(i, t);
      requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);
    return { destroy: () => (alive = false), redraw: () => (dirty = true) };
  }

  // 角落图例：每一步只列这一步画面上出现的编码
  const LEGEND = {
    ok: ["line", "说得通"], bad: ["lineBad", "矛盾"], unsure: ["dash", "拿不准"],
    hull: ["hull", "能同时成立"], ring: ["ring", "对不上"], chip: ["chip", "补进的证据"],
    c_ok: ["sq_ok", "讲清楚了"], c_unsure: ["sq_unsure", "拿不准"], c_no: ["sq_no", "没讲清"],
    list: ["accent", "书单"], levels: ["levels", "材料长度"],
    host: ["host", "主租人"], seeker: ["seeker", "找房人"], cut: ["lineBad", "预算或日期不合"], score: ["dot", "一次判断"],
    propose: ["dashAccent", "求租"], reject: ["lineBad", "被拒"], hold: ["ink", "暂时留下"], moved: ["line", "入住"],
  };
  function legendItem(key) {
    const [kind, text] = LEGEND[key];
    const g = sv("svg", { viewBox: "0 0 18 12", "aria-hidden": "true" });
    const S = {
      line: () => sv("line", { x1: 1, y1: 6, x2: 17, y2: 6, stroke: "var(--ok)", "stroke-width": 3 }),
      lineBad: () => sv("line", { x1: 1, y1: 6, x2: 17, y2: 6, stroke: "var(--bad)", "stroke-width": 3 }),
      dash: () => sv("line", { x1: 1, y1: 6, x2: 17, y2: 6, stroke: "var(--unsure)", "stroke-width": 3, "stroke-dasharray": "4 3" }),
      dashAccent: () => sv("line", { x1: 1, y1: 6, x2: 17, y2: 6, stroke: "var(--accent)", "stroke-width": 3, "stroke-dasharray": "4 3" }),
      accent: () => sv("line", { x1: 1, y1: 6, x2: 17, y2: 6, stroke: "var(--accent)", "stroke-width": 4 }),
      ink: () => sv("line", { x1: 1, y1: 6, x2: 17, y2: 6, stroke: "var(--ink)", "stroke-width": 3.5 }),
      hull: () => sv("rect", { x: 1, y: 1, width: 16, height: 10, rx: 3, fill: "var(--ok)", "fill-opacity": 0.3, stroke: "var(--ok)", "stroke-width": 1.2 }),
      ring: () => sv("circle", { cx: 9, cy: 6, r: 4.5, fill: "none", stroke: "var(--bad)", "stroke-width": 2 }),
      chip: () => sv("rect", { x: 5, y: 2, width: 8, height: 8, rx: 1.5, fill: "var(--accent)" }),
      dot: () => sv("circle", { cx: 9, cy: 6, r: 3.5, fill: "var(--accent)" }),
      sq_ok: () => sv("rect", { x: 3, y: 0, width: 12, height: 12, rx: 2, fill: "var(--ok)" }),
      sq_unsure: () => sv("rect", { x: 3, y: 0, width: 12, height: 12, rx: 2, fill: "var(--unsure)" }),
      sq_no: () => sv("rect", { x: 3, y: 0, width: 12, height: 12, rx: 2, fill: "var(--cell-no)" }),
      levels: () => [0, 1, 2].map((d) => sv("rect", { x: 1 + d * 5.6, y: 3.5, width: 4.6, height: 5, rx: 1, fill: d < 2 ? "var(--ink-2)" : "var(--line)" })),
      host: () => sv("rect", { x: 3, y: 0.5, width: 11, height: 11, rx: 2.5, fill: "none", stroke: "var(--ink)", "stroke-width": 1.6 }),
      seeker: () => sv("circle", { cx: 9, cy: 6, r: 5, fill: "none", stroke: "var(--ink)", "stroke-width": 1.6 }),
    }[kind]();
    g.append(...[].concat(S));
    return el("span", {}, g, text);
  }

  // =====================================================================
  // 01 谁在说谎：证词关系图 → 最大团 → 补证据 → 重判
  // =====================================================================
  function scene01(svg, cid, thumb) {
    const c = D.c01.cases[cid], n = c.names.length, R = c.rounds, last = R.length - 1, fin = c.final;
    const liars = D.c01.truth[cid].liars || [];
    const truth = new Set(liars.map((nm) => c.names.indexOf(nm)));
    const unsureN = R.map((r) => r.edges.filter((e) => kindOf(e.exit) === "unsure").length);
    const edgeOf = (r, pair) => r.edges.find((e) => pairKey(...e.ends) === pairKey(...pair));
    const steps = [];
    steps.push({ kind: "intro", k: 0, title: `${n} 个证人，${n} 份证词`, dur: 2400 });
    steps.push({ kind: "judge0", k: 0, title: "两两判断：能否同时为真", dur: 4600 });
    steps.push({ kind: "clique", k: 0, title: `最多 ${R[0].lo.omega} 人的证词能同时成立`, dur: 3400 });
    R.forEach((r, k) => {
      if (k > 0) {
        const w0 = R[k - 1].lo.omega, w = r.lo.omega, a = unsureN[k - 1], b = unsureN[k];
        steps.push({ kind: "rejudge", k, title: `补证后${w === w0 ? "仍是" : "变成"} ${w} 人成立，` + (a === b ? `拿不准仍是 ${b} 对` : `拿不准从 ${a} 对变 ${b} 对`), dur: 4800 });
      }
      if (r.enrich.length) {
        const picked = r.enrich.filter((x) => x.table).length, m = r.enrich.length;
        const nU = r.enrich.filter((x) => kindOf(edgeOf(r, x.pair).exit) === "unsure").length;
        const what = nU === m ? "拿不准" : nU === 0 ? "矛盾" : "关键的";
        steps.push({ kind: "enrich", k, picked, title: picked ? `${m} 对${what}，去证据库${nU ? "补证" : "核实"}` : `${m} 对${what}，选不出该补什么`, dur: 4400 });
      }
    });
    {
      const S = fin.suspects, same = S.length === liars.length && S.every((x) => liars.includes(x)), inside = liars.every((x) => S.includes(x));
      const head = !S.length ? "程序没有认定任何人" : same ? `程序认定${S.join("、")}，与出题设定一致` : inside ? `程序圈出${S.join("、")}，真凶${liars.join("、")}在其中` : `程序认定${S.join("、")}，出题设定的是${liars.join("、")}`;
      steps.push({ kind: "final", k: last, title: head + (fin.independent ? "" : "，仍有拿不准"), dur: 5600 });
    }

    // 元素
    const emap = new Map(); // key -> round index -> edge
    R.forEach((r, k) => r.edges.forEach((e) => { const key = pairKey(...e.ends); if (!emap.has(key)) emap.set(key, []); emap.get(key)[k] = e; }));
    const keys = [...emap.keys()];
    const ev = R.map((r) => r.nodes.map((t) => Math.min(3, (t.match(/【证据库|【记录比对/g) || []).length)));
    let stepK = 0; // 浮层读哪一轮
    const gHull = sv("g"), gE = sv("g"), gHit = sv("g"), gLib = sv("g"), gN = sv("g"), gChip = sv("g");
    svg.append(gHull, gE, gHit, gLib, gN, gChip);
    const hullA = sv("path", {}), hullB = sv("path", {});
    gHull.append(hullA, hullB);
    const E = keys.map((key, idx) => {
      const [a, b] = key.split("-").map(Number);
      const line = sv("line", { "stroke-linecap": "round" });
      gE.append(line);
      let hit = null;
      if (!thumb) {
        hit = sv("line", { class: "hit" });
        hit.addEventListener("click", (e) => showEdge(e, a, b));
        gHit.append(hit);
      }
      return { key, a, b, idx, line, hit };
    });
    // 证据库：补证那一步出现，蓝色小方块从这里飞到两端的人
    const libCards = [0, 1, 2].map(() => sv("rect", { rx: 4 }));
    const libText = sv("text", { class: "lbl", "text-anchor": "middle" }, "证据库");
    gLib.append(...libCards, libText);
    const N = c.names.map((nm, k) => {
      const g = sv("g", { class: thumb ? "" : "clickable" });
      const circ = sv("circle", {});
      const sats = [0, 1, 2].map(() => sv("rect", { rx: 1.2 }));
      const label = sv("text", { class: "lbl" }, nm);
      // 出题设定的真凶：名字旁边一块深底白字的牌子「◆ 真凶」
      const tagBg = sv("rect", {});
      const tag = sv("text", { class: "tag", "text-anchor": "middle" }, "◆ 真凶");
      g.append(circ, ...sats, label, tagBg, tag);
      if (!thumb) g.addEventListener("click", (e) => showNode(e, k));
      gN.append(g);
      return { g, circ, sats, label, tagBg, tag };
    });
    const maxChips = Math.max(0, ...R.map((r) => r.enrich.filter((x) => x.table).length * 2));
    const chips = Array.from({ length: maxChips }, () => { const r = sv("rect", { rx: 2 }); gChip.append(r); return r; });

    let L = null;
    function layout(W, H) {
      const mobile = W <= 720;
      const fs = thumb ? 0 : mobile ? 14 : clamp(Math.min(W, H) * 0.022, 15, 18);
      const top = thumb ? 4 : 8, bot = thumb ? 4 : mobile ? 30 : 12;
      const avail = H - top - bot;
      const nr = thumb ? clamp(avail * 0.035, 4, 8) : clamp(avail * 0.028, 11, 22);
      const cx = W / 2, cy = top + avail / 2;
      const Rr = thumb ? Math.min(W * 0.4, avail * 0.44) : Math.min(W / 2 - nr - fs * 3.6 - 16, avail / 2 - nr - fs - 12);
      // 手机竖屏：横向放不宽，改成竖着的椭圆，用上纵向的高度
      let Rx = Rr, Ry = Rr;
      if (mobile && !thumb) Ry = Math.min(avail / 2 - nr - fs * 2.6 - 12, Rr * 1.7);
      // 桌面横屏：圈高已顶满，横向拉成椭圆，左边给证据库留 200px
      else if (!thumb) Rx = Math.max(Rr, Math.min(Rr * 1.45, W / 2 - nr - fs * 3.6 - 230));
      const P = c.names.map((_, k) => { const a = (2 * Math.PI * k) / n - Math.PI / 2; return { x: cx + Rx * Math.cos(a), y: cy + Ry * Math.sin(a), a }; });
      const sc = thumb ? 0.6 : clamp(Rr / 260, 1, 1.5);
      // 证据库放在圈左边；手机上放左上角
      const lib = mobile ? { x: 34, y: 34 } : { x: Math.max(60, cx - Rx - nr - fs * 3.6 - 130), y: cy };
      L = { cx, cy, R: Rr, Rx, Ry, nr, P, fs, sc, lib, mobile };
      E.forEach((e) => { const A = P[e.a], B = P[e.b]; if (e.hit) at(e.hit, { x1: A.x, y1: A.y, x2: B.x, y2: B.y }); });
      N.forEach((nd, k) => {
        const p = P[k], off = nr + 8 + fs * 0.35, cs = Math.cos(p.a), sn = Math.sin(p.a);
        const anchor = cs > 0.3 ? "start" : cs < -0.3 ? "end" : "middle";
        const lx = cx + (Rx + off) * cs, ly = cy + (Ry + off) * sn + fs * 0.36 + (sn > 0.9 ? fs * 0.5 : sn < -0.9 ? -fs * 0.3 : 0);
        at(nd.label, { x: lx, y: ly, "text-anchor": anchor });
        if (thumb) return;
        setFont(nd.label, fs); setFont(nd.tag, fs);
        // 真凶牌子：桌面放在名字外侧同一行；手机放在名字下面一行（最上面那个人放上面）
        const w = nd.label.getComputedTextLength(), tw = fs * 4.1, th = fs * 1.6;
        let tx, ty;
        if (!mobile) {
          tx = anchor === "start" ? lx + w + 10 + tw / 2 : anchor === "end" ? lx - w - 10 - tw / 2 : lx + w / 2 + 10 + tw / 2;
          ty = ly - fs * 0.36;
        } else {
          tx = anchor === "start" ? lx + w / 2 : anchor === "end" ? lx - w / 2 : lx;
          tx = clamp(tx, tw / 2 + 2, W - tw / 2 - 2);
          ty = ly - fs * 0.36 + (sn < -0.9 ? -th - 4 : th + 4);
        }
        at(nd.tagBg, { x: tx - tw / 2, y: ty - th / 2, width: tw, height: th, rx: th / 2 });
        at(nd.tag, { x: tx, y: ty + fs * 0.36 });
      });
      const cw = 40, ch = 28;
      libCards.forEach((q, i) => at(q, { x: lib.x - cw / 2 + i * 5, y: lib.y - ch / 2 - 10 + i * 5, width: cw, height: ch }));
      if (!thumb) setFont(libText, fs);
      at(libText, { x: lib.x + 5, y: lib.y + ch / 2 + 10 + fs });
    }
    const ES = { act: { c: "ok", w: 1.6, o: 0.55, d: 0 }, ignore: { c: "bad", w: 2.8, o: 0.95, d: 0 }, unsure: { c: "unsure", w: 2.6, o: 1, d: 1 } };
    const kindAt = (key, k) => kindOf(emap.get(key)[k].exit);
    const hullPath = (m) => m.length ? "M" + m.map((i) => `${L.P[i].x} ${L.P[i].y}`).join("L") + "Z" : "";

    function render(si, t) {
      if (!L) return;
      const st = steps[si], k = st.k, r = R[k], sc = L.sc;
      stepK = k;
      let nodeIn = 1, edgeGrow = 1, hullOp = 0, hullMorph = 1, truthOp = 0;
      let members = r.lo.alts[0], prevMembers = members;
      const enrichSet = new Set(st.kind === "enrich" ? r.enrich.map((x) => pairKey(...x.pair)) : []);
      if (st.kind === "intro") { nodeIn = t; edgeGrow = 0; }
      if (st.kind === "judge0") edgeGrow = t;
      if (st.kind === "clique") hullOp = ease(seg(t, 0.05, 0.55));
      if (st.kind === "enrich") hullOp = 0.45;
      if (st.kind === "rejudge") { prevMembers = R[k - 1].lo.alts[0]; hullOp = 1; hullMorph = ease(seg(t, 0.55, 0.95)); }
      if (st.kind === "final") { hullOp = 1; truthOp = ease(seg(t, 0.3, 0.6)); }

      // 边
      E.forEach((e) => {
        const A = L.P[e.a], B = L.P[e.b];
        let cur = ES[kindAt(e.key, k)], col = COL[cur.c], w = cur.w, o = cur.o, dash = cur.d;
        let g = 1;
        if (st.kind === "intro") g = 0;
        if (st.kind === "judge0") g = ease(seg(edgeGrow, (e.idx / E.length) * 0.78, (e.idx / E.length) * 0.78 + 0.18));
        if (st.kind === "rejudge" && emap.get(e.key)[k].fresh) {
          const prev = ES[kindAt(e.key, k - 1)];
          const m = ease(seg(t, 0.04 + (e.idx / E.length) * 0.4, 0.16 + (e.idx / E.length) * 0.4));
          col = mix(COL[prev.c], COL[cur.c], m); w = lerp(prev.w, cur.w, m) + 2.6 * Math.sin(Math.PI * m); o = lerp(prev.o, cur.o, m) + 0.3 * Math.sin(Math.PI * m); dash = m < 0.5 ? prev.d : cur.d;
        }
        if (enrichSet.has(e.key)) { const m = ease(seg(t, 0, 0.22)); col = mix(col, COL.accent, m); w = lerp(w, 5, m); o = lerp(o, 1, m); dash = m > 0.5 ? 0 : dash; }
        if (st.kind === "final" && kindAt(e.key, k) === "unsure") { o = 0.55 + 0.45 * Math.cos(t * Math.PI * 6); w = 3.4; }
        at(e.line, { x1: A.x, y1: A.y, x2: lerp(A.x, B.x, g), y2: lerp(A.y, B.y, g), stroke: rgb(col), "stroke-width": w * sc, "stroke-opacity": g > 0 ? clamp(o) : 0, "stroke-dasharray": dash ? `${7 * sc} ${5 * sc}` : "none" });
      });
      // 团：只信已决的边时最多几人能同时说真话；重判时新旧两块交叉淡变
      at(hullA, { d: hullPath(prevMembers), fill: rgb(COL.ok), "fill-opacity": 0.13 * hullOp * (1 - hullMorph), stroke: rgb(COL.ok), "stroke-opacity": 0.5 * hullOp * (1 - hullMorph), "stroke-width": 1.5 });
      at(hullB, { d: hullPath(members), fill: rgb(COL.ok), "fill-opacity": 0.13 * hullOp * hullMorph, stroke: rgb(COL.ok), "stroke-opacity": 0.5 * hullOp * hullMorph, "stroke-width": 1.5 });
      if (st.kind !== "rejudge") { at(hullA, { "fill-opacity": 0, "stroke-opacity": 0 }); at(hullB, { d: hullPath(members), "fill-opacity": 0.13 * hullOp, "stroke-opacity": 0.5 * hullOp }); }

      // 证据库与证据：要补的对，从证据库沿弧线飞到两端的人；落地后成了这个人身边的小方块
      const libOp = thumb ? 0 : st.kind === "enrich" && st.picked ? ease(seg(t, 0.05, 0.25)) * (1 - ease(seg(t, 0.9, 1))) : 0;
      libCards.forEach((q, i) => at(q, { fill: rgb(mix(COL.card, COL.accent, 0.25 + i * 0.25)), stroke: rgb(COL.accent), "stroke-width": 1.5, opacity: libOp }));
      at(libText, { opacity: libOp, fill: rgb(COL.accent) });
      const arrived = new Array(n).fill(0);
      let ci = 0;
      if (st.kind === "enrich") {
        const from = thumb ? null : L.lib;
        r.enrich.forEach((x, xi) => {
          if (!x.table) return;
          x.pair.forEach((p, side) => {
            const ch = chips[ci++], P = L.P[p];
            const s0 = 0.25 + (xi / Math.max(1, r.enrich.length)) * 0.35 + side * 0.04, m = ease(seg(t, s0, s0 + 0.28));
            const far = L.R + L.nr + 60;
            const fx = from ? from.x : L.cx + far * Math.cos(P.a), fy = from ? from.y : L.cy + far * Math.sin(P.a);
            // 二次贝塞尔：控制点抬高，走一条弧
            const qx = (fx + P.x) / 2, qy = Math.min(fy, P.y) - 80;
            const x0 = (1 - m) * (1 - m) * fx + 2 * (1 - m) * m * qx + m * m * P.x, y0 = (1 - m) * (1 - m) * fy + 2 * (1 - m) * m * qy + m * m * P.y;
            const sz = 14 * sc;
            at(ch, { x: x0 - sz / 2, y: y0 - sz / 2, width: sz, height: sz, fill: rgb(COL.accent), opacity: m > 0 && m < 1 ? 1 : 0 });
            if (m >= 1) arrived[p]++;
          });
        });
      }
      for (; ci < chips.length; ci++) at(chips[ci], { opacity: 0 });
      // 人
      const inHull = new Set(members), inPrev = new Set(prevMembers);
      N.forEach((nd, k2) => {
        const P = L.P[k2];
        const s = st.kind === "intro" ? ease(seg(nodeIn, (k2 / n) * 0.7, (k2 / n) * 0.7 + 0.3)) : 1;
        const isIn = st.kind === "rejudge" ? (hullMorph > 0.5 ? inHull.has(k2) : inPrev.has(k2)) : inHull.has(k2);
        const out = hullOp > 0.3 && !isIn;
        let ring = out ? 3.2 : 2;
        if (st.kind === "final" && out) ring = 3.2 + 2.2 * (0.5 + 0.5 * Math.sin(t * Math.PI * 8));
        at(nd.circ, { cx: P.x, cy: P.y, r: L.nr * s, fill: rgb(hullOp > 0.3 && isIn ? mix(COL.card, COL.ok, hullOp) : COL.card), stroke: rgb(out ? COL.bad : hullOp > 0.3 && isIn ? COL.ok : COL["ink-2"]), "stroke-width": ring * sc });
        // 证据卫星
        let cnt = ev[k][k2];
        if (st.kind === "enrich" && k < last) cnt = t >= 0.97 ? ev[k + 1][k2] : Math.min(ev[k + 1][k2], ev[k][k2] + arrived[k2]);
        nd.sats.forEach((q, m) => {
          const a = P.a + Math.PI + (m - 1) * 0.5, d = L.nr + 9 * sc, sz = 8 * sc;
          at(q, { x: P.x + d * Math.cos(a) - sz / 2, y: P.y + d * Math.sin(a) - sz / 2, width: sz, height: sz, fill: rgb(COL.accent), opacity: m < cnt && s > 0.9 ? 1 : 0 });
        });
        at(nd.label, { opacity: thumb ? 0 : s, fill: rgb(out ? COL.bad : COL.ink), "font-weight": out ? 700 : 600 });
        const tOp = truth.has(k2) && !thumb ? truthOp : 0;
        at(nd.tagBg, { fill: rgb(COL.ink), opacity: tOp });
        at(nd.tag, { opacity: tOp, fill: rgb(COL.paper) });
      });
    }

    function stats(si, t) {
      const st = steps[si], k = st.k, cpE = c.cp.edges, cpN = c.cp.enrich, end = c.tl.length - 1;
      let idx = -1, round = st.kind === "intro" ? 0 : k + 1;
      if (st.kind === "judge0") idx = lerp(-1, cpE[0], seg(t, 0, 0.96));
      if (st.kind === "clique") idx = cpE[0];
      if (st.kind === "enrich") idx = lerp(cpE[k], cpN[k], seg(t, 0.05, 0.5));
      if (st.kind === "rejudge") idx = lerp(cpN[k - 1], cpE[k], seg(t, 0.04, 0.56));
      if (st.kind === "final") idx = lerp(Math.max(cpE[last], cpN[last]), end, seg(t, 0, 0.3));
      const [j, u] = tlAt(c.tl, Math.round(idx));
      return [[j, "次判断"], [round, "轮"], [usd(u), "实际花费"]];
    }
    const LG = { intro: [], judge0: ["ok", "bad", "unsure"], clique: ["hull", "ring"], enrich: ["chip"], rejudge: ["ok", "bad", "unsure"], final: ["unsure", "ring"] };

    function showEdge(ev, a, b) {
      const e = emap.get(pairKey(a, b))[stepK], r = R[stepK];
      openPop(ev, `${c.names[a]} — ${c.names[b]}`,
        el("div", {}, "判断结果 ", pill(e.exit), e.fresh ? "　这一轮新判" : "　沿用上一轮"),
        meter(e.p, c.line), lab("读数；0.3 以下判「否」，0.7 以上判「是」，中间拿不准"),
        lab("题"), el("div", { class: "q" }, c.questions.compat),
        lab(c.names[a] + " 的材料"), el("div", { class: "mat" }, r.nodes[a]),
        lab(c.names[b] + " 的材料"), el("div", { class: "mat" }, r.nodes[b]));
    }
    function showNode(ev, k) {
      const w = c.witnesses[k];
      const ask = R[stepK].enrich.filter((x) => x.pair.includes(k));
      openPop(ev, `${w.name}（${w.role}）`,
        truth.has(k) ? lab("出题设定的真凶：这个人在说谎（出题时设定，程序看不到）") : null,
        lab(`第 ${stepK + 1} 轮的材料`), el("div", { class: "mat" }, R[stepK].nodes[k]),
        ask.length ? lab("这一轮对他问「最缺哪类信息」") : null,
        ask.map((x) => el("div", {}, x.pair.map((i) => c.names[i]).join(" — "), " ", x.table ? pill("pick", "补 " + x.table) : pill("unsure", "拿不准"))));
    }
    function info(ev) {
      const reason = { settled: "结论已定", exhausted: "没有可补的了", bound: "轮数用完" }[c.reason] || c.reason;
      openPop(ev, c.title, el("div", {}, c.summary),
        lab("程序怎么做"), el("div", {}, "语义判断模型判每一对证词能不能同时为真；最大团算法求最多几人能同时说真话。拿不准、又会改变结论的那几对，先问「最缺哪类信息」，再由代码从证据库取真实记录接进证词，重判。"),
        lab("拿不准的对，逐轮"), el("div", {}, unsureN.map((x, i) => `第 ${i + 1} 轮 ${x} 对`).join("，") + "。补进的证据不一定让判断更确定，程序照实记下。"),
        lab("结果"), el("div", {}, `停在第 ${R.length} 轮（${reason}）。对不上的：${fin.suspects.join("、") || "无"}。` + (fin.independent ? "结论不依赖拿不准的对。" : "结论仍取决于拿不准的对，程序照实交出，不当定论。")),
        lab("出题设定的真凶"), el("div", {}, `${liars.join("、")}（出题时设定，程序看不到；设计结局：${D.c01.truth[cid].design}）`),
        lab("对照：一次让模型从八人里选一个"), el("div", {}, c.control.pick ? `选了 ${c.control.pick}` : "拿不准，没有选"),
        lab("账本"), el("div", {}, `判断 ${c.counts.Judge} 次，生成 ${c.counts["Effect/gen"] || 0} 次，代码与图算法 ${c.counts["Effect/do"] || 0} 次，实际花费 ${usd(c.cost)}。数据：${D.built_from}`));
    }
    return {
      steps, layout, render, info, stats, legend: (i) => (steps[i].kind === "enrich" && !steps[i].picked ? ["unsure"] : LG[steps[i].kind] || []),
      aria: "证词关系图：八个人，两两之间的边表示能否同时为真",
    };
  }

  // =====================================================================
  // 03 最短书单：覆盖矩阵 → 集合覆盖收拢 → 补长摘录 → 重判
  // 横屏：每列一篇条目、每行一个概念；竖屏：每行一篇条目、每列一个概念
  // =====================================================================
  function scene03(svg, thumb) {
    const c = D.c03, A = c.articles, C = c.concepts, nA = A.length, nC = C.length, R = c.rounds, last = R.length - 1;
    const artName = (t) => t.replace(/\s*\(.*?\)\s*/g, "").replace(/ (protocol|design principle)$/, "");
    const steps = [];
    steps.push({ kind: "intro", k: 0, title: `${nA} 篇维基条目 × ${nC} 个概念`, dur: 2600 });
    steps.push({ kind: "judge0", k: 0, title: "逐格判断：这篇讲清楚了吗", dur: 5000 });
    steps.push({ kind: "cover", k: 0, title: `书单收拢到 ${R[0].lo.size} 篇`, dur: 3800 });
    R.forEach((r, k) => {
      if (k > 0) {
        const p = R[k - 1], got = p.lo.uncovered.filter((j) => !r.lo.uncovered.includes(j)).length, lost = r.lo.uncovered.filter((j) => !p.lo.uncovered.includes(j)).length;
        const moreAlts = r.lo.alts.length - p.lo.alts.length;
        let title;
        if (r.lo.size !== p.lo.size && got && !lost) title = `补摘录后多讲清 ${got} 个概念，书单变 ${r.lo.size} 篇`;
        else if (r.lo.size !== p.lo.size && lost && !got) title = `重判后 ${lost} 个概念没讲清了，书单变 ${r.lo.size} 篇`;
        else if (r.lo.size === p.lo.size && moreAlts > 0) title = `重判 ${r.fresh.length} 篇，仍是 ${r.lo.size} 篇，多出 ${moreAlts} 种选法`;
        else title = `重判 ${r.fresh.length} 篇，书单${r.lo.size === p.lo.size ? "仍是" : "变成"} ${r.lo.size} 篇`;
        steps.push({ kind: "rejudge", k, title, dur: 5000 });
      }
      if (r.enrich.length && k < last) steps.push({ kind: "enrich", k, title: `${r.enrich.length} 篇拿不准，补更长的摘录`, dur: 4200 });
    });
    const fin = R[last];
    steps.push({ kind: "final", k: last, title: `${fin.lo.size} 篇书单，${fin.lo.uncovered.length} 个概念没人讲清` + (c.final.independent ? "" : "，仍有拿不准"), dur: 5200 });

    const gBg = sv("g"), gCells = sv("g"), gFg = sv("g"), gTxt = sv("g");
    svg.append(gBg, gCells, gFg, gTxt);
    const scan = sv("rect", { rx: 4 });
    gBg.append(scan);
    const cells = A.map((_, i) => C.map((_, j) => {
      const r = sv("rect", { class: thumb ? "" : "clickable", "data-i": i, "data-j": j });
      gCells.append(r);
      return r;
    }));
    const lvl = A.map(() => [0, 1, 2].map(() => { const r = sv("rect", { rx: 1 }); gFg.append(r); return r; }));
    const mark = A.map(() => { const r = sv("rect", { rx: 2 }); gFg.append(r); return r; });
    const rowMark = C.map(() => { const r = sv("rect", { rx: 2 }); gFg.append(r); return r; });
    const colName = A.map((a) => { const t = sv("text", { class: "lbl" }, artName(a.title)); gTxt.append(t); return t; });
    const rowName = C.map((x) => { const t = sv("text", { class: "lbl" }, short(x)); gTxt.append(t); return t; });
    const hintA = sv("text", { class: "lbl hint" }), hintC = sv("text", { class: "lbl hint" });
    gTxt.append(hintA, hintC);
    let stepK = 0;
    if (!thumb) gCells.addEventListener("click", (e) => { const t = e.target; if (t.dataset.i != null) showCell(e, +t.dataset.i, +t.dataset.j); });

    let L = null;
    function layout(W, H) {
      const portrait = W < H * 0.9, mobile = W <= 720;
      const fs = thumb ? 0 : mobile ? 14 : clamp(Math.min(W, H) * 0.02, 15, 18);
      const mx = thumb ? 10 : mobile ? 12 : 28;
      if (!portrait) {
        const labelW = thumb ? 0 : fs * 6.4 + 14;               // 左边留给「没讲清」的概念名
        const top = thumb ? 12 : fs + 30, bot = thumb ? 14 : 52;  // 上面：文章名与书单条；下面：材料长度与图例
        const x0 = mx + labelW, len = W - x0 - mx - (thumb ? 0 : fs * 2);
        const s = (H - top - bot) / nC;
        L = { portrait, W, H, fs, x0, len, s, gap: Math.max(1.5, Math.min(s, len / nA) * 0.1), c0: top, top, bot, mobile };
      } else {
        const lvW = thumb ? 0 : 18, top = thumb ? 10 : 28, bot = thumb ? 10 : 34;
        const across = W - 2 * mx - lvW, s = across / nC;
        L = { portrait, W, H, fs, x0: top, len: H - top - bot, s, gap: Math.max(1.5, Math.min(s, (H - top - bot) / nA) * 0.1), c0: mx, top, bot, mobile, vTop: fs * 5.6 + 12 };
      }
    }
    // 各篇沿长轴的位置：收拢时书单外的篇压成细条，书单里的篇铺开占满
    function positions(r, cAmt, extra) {
      const first = new Set(r.lo.alts[0]), any = new Set(r.lo.alts.flat());
      const len = L.len - extra, u = len / nA;
      const thin = L.portrait ? 1.5 : Math.max(3, u * 0.14), nOther = nA - any.size, nAlt = any.size - first.size;
      const big = (len - thin * nOther) / (first.size + 0.45 * nAlt), mid = big * 0.45;
      let x = L.x0 + extra;
      return A.map((_, i) => { const w = lerp(u, first.has(i) ? big : any.has(i) ? mid : thin, cAmt), p = x; x += w; return [p, w]; });
    }
    const colOf = (k) => (k === "act" ? COL.ok : k === "unsure" ? COL.unsure : COL["cell-no"]);
    function render(si, t) {
      if (!L) return;
      const st = steps[si], k = st.k, r = R[k];
      stepK = k;
      let cAmt = 0, judged = 1, levels = r.levels.slice(), keyOp = 0, rowBad = 0, scanOp = 0, scanPos = 0, gridOp = 1, lvOp = 0, hint = 0;
      const fresh = new Set(st.kind === "rejudge" ? r.fresh : []);
      const enrichArts = new Map(st.kind === "enrich" ? r.enrich.map((x) => [x.art, x.to_level]) : []);
      if (st.kind === "intro") { judged = -1; gridOp = t; hint = ease(seg(t, 0.3, 0.7)); }
      if (st.kind === "judge0") { judged = t; scanOp = t < 0.97 ? 1 : 0; scanPos = t; hint = 1; }
      if (st.kind === "cover") { cAmt = ease(seg(t, 0.1, 0.75)); hint = 1 - seg(t, 0, 0.15); }
      if (st.kind === "enrich") { cAmt = 1 - ease(seg(t, 0, 0.25)); keyOp = seg(t, 0.25, 0.4); lvOp = seg(t, 0.2, 0.35); }
      if (st.kind === "rejudge") { cAmt = ease(seg(t, 0.5, 0.92)); lvOp = 1 - seg(t, 0.5, 0.7); }
      if (st.kind === "final") { cAmt = 1; rowBad = ease(seg(t, 0.15, 0.45)); keyOp = seg(t, 0.4, 0.6); }
      if (thumb) hint = 0;
      const extra = L.portrait && !thumb ? rowBad * L.vTop : 0; // 竖屏最后一步给竖排的概念名腾出上方空间
      const pos = positions(r, cAmt, extra);
      const uncovered = new Set(r.lo.uncovered), first = new Set(r.lo.alts[0]), any = new Set(r.lo.alts.flat());
      const keys = new Set(st.kind === "enrich" || st.kind === "final" ? r.key.map((x) => x.join("-")) : []);
      const s = L.s, gap = L.gap, fs = L.fs;
      for (let i = 0; i < nA; i++) {
        const [p, wi] = pos[i];
        const colDim = lerp(1, first.has(i) ? 1 : any.has(i) ? 0.75 : 0.5, cAmt);
        let prog = 1, from = null;
        if (st.kind === "judge0") prog = ease(seg(judged, (i / nA) * 0.88, (i / nA) * 0.88 + 0.1));
        if (judged < 0) prog = -1;
        if (st.kind === "rejudge" && fresh.has(i)) { from = R[k - 1]; prog = ease(seg(t, 0.06 + (i / nA) * 0.1, 0.4)); }
        // 竖屏收拢后，书单里每篇的格子让出上方一行写文章名
        const namePad = L.portrait && !thumb && any.has(i) ? cAmt * (fs + 8) : 0;
        for (let j = 0; j < nC; j++) {
          const kd = kindOf(r.grid[i][j]);
          let fill, op = colDim * (uncovered.has(j) && !first.has(i) ? lerp(1, 0.6, cAmt) : 1), stroke = "none", sw = 0;
          if (prog < 0) { fill = COL.card; stroke = rgb(COL.line); sw = 1; op *= gridOp; }
          else if (from) { const pk = kindOf(from.grid[i][j]); fill = mix(colOf(pk), colOf(kd), prog); op = Math.min(1, op + 0.6 * Math.sin(Math.PI * prog)); }
          else if (prog < 1) { fill = mix(COL.card, colOf(kd), prog); stroke = rgb(COL.line); sw = 1 - prog; }
          else fill = colOf(kd);
          if (keys.has(i + "-" + j)) { stroke = rgb(COL.accent); sw = (2.5 + 1.5 * Math.sin(t * Math.PI * 6)) * keyOp; }
          const g2 = gap * lerp(1, 0.3, cAmt);
          const along = Math.max(0.6, wi - g2 - namePad);
          const x = L.portrait ? L.c0 + j * s : p, y = L.portrait ? p + namePad : L.c0 + j * s;
          const ww = L.portrait ? s - gap : along, hh = L.portrait ? along : s - gap;
          at(cells[i][j], { x, y, width: ww, height: hh, rx: Math.min(4, s * 0.12), fill: rgb(fill), opacity: op, stroke, "stroke-width": sw });
        }
        // 材料长度：导言 / 摘录一 / 摘录二（只在补摘录和重判时出现）
        let lv = levels[i] + 1;
        if (enrichArts.has(i)) lv = levels[i] + 1 + ease(seg(t, 0.45, 0.8)) * (enrichArts.get(i) - levels[i]);
        lvl[i].forEach((q, d) => {
          const f = clamp(lv - d), bs = L.portrait ? 4 : clamp(wi * 0.16, 3, 9);
          const x = L.portrait ? L.c0 + nC * s + 3 + d * (bs + 1.5) : p + (wi - gap) / 2 - (3 * bs + 3) / 2 + d * (bs + 1.5);
          const y = L.portrait ? p + (wi - gap) / 2 - bs / 2 : L.c0 + nC * s + 6;
          at(q, { x, y, width: bs, height: bs, fill: rgb(f > 0 ? (enrichArts.has(i) && d >= levels[i] + 1 ? COL.accent : COL["ink-2"]) : COL.line), opacity: (thumb ? 0 : lvOp) * lerp(0.35, 1, f) });
        });
        // 书单标记：实线是书单，淡色是并列的另一种选法
        const mOp = cAmt * (first.has(i) ? 1 : any.has(i) ? 0.45 : 0);
        at(mark[i], L.portrait ? { x: L.c0 - 8, y: p, width: 5, height: Math.max(0, wi - gap), fill: rgb(COL.accent), opacity: mOp }
          : { x: p, y: L.c0 - 9, width: Math.max(0, wi - gap), height: 5, fill: rgb(COL.accent), opacity: mOp });
        // 文章名：书单收拢后才标，书单外的不标
        const nOp = thumb ? 0 : cAmt * (first.has(i) ? 1 : any.has(i) ? 0.6 : 0);
        if (!thumb) setFont(colName[i], fs);
        if (L.portrait) at(colName[i], { x: L.c0, y: p + fs * 0.95, "text-anchor": "start", opacity: nOp, fill: rgb(COL.ink) });
        else at(colName[i], { x: p + (wi - gap) / 2, y: L.c0 - 16, "text-anchor": "middle", opacity: nOp, fill: rgb(COL.ink) });
      }
      // 没有一篇讲清楚的概念：行首（竖屏是列顶）红条和名字
      C.forEach((_, j) => {
        const on = uncovered.has(j) ? rowBad : 0;
        if (!thumb) setFont(rowName[j], fs);
        if (L.portrait) {
          at(rowMark[j], { x: L.c0 + j * s, y: pos[0][0] - 8, width: s - gap, height: 5, fill: rgb(COL.bad), opacity: on });
          at(rowName[j], { x: L.c0 + j * s + (s - gap) / 2, y: pos[0][0] - 14, "text-anchor": "end", "writing-mode": "tb", opacity: thumb ? 0 : on, fill: rgb(COL.bad) });
        } else {
          at(rowMark[j], { x: L.x0 - 11, y: L.c0 + j * s, width: 5, height: s - gap, fill: rgb(COL.bad), opacity: on });
          at(rowName[j], { x: L.x0 - 18, y: L.c0 + j * s + (s - gap) / 2 + fs * 0.36, "text-anchor": "end", opacity: thumb ? 0 : on, fill: rgb(COL.bad) });
        }
      });
      // 读图提示：一列/一行代表什么（只在开头两步）
      if (!thumb) {
        setFont(hintA, fs); setFont(hintC, fs);
        if (L.portrait) {
          hintA.textContent = "每行一篇条目，每列一个概念"; hintC.textContent = "";
          at(hintA, { x: L.c0, y: L.x0 - 10, "text-anchor": "start", opacity: hint, fill: rgb(COL.muted) });
        } else {
          hintA.textContent = "每列一篇条目 →"; hintC.textContent = "每行一个概念";
          at(hintA, { x: L.x0, y: L.c0 - 14, "text-anchor": "start", opacity: hint, fill: rgb(COL.muted) });
          at(hintC, { x: L.x0 - 18, y: L.c0 + (s - gap) / 2 + fs * 0.36, "text-anchor": "end", opacity: hint, fill: rgb(COL.muted) });
        }
      }
      // 扫描条：第一轮每篇一次请求
      const sp = Math.min(nA - 1, Math.floor(scanPos * nA / 0.98));
      const [p0, w0] = pos[sp];
      at(scan, L.portrait ? { x: L.c0 - 4, y: p0 - 2, width: nC * s + 4, height: w0 + 2, fill: rgb(COL.accent), opacity: 0.22 * scanOp }
        : { x: p0 - 2, y: L.c0 - 4, width: w0 + 2, height: nC * s + 4, fill: rgb(COL.accent), opacity: 0.22 * scanOp });
    }
    function stats(si, t) {
      const st = steps[si], k = st.k, cp = c.cp, end = c.tl.length - 1;
      let idx = -1;
      if (st.kind === "judge0") idx = Math.floor(lerp(-1, cp[0] + 0.99, seg(t, 0, 0.97)));
      if (st.kind === "cover" || st.kind === "enrich") idx = cp[k];
      if (st.kind === "rejudge") idx = Math.round(lerp(cp[k - 1], cp[k], seg(t, 0.06, 0.4)));
      if (st.kind === "final") idx = end;
      const [j, u] = tlAt(c.tl, idx);
      return [[j, "次判断"], [idx + 1, "批判断请求"], [usd(u), "实际花费"]];
    }
    const LG = { intro: [], judge0: ["c_ok", "c_unsure", "c_no"], cover: ["list", "c_ok"], enrich: ["levels", "c_unsure"], rejudge: ["c_ok", "c_unsure", "c_no"], final: ["list"] };
    function showCell(ev, i, j) {
      const r = R[stepK], a = A[i], lv = r.levels[i];
      const mat = `导言：${a.lead}` + (lv >= 1 ? `\n\n正文摘录：${a.more[0]}` : "") + (lv >= 2 ? `\n\n${a.more[1]}` : "");
      openPop(ev, `${a.title} × ${short(C[j])}`,
        el("div", {}, "判断结果 ", pill(r.grid[i][j]), `　${nC} 道题合成一次请求`),
        meter(r.p[i][j], c.line), lab("读数；0.3 以下「没讲清楚」，0.7 以上「讲清楚了」"),
        lab("题"), el("div", { class: "q" }, c.questions[j]),
        lab(`材料（${["导言", "导言＋摘录一", "导言＋摘录一、二"][lv]}）`), el("div", { class: "mat" }, mat),
        el("div", { class: "lab" }, el("a", { href: a.url, target: "_blank", rel: "noopener" }, "条目原文"), ` · 修订 ${a.revid}`));
    }
    function info(ev) {
      const f = c.final, k = c.counts;
      openPop(ev, `最短书单：${c.domain}`,
        el("div", {}, `生成模型列出 ${nC} 个核心概念；语义判断模型判 ${nA} 篇英文维基百科条目各自有没有把每个概念讲清楚；集合覆盖求最少几篇。拿不准、又会改变书单的格子，给那一篇补更长的正文摘录，只重判那一篇。`),
        lab("书单为什么从 " + R[0].lo.size + " 篇变成 " + fin.lo.size + " 篇"), el("div", {}, `第 1 轮只有 ${nC - R[0].lo.uncovered.length} 个概念有条目讲清楚，${R[0].lo.size} 篇就够覆盖；补摘录重判后，${R[0].lo.uncovered.filter((j) => !R[1].lo.uncovered.includes(j)).map((j) => short(C[j])).join("、")} 也有条目讲清楚了，要覆盖的概念多了，书单跟着变长。`),
        lab("书单"), el("div", {}, f.alternatives.map((x) => x.join("、")).join("\n或：")),
        lab("没有一篇讲清楚的概念"), el("div", {}, f.uncovered.map(short).join("、")),
        f.independent ? null : lab("仍拿不准的格（蓝框）"),
        f.independent ? null : el("div", {}, R[last].key.map(([i, j]) => `${A[i].title} × ${short(C[j])}`).join("；") + "。这一格会影响选哪几篇（篇数不变），材料已补到最长，程序照实交出，不当定论。"),
        lab("账本"), el("div", {}, `${c.judgments} 道判断合成 ${c.requests} 批判断请求；报告里一共 ${c.calls} 次调用 = ${c.requests} 批判断请求 + ${k["Effect/do"] || 0} 次取材料 + ${k["Effect/gen"] || 0} 次生成。实际花费 ${usd(c.cost)}。`),
        lab("材料"), el("div", {}, `英文维基百科，${c.fetched_at.slice(0, 10)} 抓取，${c.license}。`));
    }
    return {
      steps, layout, render, info, stats, legend: (i) => LG[steps[i].kind] || [],
      aria: "概念覆盖矩阵：每列一篇条目，每行一个概念",
    };
  }

  // =====================================================================
  // 04 合租分配：筛掉不可能 → 双方打分 → 求租与拒绝 → 稳定入住
  // =====================================================================
  function scene04(svg, thumb) {
    const c = D.c04, H = c.hosts, S = c.seekers, nH = H.length, nS = S.length;
    // 挑轮数最多的一种排序来放（找房人提出）
    let run = c.runs[0];
    c.runs.forEach((x) => { if (x.seekers_propose.trace.length > run.seekers_propose.trace.length) run = x; });
    const rp = run.seekers_propose, trace = rp.trace;
    const feas = new Set(c.feasible.map(([i, j]) => i + "-" + j));
    // 不同的稳定匹配各由几种排序得到
    const tally = new Map();
    c.runs.forEach((x) => { const key = x.seekers_propose.pairs.map((p) => p.join("-")).sort().join("|"); tally.set(key, (tally.get(key) || 0) + 1); });
    const variants = [...tally.entries()].map(([key, cnt]) => ({ key, cnt, pairs: key.split("|").map((p) => p.split("-").map(Number)) }));
    const myKey = rp.pairs.map((p) => p.join("-")).sort().join("|");

    const steps = [];
    steps.push({ kind: "intro", title: `${nH} 位主租人，${nS} 位找房人`, dur: 2600 });
    steps.push({ kind: "filter", title: `预算和日期先筛掉 ${nH * nS - c.feasible.length} 对`, dur: 3600 });
    steps.push({ kind: "score", title: `${c.counts.Judge} 次判断：双方各自打分`, dur: 4200 });
    trace.forEach((tr, k) => steps.push({ kind: "round", k, title: `第 ${k + 1} 轮：${tr.proposals.length} 人求租，${tr.rejected.length} 人被拒`, dur: 3800 }));
    steps.push({ kind: "final", title: `稳定匹配：${rp.pairs.length} 对入住`, dur: 4400 });
    if (!thumb) steps.push({ kind: "variants", title: `同分时换 ${c.runs.length} 种排序，得到 ${variants.length} 种稳定匹配`, dur: 5200 });

    const gMain = sv("g"), gVar = sv("g");
    svg.append(gMain, gVar);
    const gL = sv("g"), gHold = sv("g"), gProp = sv("g"), gDots = sv("g"), gN = sv("g");
    gMain.append(gL, gHold, gProp, gDots, gN);
    const pairs = [];
    for (let i = 0; i < nH; i++) for (let j = 0; j < nS; j++) {
      const base = sv("line", { "stroke-linecap": "round" }), hold = sv("line", { "stroke-linecap": "round" }), prop = sv("line", { "stroke-linecap": "round" });
      gL.append(base); gHold.append(hold); gProp.append(prop);
      let hit = null;
      if (!thumb && feas.has(i + "-" + j)) { hit = sv("line", { class: "hit" }); hit.addEventListener("click", (e) => showPair(e, i, j)); gL.append(hit); }
      const d1 = sv("circle", {}), d2 = sv("circle", {});
      gDots.append(d1, d2);
      pairs.push({ i, j, base, hold, prop, hit, d1, d2, ok: feas.has(i + "-" + j), fi: c.feasible.findIndex(([a, b]) => a === i && b === j) });
    }
    const mkNode = (who, idx) => {
      const g = sv("g", { class: thumb ? "" : "clickable" });
      const shape = who === "h" ? sv("rect", {}) : sv("circle", {});
      const label = sv("text", { class: "lbl" }, (who === "h" ? H : S)[idx].name);
      g.append(shape, label);
      if (!thumb) g.addEventListener("click", (e) => showPerson(e, who, idx));
      gN.append(g);
      return { who, idx, shape, label };
    };
    const NH = H.map((_, i) => mkNode("h", i)), NS2 = S.map((_, j) => mkNode("s", j));
    // 变体小图
    const V = variants.map((v) => {
      const g = sv("g"), frame = sv("rect", { rx: 14 }), lines = v.pairs.map(() => sv("line", { "stroke-linecap": "round" }));
      const dotsH = H.map(() => sv("rect", { rx: 2 })), dotsS = S.map(() => sv("circle", {}));
      const cap = sv("text", { class: "lbl", "text-anchor": "middle" }, `${v.cnt} 种排序`);
      g.append(frame, ...lines, ...dotsH, ...dotsS, cap);
      gVar.append(g);
      return { v, g, frame, lines, dotsH, dotsS, cap };
    });

    let L = null;
    function layout(W, Hh) {
      const portrait = W < Hh * 0.9, mobile = W <= 720;
      const fs = thumb ? 0 : mobile ? 14 : clamp(Math.min(W, Hh) * 0.022, 15, 18);
      const top = thumb ? 12 : 10, bot = thumb ? 12 : mobile ? 34 : 44;
      const avail = Hh - top - bot, cx = W / 2, cy = top + avail / 2;
      const nr = thumb ? 7 : clamp(avail * 0.03, 11, 22);
      const nameW = fs * 3 + 14;
      let ph, ps, fp = new Map();
      const m = rp.pairs.length;
      if (!portrait) {
        const dx = thumb ? Math.min(W * 0.3, 240) : Math.min(W / 2 - nameW - nr - 40, W * 0.4);
        const gy = (avail - 2 * nr - 8) / (nH - 1);
        ph = H.map((_, i) => ({ x: cx - dx, y: cy + (i - (nH - 1) / 2) * gy }));
        ps = S.map((_, j) => ({ x: cx + dx, y: cy + (j - (nS - 1) / 2) * gy }));
        const gy2 = Math.min((avail * 0.86) / Math.max(1, m - 1), gy), half = thumb ? 36 : Math.max(70, dx * 0.22);
        rp.pairs.forEach(([i, j], q) => { const y = cy + (q - (m - 1) / 2) * gy2; fp.set("h" + i, { x: cx - half, y }); fp.set("s" + j, { x: cx + half, y }); });
      } else {
        const gx = (W - 2 * (thumb ? 20 : 30)) / (nH - 1), dy = avail * 0.36;
        ph = H.map((_, i) => ({ x: cx + (i - (nH - 1) / 2) * gx, y: cy - dy }));
        ps = S.map((_, j) => ({ x: cx + (j - (nS - 1) / 2) * gx, y: cy + dy }));
        rp.pairs.forEach(([i, j], q) => { const x = cx + (q - (m - 1) / 2) * gx; fp.set("h" + i, { x, y: cy - avail * 0.12 }); fp.set("s" + j, { x, y: cy + avail * 0.12 }); });
      }
      // 变体小图的格子
      const nv = V.length, cols = portrait ? 2 : nv, rows = Math.ceil(nv / cols);
      const cw = (W - 2 * 16) / cols, chh = Math.min(avail / rows, portrait ? cw * 1.35 : avail * 0.8);
      const vb = V.map((_, q) => ({ x: 16 + (q % cols) * cw, y: cy - (rows * chh) / 2 + Math.floor(q / cols) * chh, w: cw, h: chh }));
      L = { portrait, cx, cy, nr, ph, ps, fp, vb, fs, sc: thumb ? 0.6 : clamp(nr / 14, 1, 1.5) };
      pairs.forEach((p) => { if (p.hit) { const a = ph[p.i], b = ps[p.j]; at(p.hit, { x1: a.x, y1: a.y, x2: b.x, y2: b.y }); } });
      if (!thumb) { [...NH, ...NS2].forEach((nd) => setFont(nd.label, fs)); V.forEach((x) => setFont(x.cap, fs)); }
    }
    function toPair(p, q) { return [q, p]; } // 找房人提出：[找房人, 主租人] → [主租人, 找房人]
    function render(si, t) {
      if (!L) return;
      const st = steps[si], sc = L.sc;
      const tr = st.kind === "round" ? trace[st.k] : null, prev = st.kind === "round" && st.k > 0 ? trace[st.k - 1] : null;
      const fin = st.kind === "final" || st.kind === "variants";
      const move = st.kind === "final" ? ease(seg(t, 0.1, 0.6)) : st.kind === "variants" ? 1 : 0;
      const mainOp = st.kind === "variants" ? 1 - ease(seg(t, 0, 0.3)) : 1;
      at(gMain, { opacity: mainOp });
      const posOf = (who, idx) => {
        const b = (who === "h" ? L.ph : L.ps)[idx], f = L.fp.get(who + idx);
        return f ? { x: lerp(b.x, f.x, move), y: lerp(b.y, f.y, move) } : b;
      };
      const matched = new Set(fin ? rp.pairs.flatMap(([i, j]) => ["h" + i, "s" + j]) : []);
      const holdNow = new Set(), holdPrev = new Set();
      if (tr) tr.holds.forEach((p, q) => { if (p >= 0) holdNow.add(q + "-" + p); });
      if (prev) prev.holds.forEach((p, q) => { if (p >= 0) holdPrev.add(q + "-" + p); });
      const props = new Map();
      if (tr) tr.proposals.forEach(([p, q], x) => { const [i, j] = toPair(p, q); props.set(i + "-" + j, { rej: !holdNow.has(i + "-" + j), x }); });
      const finSet = new Set(rp.pairs.map((p) => p.join("-")));
      pairs.forEach((p) => {
        const a = posOf("h", p.i), b = posOf("s", p.j), key = p.i + "-" + p.j;
        // 底线：可行的对
        let op = 0, col = COL.line, w = 1.3;
        if (st.kind === "intro") op = 0;
        else if (st.kind === "filter") {
          op = 0.9 * ease(seg(t, 0, 0.3));
          if (!p.ok) { col = mix(COL.line, COL.bad, seg(t, 0.42, 0.55)); op *= 1 - ease(seg(t, 0.6, 0.9)); w = lerp(1.3, 2.4, seg(t, 0.42, 0.55)); }
        } else op = p.ok ? (fin ? 0.9 * (1 - move) : 0.9) : 0;
        at(p.base, { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: rgb(col), "stroke-width": w * sc, "stroke-opacity": op });
        // 打分：两端各一次判断，小点从两头走向中间
        if (st.kind === "score" && p.ok) {
          const s0 = (p.fi / c.feasible.length) * 0.6, m = seg(t, s0, s0 + 0.32), mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2, o = Math.sin(Math.PI * m);
          at(p.d1, { cx: lerp(a.x, mx, m), cy: lerp(a.y, my, m), r: 4 * sc, fill: rgb(COL.accent), opacity: o });
          at(p.d2, { cx: lerp(b.x, mx, m), cy: lerp(b.y, my, m), r: 4 * sc, fill: rgb(COL.accent), opacity: o });
        } else { at(p.d1, { opacity: 0 }); at(p.d2, { opacity: 0 }); }
        // 暂时留下的对（上一轮留下的先在，被顶掉的变红消失）
        let hOp = 0, hCol = COL.ink, hw = 3.2;
        if (tr) {
          if (holdPrev.has(key) && holdNow.has(key)) hOp = 0.85;
          else if (holdPrev.has(key)) { hOp = 0.85 * (1 - ease(seg(t, 0.62, 0.9))); hCol = mix(COL.ink, COL.bad, seg(t, 0.5, 0.62)); }
          else if (holdNow.has(key) && props.has(key)) hOp = 0.85 * ease(seg(t, 0.55, 0.75));
        }
        if (fin) { hOp = finSet.has(key) ? 1 : 0; hCol = mix(COL.ink, COL.ok, move); hw = lerp(3.2, 5, move); }
        at(p.hold, { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: rgb(hCol), "stroke-width": hw * sc, "stroke-opacity": hOp });
        // 求租：从找房人一端伸向主租人，被拒的变红缩回
        const pr = props.get(key);
        if (pr) {
          const s0 = pr.x * 0.04, g0 = ease(seg(t, s0, s0 + 0.32));
          let g = g0, pc = COL.accent, po = 1;
          if (pr.rej) { pc = mix(COL.accent, COL.bad, seg(t, 0.45, 0.55)); g = g0 * (1 - ease(seg(t, 0.62, 0.9))); }
          else po = 1 - seg(t, 0.62, 0.78);
          at(p.prop, { x1: b.x, y1: b.y, x2: lerp(b.x, a.x, g), y2: lerp(b.y, a.y, g), stroke: rgb(pc), "stroke-width": 3 * sc, "stroke-opacity": g > 0.01 ? po : 0, "stroke-dasharray": `${8 * sc} ${5 * sc}` });
        } else at(p.prop, { "stroke-opacity": 0 });
      });
      // 人：名字从第一步起就在
      const held = new Set();
      if (tr) tr.holds.forEach((p, q) => { if (p >= 0 && t > 0.6) { held.add("h" + q); held.add("s" + p); } });
      if (prev && (!tr || t <= 0.6)) prev.holds.forEach((p, q) => { if (p >= 0) { held.add("h" + q); held.add("s" + p); } });
      const rejNow = new Set(tr && t > 0.5 ? tr.rejected.map((p) => "s" + p) : []);
      [...NH, ...NS2].forEach((nd, q) => {
        const key = nd.who + nd.idx, p0 = posOf(nd.who, nd.idx);
        const sIn = st.kind === "intro" ? ease(seg(t, (q / 12) * 0.7, (q / 12) * 0.7 + 0.3)) : 1;
        const r = L.nr * sIn;
        const on = held.has(key) || matched.has(key);
        const fade = fin && !matched.has(key) ? lerp(1, 0.3, move) : 1;
        const fill = rgb(on ? (fin ? mix(COL.ink, COL.ok, move) : COL.ink) : COL.card);
        const stroke = rgb(rejNow.has(key) ? COL.bad : COL.ink), sw = (rejNow.has(key) ? 3.4 : 2) * sc;
        if (nd.who === "h") at(nd.shape, { x: p0.x - r, y: p0.y - r, width: 2 * r, height: 2 * r, rx: r * 0.3, fill, stroke, "stroke-width": sw, opacity: fade });
        else at(nd.shape, { cx: p0.x, cy: p0.y, r, fill, stroke, "stroke-width": sw, opacity: fade });
        const lo = thumb ? 0 : sIn * fade;
        const lcol = rgb(rejNow.has(key) ? COL.bad : COL.ink);
        if (L.portrait) at(nd.label, { x: p0.x, y: nd.who === "h" ? p0.y - L.nr - 8 : p0.y + L.nr + L.fs + 4, "text-anchor": "middle", opacity: lo, fill: lcol });
        else at(nd.label, { x: nd.who === "h" ? p0.x - L.nr - 10 : p0.x + L.nr + 10, y: p0.y + L.fs * 0.36, "text-anchor": nd.who === "h" ? "end" : "start", opacity: lo, fill: lcol });
      });
      // 变体：每种稳定匹配一张小图，下面写有几种排序得到它；刚才播放的那种描蓝框
      const vOp = st.kind === "variants" ? ease(seg(t, 0.25, 0.6)) : 0;
      at(gVar, { opacity: vOp, display: vOp > 0 ? "inline" : "none" });
      if (vOp > 0) V.forEach((x, q) => {
        const b = L.vb[q], pad = Math.min(b.w, b.h) * 0.14, x1 = b.x + b.w * 0.3, x2 = b.x + b.w * 0.7, capH = L.fs + 16;
        const yy = (k) => b.y + pad + (k * (b.h - 2 * pad - capH)) / (nH - 1), rr = Math.max(4, Math.min(b.w, b.h) * 0.03);
        const mine = x.v.key === myKey;
        at(x.frame, { x: b.x + 8, y: b.y + 4, width: b.w - 16, height: b.h - 8, fill: "none", stroke: rgb(mine ? COL.accent : COL.line), "stroke-width": mine ? 2.5 : 1.2 });
        x.lines.forEach((ln, m) => { const [i, j] = x.v.pairs[m]; const g = ease(seg(t, 0.35 + q * 0.06, 0.65 + q * 0.06)); at(ln, { x1: x1, y1: yy(i), x2: lerp(x1, x2, g), y2: lerp(yy(i), yy(j), g), stroke: rgb(COL.ok), "stroke-width": 3.5 }); });
        x.dotsH.forEach((d, i) => at(d, { x: x1 - rr, y: yy(i) - rr, width: 2 * rr, height: 2 * rr, fill: rgb(COL.card), stroke: rgb(COL.ink), "stroke-width": 1.6 }));
        x.dotsS.forEach((d, j) => at(d, { cx: x2, cy: yy(j), r: rr, fill: rgb(COL.card), stroke: rgb(COL.ink), "stroke-width": 1.6 }));
        at(x.cap, { x: b.x + b.w / 2, y: b.y + b.h - pad * 0.6 - 4, fill: rgb(mine ? COL.accent : COL.ink) });
      });
    }
    function stats(si, t) {
      const st = steps[si], end = c.tl.length - 1;
      let idx = -1, round = 0;
      if (st.kind === "score") idx = Math.floor(lerp(-1, end + 0.99, seg(t, 0, 0.92)));
      if (st.kind === "round") { idx = end; round = st.k + 1; }
      if (st.kind === "final" || st.kind === "variants") { idx = end; round = trace.length; }
      const [j, u] = tlAt(c.tl, idx);
      return [[j, "次判断"], [round, "轮求租"], [usd(u), "实际花费"]];
    }
    const LG = { intro: ["host", "seeker"], filter: ["cut"], score: ["score"], round: ["propose", "reject", "hold"], final: ["moved"], variants: [] };
    const exp = (v) => v.reduce((a, p, k) => a + p * k, 0);
    const sc2 = (i, j) => c.scores.find((x) => x.h === i && x.s === j);
    function showPerson(ev, who, idx) {
      const Pp = who === "h" ? H[idx] : S[idx], tiers = who === "h" ? c.host_tiers[idx] : c.seeker_tiers[idx], other = who === "h" ? S : H;
      const feasO = c.feasible.filter(([i, j]) => (who === "h" ? i : j) === idx).map(([i, j]) => (who === "h" ? j : i));
      const out = other.map((_, k) => k).filter((k) => !feasO.includes(k));
      openPop(ev, Pp.name + (who === "h" ? "（主租人）" : "（找房人）"),
        el("div", { class: "mat" }, Pp.desc),
        el("div", { class: "lab" }, who === "h" ? `出租 ${Pp.rent} 元，${Pp.available} 起可住` : `预算 ${Pp.budget} 元，${Pp.move_in_by} 前入住`),
        lab("偏好表（打分题五档的期望档位 0–4；只取名次，差在 " + c.tie + " 以内算同分，并成一档）"),
        el("div", { class: "tiers" }, tiers.map((tier, k) => el("div", {}, el("b", {}, `第 ${k + 1} 档　`),
          tier.map((o) => { const x = who === "h" ? sc2(idx, o) : sc2(o, idx); return `${other[o].name}（${fmt(exp(who === "h" ? x.host : x.seeker))}）`; }).join("、")))),
        out.length ? lab("代码按预算和日期先筛掉：" + out.map((k) => other[k].name).join("、")) : null);
    }
    function showPair(ev, i, j) {
      const x = sc2(i, j);
      openPop(ev, `${H[i].name} — ${S[j].name}`,
        lab(`站在 ${H[i].name} 的角度`), bars(c.scale.map((t, k) => ({ label: t, v: x.host[k] }))),
        lab(`站在 ${S[j].name} 的角度`), bars(c.scale.map((t, k) => ({ label: t, v: x.seeker[k] }))),
        lab("题（主租人一方）"), el("div", { class: "q" }, c.questions.host),
        lab("题（找房人一方）"), el("div", { class: "q" }, c.questions.seeker));
    }
    function info(ev) {
      const lines = variants.map((v) => `${v.cnt} 种排序 → ` + v.pairs.map(([i, j]) => `${H[i].name}—${S[j].name}`).join("，"));
      openPop(ev, "合租分配",
        el("div", {}, "代码先按预算和日期筛掉不可能的对；语义判断模型站在每个人的角度给「和对方合住满不满意」打分；程序只取名次排出偏好表，再用 Gale–Shapley 一轮轮求租、拒绝，得到稳定匹配。偏好表里同分的人谁先谁后，用 " + c.runs.length + " 个种子各排一次、各跑一次。"),
        lab("这里播放的是"), el("div", {}, `第 ${run.seed + 1} 种排序（种子 ${run.seed}），找房人提出。` + (c.summary.same_both_ways ? "每种排序下，主租人提出的结果也一样。" : "")),
        lab("各种排序的结果"), el("div", {}, lines.join("\n")),
        lab("账本"), el("div", {}, `判断 ${c.counts.Judge} 次，实际花费 ${usd(c.cost)}；没有调用生成模型。程序核对过：${c.summary.any_blocking ? "存在" : "没有"}一对人彼此都更想和对方住。`));
    }
    return {
      steps, layout, render, info, stats, legend: (i) => LG[steps[i].kind] || [],
      aria: "稳定匹配过程：左边主租人，右边找房人",
    };
  }

  // =====================================================================
  // 02 杭州聚餐（只做首页缩略图）：真实餐厅坐标，一层层筛掉，最后选中的一家放大亮起，其余淡掉
  // =====================================================================
  function scene02(svg) {
    const c = D.c02;
    const steps = [{ title: "", dur: 1000 }];
    if (!c) return { steps, layout() {}, render() {} };
    const lake = sv("path", {}), gP = sv("g"), gH = sv("g"), gTop = sv("g");
    svg.append(lake, gH, gP, gTop);
    const dots = c.points.map(() => { const d = sv("circle", {}); gP.append(d); return d; });
    const homes = c.homes.map(() => { const l = sv("line", {}), r = sv("rect", { rx: 1.5 }); gH.append(l, r); return { l, r }; });
    const halo = sv("circle", {}), star = sv("circle", {});
    gTop.append(halo, star);
    const nL = c.layers.length, top = c.points.find((p) => p[2] === nL);
    let L = null;
    function layout(W, H) {
      const all = c.points.filter((p) => p[2] >= 4).concat(c.homes.map((h) => [h[0], h[1]]));
      const lat0 = all.reduce((a, p) => a + p[0], 0) / all.length, k = Math.cos((lat0 * Math.PI) / 180);
      const xs = all.map((p) => p[1] * k), ys = all.map((p) => -p[0]);
      const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
      const s = Math.min((W - 60) / (x1 - x0), (H - 50) / (y1 - y0));
      const ox = (W - (x1 - x0) * s) / 2, oy = (H - (y1 - y0) * s) / 2;
      L = { f: (lat, lon) => [ox + (lon * k - x0) * s, oy + (-lat - y0) * s] };
      at(lake, { d: "M" + c.lake.map(([a, b]) => L.f(a, b).join(" ")).join("L") + "Z" });
    }
    function render(_, t) {
      if (!L) return;
      at(lake, { fill: rgb(COL.accent), "fill-opacity": 0.12, stroke: "none" });
      // 0–0.12 出现；之后每层一个时段；最后选中的店放大亮起，其余淡掉，五个人的家连过去
      const appear = ease(seg(t, 0, 0.12)), stage = seg(t, 0.14, 0.78) * nL, glow = ease(seg(t, 0.78, 0.92));
      c.points.forEach((p, q) => {
        const [x, y] = L.f(p[0], p[1]);
        const alive = p[2] >= stage ? 1 : clamp(1 - (stage - p[2]) * 1.4);
        if (p === top) { at(dots[q], { opacity: 0 }); return; }
        at(dots[q], { cx: x, cy: y, r: p[2] >= nL - 1 ? 2.4 : 1.7, fill: rgb(p[2] >= nL - 1 && stage > nL - 1.5 ? COL.unsure : COL["ink-2"]), opacity: appear * lerp(0.1, 0.85, alive) * (1 - 0.85 * glow) });
      });
      if (top) {
        const [x, y] = L.f(top[0], top[1]), pulse = glow >= 1 ? 0.5 + 0.5 * Math.sin(t * Math.PI * 10) : 0;
        at(star, { cx: x, cy: y, r: lerp(2.4, 9, glow), fill: rgb(glow > 0 ? mix(COL.unsure, COL.ok, glow) : COL["ink-2"]), stroke: rgb(COL.paper), "stroke-width": 2 * glow, opacity: appear });
        at(halo, { cx: x, cy: y, r: 9 + 10 * glow + 4 * pulse, fill: rgb(COL.ok), "fill-opacity": 0.18 * glow, stroke: rgb(COL.ok), "stroke-opacity": 0.6 * glow, "stroke-width": 1.5 });
      }
      c.homes.forEach((h, q) => {
        const [x, y] = L.f(h[0], h[1]), [tx, ty] = top ? L.f(top[0], top[1]) : [x, y];
        at(homes[q].r, { x: x - 4.5, y: y - 4.5, width: 9, height: 9, fill: rgb(COL.accent), opacity: appear });
        at(homes[q].l, { x1: x, y1: y, x2: lerp(x, tx, glow), y2: lerp(y, ty, glow), stroke: rgb(COL.accent), "stroke-width": 1.6, "stroke-opacity": glow > 0 ? 0.75 : 0, "stroke-dasharray": "4 3" });
      });
    }
    return { steps, layout, render };
  }


  // =====================================================================
  // 05 通爻网络（只做首页缩略图）：一句话从中心发出，信号一波波传开，最后几个角色连成环
  // =====================================================================
  function scene05(svg) {
    const N = 46, steps = [{ title: "", dur: 1000 }];
    const gE = sv("g"), gN = sv("g"), gR = sv("g");
    svg.append(gE, gN, gR);
    // 固定的伪随机布点（与数据无关，只是缩略画面）
    let s = 7; const rnd = () => ((s = (s * 16807) % 2147483647) / 2147483647);
    const pts = Array.from({ length: N }, (_, i) => ({ a: rnd() * Math.PI * 2, r: 0.18 + 0.8 * Math.sqrt(rnd()), wave: 0 }));
    pts.forEach((p) => { p.wave = p.r < 0.45 ? 1 : p.r < 0.75 ? 2 : 3; });
    const ring = [3, 11, 19, 27].map((i) => pts[i]);
    const dots = pts.map(() => { const d = sv("circle", {}); gN.append(d); return d; });
    const rays = pts.map(() => { const l = sv("line", {}); gE.append(l); return l; });
    const center = sv("circle", {}), ringPath = sv("path", {});
    gR.append(ringPath, center);
    let L = null;
    function layout(W, H) { L = { cx: W / 2, cy: H / 2, R: Math.min(W, H) * 0.44 }; }
    const pos = (p) => [L.cx + Math.cos(p.a) * p.r * L.R * 1.25, L.cy + Math.sin(p.a) * p.r * L.R];
    function render(_, t) {
      if (!L) return;
      at(center, { cx: L.cx, cy: L.cy, r: 4 + 2 * Math.sin(t * Math.PI * 6), fill: rgb(COL.accent), opacity: 0.95 });
      const front = seg(t, 0.05, 0.6) * 3.2, close = ease(seg(t, 0.62, 0.85));
      pts.forEach((p, i) => {
        const [x, y] = pos(p), hit = clamp(front - p.wave + 1), inRing = ring.includes(p);
        at(rays[i], { x1: L.cx, y1: L.cy, x2: lerp(L.cx, x, hit), y2: lerp(L.cy, y, hit), stroke: rgb(COL.accent), "stroke-opacity": 0.18 * (1 - close), "stroke-width": 1 });
        const lit = inRing ? mix(COL.unsure, COL.ok, close) : COL["ink-2"];
        at(dots[i], { cx: x, cy: y, r: inRing ? lerp(2.4, 5.5, close) : 2.1, fill: rgb(hit > 0.9 ? lit : COL["ink-2"]), opacity: inRing ? 0.95 : lerp(0.35, 0.85, hit) * (1 - 0.6 * close) });
      });
      const rp = ring.map(pos);
      const d = "M" + rp.map((q) => q.join(" ")).join("L") + "Z";
      at(ringPath, { d, fill: rgb(COL.ok), "fill-opacity": 0.08 * close, stroke: rgb(COL.ok), "stroke-width": 1.8, "stroke-opacity": close, "stroke-dasharray": `${600 * close} 600` });
    }
    return { steps, layout, render };
  }

  // ---------- 页面 ----------
  function fit(stage, scene, after) {
    const ro = new ResizeObserver(() => { const r = stage.getBoundingClientRect(); if (r.width && r.height) { scene.layout(r.width, r.height); after && after(); } });
    ro.observe(stage);
    return ro;
  }
  function home() {
    const cards = [
      { href: "#/01", no: "01", t: "谁在说谎", s: "八份证词两两对质", mk: (g) => scene01(g, "B", true) },
      { href: "./dinner/", no: "02", t: "杭州聚餐", s: D.c02 ? `${D.c02.points.length} 家餐厅筛到一家` : "餐厅筛到一家", mk: (g) => scene02(g) },
      { href: "#/03", no: "03", t: "最短书单", s: `${D.c03.articles.length} 篇条目收成书单`, mk: (g) => scene03(g, true) },
      { href: "#/04", no: "04", t: "合租分配", s: `${D.c04.hosts.length + D.c04.seekers.length} 人配成稳定合租`, mk: (g) => scene04(g, true) },
      { href: "./towow-net/", no: "05", t: "通爻网络", s: "一句话长出多方方案", tag: "假结果", mk: (g) => scene05(g) },
    ];
    const grid = el("div", { class: "home" });
    main.append(grid);
    const loops = [], obs = [];
    cards.forEach((cd) => {
      const svg = sv("svg", { "aria-hidden": "true" });
      const thumb = el("div", { class: "thumb" }, svg);
      grid.append(el("a", { class: "card", href: cd.href }, thumb, el("p", {}, el("span", { class: "no" }, cd.no), el("b", {}, cd.t), el("span", { class: "sub" }, cd.s), cd.tag ? el("span", { class: "sub", style: "font-size:12px;opacity:.6;margin-left:6px" }, cd.tag) : "")));
      const sc = cd.mk(svg);
      const total = cd.no === "02" || cd.no === "05" ? 9000 : 10000;
      const loop = Loop(cd.no === "02" || cd.no === "05" ? { steps: [{ dur: 1 }], render: (_, t) => sc.render(0, t) } : sc, total);
      loops.push(loop);
      obs.push(fit(thumb, sc, loop.redraw));
    });
    current = { destroy: () => { loops.forEach((l) => l.destroy()); obs.forEach((o) => o.disconnect()); } };
  }
  function casePage(key, params) {
    const svg = sv("svg", {});
    let scene;
    const cid = ["A", "B", "C"].includes(params.c) ? params.c : "B";
    if (key === "01") scene = scene01(svg, cid, false);
    if (key === "03") scene = scene03(svg, false);
    if (key === "04") scene = scene04(svg, false);
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", scene.aria);
    const h = el("h1", { class: "headline", "aria-live": "polite" });
    const infoBtn = el("button", { class: "icon-btn", type: "button", "aria-label": "说明", title: "说明", html: ICON.info, onclick: (e) => scene.info(e) });
    const statEls = scene.stats(0, 0).map(() => { const b = el("b"), s = el("span"); return { box: el("div", { class: "stat" }, b, s), b, s, v: null, l: null }; });
    const stats = el("div", { class: "stats" }, statEls.map((x) => x.box));
    const legend = el("div", { class: "legend", "aria-hidden": "true" });
    const stage = el("div", { class: "stage" }, svg, legend);
    const bar = el("div", { class: "bar" });
    if (key === "01") bar.append(el("div", { class: "tabs", role: "tablist" }, ["A", "B", "C"].map((k) => el("button", {
      class: k === cid ? "on" : "", type: "button", title: D.c01.cases[k].title, "aria-label": "案子 " + k + "：" + D.c01.cases[k].title,
      onclick: () => { location.hash = "#/01?c=" + k; } }, k))));
    main.append(el("div", { class: "case" }, el("div", { class: "band" }, el("div", { class: "head" }, h, infoBtn), stats), stage, bar));
    let timer = null;
    const player = Player(scene.steps, {
      render: (i, t) => {
        scene.render(i, t);
        scene.stats(i, t).forEach(([v, l], q) => {
          const x = statEls[q];
          if (x.v !== v) { x.b.textContent = v; x.v = v; }
          if (x.l !== l) { x.s.textContent = l; x.l = l; }
        });
      },
      onStep: (i) => {
        legend.replaceChildren(...scene.legend(i).map(legendItem));
        const t = scene.steps[i].title, parts = t.split("，");
        const put = () => h.replaceChildren(...parts.map((x, q) => el("span", { class: "cl" }, x + (q < parts.length - 1 ? "，" : ""))));
        if (REDUCE || !h.textContent) { put(); return; }
        h.classList.add("fade"); clearTimeout(timer);
        timer = setTimeout(() => { put(); h.classList.remove("fade"); }, 180);
      },
    }, bar);
    const ro = fit(stage, scene, () => player.redraw());
    if (params.s != null) player.jump(+params.s, params.t != null ? +params.t : 0, params.p != null);
    else player.play();
    current = { player, destroy: () => { player.destroy(); ro.disconnect(); clearTimeout(timer); } };
  }

  function route() {
    closePop();
    if (current) current.destroy();
    current = null;
    const raw = (location.hash || "#/").replace(/^#\/?/, "");
    const [key, qs] = raw.split("?");
    const params = Object.fromEntries(new URLSearchParams(qs || ""));
    document.querySelectorAll(".top nav a").forEach((a) => a.classList.toggle("on", a.dataset.r === key));
    main.innerHTML = "";
    const isCase = ["01", "03", "04"].includes(key);
    document.body.classList.toggle("is-case", isCase);
    if (isCase) casePage(key, params);
    else home();
    window.scrollTo(0, 0);
  }
  window.addEventListener("hashchange", route);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") return closePop();
    const p = current && current.player;
    if (!p || e.target.closest("input,textarea")) return;
    if (e.key === " ") { e.preventDefault(); p.toggle(); }
    if (e.key === "ArrowRight") p.step(1);
    if (e.key === "ArrowLeft") p.step(-1);
  });
  document.getElementById("theme").addEventListener("click", () => {
    const r = document.documentElement;
    const dark = r.dataset.theme ? r.dataset.theme === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
    r.dataset.theme = dark ? "light" : "dark";
    readColors();
    if (current && current.player) current.player.redraw();
  });
  if (window.matchMedia) window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { readColors(); if (current && current.player) current.player.redraw(); });
  route();
})();

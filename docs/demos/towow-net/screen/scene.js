// 3D 网络：节点、关系、构型簇、信号粒子、冲击波。
// 结构状态（节点位置、关系线、构型）每帧从 store 读；瞬时效果（信号、闪光）由事件触发，
// 并经 EffectQueue 限流：事件再多，每帧只渲染一部分，计数不受影响。

import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js';
import { exitClass } from '../shared/store.js';

export const COLORS = {
  act: '#3ef0b0',
  ignore: '#42506b',
  unsure: '#ffb547',
  pick: '#b58cff',
  unknown: '#8899aa',
  direct: '#57d4ff',
  relay: '#c38bff',
  relUnsure: '#ffb547',
  config: '#ffd27a',
  coarse: '#7aa7ff',
  fine: '#bfe3ff',
  trace: '#ffcf6b',
  enrich: '#7fdcff',
  intent: '#ffffff',
  live: '#ff7ad9',
  yes: '#3ef0b0',
  edit: '#ffb547',
  no: '#ff5b6e',
};

// 行业调色：按出现顺序分配，超过的归到「其他」
const INDUSTRY_PALETTE = ['#4cc9f0', '#f72585', '#7bd389', '#ffb703', '#b388eb', '#ff8c61', '#43aa8b',
  '#f9c74f', '#90e0ef', '#e5989b', '#a0c4ff', '#caffbf', '#ffadad', '#9bf6ff', '#fdffb6', '#bdb2ff'];
const KIND_COLORS = { person: '#4cc9f0', company: '#ffb703', org: '#b388eb', unknown: '#8899aa', ghost: '#cfd8e3' };

const CAP_NODES = 4096;
const CAP_PARTICLES = 6000;
const CAP_REL_SEG = 12000;
const CAP_CFG_SEG = 12000;
const CAP_HALOS = 1024;

function hashStr(s) {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}

function fibDir(i, n) {
  const g = Math.PI * (3 - Math.sqrt(5));
  const y = 1 - (2 * (i + 0.5)) / n;
  const r = Math.sqrt(1 - y * y);
  return new THREE.Vector3(Math.cos(g * i) * r, y, Math.sin(g * i) * r);
}

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => { s ^= s << 13; s >>>= 0; s ^= s >> 17; s ^= s << 5; s >>>= 0; return s / 4294967296; };
}

const glowVert = `
attribute float size; attribute vec3 color; attribute float flash; attribute vec3 fcolor; attribute float alpha;
uniform float uScale;
varying vec3 vColor; varying float vAlpha; varying float vFlash; varying vec3 vF;
void main(){
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mv;
  float s = size * (1.0 + flash * 1.4);
  gl_PointSize = clamp(s * uScale / max(1.0, -mv.z), 0.0, 256.0);
  vColor = color; vAlpha = alpha; vFlash = flash; vF = fcolor;
}`;
const glowFrag = `
varying vec3 vColor; varying float vAlpha; varying float vFlash; varying vec3 vF;
void main(){
  vec2 c = gl_PointCoord - 0.5; float d = length(c);
  if (d > 0.5) discard;
  float core = pow(smoothstep(0.5, 0.0, d), 3.2);
  float halo = exp(-d * d * 22.0) * smoothstep(0.5, 0.3, d);
  float hot = smoothstep(0.12, 0.0, d);
  vec3 col = mix(vColor, vF, clamp(vFlash, 0.0, 1.0));
  vec3 outc = col * (core * 1.25 + halo * (0.32 + 0.35 * vFlash)) + vec3(1.0) * hot * (0.3 + 0.7 * vFlash);
  gl_FragColor = vec4(outc * vAlpha, 1.0);
}`;

function makeGlowMaterial(scale) {
  return new THREE.ShaderMaterial({
    uniforms: { uScale: { value: scale } },
    vertexShader: glowVert,
    fragmentShader: glowFrag,
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  });
}

function makePointsBuffer(cap) {
  const g = new THREE.BufferGeometry();
  const attrs = {
    position: new THREE.BufferAttribute(new Float32Array(cap * 3), 3),
    color: new THREE.BufferAttribute(new Float32Array(cap * 3), 3),
    fcolor: new THREE.BufferAttribute(new Float32Array(cap * 3), 3),
    size: new THREE.BufferAttribute(new Float32Array(cap), 1),
    flash: new THREE.BufferAttribute(new Float32Array(cap), 1),
    alpha: new THREE.BufferAttribute(new Float32Array(cap), 1),
  };
  for (const [k, a] of Object.entries(attrs)) { a.setUsage(THREE.DynamicDrawUsage); g.setAttribute(k, a); }
  g.setDrawRange(0, 0);
  g.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 1e4);
  return { g, attrs };
}

function makeLineBuffer(cap) {
  const g = new THREE.BufferGeometry();
  const pos = new THREE.BufferAttribute(new Float32Array(cap * 2 * 3), 3);
  const col = new THREE.BufferAttribute(new Float32Array(cap * 2 * 4), 4);
  pos.setUsage(THREE.DynamicDrawUsage); col.setUsage(THREE.DynamicDrawUsage);
  g.setAttribute('position', pos); g.setAttribute('color', col);
  g.setDrawRange(0, 0);
  g.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 1e4);
  const m = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending });
  return { g, pos, col, mesh: new THREE.LineSegments(g, m), n: 0, cap };
}

const tmpC = new THREE.Color();

export class NetScene {
  constructor(canvas, store, opts = {}) {
    this.store = store;
    this.canvas = canvas;
    this.opts = opts;
    this.colorBy = opts.colorBy || 'industry';
    this.speedHint = 1;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: false, powerPreference: 'high-performance' });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.setClearColor(0x04060c, 1);
    this.scene = new THREE.Scene();
    // 背景用 scene.background：直接 setClearColor 在后期处理链里会被按 sRGB 编码两次，整屏发灰
    this.scene.background = new THREE.Color(0x04060c);
    this.scene.fog = new THREE.FogExp2(0x04060c, 0.0022);
    this.camera = new THREE.PerspectiveCamera(50, 1, 0.5, 4000);
    this.camera.position.set(0, 38, 205);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.06;
    this.controls.autoRotate = true;
    this.controls.autoRotateSpeed = 0.22;
    this.controls.minDistance = 40;
    this.controls.maxDistance = 600;

    // 泛光默认关：部分环境里 UnrealBloomPass 的低层 mip 会出现方块光斑；点精灵自带两层光晕
    this.bloomOn = opts.bloom === true;
    this.composer = new EffectComposer(this.renderer);
    this.composer.addPass(new RenderPass(this.scene, this.camera));
    this.bloom = new UnrealBloomPass(new THREE.Vector2(512, 512), 0.9, 0.3, 0.35);
    this.composer.addPass(this.bloom);
    this.composer.addPass(new OutputPass());

    // 背景：极淡的星尘，给空间纵深
    this.addDust();

    // 节点
    this.nodeIndex = new Map(); // id -> slot
    this.slots = []; // slot -> {id, ghost}
    this.nodeBuf = makePointsBuffer(CAP_NODES);
    this.nodeMat = makeGlowMaterial(900);
    this.nodePoints = new THREE.Points(this.nodeBuf.g, this.nodeMat);
    this.scene.add(this.nodePoints);
    this.pos = new Float32Array(CAP_NODES * 3);
    this.home = new Float32Array(CAP_NODES * 3);
    this.target = new Float32Array(CAP_NODES * 3);
    this.flash = new Float32Array(CAP_NODES);
    this.flashCol = new Float32Array(CAP_NODES * 3);
    this.alphaCur = new Float32Array(CAP_NODES);
    this.boost = new Float32Array(CAP_NODES); // 入场、选中时放大
    this.industryColor = new Map();

    // 关系线、构型线、环线
    this.rel = makeLineBuffer(CAP_REL_SEG);
    this.rel.mesh.material.opacity = 0.9;
    this.scene.add(this.rel.mesh);
    this.cfg = makeLineBuffer(CAP_CFG_SEG);
    this.scene.add(this.cfg.mesh);

    // 构型光晕
    this.haloBuf = makePointsBuffer(CAP_HALOS);
    this.haloMat = makeGlowMaterial(900);
    this.scene.add(new THREE.Points(this.haloBuf.g, this.haloMat));
    this.cfgPulse = new Map(); // cfg id -> pulse strength
    this.cfgCentroid = new Map();

    // 粒子
    this.parts = [];
    this.partBuf = makePointsBuffer(CAP_PARTICLES);
    this.partMat = makeGlowMaterial(900);
    this.scene.add(new THREE.Points(this.partBuf.g, this.partMat));

    // 冲击波环
    this.waves = [];
    const ringGeo = new THREE.RingGeometry(0.92, 1.0, 64);
    for (let i = 0; i < 24; i++) {
      const m = new THREE.Mesh(ringGeo, new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0, side: THREE.DoubleSide, depthWrite: false, blending: THREE.AdditiveBlending }));
      m.visible = false;
      this.scene.add(m);
      this.waves.push({ mesh: m, t0: 0, dur: 1, max: 10, busy: false });
    }
    // 选中标记
    this.selMarker = new THREE.Mesh(new THREE.RingGeometry(0.85, 1.0, 48), new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.85, side: THREE.DoubleSide, depthWrite: false }));
    this.selMarker.visible = false;
    this.scene.add(this.selMarker);
    this.selected = null;

    // 抽样统计
    this.queue = [];
    this.qHead = 0;
    this.maxEffectsPerFrame = opts.maxEffectsPerFrame || 90;
    this.maxQueue = 3000; // 积压上限：超过就丢最旧的瞬时效果
    this.stats = { effectsIn: 0, effectsDrawn: 0, effectsDropped: 0 };

    this.clock = performance.now();
    this.time = 0;
    this.resize();
    window.addEventListener('resize', () => this.resize());
    store.on((ev, ctx) => this.onEvent(ev, ctx));
  }

  addDust() {
    const n = 1400;
    const g = new THREE.BufferGeometry();
    const p = new Float32Array(n * 3);
    const r = rng(42);
    for (let i = 0; i < n; i++) {
      const d = fibDir(i, n).multiplyScalar(260 + r() * 500);
      p.set([d.x, d.y, d.z], i * 3);
    }
    g.setAttribute('position', new THREE.BufferAttribute(p, 3));
    const m = new THREE.PointsMaterial({ color: 0x33415c, size: 1.2, sizeAttenuation: true, transparent: true, opacity: 0.55, depthWrite: false });
    this.scene.add(new THREE.Points(g, m));
  }

  resize() {
    const w = this.canvas.clientWidth || window.innerWidth;
    const h = this.canvas.clientHeight || window.innerHeight;
    this.renderer.setSize(w, h, false);
    this.composer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    const pr = this.renderer.getPixelRatio();
    const s = (h / 1080) * pr * 620;
    this.nodeMat.uniforms.uScale.value = s;
    this.haloMat.uniforms.uScale.value = s;
    this.partMat.uniforms.uScale.value = s;
    this.viewW = w; this.viewH = h;
  }

  // ---------- 节点 ----------
  industryOf(node) {
    return (node && node.tags && node.tags[0]) || '未标行业';
  }

  colorFor(node) {
    if (!node) return KIND_COLORS.unknown;
    if (node.ghost) return KIND_COLORS.ghost;
    if (this.colorBy === 'kind') return KIND_COLORS[node.kind] || KIND_COLORS.unknown;
    const ind = this.industryOf(node);
    if (!this.industryColor.has(ind)) {
      const i = this.industryColor.size;
      // 行业超过调色板长度时，按黄金角取色相，保持可区分
      this.industryColor.set(ind, i < INDUSTRY_PALETTE.length ? INDUSTRY_PALETTE[i] : `hsl(${Math.round((i * 137.5) % 360)}, 62%, 66%)`);
    }
    return this.industryColor.get(ind);
  }

  legend() {
    if (this.colorBy === 'kind') return [['个人', KIND_COLORS.person], ['公司', KIND_COLORS.company], ['机构', KIND_COLORS.org]];
    return [...this.industryColor.entries()];
  }

  slotFor(id, ghostOf = null) {
    let s = this.nodeIndex.get(id);
    if (s != null) return s;
    s = this.slots.length;
    if (s >= CAP_NODES) return -1;
    this.nodeIndex.set(id, s);
    const node = this.store.nodes.get(id);
    this.slots.push({ id, ghost: !!ghostOf, ghostOf });
    // 家的位置：同行业聚在球面同一方向，按 id 散开
    const r = rng(hashStr(String(id)));
    let hx, hy, hz;
    if (ghostOf != null && this.nodeIndex.has(ghostOf)) {
      const g = this.nodeIndex.get(ghostOf);
      hx = this.home[g * 3] * 1.12 + (r() - 0.5) * 10;
      hy = this.home[g * 3 + 1] * 1.12 + (r() - 0.5) * 10;
      hz = this.home[g * 3 + 2] * 1.12 + (r() - 0.5) * 10;
    } else {
      const ind = this.industryOf(node);
      const dir = fibDir(hashStr(ind) % 48, 48);
      const R = 62 + r() * 22;
      const t1 = new THREE.Vector3().crossVectors(dir, new THREE.Vector3(0, 1, 0.3)).normalize();
      const t2 = new THREE.Vector3().crossVectors(dir, t1).normalize();
      const a = (r() - 0.5) * 34, b = (r() - 0.5) * 34;
      const v = dir.clone().multiplyScalar(R).addScaledVector(t1, a).addScaledVector(t2, b);
      hx = v.x; hy = v.y; hz = v.z;
    }
    this.home.set([hx, hy, hz], s * 3);
    this.target.set([hx, hy, hz], s * 3);
    this.pos.set([hx, hy, hz], s * 3);
    this.nodeBuf.g.setDrawRange(0, this.slots.length);
    this.refreshNodeColor(s);
    return s;
  }

  refreshNodeColor(s) {
    const slot = this.slots[s];
    const node = slot.ghost ? { ghost: true } : this.store.nodes.get(slot.id);
    tmpC.set(this.colorFor(node));
    // 节点底色压暗一点，让判断闪光更显眼
    this.nodeBuf.attrs.color.setXYZ(s, tmpC.r * 0.8, tmpC.g * 0.8, tmpC.b * 0.8);
    this.nodeBuf.attrs.color.needsUpdate = true;
  }

  setColorBy(mode) {
    this.colorBy = mode;
    for (let i = 0; i < this.slots.length; i++) this.refreshNodeColor(i);
  }

  nodePos(id, out = new THREE.Vector3()) {
    const s = this.nodeIndex.get(id);
    if (s == null) return null;
    return out.set(this.pos[s * 3], this.pos[s * 3 + 1], this.pos[s * 3 + 2]);
  }

  // about 的源头位置：意图 → 发信人；构型 → 成员质心
  sourcePos(about, out = new THREE.Vector3()) {
    const it = this.store.intents.get(about);
    if (it) return this.nodePos(it.from, out);
    const c = this.cfgCentroid.get(about);
    if (c) return out.copy(c);
    const cfg = this.store.configs.get(about);
    if (cfg) return this.centroidOf(cfg.members, out);
    return null;
  }

  centroidOf(members, out = new THREE.Vector3()) {
    out.set(0, 0, 0);
    let n = 0;
    for (const m of members) {
      const s = this.nodeIndex.get(m);
      if (s == null) continue;
      out.x += this.pos[s * 3]; out.y += this.pos[s * 3 + 1]; out.z += this.pos[s * 3 + 2]; n++;
    }
    return n ? out.multiplyScalar(1 / n) : null;
  }

  ghostId(rel) {
    return `ghost:${rel.to}`;
  }

  // 参照读数 → 位移方向：每道参照题一个固定方向，act 朝外推，ignore 轻微回收
  refOffset(ref) {
    const v = new THREE.Vector3();
    for (const [q, ex] of Object.entries(ref || {})) {
      const d = fibDir(hashStr(q) % 32, 32);
      const w = ex === 'act' ? 1 : ex === 'unsure' ? 0.4 : -0.35;
      v.addScaledVector(d, w);
    }
    return v.multiplyScalar(6.5);
  }

  // 把 store 里的结构同步到场景（跳转后或首帧）
  syncAll() {
    for (const n of this.store.nodes.values()) {
      if (n.placeholder) continue;
      const s = this.slotFor(n.id);
      if (s >= 0) this.updateBaseTarget(n, s);
    }
    for (const r of this.store.relations.values()) this.ensureRelEndpoints(r);
    // 跳转后直接落到目标位置
    this.computeTargets();
    this.pos.set(this.target.subarray(0, this.slots.length * 3));
  }

  updateBaseTarget(n, s) {
    const off = this.refOffset(n.ref);
    n._base = [this.home[s * 3] + off.x, this.home[s * 3 + 1] + off.y, this.home[s * 3 + 2] + off.z];
  }

  ensureRelEndpoints(r) {
    for (const id of [r.from, r.via]) if (id != null && this.store.nodes.has(id) && !this.store.nodes.get(id).placeholder) this.slotFor(id);
    if (r.to != null) {
      const tn = this.store.nodes.get(r.to);
      if (tn && !tn.placeholder) this.slotFor(r.to);
      else this.slotFor(this.ghostId(r), r.via ?? r.from);
    }
  }

  relEnd(r) {
    const tn = this.store.nodes.get(r.to);
    return tn && !tn.placeholder ? r.to : this.ghostId(r);
  }

  // 目标位置 = 家 + 参照读数位移，再被最近活跃的构型拉向质心
  computeTargets() {
    const n = this.slots.length;
    const pull = new Float32Array(n * 3);
    const pullW = new Float32Array(n);
    for (let s = 0; s < n; s++) {
      const slot = this.slots[s];
      const node = slot.ghost ? null : this.store.nodes.get(slot.id);
      const b = node && node._base ? node._base : [this.home[s * 3], this.home[s * 3 + 1], this.home[s * 3 + 2]];
      this.target[s * 3] = b[0]; this.target[s * 3 + 1] = b[1]; this.target[s * 3 + 2] = b[2];
    }
    // 只让最近更新的若干个构型产生聚拢，避免整张网塌成一团
    const active = [];
    for (const c of this.store.configs.values()) if (c.status !== 'dropped') active.push(c);
    active.sort((a, b) => (b.lastSeq ?? 0) - (a.lastSeq ?? 0));
    const K = 28;
    for (let i = 0; i < Math.min(K, active.length); i++) {
      const c = active[i];
      const cx = [0, 0, 0];
      let m = 0;
      for (const id of c.members) {
        const s = this.nodeIndex.get(id);
        if (s == null) continue;
        cx[0] += this.target[s * 3]; cx[1] += this.target[s * 3 + 1]; cx[2] += this.target[s * 3 + 2]; m++;
      }
      if (m < 2) continue;
      cx[0] /= m; cx[1] /= m; cx[2] /= m;
      // 簇往中心收一点，让新长出来的构型出现在网内部
      const inward = c.status === 'stable' ? 0.8 : 0.9;
      const w = (c.status === 'stable' ? 0.62 : 0.45) * (1 - i / (K * 1.4));
      for (const id of c.members) {
        const s = this.nodeIndex.get(id);
        if (s == null) continue;
        pull[s * 3] += cx[0] * inward * w; pull[s * 3 + 1] += cx[1] * inward * w; pull[s * 3 + 2] += cx[2] * inward * w;
        pullW[s] += w;
      }
    }
    for (let s = 0; s < n; s++) {
      const w = Math.min(pullW[s], 0.58);
      if (w <= 0) continue;
      const k = w / pullW[s];
      for (let j = 0; j < 3; j++) {
        this.target[s * 3 + j] = this.target[s * 3 + j] * (1 - w) + pull[s * 3 + j] * k;
      }
    }
  }

  // ---------- 事件 → 效果 ----------
  onEvent(ev, ctx) {
    if (ev.type === '__reset') { this.resetVisual(); return; }
    if (this.silent) return;
    // 结构类事件立刻落地；瞬时效果进队列
    switch (ev.type) {
      case 'node_join': {
        const s = this.slotFor(ev.node.id);
        if (s >= 0) {
          this.updateBaseTarget(ctx.node, s);
          this.refreshNodeColor(s);
          if (ctx.node.lateJoin || ctx.node.live) this.enqueue({ kind: 'join', id: ev.node.id, live: ctx.node.live });
          else this.alphaCur[s] = 0; // 开场铺网：淡入
        }
        return;
      }
      case 'coord': {
        const s = this.slotFor(ev.node);
        if (s >= 0 && ctx.node) this.updateBaseTarget(ctx.node, s);
        return;
      }
      case 'relation':
        this.ensureRelEndpoints(ctx.relation);
        this.enqueue({ kind: 'relation', rel: ctx.relation });
        return;
      default:
        break;
    }
    this.enqueue({ kind: ev.type, ev, ctx });
  }

  enqueue(e) {
    this.stats.effectsIn++;
    this.queue.push(e);
    // 积压太多（倍速很高或标签页在后台）：丢掉最旧的瞬时效果。
    // 用队头下标出队，丢弃只移动下标，每个事件 O(1)
    const over = this.queue.length - this.qHead - this.maxQueue;
    if (over > 0) {
      this.qHead += over;
      this.stats.effectsDropped += over;
    }
    if (this.qHead > 4096 && this.qHead * 2 > this.queue.length) {
      this.queue = this.queue.slice(this.qHead);
      this.qHead = 0;
    }
  }

  get queueLen() {
    return this.queue.length - this.qHead;
  }

  resetVisual() {
    this.queue = [];
    this.qHead = 0;
    this.parts.length = 0;
    this.cfgPulse.clear();
    this.flash.fill(0);
    for (const w of this.waves) { w.busy = false; w.mesh.visible = false; }
  }

  dur(base) {
    return base / Math.sqrt(Math.min(Math.max(this.speedHint, 1), 16));
  }

  spawnParticle(from, toId, color, opts = {}) {
    if (this.parts.length * 3 >= CAP_PARTICLES - 3) return false;
    this.parts.push({
      from: from.clone(), toId, toPos: opts.toPos ? opts.toPos.clone() : null, t0: this.time,
      dur: opts.dur ?? this.dur(0.9), col: new THREE.Color(color), size: opts.size ?? 5, lift: opts.lift ?? 0.18,
      onArrive: opts.onArrive || null, trail: opts.trail ?? true,
    });
    return true;
  }

  flashNode(id, color, amount = 1) {
    const s = this.nodeIndex.get(id);
    if (s == null) return;
    tmpC.set(color);
    this.flash[s] = Math.max(this.flash[s], amount);
    this.flashCol[s * 3] = tmpC.r; this.flashCol[s * 3 + 1] = tmpC.g; this.flashCol[s * 3 + 2] = tmpC.b;
  }

  wave(pos, color, max = 14, dur = 1.2) {
    const w = this.waves.find((x) => !x.busy) || this.waves.reduce((a, b) => (a.t0 < b.t0 ? a : b));
    w.busy = true; w.t0 = this.time; w.dur = dur; w.max = max;
    w.mesh.position.copy(pos);
    w.mesh.material.color.set(color);
    w.mesh.visible = true;
  }

  runEffect(e) {
    const v = new THREE.Vector3();
    switch (e.kind) {
      case 'join': {
        const s = this.nodeIndex.get(e.id);
        const p = this.nodePos(e.id, v);
        if (!p) return;
        this.alphaCur[s] = 0;
        this.boost[s] = e.live ? 3.2 : 1.6;
        this.flashNode(e.id, e.live ? COLORS.live : '#ffffff', 1.5);
        this.wave(p, e.live ? COLORS.live : '#9fd8ff', e.live ? 34 : 16, e.live ? 2.2 : 1.3);
        if (e.live) {
          this.wave(p, '#ffffff', 22, 1.6);
          // 入网爆点：粒子向四周散开
          for (let i = 0; i < 28; i++) {
            const d = fibDir(i, 28).multiplyScalar(18 + Math.random() * 10);
            this.spawnParticle(p, null, COLORS.live, { toPos: p.clone().add(d), dur: 1.4, size: 4, lift: 0, trail: true });
          }
        }
        this.onJoinFx && this.onJoinFx(e.id, e.live);
        return;
      }
      case 'intent': {
        const it = e.ctx.intent;
        const p = this.nodePos(it.from, v);
        if (p) {
          this.flashNode(it.from, COLORS.intent, 2);
          this.wave(p, '#ffffff', 26, 1.8);
        }
        return;
      }
      case 'route': {
        const ev = e.ev;
        const src = this.sourcePos(ev.about, v);
        if (!src) return;
        const col = ev.stage === 'trace' ? COLORS.trace : ev.stage === 'fine' ? COLORS.fine : COLORS.coarse;
        const size = ev.stage === 'coarse' ? 3.6 : 5.2;
        for (const id of ev.to || []) {
          if (!this.nodeIndex.has(id)) continue;
          this.spawnParticle(src, id, col, { size, dur: this.dur(ev.stage === 'coarse' ? 1.0 : 0.8) * (0.8 + Math.random() * 0.45) });
        }
        return;
      }
      case 'judge': {
        const ev = e.ev;
        const ex = exitClass(ev.exit);
        const col = COLORS[ex];
        const amt = ex === 'ignore' ? 0.35 : ex === 'act' ? 1.2 : 1.0;
        this.flashNode(ev.node, col, amt);
        if (ex === 'act' || ex === 'unsure' || ex === 'pick') {
          const p = this.nodePos(ev.node, v);
          const src = this.sourcePos(ev.about);
          if (p && src) this.spawnParticle(p, null, col, { toPos: src, size: 4.2, dur: this.dur(0.7), lift: 0.12 });
        }
        return;
      }
      case 'enrich': {
        const ev = e.ev;
        const src = this.sourcePos(ev.about, v);
        if (src && this.nodeIndex.has(ev.node)) this.spawnParticle(src, ev.node, COLORS.enrich, { size: 3.4, dur: this.dur(0.6), lift: 0.3 });
        this.flashNode(ev.node, COLORS.enrich, 0.8);
        return;
      }
      case 'relation': {
        const r = e.rel;
        const a = this.nodePos(r.from, v);
        const endId = this.relEnd(r);
        if (!a) return;
        const col = r.kind === 'relay' ? COLORS.relay : r.kind === 'unsure' ? COLORS.relUnsure : COLORS.direct;
        if (r.kind === 'relay' && r.via != null && this.nodeIndex.has(r.via)) {
          this.spawnParticle(a, r.via, col, { size: 6, dur: this.dur(0.7), onArrive: () => {
            const b = this.nodePos(r.via);
            if (b) this.spawnParticle(b, endId, col, { size: 6, dur: this.dur(0.7) });
          } });
        } else {
          this.spawnParticle(a, endId, col, { size: 6, dur: this.dur(0.9) });
        }
        this.flashNode(endId, col, 1.2);
        return;
      }
      case 'config': {
        const cfg = e.ctx.config;
        const prev = e.ctx.prevStatus;
        const pulse = cfg.status === 'stable' ? 2.2 : cfg.status === 'dropped' ? 0 : 1.2;
        this.cfgPulse.set(cfg.id, Math.max(this.cfgPulse.get(cfg.id) || 0, pulse));
        for (const m of cfg.members) this.flashNode(m, COLORS.config, cfg.status === 'stable' ? 0.9 : 0.6);
        const c = this.centroidOf(cfg.members, v);
        if (c && cfg.status === 'stable' && prev !== 'stable') this.wave(c, COLORS.config, 30, 2.0);
        if (c && !prev) this.wave(c, COLORS.config, 16, 1.2);
        // 构型再传：父构型质心 → 新构型质心
        if (!prev && cfg.parent) {
          const pc = this.cfgCentroid.get(cfg.parent);
          if (pc && c) this.spawnParticle(pc, null, COLORS.config, { toPos: c, size: 8, dur: this.dur(1.0) });
        }
        this.onConfigFx && this.onConfigFx(cfg, prev);
        return;
      }
      case 'ring': {
        const ev = e.ev;
        const cyc = ev.cycle || [];
        const edges = ev.edges || [];
        // 沿环依次放信号，颜色按每条边的出口
        cyc.forEach((id, i) => {
          const nextId = cyc[(i + 1) % cyc.length];
          const edge = edges[i] || {};
          const col = COLORS[exitClass(edge.exit)] || COLORS.config;
          const p = this.nodePos(id);
          if (p && this.nodeIndex.has(nextId)) {
            const delay = i * 0.18;
            this.spawnParticle(p, nextId, col, { size: 7, dur: this.dur(0.8) + delay, lift: 0.08 });
          }
        });
        return;
      }
      case 'plan': {
        const cfg = e.ctx.config;
        if (cfg) {
          const c = this.centroidOf(cfg.members, v);
          if (c) { this.wave(c, '#ffffff', 24, 2.0); }
          this.cfgPulse.set(cfg.id, 3);
        }
        return;
      }
      case 'feedback': {
        const ev = e.ev;
        const col = COLORS[ev.verdict] || '#ffffff';
        const p = this.nodePos(ev.member, v);
        if (p) { this.flashNode(ev.member, col, 2); this.wave(p, col, 20, 1.6); }
        return;
      }
      case 'node_leave': {
        const p = this.nodePos(e.ev.id, v);
        if (p) this.wave(p, '#556070', 10, 1.0);
        return;
      }
      default:
    }
  }

  // ---------- 每帧 ----------
  frame() {
    const now = performance.now();
    const dt = Math.min(0.1, (now - this.clock) / 1000);
    this.clock = now;
    this.time += dt;

    // 抽样：每帧只处理一部分瞬时效果
    let budget = this.maxEffectsPerFrame;
    let drawn = 0;
    while (this.qHead < this.queue.length && budget > 0) {
      const e = this.queue[this.qHead];
      this.queue[this.qHead++] = undefined;
      this.runEffect(e);
      budget -= e.kind === 'route' ? Math.max(1, Math.ceil((e.ev.to || []).length / 6)) : 1;
      drawn++;
    }
    this.stats.effectsDrawn += drawn;

    this.computeTargets();
    const n = this.slots.length;
    const k = 1 - Math.exp(-dt * 0.9); // 缓慢漂移
    const A = this.nodeBuf.attrs;
    for (let s = 0; s < n; s++) {
      const slot = this.slots[s];
      const node = slot.ghost ? null : this.store.nodes.get(slot.id);
      const i3 = s * 3;
      // 呼吸：极小的扰动，让静止的网也有生命
      const ph = s * 1.37 + this.time * 0.4;
      for (let j = 0; j < 3; j++) this.pos[i3 + j] += (this.target[i3 + j] - this.pos[i3 + j]) * k;
      A.position.setXYZ(s, this.pos[i3] + Math.sin(ph) * 0.35, this.pos[i3 + 1] + Math.cos(ph * 1.1) * 0.35, this.pos[i3 + 2]);
      this.flash[s] *= Math.exp(-dt * 3.2);
      this.boost[s] *= Math.exp(-dt * 0.6);
      const alive = slot.ghost ? this.ghostAlive(slot) : node && node.alive;
      const aT = alive ? 1 : 0;
      this.alphaCur[s] += (aT - this.alphaCur[s]) * (1 - Math.exp(-dt * 2.5));
      const sel = this.selected && this.selected.type === 'node' && this.selected.id === slot.id;
      const baseSize = slot.ghost ? 3.2 : node && node.live ? 9 : node && node.kind === 'company' ? 7.2 : node && node.kind === 'org' ? 7.8 : 6;
      const activity = node ? Math.min(1, (node.judgeN || 0) / 60) : 0;
      A.size.setX(s, (baseSize + activity * 2.2) * (1 + this.boost[s]) * (sel ? 1.8 : 1));
      A.flash.setX(s, this.flash[s]);
      A.fcolor.setXYZ(s, this.flashCol[i3], this.flashCol[i3 + 1], this.flashCol[i3 + 2]);
      A.alpha.setX(s, this.alphaCur[s] * (slot.ghost ? 0.6 : 1));
    }
    for (const a of Object.values(A)) a.needsUpdate = true;

    this.drawRelations();
    this.drawConfigs(dt);
    this.drawParticles();
    this.drawWaves();

    if (this.selected && this.selected.type === 'node') {
      const p = this.nodePos(this.selected.id);
      if (p) {
        this.selMarker.visible = true;
        this.selMarker.position.copy(p);
        this.selMarker.quaternion.copy(this.camera.quaternion);
        this.selMarker.scale.setScalar(7 + Math.sin(this.time * 4) * 0.8);
      }
    } else if (this.selected && this.selected.type === 'config') {
      const c = this.cfgCentroid.get(this.selected.id);
      if (c) {
        this.selMarker.visible = true;
        this.selMarker.position.copy(c);
        this.selMarker.quaternion.copy(this.camera.quaternion);
        this.selMarker.scale.setScalar(14 + Math.sin(this.time * 4) * 1.2);
      }
    } else this.selMarker.visible = false;

    this.controls.update();
    if (this.bloomOn) this.composer.render();
    else this.renderer.render(this.scene, this.camera);
  }

  ghostAlive(slot) {
    return true;
  }

  drawRelations() {
    const L = this.rel;
    let i = 0;
    const sel = this.selected;
    const put = (a, b, col, alpha) => {
      if (i >= L.cap) return;
      L.pos.setXYZ(i * 2, a.x, a.y, a.z); L.pos.setXYZ(i * 2 + 1, b.x, b.y, b.z);
      tmpC.set(col);
      L.col.setXYZW(i * 2, tmpC.r, tmpC.g, tmpC.b, alpha); L.col.setXYZW(i * 2 + 1, tmpC.r, tmpC.g, tmpC.b, alpha);
      i++;
    };
    const a = new THREE.Vector3(), b = new THREE.Vector3(), c = new THREE.Vector3();
    const curRoot = this.focusIntent;
    for (const r of this.store.relations.values()) {
      if (!this.nodePos(r.from, a)) continue;
      const end = this.relEnd(r);
      if (!this.nodePos(end, b)) continue;
      const col = r.kind === 'relay' ? COLORS.relay : r.kind === 'unsure' ? COLORS.relUnsure : COLORS.direct;
      let alpha = r.kind === 'unsure' ? 0.22 : 0.34;
      if (curRoot && r.root === curRoot) alpha = Math.min(1, alpha * 2.4);
      if (sel && ((sel.type === 'relation' && sel.id === r.id) || (sel.type === 'node' && (r.from === sel.id || r.to === sel.id || r.via === sel.id)))) alpha = 1;
      if (r.kind === 'relay' && r.via != null && this.nodePos(r.via, c)) {
        put(a, c, col, alpha); put(c, b, col, alpha);
      } else put(a, b, col, alpha);
    }
    L.n = i;
    L.g.setDrawRange(0, i * 2);
    L.pos.needsUpdate = true; L.col.needsUpdate = true;
  }

  drawConfigs(dt) {
    const L = this.cfg;
    const H = this.haloBuf.attrs;
    let i = 0, h = 0;
    const c = new THREE.Vector3(), p = new THREE.Vector3(), q = new THREE.Vector3();
    const col = new THREE.Color(COLORS.config);
    const sel = this.selected;
    for (const cfg of this.store.configs.values()) {
      const pulse = (this.cfgPulse.get(cfg.id) || 0);
      if (pulse > 0.01) this.cfgPulse.set(cfg.id, pulse * Math.exp(-dt * 1.4));
      if (cfg.status === 'dropped' && pulse < 0.05) { this.cfgCentroid.delete(cfg.id); continue; }
      if (!this.centroidOf(cfg.members, c)) continue;
      const cen = this.cfgCentroid.get(cfg.id) || new THREE.Vector3();
      cen.copy(c); this.cfgCentroid.set(cfg.id, cen);
      const isSel = sel && sel.type === 'config' && sel.id === cfg.id;
      const base = cfg.status === 'stable' ? 0.42 : cfg.status === 'dropped' ? 0.08 : 0.2;
      const alpha = Math.min(1, base + pulse * 0.3 + (isSel ? 0.6 : 0));
      // 成员连向质心：发光的簇
      for (const m of cfg.members) {
        if (!this.nodePos(m, p) || i >= L.cap) continue;
        L.pos.setXYZ(i * 2, c.x, c.y, c.z); L.pos.setXYZ(i * 2 + 1, p.x, p.y, p.z);
        L.col.setXYZW(i * 2, col.r, col.g, col.b, alpha); L.col.setXYZW(i * 2 + 1, col.r, col.g, col.b, alpha * 0.35);
        i++;
      }
      // 环清算闭合的构型：画出环
      if (cfg.ring && cfg.status === 'stable') {
        const cyc = cfg.ring.cycle || [];
        for (let k = 0; k < cyc.length && i < L.cap; k++) {
          if (!this.nodePos(cyc[k], p) || !this.nodePos(cyc[(k + 1) % cyc.length], q)) continue;
          const e = (cfg.ring.edges || [])[k] || {};
          tmpC.set(COLORS[exitClass(e.exit)] || COLORS.config);
          const a2 = Math.min(1, 0.3 + pulse * 0.25 + (isSel ? 0.5 : 0));
          L.pos.setXYZ(i * 2, p.x, p.y, p.z); L.pos.setXYZ(i * 2 + 1, q.x, q.y, q.z);
          L.col.setXYZW(i * 2, tmpC.r, tmpC.g, tmpC.b, a2); L.col.setXYZW(i * 2 + 1, tmpC.r, tmpC.g, tmpC.b, a2);
          i++;
        }
      }
      if (h < CAP_HALOS) {
        H.position.setXYZ(h, c.x, c.y, c.z);
        H.color.setXYZ(h, col.r, col.g * 0.85, col.b * 0.6);
        H.fcolor.setXYZ(h, 1, 1, 1);
        H.flash.setX(h, Math.min(1, pulse * 0.4));
        H.size.setX(h, (cfg.status === 'stable' ? 16 : 10) + cfg.members.length * 2.2 + pulse * 6 + (isSel ? 10 : 0));
        H.alpha.setX(h, (cfg.status === 'stable' ? 0.2 : cfg.status === 'dropped' ? 0.04 : 0.12) + (isSel ? 0.25 : 0));
        h++;
      }
    }
    L.g.setDrawRange(0, i * 2);
    L.pos.needsUpdate = true; L.col.needsUpdate = true;
    this.haloBuf.g.setDrawRange(0, h);
    for (const a of Object.values(H)) a.needsUpdate = true;
  }

  drawParticles() {
    const P = this.partBuf.attrs;
    let w = 0;
    const to = new THREE.Vector3(), mid = new THREE.Vector3(), pt = new THREE.Vector3();
    const keep = [];
    const arrivals = [];
    for (const p of this.parts) {
      const u = (this.time - p.t0) / p.dur;
      if (u >= 1) { if (p.onArrive) arrivals.push(p.onArrive); if (p.toId) this.flashNode(p.toId, '#' + p.col.getHexString(), 0.5); continue; }
      if (u < 0) { keep.push(p); continue; }
      if (p.toId) { if (!this.nodePos(p.toId, to)) continue; } else to.copy(p.toPos);
      // 二次贝塞尔：中点往外抬，让信号走弧线
      mid.addVectors(p.from, to).multiplyScalar(0.5);
      const len = p.from.distanceTo(to);
      mid.addScaledVector(mid.clone().normalize(), len * p.lift);
      const steps = p.trail ? 3 : 1;
      for (let k = 0; k < steps && w < CAP_PARTICLES; k++) {
        const uu = Math.max(0, u - k * 0.045);
        const e = uu < 0.5 ? 2 * uu * uu : 1 - Math.pow(-2 * uu + 2, 2) / 2;
        const a1 = 1 - e, a2 = e;
        pt.set(0, 0, 0).addScaledVector(p.from, a1 * a1).addScaledVector(mid, 2 * a1 * a2).addScaledVector(to, a2 * a2);
        P.position.setXYZ(w, pt.x, pt.y, pt.z);
        P.color.setXYZ(w, p.col.r, p.col.g, p.col.b);
        P.fcolor.setXYZ(w, 1, 1, 1);
        P.flash.setX(w, 0);
        P.size.setX(w, p.size * (1 - k * 0.3));
        P.alpha.setX(w, (1 - k * 0.35) * Math.min(1, (1 - u) * 4));
        w++;
      }
      keep.push(p);
    }
    this.parts = keep;
    for (const f of arrivals) f();
    this.partBuf.g.setDrawRange(0, w);
    for (const a of Object.values(P)) a.needsUpdate = true;
    this.liveParticles = this.parts.length;
  }

  drawWaves() {
    for (const w of this.waves) {
      if (!w.busy) continue;
      const u = (this.time - w.t0) / w.dur;
      if (u >= 1) { w.busy = false; w.mesh.visible = false; continue; }
      const e = 1 - Math.pow(1 - u, 3);
      w.mesh.scale.setScalar(1 + e * w.max);
      w.mesh.material.opacity = (1 - u) * (1 - u) * 0.45;
      w.mesh.quaternion.copy(this.camera.quaternion);
    }
  }

  // ---------- 拾取：屏幕空间最近点 ----------
  project(v) {
    const p = v.clone().project(this.camera);
    if (p.z > 1) return null;
    return { x: (p.x * 0.5 + 0.5) * this.viewW, y: (-p.y * 0.5 + 0.5) * this.viewH };
  }

  pick(x, y) {
    let best = null, bd = 16;
    const v = new THREE.Vector3();
    for (let s = 0; s < this.slots.length; s++) {
      if (this.alphaCur[s] < 0.2) continue;
      const slot = this.slots[s];
      v.set(this.pos[s * 3], this.pos[s * 3 + 1], this.pos[s * 3 + 2]);
      const p = this.project(v);
      if (!p) continue;
      const d = Math.hypot(p.x - x, p.y - y);
      if (d < bd) { bd = d; best = slot.ghost ? { type: 'ghost', id: slot.id } : { type: 'node', id: slot.id }; }
    }
    if (best) return best;
    bd = 22;
    for (const [id, c] of this.cfgCentroid) {
      const p = this.project(c);
      if (!p) continue;
      const d = Math.hypot(p.x - x, p.y - y);
      if (d < bd) { bd = d; best = { type: 'config', id }; }
    }
    if (best) return best;
    bd = 6;
    const a = new THREE.Vector3(), b = new THREE.Vector3();
    for (const r of this.store.relations.values()) {
      if (!this.nodePos(r.from, a) || !this.nodePos(this.relEnd(r), b)) continue;
      const pa = this.project(a), pb = this.project(b);
      if (!pa || !pb) continue;
      const dx = pb.x - pa.x, dy = pb.y - pa.y;
      const L2 = dx * dx + dy * dy || 1;
      const t = Math.max(0, Math.min(1, ((x - pa.x) * dx + (y - pa.y) * dy) / L2));
      const d = Math.hypot(pa.x + t * dx - x, pa.y + t * dy - y);
      if (d < bd) { bd = d; best = { type: 'relation', id: r.id }; }
    }
    return best;
  }

  screenPosOf(id) {
    const v = this.nodePos(id);
    return v ? this.project(v) : null;
  }
}

// 播放器：两种数据来源
//   静态 jsonl：整份读进来，按事件 t 用「墙钟增量 × 倍速」推进；可暂停、倍速、跳转、压缩空闲。
//   SSE（GET /events）：来一条收一条；暂停时缓存，继续时一次补齐。
// 归约器每帧不限量地应用到期事件（计数永远是全量），渲染抽样在场景层做。

import { parseJsonl } from './store.js';

const KNOWN_TYPES = ['node_join', 'node_leave', 'coord', 'intent', 'route', 'judge', 'enrich', 'relation',
  'config', 'ring', 'plan', 'feedback', 'metric', 'baseline', 'batch', 'anchors', 'discovery', 'node_status', 'compile'];

export class Player {
  constructor(store) {
    this.store = store;
    this.events = [];
    this.idx = 0;
    this.simT = 0;
    this.speed = 1;
    this.playing = false;
    this.compressIdle = true;
    this.idleCapMs = 1200; // 空闲超过这么久（运行时间）就直接跳到下一个事件前
    this.mode = 'static';
    this.status = 'idle';
    this.bad = 0;
    this.silent = false;
    this.onStatus = () => {};
    this.onSeek = () => {};
    this.es = null;
    this.sseCount = 0;
    // 一帧要应用的事件超过这么多（SSE 连上时补发历史、标签页切回、极高倍速），
    // 就整批静默应用、再让场景一次同步，不给已经过去的事件逐个放效果
    this.bulkThreshold = 1500;
    this.onBulk = () => {};
    this.bulkCount = 0;
  }

  get totalT() {
    return this.events.length ? this.events[this.events.length - 1].t ?? 0 : 0;
  }

  get firstT() {
    return this.events.length ? this.events[0].t ?? 0 : 0;
  }

  async loadJsonl(url) {
    this.mode = 'static';
    this.status = 'loading';
    this.onStatus();
    const res = await fetch(url, { cache: 'no-store' });
    if (!res.ok) throw new Error(`读取事件文件失败：${res.status} ${url}`);
    const { events, bad } = parseJsonl(await res.text());
    this.events = events;
    this.bad = bad;
    this.idx = 0;
    this.simT = this.firstT;
    this.status = 'ready';
    this.onStatus();
  }

  connectSSE(url) {
    this.mode = 'sse';
    this.status = 'connecting';
    this.onStatus();
    const es = new EventSource(url);
    this.es = es;
    const handle = (msg) => {
      let ev;
      try { ev = JSON.parse(msg.data); } catch { this.bad++; return; }
      // 服务可能按批推送（event: batch，data 是事件数组）
      const list = Array.isArray(ev) ? ev : Array.isArray(ev.events) && msg.type === 'batch' ? ev.events : [ev];
      for (const e of list) {
        if (!e || typeof e !== 'object') continue;
        if (!e.type && msg.type && msg.type !== 'message' && msg.type !== 'batch') e.type = msg.type;
        this.sseCount++;
        this.events.push(e);
      }
    };
    es.onmessage = handle;
    for (const t of KNOWN_TYPES) es.addEventListener(t, handle);
    es.onopen = () => { this.status = 'live'; this.onStatus(); };
    es.onerror = () => { this.status = es.readyState === 2 ? 'closed' : 'reconnecting'; this.onStatus(); };
  }

  play() { this.playing = true; this.onStatus(); }
  pause() { this.playing = false; this.onStatus(); }
  toggle() { this.playing ? this.pause() : this.play(); }
  setSpeed(s) { this.speed = s; this.onStatus(); }

  // 每帧调用。返回本帧应用的事件（供场景做视觉效果）
  tick(dtWallMs) {
    const applied = [];
    if (!this.playing) return applied;
    if (this.mode === 'sse') {
      this.applyUntil(this.events.length, applied);
      this.simT = this.store.c.elapsed;
      return applied;
    }
    if (this.idx >= this.events.length) { this.playing = false; this.onStatus(); return applied; }
    this.simT += Math.min(dtWallMs, 1000 * 60) * this.speed;
    const next = this.events[this.idx];
    if (this.compressIdle && next && (next.t ?? 0) - this.simT > this.idleCapMs) {
      // 只往前跳：留 120ms 墙钟的余量，但不能超过空闲上限，否则高倍速时会把时钟拨回去
      this.simT = Math.max(this.simT, (next.t ?? 0) - Math.min(120 * this.speed, this.idleCapMs));
    }
    let end = this.idx;
    while (end < this.events.length && (this.events[end].t ?? 0) <= this.simT) end++;
    this.applyUntil(end, applied);
    return applied;
  }

  applyUntil(end, applied) {
    const bulk = end - this.idx > this.bulkThreshold;
    if (bulk) { this.bulkCount++; this.onBulk(true); }
    try {
      while (this.idx < end) {
        const ev = this.store.apply(this.events[this.idx++]);
        if (ev && !bulk) applied.push(ev);
      }
    } finally {
      if (bulk) this.onBulk(false);
    }
  }

  // 跳到运行时间 t（毫秒）。只跑归约器，不产生视觉效果。
  seek(t) {
    if (this.mode !== 'static') return;
    if (t < this.simT) {
      this.store.reset();
      this.idx = 0;
    }
    this.simT = t;
    while (this.idx < this.events.length && (this.events[this.idx].t ?? 0) <= t) {
      this.store.apply(this.events[this.idx++]);
    }
    this.onSeek();
    this.onStatus();
  }

  seekEnd() { this.seek(this.totalT); }
  restart() { this.seek(this.firstT - 1); }

  get progress() {
    if (this.mode !== 'static' || !this.events.length) return 1;
    const span = this.totalT - this.firstT || 1;
    return Math.min(1, Math.max(0, (this.simT - this.firstT) / span));
  }
}

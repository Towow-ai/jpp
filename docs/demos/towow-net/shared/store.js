// 事件归约器：只认 接口.md 第二节的事件流。
// 每个事件按 seq 去重、只应用一次；所有计数都从这里的全量事件累计，
// 渲染层可以抽样，但不影响这里的数字。

export const EXITS = ['act', 'ignore', 'unsure', 'pick'];

export function exitClass(exit) {
  if (!exit) return 'unknown';
  if (exit === 'act' || exit === 'ignore' || exit === 'unsure') return exit;
  if (String(exit).startsWith('pick')) return 'pick';
  return 'unknown';
}

// 现场参与者怎么识别，接口里还没有专门字段（见 open_questions）。暂用任一信号：
//   节点带 live:true 或 source=="live"；id 以 live 开头；tags 含「现场」；
//   synthetic:false、没有 source、也不是 demo_only（template.md：服务端填 synthetic 与 source，
//   新闻里的真人带 source，现场的人没有）。
export function isLiveParticipant(node) {
  if (!node) return false;
  if (node.live === true || node.source === 'live') return true;
  if (typeof node.id === 'string' && /^live/i.test(node.id)) return true;
  if (Array.isArray(node.tags) && node.tags.some((t) => String(t).includes('现场'))) return true;
  return node.synthetic === false && !node.source && !node.demo_only;
}

// 页面一律显示角色、不显示名字（接口.md 第一节，Nature 2026-09-26 晚）。
// 缺 role 的旧数据退化为 tags[0] 加 kind；什么都没有的现场参与者叫「现场参与者」。
const KIND_WORD = { person: '人', company: '公司', org: '机构' };
export function roleOf(n) {
  if (!n) return '';
  if (n.role && String(n.role).trim()) return String(n.role).trim();
  const t = Array.isArray(n.tags) && n.tags.length ? String(n.tags[0]) : '';
  if (t) return `${t}${KIND_WORD[n.kind] || ''}`;
  if (n.live || isLiveParticipant(n)) return '现场参与者';
  return n.placeholder ? String(n.id ?? '') : (KIND_WORD[n.kind] ? `一位${KIND_WORD[n.kind] === '人' ? '参与者' : KIND_WORD[n.kind]}` : String(n.id ?? ''));
}

export class Store {
  constructor() {
    this.listeners = [];
    this.reset();
  }

  reset() {
    this.seen = new Set();
    this.maxSeq = 0;
    this.nodes = new Map();
    this.judges = new Map();
    this.intents = new Map();
    this.intentOrder = [];
    this.configs = new Map();
    this.relations = new Map();
    this.plans = new Map();
    this.rings = [];
    this.baselines = [];
    this.feedback = [];
    this.enrich = [];
    this.anyMock = false;
    this.firstIntentT = null;
    this.lastJoinT = null;
    this.c = {
      events: 0,
      judgments: 0,
      usd: 0,
      exits: { act: 0, ignore: 0, unsure: 0, pick: 0, unknown: 0 },
      routes: 0,
      routeTargets: 0,
      enrich: 0,
      relations: 0,
      relKinds: { direct: 0, relay: 0, unsure: 0 },
      rings: 0,
      ringsClosed: 0,
      plans: 0,
      intents: 0,
      joins: 0,
      leaves: 0,
      liveJoins: 0,
      feedback: { yes: 0, edit: 0, no: 0 },
      baselines: 0,
      baselineUsd: 0,
      elapsed: 0,
      unknownTypes: 0,
    };
    this.lastMetric = null;
    this.anchors = new Map(); // 参照题 id → 题面（服务若发 anchors 事件）
    // 每秒判断：按运行时间（事件 t）分桶，500ms 一桶
    this.judgeBuckets = new Map();
    for (const fn of this.listeners) fn({ type: '__reset' }, null);
  }

  on(fn) {
    this.listeners.push(fn);
  }

  // 构型数：出现过、且当前状态不是 dropped 的构型 id 个数
  get configCount() {
    let n = 0;
    for (const c of this.configs.values()) if (c.status !== 'dropped') n++;
    return n;
  }

  get stableConfigCount() {
    let n = 0;
    for (const c of this.configs.values()) if (c.status === 'stable') n++;
    return n;
  }

  // 显示用的总数：事件逐条累计与服务端 metric 汇总取大者。
  // 服务如果只推汇总、不推每条判断，数字也不会少算。
  get shownJudgments() {
    const m = this.lastMetric && Number(this.lastMetric.judgments);
    return Math.max(this.c.judgments, Number.isFinite(m) ? m : 0);
  }

  get shownUsd() {
    const m = this.lastMetric && Number(this.lastMetric.usd);
    return Math.max(this.c.usd, Number.isFinite(m) ? m : 0);
  }

  get aliveCount() {
    let n = 0;
    for (const x of this.nodes.values()) if (x.alive) n++;
    return n;
  }

  // about 可能是意图 id，也可能是构型 id；沿 parent/about 链追到根意图
  rootIntent(about) {
    let cur = about;
    for (let i = 0; i < 32 && cur != null; i++) {
      if (this.intents.has(cur)) return cur;
      const cfg = this.configs.get(cur);
      if (!cfg) return null;
      cur = cfg.about ?? cfg.parent;
    }
    return null;
  }

  ensureNode(id) {
    let n = this.nodes.get(id);
    if (!n) {
      // 被引用但没见过 node_join 的主体：先占位，标为未入网
      n = {
        id, name: String(id), kind: 'unknown', tags: [], summary: '', synthetic: null,
        demo_only: false, alive: false, placeholder: true,
        ref: {}, trace: [], judges: [], relations: [], enrich: [], joinT: null, live: false,
      };
      this.nodes.set(id, n);
    }
    return n;
  }

  intentStat(root) {
    return root ? this.intents.get(root) : null;
  }

  apply(ev) {
    if (!ev || typeof ev !== 'object') return null;
    const seq = ev.seq;
    if (seq != null) {
      if (this.seen.has(seq)) return null;
      this.seen.add(seq);
      if (seq > this.maxSeq) this.maxSeq = seq;
    }
    const c = this.c;
    c.events++;
    if (ev.mock === true) this.anyMock = true;
    if (typeof ev.t === 'number' && ev.t > c.elapsed) c.elapsed = ev.t;
    const ctx = {};

    switch (ev.type) {
      case 'node_join': {
        const src = ev.node || {};
        if (src.id == null) break;
        const n = this.ensureNode(src.id);
        const wasPlaceholder = n.placeholder;
        Object.assign(n, {
          name: src.name ?? n.name, role: src.role ?? n.role ?? null, kind: src.kind ?? n.kind, synthetic: src.synthetic ?? n.synthetic,
          demo_only: !!src.demo_only, tags: Array.isArray(src.tags) ? src.tags : [], summary: src.summary ?? '',
          source: src.source ?? n.source ?? null, city: src.city ?? n.city,
        });
        n.placeholder = false;
        n.alive = true;
        n.joinT = ev.t;
        n.live = isLiveParticipant(src);
        // 开场那一批之后才进来的都算「后来入网」：已有意图，或离上一次入网隔了 2 秒以上
        n.lateJoin = this.firstIntentT != null || (this.lastJoinT != null && (ev.t ?? 0) - this.lastJoinT > 2000);
        this.lastJoinT = ev.t ?? this.lastJoinT;
        c.joins++;
        if (n.live) c.liveJoins++;
        ctx.node = n;
        ctx.wasPlaceholder = wasPlaceholder;
        break;
      }
      case 'node_leave': {
        const n = this.nodes.get(ev.id);
        if (n) { n.alive = false; n.leftT = ev.t; ctx.node = n; }
        c.leaves++;
        break;
      }
      case 'coord': {
        const n = this.ensureNode(ev.node);
        if (ev.ref && typeof ev.ref === 'object') n.ref = { ...ev.ref };
        if (Array.isArray(ev.trace)) n.trace = ev.trace.slice();
        n.coordN = (n.coordN || 0) + 1;
        ctx.node = n;
        break;
      }
      case 'intent': {
        c.intents++;
        if (this.firstIntentT == null) this.firstIntentT = ev.t;
        const it = {
          id: ev.id, from: ev.from, text: ev.text ?? '', t: ev.t, seq: ev.seq, index: this.intentOrder.length,
          judges: 0, usd: 0, exits: { act: 0, ignore: 0, unsure: 0, pick: 0, unknown: 0 },
          configs: new Set(), relations: [], plans: [], rings: [], baselines: {}, lastT: ev.t,
          firstConfigT: null, judgesToFirstConfig: null, firstStableT: null,
        };
        this.intents.set(ev.id, it);
        this.intentOrder.push(ev.id);
        ctx.intent = it;
        break;
      }
      case 'route': {
        c.routes++;
        const to = Array.isArray(ev.to) ? ev.to : [];
        c.routeTargets += to.length;
        ctx.root = this.rootIntent(ev.about);
        break;
      }
      case 'judge': {
        c.judgments++;
        const usd = Number(ev.usd) || 0;
        c.usd += usd;
        const ex = exitClass(ev.exit);
        c.exits[ex]++;
        if (ev.id != null) this.judges.set(ev.id, ev);
        if (ev.node != null) {
          const n = this.ensureNode(ev.node);
          n.judges.push(ev.id ?? ev);
          n.judgeN = (n.judgeN || 0) + 1;
        }
        const root = this.rootIntent(ev.about);
        const it = this.intentStat(root);
        if (it) {
          it.judges++; it.usd += usd; it.exits[ex]++; it.lastT = Math.max(it.lastT, ev.t ?? 0);
        }
        const b = Math.floor((ev.t ?? 0) / 500);
        this.judgeBuckets.set(b, (this.judgeBuckets.get(b) || 0) + 1);
        ctx.root = root;
        ctx.exit = ex;
        break;
      }
      case 'enrich': {
        c.enrich++;
        this.enrich.push(ev);
        if (ev.node != null) this.ensureNode(ev.node).enrich.push(ev);
        ctx.root = this.rootIntent(ev.about);
        break;
      }
      case 'relation': {
        const isNew = !this.relations.has(ev.id);
        const root = this.rootIntent(ev.about);
        const r = { ...ev, root };
        this.relations.set(ev.id, r);
        if (isNew) {
          c.relations++;
          if (c.relKinds[ev.kind] != null) c.relKinds[ev.kind]++;
          for (const id of [ev.from, ev.to, ev.via]) {
            if (id != null && this.nodes.has(id)) this.nodes.get(id).relations.push(ev.id);
          }
          const it = this.intentStat(root);
          if (it) { it.relations.push(ev.id); it.lastT = Math.max(it.lastT, ev.t ?? 0); }
        }
        ctx.relation = r;
        ctx.root = root;
        break;
      }
      case 'config': {
        let cfg = this.configs.get(ev.id);
        const root = this.rootIntent(ev.about) ?? (ev.parent ? this.rootIntent(ev.parent) : null);
        const prev = cfg ? cfg.status : null;
        if (!cfg) {
          cfg = { id: ev.id, history: [], ring: null, plan: null, firstT: ev.t, firstSeq: ev.seq };
          this.configs.set(ev.id, cfg);
        }
        Object.assign(cfg, {
          about: ev.about, members: Array.isArray(ev.members) ? ev.members : [], parent: ev.parent ?? null,
          summary: ev.summary ?? '', status: ev.status ?? 'forming', root, lastT: ev.t, lastSeq: ev.seq,
        });
        cfg.history.push({ t: ev.t, status: cfg.status, members: cfg.members.length });
        const it = this.intentStat(root);
        if (it) {
          it.configs.add(ev.id);
          it.lastT = Math.max(it.lastT, ev.t ?? 0);
          if (it.firstConfigT == null) { it.firstConfigT = ev.t; it.judgesToFirstConfig = it.judges; }
          if (cfg.status === 'stable' && it.firstStableT == null) it.firstStableT = ev.t;
        }
        ctx.config = cfg;
        ctx.prevStatus = prev;
        ctx.root = root;
        break;
      }
      case 'ring': {
        c.rings++;
        const closed = ['closed', 'ok', 'stable', 'cleared', 'conditional'].includes(ev.status);
        if (closed) c.ringsClosed++;
        this.rings.push(ev);
        const cfg = this.configs.get(ev.config);
        if (cfg) cfg.ring = ev;
        const root = this.rootIntent(ev.config);
        const it = this.intentStat(root);
        if (it) it.rings.push(ev);
        ctx.config = cfg; ctx.root = root; ctx.closed = closed;
        break;
      }
      case 'plan': {
        if (!this.plans.has(ev.id)) c.plans++;
        this.plans.set(ev.id, ev);
        const cfg = this.configs.get(ev.config);
        if (cfg) cfg.plan = ev;
        const root = this.rootIntent(ev.config);
        const it = this.intentStat(root);
        if (it && !it.plans.includes(ev.id)) it.plans.push(ev.id);
        ctx.config = cfg; ctx.root = root;
        break;
      }
      case 'feedback': {
        if (c.feedback[ev.verdict] != null) c.feedback[ev.verdict]++;
        this.feedback.push(ev);
        break;
      }
      case 'metric': {
        this.lastMetric = ev;
        break;
      }
      case 'anchors': {
        for (const a of ev.items || []) if (a && a.id != null) this.anchors.set(a.id, a.text ?? '');
        break;
      }
      case 'discovery': {
        // 每条意图的第一个有效构型（设计稿里的新增事件）；有就用它，没有就用第一条 config 推算
        const it = this.intentStat(this.rootIntent(ev.about) ?? ev.about);
        if (it) { it.discovery = ev; }
        ctx.root = it ? it.id : null;
        break;
      }
      case 'node_status':
      case 'compile': {
        const id = ev.id ?? ev.node;
        if (id != null && this.nodes.has(id)) { this.nodes.get(id).tree = ev.tree ?? ev.status; ctx.node = this.nodes.get(id); }
        break;
      }
      case 'baseline': {
        c.baselines++;
        c.baselineUsd += Number(ev.usd) || 0;
        this.baselines.push(ev);
        const root = this.rootIntent(ev.about) ?? ev.about;
        const it = this.intentStat(root);
        if (it) it.baselines[ev.arm] = ev;
        ctx.root = root;
        break;
      }
      default:
        c.unknownTypes++;
    }
    for (const fn of this.listeners) fn(ev, ctx);
    return ev;
  }

  // 最近 windowMs 运行时间内每秒判断数
  judgeRate(windowMs = 2000) {
    const end = Math.floor(this.c.elapsed / 500);
    const k = Math.max(1, Math.round(windowMs / 500));
    let n = 0;
    for (let b = end - k + 1; b <= end; b++) n += this.judgeBuckets.get(b) || 0;
    return n / (k * 0.5);
  }

  peakRate() {
    let m = 0;
    for (const v of this.judgeBuckets.values()) if (v > m) m = v;
    return m * 2;
  }
}

// 解析 jsonl；坏行跳过并计数
export function parseJsonl(text) {
  const out = [];
  let bad = 0;
  for (const line of text.split('\n')) {
    const s = line.trim();
    if (!s) continue;
    try { out.push(JSON.parse(s)); } catch { bad++; }
  }
  out.sort((a, b) => (a.seq ?? 0) - (b.seq ?? 0));
  return { events: out, bad };
}

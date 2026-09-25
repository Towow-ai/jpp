"""运行时（§4 编译 pass、§6.0 执行模型）。

逐语句即时执行；`judge`/`do` 惰性登记；到刷新点（cut / fit / .content / gen·transform·ask 的输入含期物 /
程序返回）时：先解析 `do`（按依赖分波并发），再把已登记的 `judge` 排成**一层**，跑九个 pass，一层一次发出。
每个 pass 一个开关（`passes` 字典），给消融留门。
"""

from __future__ import annotations

import hashlib
import inspect
import json
import os
import threading
import time
import warnings
from concurrent.futures import ThreadPoolExecutor
from typing import Any, Callable

from foundation.core.canon import H, canon

from . import ir
from .calib import CalibStore, FitRegistry, cost_line
from .client import validate_answers
from .effects import DoEffect, JudgeEffect, MatFuture
from . import spec as _spec
from .ir import (_is, Act, Action, At, Budget, CalibRef, Escalated, Exit, Fail, Ignore, JvError, JvTypeError,
                 Mat, Pending, Pick, Q, Reading, Readings, ReadingsVec, State, Unsure, _Future, site_of,
                 TRUSTED, UNTRUSTED)
from .store import Books, MatStore, HANDLER_VERSION, cache_key, effect_key, ledger_key

PROFILE_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "profile", "profiles")
DEFAULT_PASSES = {"lift": True, "fuse": True, "fission": True, "lower": True, "schedule": True,
                  "plan": True, "ledger": True,
                  "speculate": True,       # 推测提升：刷新点向前找直线段（含分支体）里同帧可求值的 judge 站点（spec.py）
                  "vectorize": True}       # 循环向量化：宿主 for 体内无 loop-carried 依赖时，其余轮次的 judge 同层
_CURRENT: list["Runtime"] = []


class _Marker:
    def __init__(self, name):
        self.name = name

    def __repr__(self):
        return f"jv.{self.name}"


class _EscalateMarker(_Marker):
    """既是 handler 的 `then=jv.escalate` 标记，也可作 `return jv.escalate(x)` 调用。"""

    def __call__(self, payload: Any = None, note: str = "", exits=None) -> Escalated:
        rt = current()
        if exits is not None:
            cands = list(exits)
        elif _is(payload, Exit):
            cands = [payload]
        elif isinstance(payload, (list, tuple)) and payload and all(_is(x, Exit) for x in payload):
            cands = list(payload)
        else:
            cands = []
        marked = []
        for x in cands:                                   # 交人即消费：出口记为 escalate（不是 drop）
            if not _is(x, Exit):
                raise JvTypeError(f"escalate(exits=…) 只收出口，收到 {type(x).__name__}")
            x.__dict__["consumed"] = True
            try:
                x.detail["consumed_by"] = "escalate"
            except Exception:
                pass
            marked.append(x)
        e = Escalated(payload, note, exits=marked)
        rt._escalations += 1
        rt._check_escalate_budget()
        rt.stats["escalated"].append({"site": site_of(), "note": note, "exits": len(marked)})
        rt._note_escalate(with_exits=bool(marked))
        return e


escalate = _EscalateMarker("escalate")
drop = _Marker("drop")
provisional = _Marker("provisional")


class _Prior:
    none = "none"
    pass_count = "pass_count"


prior = _Prior()


def current() -> "Runtime":
    if not _CURRENT:
        raise JvError("没有运行时：先 jv.use(jv.Runtime(client=...)) 或 with jv.Runtime(...):")
    return _CURRENT[-1]


def load_profile(name: str = "jev-1.13.0") -> dict:
    p = os.path.join(PROFILE_DIR, f"{name}.json")
    with open(p, encoding="utf-8") as fh:
        return json.load(fh)


# 已决区边界的往返容差（与 Rust `jpp_value::stat::BOUNDARY_EPS` 同值同理由，步 15d-2）：线存 hi = h − δ，
# 判区算 hi + δ，浮点下可能比 h 大 1ulp；1e-12 只收回这个往返误差。依据：主会话 2026-09-25（15d-2）。
BOUNDARY_EPS = 1e-12


class Runtime:
    def __init__(self, client, *, profile: dict | str | None = "jev-1.13.0", root: str | None = None,
                 passes: dict | None = None, max_workers: int | None = None,
                 generator: Callable | None = None, calib: CalibStore | None = None,
                 fits: FitRegistry | None = None, retry: str = "none"):
        self.client = client
        self.profile = load_profile(profile) if isinstance(profile, str) else (profile or {})
        self.model_id = self.profile.get("model_version") or getattr(client, "model_id", "unknown")
        self._warned_sums1 = False
        self.root = root
        self.passes = dict(DEFAULT_PASSES, **(passes or {}))
        conc = int((self.profile.get("concurrency") or {}).get("lower_bound_ok", 8))
        self.max_workers = max_workers or min(8, conc)
        self.generator = generator
        self.calib = calib or CalibStore(os.path.join(root, "calib") if root else None)
        self.fits = fits or FitRegistry()
        self.retry = retry
        self.mats = MatStore(os.path.join(root, "mats.jsonl") if root else None)
        self.books: Books | None = None
        self.budget: Budget = Budget()
        self.program_name = ""
        self._seq = 0
        self._segment = 0
        self._fusion_ok = True
        self.pending_judges: list[JudgeEffect] = []
        self.pending_dos: list[DoEffect] = []
        self.exits: list[Exit] = []
        self._escalations = 0
        self._loop_keys: list[set] = []
        self._lock = threading.Lock()
        self._transform_seen: dict[str, str] = {}
        self._pending_ask: list[dict] = []
        self._frames: list[_Frame] = []                     # 嵌套程序的子账帧栈
        self._mem_books: dict[str, Books] = {}              # root=None 时按程序名保留在内存的账本（人答后重跑仍能重放）
        self._warned_actions: set[str] = set()
        self._speculating = False                            # spec.py 求值期间的重入保护
        self.reset_stats()

    # ------------------------------------------------------------ 生命周期
    def reset_stats(self):
        self.stats = {"calls": 0, "questions": 0, "tokens": 0, "cost": 0.0, "layers": [],
                      "ledger_hits": 0, "cache_hits": 0, "warnings": [], "unsure": 0, "exits": 0,
                      "escalated": [], "do": 0, "do_cost": 0.0, "gen": 0, "transform": 0, "ask": 0, "fission": 0,
                      "spec": {"lift": 0, "vectorize": 0, "hits": 0, "unused": 0, "dropped": 0, "aborted": 0,
                               "skipped": []},
                      "returned_unsure": 0}

    def __enter__(self):
        _CURRENT.append(self)
        return self

    def __exit__(self, *exc):
        _CURRENT.remove(self)
        self.mats.close()

    def use(self):
        _CURRENT.append(self)
        return self

    def warn(self, msg: str):
        self.stats["warnings"].append(msg)
        warnings.warn(msg, stacklevel=3)

    def header(self, budget: Budget) -> dict:
        return {"budget": budget.to_dict(), "profile_hash": H(self.profile),
                "model_id": self.model_id, "render_version": ir.RENDER_VERSION,
                "handler_version": HANDLER_VERSION, "retry": self.retry}

    def begin(self, name: str, budget: Budget, fn=None, returns_unsure: bool = False) -> "_Frame":
        """进入一个 `@jv.program`。最外层：重置统计、开账本、写账本头；内层：只压一个子账帧，复用运行时。
        `fn`：程序函数本体（推测提升要读它的 AST 与帧）；`returns_unsure`：返回注解含 Unsure（J-05 交给调用者）。"""
        if self._frames:                                        # 嵌套：程序调用程序
            f = _Frame(name, budget, parent=self._frames[-1], calls0=self.stats["calls"], cost0=self.stats["cost"],
                       layers0=len(self.stats["layers"]), esc0=self._escalations, depth=len(self._frames),
                       fn=fn, returns_unsure=returns_unsure)
            self._frames.append(f)
            return f
        self.program_name = name
        self.budget = budget
        self.reset_stats()
        self.pending_judges.clear()
        self.pending_dos.clear()
        self.exits.clear()
        self._escalations = 0
        self._segment = 0
        self._fusion_ok = True
        self._transform_seen.clear()
        self._pending_ask.clear()
        root = os.path.join(self.root, "books", name) if self.root else None
        if root is None and name in self._mem_books and self._mem_books[name].header_matches(self.header(budget)):
            self.books = self._mem_books[name]           # 无 root：同一运行时里同名程序复用内存账本（ask 人答后重跑即重放）
        else:
            self.books = Books(root, self.header(budget))
            if root is None:
                self._mem_books[name] = self.books
        if self.books.header_warning:
            self.warn(self.books.header_warning)          # J-18
        f = _Frame(name, budget, parent=None, calls0=0, cost0=0.0, layers0=0, esc0=0, depth=0,
                   fn=fn, returns_unsure=returns_unsure)
        self._frames = [f]
        return f

    def end(self, result: Any = None, *, check_consumed: bool = True):
        """离开当前帧：刷新；只核本帧作用域内的 Exit.consumed（J-05）；最外层落账本。"""
        f = self._frames[-1] if self._frames else None
        try:
            self.flush(reason="return")
            result = _materialize(result)                    # 返回值里的期物解析成 Mat（保留来源链与 taint；.content 一步可得）
            if self.books and (f is None or f.parent is None):
                self.books.save()
            if check_consumed:
                scope = f.exits if f is not None else self.exits
                in_result = _exits_in(result)
                handed: list = []
                bad: list = []
                for e in scope:
                    if not _is(e, Unsure) or e.__dict__.get("consumed") or e.__dict__.get("handed_from"):
                        continue
                    if f is not None and f.returns_unsure and id(e) in in_result:
                        handed.append(e)                     # J-05「被返回类型消费」：注解说了会返回 Unsure，交给调用者
                    else:
                        bad.append(e)
                if bad:
                    who = f"程序 {f.name} " if f is not None else "程序"
                    returned = [e for e in bad if id(e) in in_result]
                    fix = ("修法：用 match 的 case jv.Unsure(c) 或 jv.consume(exits, unsure=jv.drop) 消费它们")
                    if returned:
                        fix += ("；若本意是把 Unsure 原样交给调用者，给函数加返回注解 -> jv.Exit | jv.Unsure"
                                "（或 list[jv.Exit] 等含 Unsure 的类型），调用者按 J-05 处理它")
                    raise JvError(f"J-05: {who}返回前有 {len(bad)} 个未消费的 Unsure（{[e.cause for e in bad][:5]}）。{fix}。")
                for e in handed:
                    e.__dict__["consumed_by"] = "return_type"
                    e.__dict__["handed_from"] = f.name
                    if f.parent is not None:                  # 重新登记到调用者帧：那里它仍是未消费的
                        e.__dict__["handed_from"] = None
                        e.__dict__["handed_via"] = f.name
                        f.parent.exits.append(e)
                    else:                                     # 最外层：调用者是宿主，账本记 returned_unsure
                        self.stats["returned_unsure"] += 1
                        if self.books:
                            self.books.effects.put(effect_key("returned_unsure", f.name, e.cause, self.stats["returned_unsure"]),
                                                   {"kind": "returned_unsure", "cause": e.cause, "program": f.name})
                if f is not None:
                    unused = [j for j in f.spec_index.values() if j.speculative and j.done and not j.claimed and not j.dropped]
                    if unused:
                        self.stats["spec"]["unused"] += len(unused)
                        self.warn(f"W-spec-unused: 程序 {f.name} 推测发出的 {len(unused)} 个判断未被后续站点用到"
                                  f"（站点 {sorted({j.site for j in unused})[:3]}；题按状态融合，多花的是这些状态的调用）")
            if f is not None:
                self.stats.setdefault("frames", []).append(f.report(self))
            b = f.budget if f is not None else self.budget
            if b.unsure is not None:
                n_exits = len(f.exits) if f is not None else self.stats["exits"]
                n_unsure = sum(1 for e in (f.exits if f is not None else self.exits) if _is(e, Unsure))
                if n_exits and n_unsure / n_exits > b.unsure:
                    self.warn(f"W-unsure: {f.name if f else ''} 实测 unsure 率 {n_unsure / n_exits:.2f} 超预算 {b.unsure}（J-10）")
            return result
        finally:
            if f is not None and self._frames and self._frames[-1] is f:
                self._frames.pop()

    def abort_frame(self):
        """异常路径离开当前帧（Pending / 静态检查错 / 宿主异常），不核 J-05。"""
        if self._frames:
            self._frames.pop()

    @property
    def frame(self) -> "_Frame | None":
        return self._frames[-1] if self._frames else None

    # ------------------------------------------------------------ 统计（jv stats，§8-10）
    def stats_report(self) -> dict:
        """一条程序跑完后的层数、每层题数/调用数、融合率、账本命中、钱。"""
        layers = self.stats["layers"]
        q = sum(l["questions"] for l in layers)
        c = sum(l["calls"] for l in layers)
        return {"program": self.program_name, "layers": len(layers),
                "questions_per_layer": [l["questions"] for l in layers],
                "calls_per_layer": [l["calls"] for l in layers],
                "questions": q, "calls": c, "fusion_rate": round(q / c, 2) if c else None,
                "ledger_hits": self.stats["ledger_hits"], "cache_hits": self.stats["cache_hits"],
                "stopped_layers": sum(1 for l in layers if l.get("stopped")),
                "fission": self.stats["fission"], "tokens": self.stats["tokens"], "cost": round(self.stats["cost"], 6),
                "unsure": self.stats["unsure"], "exits": self.stats["exits"],
                "spec": {k: v for k, v in self.stats["spec"].items() if k != "skipped"},
                "returned_unsure": self.stats["returned_unsure"],
                "warnings": [w.split(":")[0] for w in self.stats["warnings"]]}

    # ------------------------------------------------------------ 档案数
    def delta_for(self, phys: str) -> float:
        d = (self.profile.get("delta") or {})
        key = {"noul": "noul", "choice": "choice_prob_chosen", "score": "score"}[phys]
        try:
            return float(d[key]["immediate"]["p99"])
        except (KeyError, TypeError):
            return {"noul": 0.05, "choice": 0.15, "score": 0.15}[phys]

    def text_window(self) -> int:
        """对象槽内单段材料的可用窗口：`window.text_slots.claim_bearing_ctx.usable_lower`（§2.1、B5）。"""
        try:
            return int(self.profile["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"])
        except (KeyError, TypeError, ValueError):
            pass
        try:
            return int(self.profile["window"]["noul_claim_bearing"]["bound"]["token"])
        except (KeyError, TypeError):
            return 500

    def safety_lines(self) -> tuple[float, float]:
        """冷校准键的保守线：只从档案 `lines.safety_default` 取（§1「凡是数字都是档案字段」）。
        字段缺失/未测 → J-15：取「无线」（hi=1, lo=0，临时出口全部 Unsure(band)）并报 W-untested。"""
        d = (self.profile.get("lines") or {}).get("safety_default")
        if not isinstance(d, dict) or d.get("value") == "未测" or "hi" not in d or "lo" not in d:
            if not getattr(self, "_warned_safety", False):
                self._warned_safety = True
                self.warn("W-untested: 档案缺 lines.safety_default（冷校准保守线），J-15 取无线：冷键不给临时出口")
            return 1.0, 0.0
        return float(d["hi"]), float(d["lo"])

    def json_ctx_window(self) -> int:
        try:
            ks = self.profile["window"]["json_slots"]["claim_bearing_ctx"]["flip_frac_by_ctx_tokens"]
            return max(int(k.strip("~")) for k in ks)
        except (KeyError, TypeError, ValueError):
            return 1800

    def k_limit(self, cand_tokens: int) -> tuple[int | None, str]:
        """返回 (K_max | None=未测, 档名)。"""
        kl = (self.profile.get("k_limit") or {}).get("by_candidate_tokens") or {}
        if cand_tokens <= 120:
            v = kl.get("<=120", {}).get("K_max", 16)
            return (int(v) if v != "未测" else None), "<=120"
        if cand_tokens < 300:
            v = kl.get("120-250", {}).get("K_max", "未测")
            return (int(v) if v != "未测" else None), "120-250"
        v = kl.get(">=300", {}).get("K_max", 4)
        return (int(v) if v != "未测" else None), ">=300"

    def price(self) -> float:
        try:
            return float(self.profile["cost"]["price_usd_per_input_token"])
        except (KeyError, TypeError):
            return 4.2e-8

    # ------------------------------------------------------------ 构造子（纯）
    @staticmethod
    def state(on, ctx=None, ref=None, over=None, repr="json") -> State:
        return State(on=on, ctx=list(ctx or []), ref=list(ref or []), over=list(over or []), repr=repr)

    def _next(self) -> int:
        self._seq += 1
        return self._seq

    # ------------------------------------------------------------ judge（惰性）
    def judge(self, s, *qs: Q):
        if not qs:
            raise JvError("judge 至少一题")
        for q in qs:
            if not isinstance(q, Q):
                raise JvTypeError(f"judge 的题必须是 jv.test/select/measure 的结果，收到 {type(q).__name__}")
        vec = isinstance(s, (list, tuple))
        states = list(s) if vec else [s]
        for st in states:
            if not isinstance(st, State):
                raise JvTypeError("J-11: judge 的状态必须是 jv.state(...) 的结果")
            if any(q.op == "select" for q in qs) and not st.over:
                raise JvError("select 题需要 over 槽（候选集）；修法：jv.state(..., over=[...])")
        eff = JudgeEffect(states=states, qs=list(qs), site=site_of(), seq=self._next(), rt=self,
                          vectorized=vec, segment=self._segment, frame=self.frame)
        eff.readings = ReadingsVec(eff) if vec else Readings(eff)
        self.pending_judges.append(eff)
        if not self.passes["lift"]:
            self.flush(reason="nolift")
        return eff.readings

    # ------------------------------------------------------------ do（惰性）
    def do(self, action: Action, *args, iter_seq: int | None = None, guard=None) -> MatFuture:
        if not isinstance(action, Action):
            raise JvTypeError("do 的第一个参数必须是 jv.Action")
        if iter_seq is None:
            raise JvError(f"J-13: do({action.name}) 缺 iter_seq。修法：循环里用 it.n / range 变量作序号，直线段用 0。")
        if action.taint_out == "trusted" and not action.registered and action.name not in self._warned_actions:
            self._warned_actions.add(action.name)
            self.warn(f"W-self-trusted: 动作 {action.name} 自声明 taint_out=trusted 但未经 S 库登记；可信来自来源登记（I6），"
                      f"不来自程序自报。修法：jv.register_action({action.name!r}, fn, taint_out='trusted', reason='为什么它的输出可信')，"
                      f"否则用 'inherit' / 'untrusted' 并让守卫经 jv.ask")
        if not action.reversible:
            self._check_guard(action, guard)                        # J-08
        eff = DoEffect(action=action, args=list(args), iter_seq=iter_seq, site=site_of(), seq=self._next(),
                       guard=guard)
        eff.future = MatFuture(self, eff)
        self.pending_dos.append(eff)
        self.stats["do"] += 1
        return eff.future

    def _check_guard(self, action: Action, guard):
        gs = guard if isinstance(guard, (list, tuple)) else ([guard] if guard is not None else [])
        ok = any(_is(g, Act) and g.taint == TRUSTED for g in gs) or \
             any(_is(g, Exit) and g.detail.get("from_ask") for g in gs)
        for g in gs:
            if _is(g, Exit):
                g.__dict__["consumed"] = True
        if not ok:
            kinds = [g.kind if _is(g, Exit) else type(g).__name__ for g in gs]
            hint = ""
            if any(k in ("pick", "at") for k in kinds):
                hint = " 守卫只收 test 题的 Act（一个被判为真的命题）；Pick/At 是选择或档位，不是命题，请再问一道 test 题作守卫。"
            elif any(k == "unsure" for k in kinds):
                hint = " Unsure 不能放行；先 handle（补证据 / 问人）。"
            elif any(k == "act" for k in kinds):
                hint = " Act 来自 untrusted 状态（gen 输出、不可信执行器输出）；可信来自来源登记（I6），请用 trusted 材料重判或经 jv.ask。"
            raise JvError(f"J-08: 不可逆动作 {action.name} 的守卫里没有来自 trusted 状态的 Act（或 ask 的答案）；收到 {kinds}。"
                          f"修法：guard=[e] 且 e 由 trusted 材料的 test 题产生，或经 jv.ask。{hint}")

    # ------------------------------------------------------------ gen（一登记就发）
    def gen(self, prompt: str, ctx=None, n: int = 4, retry_seq: int | None = None, generator=None) -> list[Mat]:
        if retry_seq is None:
            raise JvError("J-13: gen 缺 retry_seq。修法：循环里用 it.n / range 变量，regen 必须递增。")
        ctx = list(ctx or [])
        for m in ctx:
            ir._check_mat(m, "ctx")
        if any(isinstance(m, _Future) and m._resolved is None for m in ctx):
            self.flush(reason="gen-input")
        mats = [m.resolve() if isinstance(m, _Future) else (m.as_mat() if _is(m, Exit) else m) for m in ctx]
        site = site_of()
        key = effect_key("gen", site, H(prompt), [m.hash for m in mats], n, retry_seq)
        taint = ir._join_taint(m.taint for m in mats)
        rec = self.books.effects.get(key) if (self.books and self.passes["ledger"]) else None
        if rec is not None:
            outs = rec["outputs"]
        else:
            g = generator or self.generator
            if g is None:
                raise JvError("gen 没有生成器：Runtime(generator=...) 或 gen(..., generator=...)")
            t0 = time.time()
            try:
                outs = list(g(prompt, mats, n, retry_seq) or [])
            except Exception as e:                                   # 失败 = 空（E9b 30%）
                self.warn(f"W-gen-fail: {site} {e}")
                outs = []
            if self.books:
                self.books.effects.put(key, {"kind": "gen", "outputs": outs, "secs": round(time.time() - t0, 3)})
        self.stats["gen"] += 1
        res = []
        for i, o in enumerate(outs):
            m = Mat(content=o, origin=("gen", site, key, i), taint=taint)
            self.mats.add(m, site)
            res.append(m)
        return res

    # ------------------------------------------------------------ ask（异步；Pending 是程序级出口）
    def ask(self, s: State, q: Q) -> Exit:
        if not isinstance(s, State) or not isinstance(q, Q):
            raise JvTypeError("ask(state, q)")
        if not s.is_ready():
            self.flush(reason="ask-input")
        rs = s.resolved()
        key = effect_key("ask", "", rs.structure_hash, q.text_hash)
        self._escalations += 1
        self._check_escalate_budget()
        self.stats["ask"] += 1
        rec = self.books.effects.get(key) if self.books else None
        if rec and rec.get("answer"):
            a = rec["answer"]
            cls = {"act": Act, "ignore": Ignore, "pick": Pick, "at": At}[a["kind"]]
            kw = {"p": 1.0, "q_hash": q.text_hash, "taint": TRUSTED, "detail": {"from_ask": True}}
            e = cls(a["k"], **kw) if a["kind"] == "pick" else (cls(a["level"], **kw) if a["kind"] == "at" else cls(**kw))
            self._register_exit(e)
            return e
        if self.books:
            self.books.effects.put(key, {"kind": "ask", "state": rs.slots(), "q": q.text, "answer": None})
            self.books.save()
        raise Pending(key, rs.structure_hash, q.text_hash)

    def answer(self, key: str, kind: str, k: int | None = None, level: int | None = None):
        """人答到达（校准集入口）：写入效应账本，下次运行同名程序时 ask 返回该出口。

        kind：test 题答 "act" / "ignore"；select 题答 "pick" 并给 k（over 下标）；measure 题答 "at" 并给 level（scale 下标）。
        """
        if kind not in ("act", "ignore", "pick", "at"):
            raise JvError(f"answer 的 kind 只能是 act | ignore | pick | at，收到 {kind!r}。修法：test 题 'act'/'ignore'；"
                          f"select 题 'pick' 并给 k=；measure 题 'at' 并给 level=")
        if kind == "pick" and k is None:
            raise JvError("answer(kind='pick') 必须给 k=（over 的下标）")
        if kind == "at" and level is None:
            raise JvError("answer(kind='at') 必须给 level=（scale 的 0 起下标）")
        rec = self.books.effects.get(key) if self.books else None
        if rec is None:
            raise KeyError(f"{key}：这个运行时的账本里没有这条 ask。修法：在抛出 Pending 的同一个 Runtime 里调 jv.answer(e.key, …)，"
                           f"或给 Runtime(root=目录) 让账本落盘后在新运行时里答")
        rec["answer"] = {"kind": kind, "k": k, "level": level}
        self.books.effects.put(key, rec)
        self.books.save()

    def _check_escalate_budget(self):
        if not self._frames:
            if self.budget.escalate is not None and self._escalations > self.budget.escalate:
                raise JvError(f"J-07: escalate 次数 {self._escalations} 超预算 {self.budget.escalate}")
            return
        for f in self._frames:                                # 每一层子账都核（内层计入外层）
            used = self._escalations - f.esc0
            if f.budget.escalate is not None and used > f.budget.escalate:
                raise JvError(f"J-07: 程序 {f.name} escalate 次数 {used} 超预算 {f.budget.escalate}")

    # ------------------------------------------------------------ transform（记账）
    def transform(self, f: Callable, *args):
        if not callable(f):
            raise JvTypeError("transform 的第一个参数必须是可调用")
        if any(isinstance(a, _Future) and a._resolved is None for a in args):
            self.flush(reason="transform-input")
        mats = [a.resolve() if isinstance(a, _Future) else a for a in args]
        site = site_of()
        try:
            src = inspect.getsource(f)
        except (OSError, TypeError):
            src = getattr(f, "__qualname__", repr(f))
        f_hash = H(getattr(f, "__qualname__", "?"), hashlib.sha256(src.encode()).hexdigest()[:16])
        key = effect_key("transform", site, f_hash, [_arg_hash(m) for m in mats])
        taint = ir._join_taint(_taints(mats))
        shapes = self.__dict__.setdefault("_transform_shape", {})
        index: dict = {}                                          # 输入材料按内容哈希索引：子集 / 重排时保留原 Mat（来源链）
        for m in _flatten(mats):
            if isinstance(m, Mat):
                index.setdefault(H(m.content), m)
        list_like = shapes.get(f_hash) == "list" or _returns_list(f)   # 按函数记形状：同一函数在哪个站点返回过列表都算
        try:
            out = f(*mats)
        except Exception as e:                                   # J-12：宿主变换失败是值，不崩
            reason = f"{type(e).__name__}: {e}"
            fail = Fail(reason=reason, action=f"transform:{getattr(f, '__qualname__', '?')}")
            self.stats["transform"] += 1
            self.stats["transform_fail"] = self.stats.get("transform_fail", 0) + 1
            if list_like:                                        # 列表型：返回空列表（形状与成功时相同），.fail 记原因
                self.warn(f"W-transform-fail: transform({getattr(f, '__qualname__', f)}) 在 {site}：{reason}；"
                          f"列表型变换失败返回空列表（jv.FailList，形状与成功时相同），jv.on_fail(xs, 替代) 可换")
                fl = ir.FailList()
                fl.fail = fail
                if self.books and (self.books.effects.get(key) is None):
                    self.books.effects.put(key, {"kind": "transform", "out_hash": H({"fail": True}), "out": [],
                                                 "fail": reason, "shape": "list"})
                return fl
            self.warn(f"W-transform-fail: transform({getattr(f, '__qualname__', f)}) 在 {site}：{reason}")
            fm = Mat(content={"fail": reason, "transform": getattr(f, "__qualname__", "?")},
                     origin=("transform", site, key, "fail"), taint=taint)
            fm.__dict__["fail"] = fail
            self.mats.add(fm, site)
            if self.books and (self.books.effects.get(key) is None):
                self.books.effects.put(key, {"kind": "transform", "out_hash": H({"fail": True}), "out": fm.content,
                                             "fail": fm.content["fail"], "shape": "one"})
            return fm
        shapes[f_hash] = "list" if isinstance(out, (list, tuple)) else "one"
        out_hash = H(_plain(out))
        prev = self._transform_seen.get(key)
        rec = self.books.effects.get(key) if (self.books and self.passes["ledger"]) else None
        replay_val = None
        if prev is not None and prev != out_hash:
            self._impure(f, site)
        elif rec is not None and rec["out_hash"] != out_hash:
            self._impure(f, site)
            replay_val = rec["out"]
        self._transform_seen[key] = out_hash if prev is None else prev
        if self.books and rec is None:
            self.books.effects.put(key, {"kind": "transform", "out_hash": out_hash, "out": _plain(out),
                                         "shape": shapes[f_hash]})
        self.stats["transform"] += 1
        val = replay_val if replay_val is not None else out
        return self._wrap_out(val, ("transform", site, key), taint, index)

    def _impure(self, f, site):
        self.warn(f"W-impure: transform({getattr(f, '__qualname__', f)}) 在 {site} 同输入异输出；本直线段禁融合，重放取账本。")
        self._fusion_ok = False

    def _wrap_out(self, val, origin: tuple, taint: str, index: dict | None = None):
        if isinstance(val, Mat):
            return val
        if isinstance(val, dict) and set(val) == {"__mat__"}:
            val = val["__mat__"]
        if isinstance(val, (list, tuple)):
            return [self._wrap_out(v, origin + (i,), taint, index) for i, v in enumerate(val)]
        if index:                                                 # 与某个输入材料内容相同 → 就是那份材料（来源链保留）
            m0 = index.get(H(val))
            if m0 is not None:
                return m0
        m = Mat(content=val, origin=origin, taint=taint)
        self.mats.add(m, origin[1] if len(origin) > 1 else "")
        return m

    # ------------------------------------------------------------ loop
    def loop(self, bound: int, variant=None):
        if bound is None or variant is None:
            raise JvError("J-06: loop 必带 bound 与 variant。修法：jv.loop(bound=N, variant=jv.decreasing(lambda: 计量))")
        return _Loop(self, bound, variant)

    # ------------------------------------------------------------ 刷新（层边界）
    def flush(self, reason: str = ""):
        self._resolve_dos()
        if self._speculating:                                 # 推测求值期间的嵌套刷新：只解析 do，判断留给外层
            return
        if reason == "cut" and self.passes["lift"] and self.frame is not None \
                and (self.passes.get("speculate", True) or self.passes.get("vectorize", True)):
            self._speculate()
        ready = [j for j in self.pending_judges if j.ready() and not j.done]
        ready, aliases = self._alias_ready(ready)
        if ready:
            self.pending_judges = [j for j in self.pending_judges if j not in ready]
            self._run_layer(ready, reason)
            self._segment += 1
            self._fusion_ok = True
        for j, sp in aliases:
            self._copy_readings(j, sp)

    # —— 推测提升 / 循环向量化（spec.py）
    def _speculate(self):
        f = self.frame
        fn = getattr(f, "fn", None)
        if fn is None:
            return
        code = getattr(fn, "__code__", None)
        fr = inspect.currentframe()
        prog = None
        while fr is not None:                                 # 找到正在执行的程序函数帧
            if fr.f_code is code:
                prog = fr
                break
            fr = fr.f_back
        if prog is None:
            return
        self._speculating = True
        try:
            st = _spec.speculate(self, prog, fn, self.passes.get("speculate", True), self.passes.get("vectorize", True))
        except Exception as e:                                # 推测永远不许改变程序语义：出错只记
            st = {"aborted": 1}
            self.stats["spec"]["skipped"].append(f"error:{type(e).__name__}")
        finally:
            self._speculating = False
        self.stats["spec"]["aborted"] += int(st.get("aborted", 0))

    def _spec_key(self, states: list, qs: list, run_seq: int = 0):
        return (tuple(s.resolved().structure_hash for s in states),
                tuple((q.text_hash, q.op, q.calib.key, q.phys, tuple(q.scale or ()), q.prior, tuple(q.evidence), q.agg)
                      for q in qs), run_seq)

    def _register_spec(self, st, qs: list, site: str, from_site: str, kind: str):
        """登记一条推测的判断（同层发出）。同键已有效应（真或推测）→ 不重复。状态含未解析期物 → 不推测。"""
        f = self.frame
        if f is None:
            return None
        vec = isinstance(st, (list, tuple))
        states = list(st) if vec else [st]
        if not states or not all(isinstance(x, State) and x.is_ready() for x in states):
            return None
        if any(q.op == "select" and not x.over for q in qs for x in states):
            return None
        key = self._spec_key(states, qs)
        if key in f.spec_index:
            return None
        for j in self.pending_judges:                         # 已登记未刷新的真效应（含触发本次刷新的那个）
            if j.ready() and not j.done and self._spec_key(j.states, j.qs, j.run_seq) == key:
                f.spec_index[key] = j
                return None
        eff = JudgeEffect(states=states, qs=list(qs), site=site, seq=self._next(), rt=self,
                          vectorized=vec, segment=self._segment, frame=f, speculative=True,
                          spec_from=from_site, spec_kind=kind)
        eff.readings = ReadingsVec(eff) if vec else Readings(eff)
        self.pending_judges.append(eff)
        f.spec_index[key] = eff
        self.stats["spec"][kind] += 1
        pair = (from_site, site)
        if pair not in f.lift_warned:
            f.lift_warned.add(pair)
            if kind == "lift":
                self.warn(f"W-lift: 站点 {site} 的题「{qs[0].text[:12]}」已随站点 {from_site} 一起发出"
                          f"（推测提升：judge 无世界效应，推错不需回滚；没用到会记 W-spec-unused）")
            else:
                self.warn(f"W-lift: 站点 {site} 的题「{qs[0].text[:12]}」其余轮次已随本轮一起发出"
                          f"（循环向量化：体内无 loop-carried 依赖、无 do/ask，其余轮次的 judge 同层并发）")
        return eff

    def _note_spec(self, why: str):
        sk = self.stats["spec"]["skipped"]
        if why not in sk:
            sk.append(why)

    def _alias_ready(self, ready: list):
        """真站点命中已推测的同键效应：不再发、不再多一层。"""
        f = self.frame
        if f is None:
            return ready, []
        keep, aliases = [], []
        for j in ready:
            if j.speculative:
                keep.append(j)
                continue
            try:
                key = self._spec_key(j.states, j.qs, j.run_seq)
            except Exception:
                keep.append(j)
                continue
            sp = f.spec_index.get(key)
            if sp is not None and sp is not j and sp.speculative and not sp.dropped and (sp.done or sp in ready):
                aliases.append((j, sp))
                self.pending_judges = [x for x in self.pending_judges if x is not j]
                continue
            f.spec_index.setdefault(key, j)
            keep.append(j)
        return keep, aliases

    def _copy_readings(self, j: JudgeEffect, sp: JudgeEffect):
        """把推测效应的读数交给真站点的读数对象（同状态同题，读数就是同一次调用的答案）。"""
        for k in range(len(j.states)):
            src = sp.readings[k] if sp.vectorized else sp.readings
            dst = j.readings[k] if j.vectorized else j.readings
            for qi in range(len(j.qs)):
                dst[qi]._ans = src[qi]._ans
        j.done = True
        sp.claimed = True
        self.stats["spec"]["hits"] += 1

    # —— do：按依赖分波并发
    def _resolve_dos(self):
        while True:
            todo = [d for d in self.pending_dos if not d.done]
            if not todo:
                self.pending_dos.clear()
                return
            wave = [d for d in todo if d.ready()]
            if not wave:
                raise JvError("do 依赖环")
            if len(wave) > 1 and any(d.deps() for d in todo if d not in wave):
                pass
            if self.passes["schedule"] and len(wave) > 1:
                with ThreadPoolExecutor(max_workers=self.max_workers) as ex:
                    list(ex.map(self._exec_do, wave))
            else:
                for d in wave:
                    self._exec_do(d)

    def _exec_do(self, d: DoEffect):
        args = [a.resolve() if isinstance(a, _Future) else (a.as_mat() if _is(a, Exit) else a) for a in d.args]
        arg_hashes = [_arg_hash(a) for a in args]
        d.key = effect_key("do", d.site, d.action.name, arg_hashes, d.iter_seq)
        in_taint = ir._join_taint(_taints(args))
        taint = {"trusted": TRUSTED, "untrusted": UNTRUSTED, "inherit": in_taint}[d.action.taint_out]
        rec = self.books.effects.get(d.key) if (self.books and self.passes["ledger"]) else None
        if rec is None:
            fn = d.action.fn
            if fn is None:
                from foundation.core import registry
                fn = registry.lookup(d.action.name)
            t0 = time.time()
            try:
                out = fn(*args)
                rec = {"kind": "do", "out": _plain(out), "fail": None, "secs": round(time.time() - t0, 3)}
            except Exception as e:
                rec = {"kind": "do", "out": None, "fail": f"{type(e).__name__}: {e}", "secs": round(time.time() - t0, 3)}
            if self.books:
                with self._lock:
                    self.books.effects.put(d.key, rec)
        if rec["fail"] is not None:
            d.future.fail = Fail(reason=rec["fail"], action=d.action.name)
            m = Mat(content={"fail": rec["fail"], "action": d.action.name}, origin=("do", d.site, d.key, "fail"), taint=taint)
        else:
            m = Mat(content=rec["out"], origin=("do", d.site, d.key), taint=taint)
        with self._lock:
            self.mats.add(m, d.site)
            if d.action.cost:                                # Action.cost 计入帧预算的 cost（层边界核）
                self.stats["cost"] += float(d.action.cost)
                self.stats["do_cost"] += float(d.action.cost)
        d.future._resolved = m
        d.done = True

    # —— judge：一层
    def _run_layer(self, effs: list[JudgeEffect], reason: str):
        plans = []                                        # 每个 (effect, obj_index) 一个 plan
        for j in effs:
            for k, st in enumerate(j.states):
                rs = st.resolved()
                self._check_window(rs, j.site)
                plans.append(self._plan(j, k, rs))
        calls = self._calls_from_plans(plans)
        groups = {c["group"] for c in calls}
        n_layer = len(self.stats["layers"]) + 1
        # 预算（层边界核，J-07；属 plan pass，关掉即不核）。嵌套程序：每个子账帧各核自己的预算，
        # 某帧超 → 该帧及其内层登记的判断停并记 Unsure(budget)，外层的判断照发；最外层超 → 整层停。
        stopped_plans: list = []
        if self.passes["plan"]:
            over = self._frames_over_budget(n_layer, plans, calls)
            if over and any(p["eff"].speculative for p in plans):     # 超预算：先丢推测的，再看真站点
                dropped = [p for p in plans if p["eff"].speculative]
                for p in dropped:
                    eff = p["eff"]
                    eff.done = True
                    eff.dropped = True
                    for item in p["items"]:
                        item["reading"]._ans = {"phys": item["phys"], "stop": "budget", "p": 0.0, "value": None, "probs": {}}
                self.stats["spec"]["dropped"] += len(dropped)
                plans = [p for p in plans if not p["eff"].speculative]
                effs = [j for j in effs if not j.speculative]
                if not plans:
                    return
                calls = self._calls_from_plans(plans)
                groups = {c["group"] for c in calls}
                over = self._frames_over_budget(n_layer, plans, calls)
            if over:
                stopped_plans = [p for p in plans if any(fr in over for fr in _chain(p["eff"].frame))] \
                    if self._frames else list(plans)
        if stopped_plans:
            names = sorted({fr.name for p in stopped_plans for fr in _chain(p["eff"].frame) if fr in over}) if self._frames else [self.program_name]
            self.warn(f"W-budget: 第 {n_layer} 层 程序 {names} 超预算，其 {len(stopped_plans)} 个状态不发，读数记 Unsure(budget)")
            for p in stopped_plans:
                for item in p["items"]:
                    item["reading"]._ans = {"phys": item["phys"], "stop": "budget", "p": 0.0, "value": None, "probs": {}}
            keep = [p for p in plans if p not in stopped_plans]
            if not keep:
                for j in effs:
                    j.done = True
                self.stats["layers"].append({"reason": reason, "calls": 0, "questions": 0, "states": len(plans), "stopped": True})
                return
            plans = keep
            calls = self._calls_from_plans(plans)
        # 发出
        if self.passes["schedule"] and len(calls) > 1:
            with ThreadPoolExecutor(max_workers=self.max_workers) as ex:
                list(ex.map(self._exec_call, calls))
        else:
            for c in calls:
                self._exec_call(c)
        # 合回读数
        for p in plans:
            self._collect(p)
        for j in effs:
            j.done = True
        self.stats["layers"].append({"reason": reason, "calls": sum(1 for c in calls if c.get("sent")),
                                     "questions": sum(len(c["items"]) for c in calls), "states": len(plans),
                                     "fused_groups": len(groups)})

    def _calls_from_plans(self, plans: list) -> list:
        """融合：同状态哈希 + 同段（W-impure 段不融合）。关 fuse = 逐题调用（§4 表「不做会坏什么」）。"""
        groups: dict[str, list] = {}
        for p in plans:
            fuse = self.passes["fuse"] and (self._fusion_ok or p["eff"].segment != self._segment)
            if fuse:
                groups.setdefault(p["state_hash"], []).append(p)
            else:
                for item in p["items"]:
                    groups[f"{p['state_hash']}#{item['qid']}"] = [dict(p, items=[item])]
        calls = []
        for gk, ps in groups.items():
            qmap: dict[str, dict] = {}
            for p in ps:
                for item in p["items"]:
                    qmap[item["qid"]] = item
            ids = list(qmap)                                  # 一次调用 ≤ 200 题
            for i in range(0, len(ids), 200):
                calls.append({"state": ps[0]["render"], "state_hash": ps[0]["state_hash"], "group": gk,
                              "items": {qid: qmap[qid] for qid in ids[i:i + 200]},
                              "frames": {fr for p in ps for fr in _chain(p["eff"].frame)}})
        return calls

    def _frames_over_budget(self, n_layer: int, plans: list, calls: list) -> set:
        """哪些子账帧在本层会超预算（层 / 调用 / 钱）。无帧时用 self.budget（返回 {None} 表示整层停）。"""
        if not self._frames:
            b = self.budget
            if (b.layers is not None and n_layer > b.layers) or \
               (b.calls is not None and self.stats["calls"] + len(calls) > b.calls) or \
               (b.cost is not None and self.stats["cost"] >= b.cost):
                return {None}
            return set()
        over = set()
        for f in self._frames:
            b = f.budget
            planned = sum(1 for c in calls if f in c["frames"])
            if b.layers is not None and (n_layer - f.layers0) > b.layers:
                over.add(f)
            if b.calls is not None and (self.stats["calls"] - f.calls0) + planned > b.calls:
                over.add(f)
            if b.cost is not None and (self.stats["cost"] - f.cost0) >= b.cost:
                over.add(f)
        return over

    def _check_window(self, rs, site: str):
        if not self.passes["fission"]:
            return
        ctx_tokens = sum(m.tokens for m in rs.ctx) + sum(m.tokens for m in rs.ref)
        if ctx_tokens > self.json_ctx_window():
            self.warn(f"W-window: {site} 语境槽 {ctx_tokens} token 超 JSON 槽已测窗口 {self.json_ctx_window()}（P24 上限未测）")

    def _plan(self, j: JudgeEffect, k: int, rs) -> dict:
        """下沉 + 裂变：把一个 (状态, 题组) 变成物理题项。"""
        items: list[dict] = []
        state_hash = rs.structure_hash
        render = rs.render()
        on_tokens = sum(m.tokens for m in (rs.on if isinstance(rs.on, tuple) else (rs.on,)))
        for qi, q in enumerate(j.qs):
            reading = j.readings[k][qi] if j.vectorized else j.readings[qi]
            base = {"reading": reading, "q": q, "site": j.site, "run_seq": j.run_seq, "chunk": None}
            pre = f"e{j.seq}_{k}_"
            if q.op == "test":
                phys = "noul"
                if self.passes["fission"] and on_tokens > self.text_window() and not isinstance(rs.on, tuple) \
                        and isinstance(rs.on.content, str):
                    chunks = _chunk(rs.on.content, self.text_window())
                    self.stats["fission"] += 1
                    for ci, ch in enumerate(chunks):
                        sub = ir.ResolvedState(on=Mat(content=ch, origin=rs.on.origin, taint=rs.on.taint,
                                                      derived_from=rs.on.derived_from), ctx=rs.ctx, ref=rs.ref, over=())
                        items.append(dict(base, qid=f"{pre}q{qi}_c{ci}", phys=phys, perm_seed=0, chunk=ci,
                                          question={"type": "noul", "instructions": q.text},
                                          sub_state=sub.render(), sub_hash=sub.structure_hash))
                    continue
                items.append(dict(base, qid=f"{pre}q{qi}", phys=phys, perm_seed=0,
                                  question={"type": "noul", "instructions": q.text}))
            elif q.op == "select":
                K = len(rs.over)
                sums1 = self.profile.get("select_sums_to_one")   # H2 类假设绑档案字段；未测按 J-15 取真并告警
                if K == 1 and sums1 is None and not self._warned_sums1:
                    self._warned_sums1 = True
                    self.warn("W-untested: 档案未测 select_sums_to_one；单候选 select 按 H2 取真直接 Pick(0)")
                if K == 1 and sums1 is not False:             # over 只剩一个候选：不发调用，cut 得 Pick(0)（G6 猜 16 的提议）
                    items.append(dict(base, qid=f"{pre}q{qi}_t", phys="choice", perm_seed=0, trivial=True,
                                      question={"type": "choice", "instructions": q.text, "criteria": {"c0": rs.over[0].text()}}))
                    continue
                cand_tokens = max(m.tokens for m in rs.over)
                kmax, band = self.k_limit(cand_tokens)
                hard = int((self.profile.get("k_limit") or {}).get("hard_max_options", 255))
                use_choice = self.passes["lower"] and kmax is not None and K <= kmax and K <= hard
                if self.passes["lower"] and kmax is None:
                    self.warn(f"W-untested: 候选 {cand_tokens} token 落在档案未测档 {band}，select 取保守物理形式 K-noul（J-15）")
                if q.phys == "choice":
                    use_choice = K <= hard
                # 裂变（§4.3）：对象槽超窗 → 分块 K-noul，每块每候选一题，合回按候选取 max 再决
                if self.passes["fission"] and on_tokens > self.text_window() and not isinstance(rs.on, tuple) \
                        and isinstance(rs.on.content, str):
                    chunks = _chunk(rs.on.content, self.text_window())
                    self.stats["fission"] += 1
                    for ci, ch in enumerate(chunks):
                        sub = ir.ResolvedState(on=Mat(content=ch, origin=rs.on.origin, taint=rs.on.taint,
                                                      derived_from=rs.on.derived_from), ctx=rs.ctx, ref=rs.ref, over=rs.over)
                        for i in range(K):
                            items.append(dict(base, qid=f"{pre}q{qi}_c{ci}_k{i}", phys="noul", perm_seed=0, cand=i, chunk=ci,
                                              question={"type": "noul", "instructions": f"对候选 c{i}：{q.text}"},
                                              sub_state=sub.render(), sub_hash=sub.structure_hash))
                    continue
                if use_choice:
                    opts = [f"c{i}" for i in range(K)]
                    perms = [list(range(K)), list(range(K))[::-1]] if K > 1 else [list(range(K))]
                    for ps, perm in enumerate(perms):
                        # 选项描述直接放候选原文（一跳字面，H4）：E-IR-SMOKE 第一遍证明「候选 c0 → 查槽」是多出的一跳
                        crit = {opts[i]: rs.over[i].text() for i in perm}
                        items.append(dict(base, qid=f"{pre}q{qi}_p{ps}", phys="choice", perm_seed=ps,
                                          question={"type": "choice", "instructions": q.text, "criteria": crit}))
                else:
                    for i in range(K):
                        items.append(dict(base, qid=f"{pre}q{qi}_k{i}", phys="noul", perm_seed=0, cand=i,
                                          question={"type": "noul", "instructions": f"对候选 c{i}：{q.text}"}))
            else:
                levels = list(q.scale)
                # 裂变（§4.3）：对象槽超窗 → 每块一道 score，合回按出口计数
                if self.passes["fission"] and on_tokens > self.text_window() and not isinstance(rs.on, tuple) \
                        and isinstance(rs.on.content, str):
                    chunks = _chunk(rs.on.content, self.text_window())
                    self.stats["fission"] += 1
                    for ci, ch in enumerate(chunks):
                        sub = ir.ResolvedState(on=Mat(content=ch, origin=rs.on.origin, taint=rs.on.taint,
                                                      derived_from=rs.on.derived_from), ctx=rs.ctx, ref=rs.ref, over=())
                        items.append(dict(base, qid=f"{pre}q{qi}_c{ci}", phys="score", perm_seed=0, chunk=ci,
                                          question={"type": "score", "instructions": q.text, "criteria": levels},
                                          sub_state=sub.render(), sub_hash=sub.structure_hash))
                    continue
                items.append(dict(base, qid=f"{pre}q{qi}", phys="score", perm_seed=0,
                                  question={"type": "score", "instructions": q.text, "criteria": levels}))
        for it in items:                                  # 键里的题标识不含效应序号（同状态同题同站点 → 同键，重放/键重复即停）
            it["kid"] = it["qid"][len(f"e{j.seq}_{k}_"):]
        return {"eff": j, "k": k, "rs": rs, "state_hash": state_hash, "render": render, "items": items}

    def _exec_call(self, call: dict):
        """一次融合调用：先查账本/缓存，缺的才发。子状态（裂变块）单独发。"""
        by_state: dict[str, dict] = {}
        for qid, it in call["items"].items():
            sh = it.get("sub_hash", call["state_hash"])
            g = by_state.setdefault(sh, {"render": it.get("sub_state", call["state"]), "items": {}})
            g["items"][qid] = it
        for sh, g in by_state.items():
            missing = {}
            for qid, it in g["items"].items():
                if it.get("trivial"):                         # 单候选 select：不发、不记账、不计钱
                    it["answer"] = {"type": "choice", "choice": "c0", "probabilities": {"c0": 1.0}, "trivial": True}
                    self.stats["trivial"] = self.stats.get("trivial", 0) + 1
                    continue
                kid = it.get("kid", qid)
                lk = ledger_key(self.model_id, sh, it["q"].text_hash, it["phys"], ir.RENDER_VERSION,
                                it["perm_seed"], it["run_seq"], it["site"] + "/" + kid)
                ck = cache_key(sh, it["q"].text_hash + "/" + kid, it["phys"], ir.RENDER_VERSION, self.model_id)
                it["ledger_key"], it["cache_key"] = lk, ck
                ans = self.books.ledger.get(lk) if (self.books and self.passes["ledger"]) else None
                if ans is not None:
                    it["answer"] = ans
                    self.stats["ledger_hits"] += 1
                    continue
                cans = self.books.cache.get(ck) if (self.books and self.passes["ledger"]) else None
                if cans is not None and it["run_seq"] == 0:
                    it["answer"] = cans
                    self.stats["cache_hits"] += 1
                    if self.books:
                        self.books.ledger.put(lk, cans)
                    continue
                missing[qid] = it["question"]
            if not missing:
                continue
            try:
                answers, tokens, cost = self.client.ask(g["render"], missing)
            except Exception as e:
                self.warn(f"W-call-fail: {e}")
                for qid in missing:
                    g["items"][qid]["answer"] = {"type": "fail", "error": str(e)}
                continue
            answers = validate_answers(missing, answers)         # 返回体键不合即 JvError（不静默成 Unsure）
            call["sent"] = True
            with self._lock:
                self.stats["calls"] += 1
                self.stats["questions"] += len(missing)
                self.stats["tokens"] += tokens
                self.stats["cost"] += cost
            for qid in missing:
                a = answers[qid]
                g["items"][qid]["answer"] = a
                if self.books:
                    with self._lock:
                        self.books.ledger.put(g["items"][qid]["ledger_key"], a)
                        self.books.cache.put(g["items"][qid]["cache_key"], a)

    def _collect(self, p: dict):
        """把物理题项合回每题一条读数。"""
        by_q: dict[int, list[dict]] = {}
        for it in p["items"]:
            by_q.setdefault(id(it["reading"]), []).append(it)
        rs = p["rs"]
        for items in by_q.values():
            r: Reading = items[0]["reading"]
            q: Q = items[0]["q"]
            if any(it["answer"].get("type") == "fail" for it in items):
                r._ans = {"phys": items[0]["phys"], "fail": True, "p": 0.0, "value": None, "probs": {}}
                continue
            if any(isinstance(m.content, dict) and "fail" in m.content and m.origin[:1] in (("do",), ("transform",))
                   for m in rs.all_mats):
                r._ans = {"phys": items[0]["phys"], "fail": True, "p": 0.0, "value": None, "probs": {}}
                continue
            if q.op == "test":
                ps = [float(it["answer"]["noul"]) for it in items]
                p_ = max(ps) if q.agg == "exists" else min(ps)
                new = {"phys": "noul", "p": p_, "value": p_, "probs": {"noul": p_}, "chunks": len(items) if items[0]["chunk"] is not None else 0}
            elif q.op == "select":
                if items[0].get("trivial"):
                    new = {"phys": "choice", "p": 1.0, "value": 0, "probs": {0: 1.0}, "mode_share": 1.0, "perms": 0, "trivial": True}
                elif items[0]["phys"] == "choice":
                    picks, probs_all = [], []
                    for it in items:
                        a = it["answer"]
                        probs = {kk: float(v) for kk, v in a["probabilities"].items()}
                        opt = a.get("choice") or max(probs, key=probs.get)
                        picks.append(int(opt[1:]))
                        probs_all.append({int(kk[1:]): v for kk, v in probs.items()})
                    from collections import Counter
                    mode, cnt = Counter(picks).most_common(1)[0]
                    p_ = sum(pr.get(mode, 0.0) for pr in probs_all) / len(probs_all)
                    new = {"phys": "choice", "p": p_, "value": mode, "probs": probs_all[0],
                           "mode_share": cnt / len(picks), "perms": len(items)}
                else:
                    probs: dict = {}
                    for it in items:                      # 裂变块：同候选跨块取 max（exists）
                        probs[it["cand"]] = max(probs.get(it["cand"], 0.0), float(it["answer"]["noul"]))
                    order = sorted(probs, key=lambda i: -probs[i])
                    top = order[0]
                    second = probs[order[1]] if len(order) > 1 else 0.0
                    n_chunks = len({it["chunk"] for it in items}) if items[0]["chunk"] is not None else 0
                    new = {"phys": "noul", "p": probs[top], "value": top, "probs": probs, "second": second,
                           "mode_share": 1.0, "knoul": True, "chunks": n_chunks}
            else:
                from collections import Counter
                from foundation.core.outlet import score_level
                lv_all, pr_all = [], []
                for it in items:
                    a = it["answer"]
                    pr = {str(kk): float(v) for kk, v in a["probabilities"].items()}
                    lv_all.append(score_level(a["score"], len(q.scale)))
                    pr_all.append(pr)
                if len(items) == 1:
                    lvl = lv_all[0]
                    new = {"phys": "score", "p": pr_all[0].get(str(lvl), 0.0), "value": lvl, "probs": pr_all[0]}
                else:                                     # 裂变块：按出口计数，众数档；p = 众数占比 × 众数档均值概率
                    lvl, cnt = Counter(lv_all).most_common(1)[0]
                    share = cnt / len(lv_all)
                    mean_p = sum(pr.get(str(lvl), 0.0) for pr in pr_all) / len(pr_all)
                    new = {"phys": "score", "p": share * mean_p, "value": lvl, "probs": pr_all[0],
                           "mode_share": share, "chunks": len(items)}
            if r._ans is not None and r._ans.get("runs") is not None:
                runs = r._ans["runs"] + [new]
                r._ans = dict(new, runs=runs)
            else:
                r._ans = new

    # ------------------------------------------------------------ cut（内核桥）
    def cut(self, r, cost: tuple | None = None):
        if isinstance(r, (ReadingsVec, list, tuple)):     # 向量化：出口列表与输入顺序一一对齐
            return [self.cut(x, cost) for x in r]
        if isinstance(r, Readings):
            if len(r) != 1:
                raise JvError("cut 收到多题读数向量；修法：jv.cut(r[i]) 指定题")
            r = r[0]
        if _is(r, Exit):
            return r
        if not isinstance(r, Reading):
            raise JvTypeError(f"J-01: cut 只接受读数，收到 {type(r).__name__}")
        ans = r._need()
        q = r.q
        rs = r.effect.states[r.obj_index].resolved()
        kw = {"q_hash": q.text_hash, "taint": rs.taint, "reading": r}
        rec = self.calib.get(q.calib.key)
        # J-02 禁自指：状态里含由本题派生的材料
        if q.text_hash in rs.derived_from:
            raise JvError(f"J-02: 由题「{q.text[:20]}」派生的材料回到了再问该题的状态（禁自指）。"
                          f"修法：换题，或把派生材料放进另一条校准键的题。")
        if ans.get("stop"):
            return self._register_exit(Unsure(ans["stop"], **kw))
        if ans.get("fail"):
            return self._register_exit(Unsure("fail", **kw))
        if ans.get("trivial"):                                   # 单候选 select：没有读数可校准，直接 Pick(0)
            return self._register_exit(Pick(0, p=1.0, detail={"trivial": "over 只有一个候选，未发调用"}, **kw))
        for slot in q.evidence:                                  # J-09 先于信任 p
            v = getattr(rs, slot, None)                          # on 是单个 Mat（禁 bool/len），只查 None 与空列表
            if v is None or (isinstance(v, (list, tuple)) and not v):
                return self._register_exit(Unsure("insufficient", detail={"missing": slot}, **kw))
        if rec.status == "停岗":
            return self._register_exit(Unsure("drift", **kw))
        delta = rec.delta if rec.delta is not None else self.delta_for(ans["phys"])
        if rec.status == "冷":
            s_hi, s_lo = self.safety_lines()
            prov = self._decide(ans, q, rs, s_hi, s_lo, delta, kw, provisional=True)
            return self._register_exit(Unsure("cold", detail={"provisional": prov}, **kw))
        hi, lo = rec.hi, rec.lo
        cost_mat = cost if cost is not None else ((rec.cost_matrix.get("fp"), rec.cost_matrix.get("fn"))
                                                  if rec.cost_matrix else None)
        if cost_mat is not None:                                 # 代价比线（§2.3）：只从标注集算，不由程序手写
            if q.op != "test":
                raise JvError(f"cut(cost=) 只对 test 题有定义（select/measure 的代价线未定）；题「{q.text[:20]}」是 {q.op}")
            if not rec.samples:
                self.warn(f"W-cost-unfit: 校准键 {q.calib.key} 无标注集（samples），cost 参数忽略；线只从记录来（I4）")
            elif not rec.label_set_id or rec.label_set_id == rec.set_id:
                raise JvError(f"J-16: 校准键 {q.calib.key} 的标注集 id（{rec.label_set_id!r}）必须给出且 ≠ 保形集 id（{rec.set_id!r}）"
                              f"；代价线与保形线不能同源。修法：calib.put(..., samples=…, label_set_id='…', set_id='…') 两者不同")
            else:
                cl = cost_line(rec.samples, *cost_mat)
                hi = lo = cl["line"]
                kw["detail"] = {"cost_line": cl}
        return self._register_exit(self._decide(ans, q, rs, hi, lo, delta, kw))

    def _decide(self, ans: dict, q: Q, rs, hi: float, lo: float, delta: float, kw: dict, provisional=False) -> Exit:
        kw = dict(kw, provisional=provisional)
        p = float(ans["p"])
        if q.op == "test":
            if p >= hi + delta - BOUNDARY_EPS:
                return Act(p=p, **kw)
            if p <= lo - delta + BOUNDARY_EPS:
                return Ignore(p=p, **kw)
            return Unsure("band", p=p, **kw)
        if q.op == "select":
            if q.prior == "pass_count":
                pc = [(m.content.get("pass_count") if isinstance(m.content, dict) else None) for m in rs.over]
                if all(x is not None for x in pc):
                    best = max(pc)
                    if pc[0] == best:
                        return Pick(0, p=1.0, detail={"prior": "default-keeps"}, **kw)
                    elig = [i for i, x in enumerate(pc) if x == best]
                    if len(elig) == 1:
                        return Pick(elig[0], p=1.0, detail={"prior": "pass-count"}, **kw)
                    probs = {i: ans["probs"].get(i, 0.0) for i in elig}
                    top = max(probs, key=probs.get)
                    return Pick(top, p=probs[top], detail={"prior": "tie-by-reading"}, **kw) \
                        if probs[top] >= hi + delta - BOUNDARY_EPS else Unsure("band", p=probs[top], **kw)
            if ans.get("mode_share", 1.0) < 1.0:
                return Unsure("tie", p=p, detail={"mode_share": ans["mode_share"]}, **kw)
            if ans.get("knoul") and (p - ans.get("second", 0.0)) <= delta:
                return Unsure("tie", p=p, detail={"second": ans.get("second")}, **kw)
            if p >= hi + delta - BOUNDARY_EPS:
                return Pick(int(ans["value"]), p=p, **kw)
            return Unsure("band", p=p, **kw)
        if p >= hi + delta - BOUNDARY_EPS:
            return At(int(ans["value"]), p=p, **kw)
        return Unsure("band", p=p, detail={"nearest_level": int(ans["value"]) if ans.get("value") is not None else None}, **kw)

    def _register_exit(self, e: Exit) -> Exit:
        self.exits.append(e)
        if self._frames:
            self._frames[-1].exits.append(e)
        self.stats["exits"] += 1
        if _is(e, Unsure):
            self.stats["unsure"] += 1
        if self._loop_keys:
            self._loop_keys[-1].add(H(e.q_hash, e.kind, getattr(e.reading, "run_seq", 0),
                                      e.reading.effect.states[e.reading.obj_index].resolved().structure_hash
                                      if e.reading is not None else ""))
        return e

    # ------------------------------------------------------------ 判断力分配（§5 组合子 allocate；§7「判断力花在哪」；J-10）
    def _readings_list(self, readings) -> list[Reading]:
        if isinstance(readings, ReadingsVec):
            if len(readings.effect.qs) != 1:
                raise JvError("allocate / unsure_bound 需要单题的向量化读数；多题请先取 [r[i] for r in rs]")
            return [r[0] for r in readings]
        if isinstance(readings, Readings):
            return list(readings)
        out = []
        for r in readings:
            if isinstance(r, Readings) and len(r) == 1:
                r = r[0]
            if not isinstance(r, Reading):
                raise JvTypeError(f"J-01: allocate / unsure_bound 只接受读数，收到 {type(r).__name__}")
            out.append(r)
        return out

    def _uncertainty(self, r: Reading) -> float:
        """与决定带的距离取负：带内 = 0（最不确定），带外越远越确定。线来自校准记录（I4），冷键用保守线。"""
        ans = r._need()
        rec = self.calib.get(r.q.calib.key)
        delta = rec.delta if rec.delta is not None else self.delta_for(ans["phys"])
        hi, lo = (rec.hi, rec.lo) if rec.status == "上岗" else self.safety_lines()
        p = float(ans["p"])
        top, bot = hi + delta, lo - delta
        if bot <= p <= top:
            return 0.0 - 1e-6 * abs(p - (top + bot) / 2) * 0.0      # 带内并列（0），不再按中点细分
        return -min(abs(p - top), abs(p - bot))

    def allocate(self, readings, k: int) -> list[int]:
        """把 k 份复核（人、第二传感器、更贵的执行）分给最不确定的读数：返回下标，按不确定度降序。
        合法性：它像 cut 一样只读校准线，不做跨题算术；返回宿主整数，不返回读数。"""
        rs = self._readings_list(readings)
        if k is None or k < 0:
            raise JvError("allocate: k 必须是非负整数（通常取 Budget.escalate）")
        self.flush(reason="allocate")
        scored = sorted(range(len(rs)), key=lambda i: (-self._uncertainty(rs[i]), i))
        return scored[:min(k, len(rs))]

    def unsure_bound(self, readings) -> dict:
        """J-10：整批读数落入 unsure 的期望数——联合界 Σuᵢ（上界）与独立估计 1−Π(1−uᵢ)（只作参考）。
        uᵢ 取各题校准记录的 unsure_rate；无记录的题计入 n_unknown，界里按 1 计（最保守）。"""
        rs = self._readings_list(readings)
        us, unknown = [], 0
        for r in rs:
            rec = self.calib.get(r.q.calib.key)
            u = rec.unsure_rate if rec.status == "上岗" else None
            if u is None:
                unknown += 1
                us.append(1.0)
            else:
                us.append(float(u))
        prod = 1.0
        for u in us:
            prod *= (1.0 - u)
        return {"n": len(rs), "union_bound": round(min(sum(us), float(len(rs))), 4),
                "independent_any": round(1.0 - prod, 4), "n_unknown": unknown}

    def current_budget(self) -> Budget:
        return self._frames[-1].budget if self._frames else self.budget

    # ------------------------------------------------------------ fit（桥库）
    def fit(self, ref: ir.FitRef, *rs: Reading):
        rec = self.fits.get(ref.name)
        if rec is None:
            raise JvError(f"J-16: fit {ref.name} 未注册；fit 只认注册表签名。修法：用训练过程注册，或改用 cut。")
        if len(rs) != len(rec["features"]):
            raise JvError(f"J-16/J-04: fit {ref.name} 期望 {len(rec['features'])} 个读数，收到 {len(rs)}")
        for r, (ck, fk) in zip(rs, rec["features"]):
            if not isinstance(r, Reading):
                raise JvTypeError("J-01: fit 只接受读数")
            if r.q.calib.key != ck or _fp_kind(r) != fk:
                raise JvError(f"J-04: fit {ref.name} 的输入指纹 ({r.q.calib.key}, {_fp_kind(r)}) 与注册特征 ({ck}, {fk}) 不同")
        score = float(rec["fn"](*[float(r._need()["p"]) for r in rs]))
        return _Score(score, ref, rs)

    def cut_score(self, s: "_Score", calib: CalibRef):
        rec = self.calib.get(calib.key)
        fitrec = self.fits.get(s.ref.name)
        if rec.set_id and rec.set_id == fitrec["trained_from"]:
            raise JvError(f"J-16: fit {s.ref.name} 的训练集与保形集相同（{rec.set_id}），不相交约束违反")
        kw = {"q_hash": H("fit", s.ref.name), "taint": ir._join_taint(
            r.effect.states[r.obj_index].resolved().taint for r in s.readings)}
        if rec.status != "上岗":
            return self._register_exit(Unsure("cold", **kw))
        delta = rec.delta or 0.0
        if s.value >= rec.hi + delta:
            return self._register_exit(Act(p=s.value, **kw))
        if s.value <= rec.lo - delta:
            return self._register_exit(Ignore(p=s.value, **kw))
        return self._register_exit(Unsure("band", p=s.value, **kw))

    # ------------------------------------------------------------ handler 库（§5）
    def handle(self, c, then=None, regen: bool = False, keep=_Marker("nokeep"), **kw):
        e = c if _is(c, Exit) else self._last_unsure(c)
        if e is None:
            raise JvError(f"handle: 找不到 cause={c} 的 Unsure")
        cause = e.cause
        e.__dict__["consumed"] = True
        if cause == "cold":
            prov = e.detail.get("provisional")
            if prov is not None and not _is(prov, Unsure):
                prov.__dict__["consumed"] = True
                self._register_exit(prov)
                return prov
        elif cause == "band" and e.reading is not None and e.reading.effect.run_seq == 0:
            e2 = self._rerun(e.reading)
            if not _is(e2, Unsure):
                return e2
            e2.__dict__["consumed"] = True
        elif cause == "budget":
            return e
        if regen:
            return None
        if not isinstance(keep, _Marker):
            return keep
        if then is escalate:
            if e.reading is None:
                return escalate(e, note=cause)
            return self.ask(e.reading.effect.states[e.reading.obj_index], e.reading.q)
        if then is drop or then is None:
            self._note_drop()
            return None
        if callable(then):
            return then(e)
        return None

    def _last_unsure(self, cause: str):
        lm = Exit._last_matched
        if _is(lm, Unsure) and lm.cause == cause:
            return lm
        for e in reversed(self.exits):
            if _is(e, Unsure) and e.cause == cause and not e.__dict__.get("consumed"):
                return e
        for e in reversed(self.exits):
            if _is(e, Unsure) and e.cause == cause:
                return e
        return None

    def _rerun(self, r: Reading) -> Exit:
        """band → 同状态同题重跑一次（run_seq+1），agg 后重切。"""
        eff = r.effect
        j2 = JudgeEffect(states=[eff.states[r.obj_index]], qs=[r.q], site=eff.site, seq=self._next(), rt=self,
                         run_seq=eff.run_seq + 1, segment=self._segment)
        j2.readings = Readings(j2)
        self.pending_judges.append(j2)
        self.flush(reason="rerun")
        r2 = j2.readings[0]
        a1, a2 = r._ans, r2._ans
        merged = ir._merge_runs([a1, a2], a1["phys"])
        merged["runs"] = [a1, a2]
        r._ans = merged
        return self.cut(r)

    def consume(self, exits, unsure=drop):
        """批量消费向量化出口（J-05）。unsure=jv.drop 记账丢弃；jv.escalate 逐个 ask（可能 Pending）。"""
        out = []
        dropped = 0
        for e in exits:
            if _is(e, Unsure):
                if unsure is escalate:
                    out.append(self.handle(e, then=escalate))
                else:
                    e.__dict__["consumed"] = True
                    dropped += 1
                    out.append(None)
            else:
                e.__dict__["consumed"] = True
                out.append(e)
        if dropped:
            self._note_drop(dropped)
        return out

    # ------------------------------------------------------------ drop 与 escalate 两条记账（G6 猜 6）
    def _flags(self):
        if self._frames:
            return self._frames[-1]
        return self.__dict__.setdefault("_noframe", _Frame("<无程序>", Budget(), None, 0, 0.0, 0, 0, 0))

    def _note_drop(self, n: int = 1):
        f = self._flags()
        f.dropped += n
        self._drop_escalate_check(f)

    def _note_escalate(self, with_exits: bool):
        f = self._flags()
        if not with_exits:
            f.escalated += 1
        self._drop_escalate_check(f)

    def _drop_escalate_check(self, f):
        if f.dropped and f.escalated and not f.warned_de:
            f.warned_de = True
            self.warn(f"W-drop-vs-escalate: 程序 {f.name} 既用 jv.drop 记账丢弃了 {f.dropped} 个 Unsure，又调用 jv.escalate 交人；"
                      f"若这些 Unsure 本意是交人，写 jv.escalate(载荷, exits=[那些出口]) 或 jv.consume(exits, unsure=jv.escalate)，"
                      f"否则被丢弃的 Unsure 不进交人记录")

    def on_truth(self, key: str, fn: Callable):
        """真值回填入口：登记回调；真值到达时写校准集（延迟真值通道）。"""
        self.stats.setdefault("on_truth", []).append(key)
        self._truth_hooks = getattr(self, "_truth_hooks", {})
        self._truth_hooks[key] = fn

    def on_fail(self, expr, alt):
        if isinstance(expr, _Future):
            expr.resolve()
            if expr.fail is not None:
                return alt
            return expr
        if isinstance(expr, ir.FailList):
            return alt
        if isinstance(expr, Mat) and isinstance(expr.content, dict) and "fail" in expr.content:
            return alt
        return expr


def _flatten(xs):
    for x in xs:
        if isinstance(x, (list, tuple)):
            yield from _flatten(x)
        else:
            yield x


def _returns_list(f) -> bool:
    """宿主函数是否声明返回列表（`-> list` / `-> list[str]` / 字符串注解）；列表型 transform 失败时返回空列表。"""
    try:
        ann = inspect.signature(f).return_annotation
    except (ValueError, TypeError):
        return False
    if ann is inspect.Signature.empty:
        return False
    if isinstance(ann, str):
        return ann.split("[")[0].strip() in ("list", "tuple", "List", "Tuple", "Sequence")
    origin = getattr(ann, "__origin__", ann)
    return origin in (list, tuple) or getattr(origin, "__name__", "") in ("Sequence", "List", "Tuple")


class _Frame:
    """嵌套 `@jv.program` 的子账帧：预算是外层的子账，Exit.consumed 只核本帧作用域。"""

    def __init__(self, name: str, budget: Budget, parent, calls0: int, cost0: float, layers0: int, esc0: int, depth: int,
                 fn=None, returns_unsure: bool = False):
        self.name, self.budget, self.parent = name, budget, parent
        self.calls0, self.cost0, self.layers0, self.esc0, self.depth = calls0, cost0, layers0, esc0, depth
        self.exits: list = []
        self.dropped, self.escalated, self.warned_de = 0, 0, False     # W-drop-vs-escalate 记账
        self.fn = fn                                                   # 程序函数本体（推测提升读它的 AST 与帧）
        self.returns_unsure = returns_unsure                           # 返回注解含 Unsure：返回值里的 Unsure 交给调用者
        self.spec_index: dict = {}                                     # (状态哈希, 题, run_seq) → 效应（真或推测）
        self.lift_warned: set = set()

    def report(self, rt: "Runtime") -> dict:
        return {"name": self.name, "depth": self.depth, "calls": rt.stats["calls"] - self.calls0,
                "layers": len(rt.stats["layers"]) - self.layers0, "cost": round(rt.stats["cost"] - self.cost0, 6),
                "exits": len(self.exits), "unsure": sum(1 for e in self.exits if _is(e, Unsure)),
                "escalations": rt._escalations - self.esc0}

    def __repr__(self):
        return f"Frame({self.name}, depth={self.depth})"


def _chain(frame):
    """帧及其全部外层帧（子账计入外层总数）。"""
    while frame is not None:
        yield frame
        frame = frame.parent


class _Score:
    def __init__(self, value: float, ref, readings):
        self.value, self.ref, self.readings = value, ref, readings

    def __repr__(self):
        return f"Score({self.value:.3f}, fit={self.ref.name})"


def _fp_kind(r: Reading) -> str:
    st = r.effect.states[r.obj_index]
    if r.q.op == "select":
        return f"select/K={len(st.over)}"
    if r.q.op == "measure":
        return f"measure/{len(r.q.scale)}"
    return "test"


class _It:
    def __init__(self, n: int):
        self.n = n

    def __repr__(self):
        return f"it(n={self.n})"


class _Loop:
    def __init__(self, rt: Runtime, bound: int, variant):
        self.rt, self.bound, self.variant = rt, bound, variant
        self.stopped_by: str | None = None
        self.stopped_exit: Unsure | None = None

    def __iter__(self):
        prev = None
        self.rt._loop_keys.append(set())
        seen_keys: set = set()
        try:
            for n in range(self.bound):
                v = self.variant()
                if prev is not None and not (v < prev):
                    self._stop("noprogress", n, {"variant": v, "prev": prev})
                    return
                prev = v
                before = set(self.rt._loop_keys[-1])
                yield _It(n)
                new = self.rt._loop_keys[-1] - before
                if new and new & seen_keys:
                    self._stop("keyrepeat", n, {"keyrepeat": True})
                    return
                seen_keys |= new
            self.stopped_by = "bound"
        finally:
            self.rt._loop_keys.pop()


    def _stop(self, why: str, n: int, detail: dict):
        """无进展 / 键重复即停（P6、§5 handler `noprogress → 退出记账`）：显式记一个 Unsure(noprogress) 出口，
        循环本身就是它的 handler（退出即记账），故置 consumed；同时 warn W-noprogress 让它可见。"""
        self.stopped_by = why
        e = Unsure("noprogress", detail=dict(detail, n=n, by=why))
        e.__dict__["consumed"] = True
        self.stopped_exit = e
        self.rt._register_exit(e)
        self.rt.stats.setdefault("noprogress", []).append({"n": n, **detail})
        self.rt.warn(f"W-noprogress: jv.loop 第 {n} 轮{('变式不降 ' + repr(detail.get('variant'))) if why == 'noprogress' else '账本键重复'}，"
                     f"停止并记 Unsure(noprogress)（P6）")


class decreasing:
    """宿主变式：严格递减的计量。"""

    def __init__(self, fn: Callable[[], float]):
        self.fn = fn

    def __call__(self):
        return self.fn()


def _chunk(text: str, tokens: int) -> list[str]:
    n = max(1, int(tokens * 1.3))
    return [text[i:i + n] for i in range(0, len(text), n)] or [text]


def _arg_hash(x) -> str:
    """效应参数的键：Mat 用其哈希；列表/字典递归；其余规范化 JSON（D2：材料列表也能进 transform/do）。"""
    if isinstance(x, Mat):
        return x.hash
    if isinstance(x, _Future):
        return _arg_hash(x.resolve())
    if _is(x, Exit):
        return _arg_hash(x.as_mat())
    if isinstance(x, (list, tuple)):
        return H([_arg_hash(v) for v in x])
    if isinstance(x, dict):
        return H({str(k): _arg_hash(v) for k, v in x.items()})
    return canon(x)


def _taints(xs):
    for x in xs:
        if isinstance(x, Mat):
            yield x.taint
        elif isinstance(x, (list, tuple)):
            yield from _taints(x)


def _plain(x):
    if isinstance(x, Mat):
        return {"__mat__": x.content}
    if isinstance(x, (list, tuple)):
        return [_plain(v) for v in x]
    return x


def _materialize(x):
    """程序返回值：期物 → Mat；列表/元组/字典递归。出口、Escalated、标量原样。"""
    if isinstance(x, _Future):
        return x.resolve()
    if isinstance(x, ir.FailList):                       # 空的失败列表原样透传（形状与 .fail 都保留）
        return x
    if isinstance(x, list):
        return [_materialize(v) for v in x]
    if isinstance(x, tuple):
        return tuple(_materialize(v) for v in x)
    if isinstance(x, dict):
        return {k: _materialize(v) for k, v in x.items()}
    return x


def _exits_in(obj, depth: int = 0, seen: set | None = None) -> set:
    """返回值里（含容器与 Escalated 载荷里）的出口 id 集合（J-05「被返回类型消费」用）。"""
    seen = seen if seen is not None else set()
    out: set = set()
    if depth > 8 or id(obj) in seen:
        return out
    if _is(obj, Exit):
        out.add(id(obj))
        return out
    seen.add(id(obj))
    if isinstance(obj, dict):
        for v in list(obj.values()) + list(obj.keys()):
            out |= _exits_in(v, depth + 1, seen)
    elif isinstance(obj, (list, tuple, set, frozenset)):
        for v in obj:
            out |= _exits_in(v, depth + 1, seen)
    elif _is(obj, Escalated):
        out |= _exits_in(getattr(obj, "payload", None), depth + 1, seen)
        out |= _exits_in(getattr(obj, "exits", None), depth + 1, seen)
    return out


def admits_unsure(fn) -> bool:
    """程序函数的返回注解是否包含 Unsure（含 Exit、Union、list/dict/tuple/set 等容器里的）。"""
    import typing
    try:
        ann = typing.get_type_hints(fn).get("return")
    except Exception:
        ann = getattr(fn, "__annotations__", {}).get("return")
        if isinstance(ann, str):
            try:
                ann = eval(ann, getattr(fn, "__globals__", {}))
            except Exception:
                ann = None
    return _admits(ann)


def _admits(t) -> bool:
    import typing
    if t is None:
        return False
    if isinstance(t, type):
        return issubclass(t, Exit) and (t is Exit or issubclass(t, Unsure))
    origin = typing.get_origin(t)
    if origin is None:
        return False
    return any(_admits(a) for a in typing.get_args(t))

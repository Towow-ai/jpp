//! 可 diff 的文本形式（`20` §2.3 `jpp-ir`，D2.4）：同一程序两次打印逐字节相同。
//!
//! 形式：一行一个节点，缩进表示嵌套；每行以 `#节点号` 开头，带站点的写 `@站点号`。
//! 末尾列站点表。给了标注表时，在对应节点行尾追加 `; 标注`。

use super::*;
use std::fmt::Write;

pub fn print(p: &Program, annot: Option<&AnnotTable>) -> String {
    let mut w = Printer {
        out: String::new(),
        annot,
    };
    let b = &p.budget;
    let _ = writeln!(w.out, "ir v{}", p.version);
    let _ = writeln!(
        w.out,
        "budget calls={} cost={} depth={} escalate={} unsure={} latency_p95={} absent={}",
        b.calls,
        b.cost,
        opt(&b.depth),
        opt(&b.escalate),
        opt(&b.unsure),
        opt(&b.latency_p95),
        b.absent
            .as_ref()
            .map(|a| format!(
                "{{retry={} backoff={} then={} breaker={}}}",
                a.retry, a.backoff, a.then, a.breaker
            ))
            .unwrap_or_else(|| "-".into())
    );
    // 入口声明（B106）：空时不打印，无入口程序的 `ir.txt` 不变
    if !p.entry.is_empty() {
        let ps: Vec<String> = p
            .entry
            .params
            .iter()
            .map(|e| {
                let k = format!("{:?}:{:?}", e.kind, e.taint).to_lowercase();
                format!("{}:{k}", e.name)
            })
            .collect();
        let _ = writeln!(w.out, "entry {}", ps.join(" "));
    }
    w.block(&p.body, 0);
    let _ = writeln!(w.out, "sites {}", p.sites.sites.len());
    for s in &p.sites.sites {
        let _ = writeln!(
            w.out,
            "  @{} #{} {} {}..{} fn={} in={}",
            s.id.0,
            s.node.0,
            site_kind(&s.kind),
            s.span.start,
            s.span.end,
            s.function
                .map(|f| format!("#{}", f.0))
                .unwrap_or("-".into()),
            s.enclosing
                .map(|f| format!("@{}", f.0))
                .unwrap_or("-".into()),
        );
    }
    w.out
}

fn opt<T: std::fmt::Display>(v: &Option<T>) -> String {
    v.as_ref()
        .map(|x| x.to_string())
        .unwrap_or_else(|| "-".into())
}

fn site_kind(k: &SiteKind) -> String {
    match k {
        SiteKind::State => "state".into(),
        SiteKind::Effect(e) => format!("effect:{}", effect_name(*e)),
        SiteKind::Cut => "cut".into(),
        SiteKind::Fit => "fit".into(),
        SiteKind::Loop => "loop".into(),
        SiteKind::Handle => "handle".into(),
        SiteKind::Consume(h) => format!("consume:{}", h.name()),
        SiteKind::Construct(n) => format!("construct:{n}"),
        SiteKind::HigherOrder(n) => format!("hof:{n}"),
        SiteKind::If => "if".into(),
    }
}

fn effect_name(e: EffectId) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn ty(t: &Option<Type>) -> String {
    t.as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_default())
        .unwrap_or_else(|| "-".into())
}

struct Printer<'a> {
    out: String,
    annot: Option<&'a AnnotTable>,
}

impl Printer<'_> {
    fn line(&mut self, depth: usize, id: Option<NodeId>, text: String) {
        let pad = "  ".repeat(depth);
        let _ = write!(self.out, "{pad}");
        if let Some(id) = id {
            let _ = write!(self.out, "#{} ", id.0);
        }
        let _ = write!(self.out, "{text}");
        if let (Some(a), Some(id)) = (self.annot, id)
            && let Some(n) = a.node(id)
        {
            let _ = write!(
                self.out,
                " ; {}",
                serde_json::to_string(n).unwrap_or_default()
            );
        }
        let _ = writeln!(self.out);
    }

    fn block(&mut self, b: &Block, depth: usize) {
        self.line(
            depth,
            None,
            format!("block {}..{}", b.span.start, b.span.end),
        );
        for s in &b.statements {
            match s {
                Stmt::Let {
                    name,
                    annotation,
                    value,
                    ..
                } => {
                    self.line(depth + 1, None, format!("let {name} : {}", ty(annotation)));
                    self.expr(value, depth + 2);
                }
                Stmt::Function { name, function, .. } => {
                    self.line(depth + 1, None, format!("fn-stmt {name}"));
                    self.function(function, depth + 2);
                }
                Stmt::Expr(e) => self.expr(e, depth + 1),
            }
        }
        if let Some(r) = &b.result {
            self.line(depth + 1, None, "result".into());
            self.expr(r, depth + 2);
        }
    }

    fn function(&mut self, f: &Function, depth: usize) {
        let params: Vec<String> = f
            .parameters
            .iter()
            .map(|p| format!("{}:{}", p.name, ty(&p.annotation)))
            .collect();
        self.line(
            depth,
            Some(f.id),
            format!(
                "fn ({}) -> {} effects={}",
                params.join(", "),
                ty(&f.result_type),
                f.effects
                    .as_ref()
                    .map(|e| format!("[{}]", e.join(",")))
                    .unwrap_or_else(|| "-".into())
            ),
        );
        self.block(&f.body, depth + 1);
    }

    fn named(&mut self, depth: usize, label: &str, e: &Expr) {
        self.line(depth, None, format!("{label}:"));
        self.expr(e, depth + 1);
    }

    fn all(&mut self, depth: usize, xs: &[Expr]) {
        for x in xs {
            self.expr(x, depth);
        }
    }

    fn expr(&mut self, e: &Expr, depth: usize) {
        let sp = format!("{}..{}", e.span.start, e.span.end);
        let site = |s: &SiteId| format!("@{}", s.0);
        match &e.node {
            Node::State {
                on,
                opts,
                rest,
                site: s,
            } => {
                self.line(depth, Some(e.id), format!("state {} {sp}", site(s)));
                self.named(depth + 1, "on", on);
                if let Some(o) = opts {
                    self.named(depth + 1, "opts", o);
                }
                self.all(depth + 1, rest);
            }
            Node::Effect {
                effect,
                inputs,
                site: s,
            } => {
                self.line(
                    depth,
                    Some(e.id),
                    format!("effect {} {} {sp}", effect_name(*effect), site(s)),
                );
                for (k, v) in inputs {
                    self.named(depth + 1, k, v);
                }
            }
            Node::Cut {
                reading,
                rest,
                site: s,
            } => {
                self.line(depth, Some(e.id), format!("cut {} {sp}", site(s)));
                self.named(depth + 1, "reading", reading);
                self.all(depth + 1, rest);
            }
            Node::Fit { args, site: s } => {
                self.line(depth, Some(e.id), format!("fit {} {sp}", site(s)));
                self.all(depth + 1, args);
            }
            Node::Loop {
                bound,
                rest,
                site: s,
            } => {
                self.line(depth, Some(e.id), format!("loop {} {sp}", site(s)));
                self.named(depth + 1, "bound", bound);
                self.all(depth + 1, rest);
            }
            Node::Handle {
                exit,
                arms,
                rest,
                site: s,
            } => {
                self.line(depth, Some(e.id), format!("handle {} {sp}", site(s)));
                self.named(depth + 1, "exit", exit);
                self.named(depth + 1, "arms", arms);
                self.all(depth + 1, rest);
            }
            Node::Consume { how, args, site: s } => {
                self.line(
                    depth,
                    Some(e.id),
                    format!("{} {} {sp}", how.name(), site(s)),
                );
                self.all(depth + 1, args);
            }
            Node::Construct {
                name,
                args,
                site: s,
            } => {
                self.line(
                    depth,
                    Some(e.id),
                    format!("construct {name} {} {sp}", site(s)),
                );
                self.all(depth + 1, args);
            }
            Node::Host(h) => self.host(e, h, depth, sp),
        }
    }

    fn host(&mut self, e: &Expr, h: &Host, depth: usize, sp: String) {
        let id = Some(e.id);
        match h {
            Host::Integer(v) => self.line(depth, id, format!("int {v}")),
            Host::Decimal(v) => self.line(depth, id, format!("dec {v:?}")),
            Host::Bool(v) => self.line(depth, id, format!("bool {v}")),
            Host::Text(v) => self.line(
                depth,
                id,
                format!("text {}", serde_json::to_string(v).unwrap_or_default()),
            ),
            Host::Unit => self.line(depth, id, "unit".into()),
            Host::Name(n) => self.line(depth, id, format!("name {n}")),
            Host::List(xs) => {
                self.line(depth, id, format!("list {sp}"));
                self.all(depth + 1, xs);
            }
            Host::Record(fs) => {
                self.line(depth, id, format!("record {sp}"));
                for (k, v) in fs {
                    self.named(depth + 1, k, v);
                }
            }
            Host::Function(f) => self.function(f, depth),
            Host::Call { callee, args, site } => {
                self.line(
                    depth,
                    id,
                    format!(
                        "call{} {sp}",
                        site.map(|s| format!(" @{}", s.0)).unwrap_or_default()
                    ),
                );
                self.named(depth + 1, "callee", callee);
                self.all(depth + 1, args);
            }
            Host::Field { value, field } => {
                self.line(depth, id, format!("field .{field}"));
                self.expr(value, depth + 1);
            }
            Host::Index { value, index } => {
                self.line(depth, id, "index".into());
                self.expr(value, depth + 1);
                self.expr(index, depth + 1);
            }
            Host::Unary { op, value } => {
                self.line(depth, id, format!("unary {op}"));
                self.expr(value, depth + 1);
            }
            Host::Binary { op, left, right } => {
                self.line(depth, id, format!("binary {op}"));
                self.expr(left, depth + 1);
                self.expr(right, depth + 1);
            }
            Host::If {
                condition,
                yes,
                no,
                site,
            } => {
                self.line(depth, id, format!("if @{} {sp}", site.0));
                self.named(depth + 1, "cond", condition);
                self.block(yes, depth + 1);
                self.block(no, depth + 1);
            }
            Host::Block(b) => {
                self.line(depth, id, "block-expr".into());
                self.block(b, depth + 1);
            }
        }
    }
}

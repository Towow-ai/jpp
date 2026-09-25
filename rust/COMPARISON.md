# Source and direct core construction / 源码与直接构造内核程序

The frontend does not introduce a second execution engine. It translates source
structure into the same `jpp_core::ast::Program` that a Rust caller can construct
directly. The shared interpreter executes both forms.

前端把源码里的函数、调用和组合关系交给同一个内核。下面两种写法的作用相同：

```text
fn compose(f: Fn(Int) -> Int, g: Fn(Int) -> Int) -> Fn(Int) -> Int {
    fn(x: Int) -> Int { g(f(x)) }
}
let method = compose(increment, twice);
let larger = compose(method, increment);
```

Direct core callers construct the corresponding function and call nodes:

```rust
let returned_method = Expr::func(
    &["x"], None,
    Block::expr(call("g", vec![call("f", vec![name("x")])])),
    span,
);
let compose = Expr::func(&["f", "g"], None, Block::expr(returned_method), span);
let method = call("compose", vec![name("increment"), name("twice")]);
let larger = call("compose", vec![name("method"), name("increment")]);
```

The complete executable comparison lives in
[`core_equivalence.rs`](crates/jpp-frontend/tests/core_equivalence.rs), including
bindings, budget and output record. It executes the independently constructed core
program and the parsed/lowered source program, compares their complete returned
JSON, and requires zero judgment calls. Both return `{"expected":43,"result":43}`.
This test has passed against the shared Rust interpreter.

这个对照已实际通过：组合得到的方法可以再交给另一次组合，最后调用得到 43。
使用语言的人可以直接写函数和调用，不必手动搭建 Rust AST 节点。它证明这段组合
结构的源码描述和实际执行一致，不代表所有源码或全部语言规则都已验证。

## Complete algorithms / 完整算法的对照

The following execution comparisons have passed through the checked CLI:

| Program | Source owns | Observable expectation |
| --- | --- | --- |
| `adaptive.jpp` | Select next question, narrow interval, stop | Locate 731 in 10 fixed observations |
| `partial.jpp` | Validate, enumerate subsets, apply constraints, capture continuation, change strategy | Cost 9 with C/D pending → cost 2 with D pending → no pending; only A/B/C checked |

For these examples the fixed observation files supply exact answers. They do not
choose questions, enumerate combinations or decide continuation policy. The host
action only records checks whose values have already been computed in source.
Behavior expectations are recorded in `examples/expected/`; end-to-end assertions
are in `crates/jpp-cli/tests/source_execution.rs`.

The core implementation also has directly constructed algorithm programs in
[`adaptive.rs`](crates/jpp-core/tests/adaptive.rs) and
[`partial.rs`](crates/jpp-core/tests/partial.rs). These make the lower-level
construction visible next to the `.jpp` examples. Their final joint test result is
recorded in the delivery report, separately from the source CLI assertions.

The earlier Python programs remain a behavior reference. Equality here concerns
specified outputs, retained observations and effect counts; it does not claim every
Python optimization or implementation detail has migrated to Rust.

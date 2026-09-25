# J++ native kernel (Rust) — status, 2026-09-23

`.jpp` source → lex → parse → lower → check → interpret. The Python tree under
`src/foundation/jv/` is the frozen reference implementation, kept as a behavioural oracle. It is
not deprecated: cross-checking the two found a failure-open bug in `cut` this week that 285
green tests could not see.

## What runs today

```
jpp parse <file.jpp> [--ast]
jpp check <file.jpp>
jpp run   <file.jpp> --fixtures <f.json> [--output <r.json>] [--ledger-out <l.json>]
          [--replay <l.json> | --resume <l.json>]
          [--profile <p.json>] [--calib <dir>] [--calib-out <dir>]
```

The current surface includes source lexing/parsing/lowering/loading, shared static
checking and interpretation, the six semantic forms (`judge`, `gen`, `do`, `ask`,
`cut`, `state`), ledger replay/resume, and host-side conformal/calibration support. The
checker still leaves `J-17` open, and the conformal family has no `.jpp` language
surface. Avoid relying on old per-module line and builtin counts here; the source
and tests are the authority.

The full `cargo test --workspace --quiet` command completed successfully on
2026-09-23: **315 passed, 0 failed, 3 ignored** across all reported targets
(two integration tests and one documentation example ignored). The output is
retained in the research workspace's [verification log](../../进展/2026-09-23/verification/docs-drift-cargo-test.log).
This verifies the local research tree, not the older public Rust snapshot or live-model quality.

`ask` is exercised by `examples/lifecycle.jpp`: `request_test` in
`lib/observations.jpp` reaches the ask path. `library_lifecycle.rs` verifies the
pending → response → resume → replay sequence and the retained input snapshot,
using fixed responses rather than an interactive UI.

## The calibration loop

The read/write round trip is covered: readings written by `--calib-out` can be read
back by `--calib`, and a second pass accumulates onto those records. This proves the
CLI serialization path only. It does not constitute complete certification of
untagged observations; host-side `commission` remains the certification step.

## Known gaps, stated plainly

- **`commission` — putting a line into service — is host-only.** The round trip works, but this
  segment of the loop is reachable only from Rust, not from `.jpp`. A `.jpp` author cannot walk
  the whole loop alone.
- **The conformal family has no language surface**: `commission`, `put`, `absorb`, `certify`,
  `drift`, `cost_line` are not builtins. This is deliberate — `J-03` forbids a program from
  writing a line — but it means those capabilities arrive through host plumbing, not the
  language.
- **`J-17` has zero implementation** anywhere in the tree.
- The calibration-state probes cover a selected subset of builtins; they do not
  establish a measured calibration-independence result for every remaining operation.

## Layout

- `crates/jpp-core` — AST, checker, interpreter, effects, ledger, conformal
- `crates/jpp-frontend` — lexer, parser, lowering, source loader
- `crates/jpp-cli` — the `jpp` binary
- `examples/` — runnable `.jpp` programs with fixtures
- `crates/jpp/INTERFACE.md` — kernel interface, including what is *not* implemented

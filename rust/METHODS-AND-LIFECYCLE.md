# Methods, source libraries and resumable runs

This package connects explicit method contracts and multi-file source programs to
the existing Rust core. The CLI also accepts fixed generation and response records,
and registers JSON file actions. It adds no interpreter or model backend.

## Method contracts and reuse

```jpp
import "../lib/methods.jpp";
budget {calls: 0, cost: 0, depth: 256};
fn increment(x: Int) -> Int !{} { x + 1 }
map_ints([1, 2, 3], increment)
```

The library declares `method: Fn(Int) -!{}-> Int`. A type such as
`Fn(Mat) -!{judge}-> Record` carries a judgment-effect upper bound into the common
checker. The old `Fn(Int) -> Int` syntax retains unknown/inferred effects.
`Fn1(...)` marks captured responsibility in the core method type; it does not
claim full static linearity. Unknown paths still have the limits documented by
the core. Creating or returning a method does not execute its body.

Snapshot boundary: core package four (`d021f33`) is supplemented with the core
owner's committed argument-contract check and regression from `0ffe7ed`.
Recognized named methods and explicitly annotated function literals are checked
against parameter effect bounds. Unresolved dynamic paths retain the documented
limits; these annotations are not complete static enforcement for every program.

`examples/library-methods.jpp` returns a composed method, stores it in a record,
and invokes it through the library. Its output is `[4,6,42]` and `42`.
`examples/lifecycle.jpp` reuses the same method library and the observation library.
Both are ordinary `.jpp` files. Imports are relative to their source file, precede
the budget, and load shared dependencies once. Library declarations share one
scope; cycles and cross-file name conflicts are diagnosed. There are no namespace
or package-manager semantics in this package.

## One complete lifecycle

From this directory, build the CLI and copy the source tree into a scratch folder
so the file actions have a disposable working directory:

```sh
cargo build --locked -p jpp
demo=$(mktemp -d)
cp -R examples lib "$demo/"
cp target/debug/jpp "$demo/jpp"
cd "$demo"
printf '%s\n' '{"goal":"a reusable plan"}' > input.json
./jpp run examples/lifecycle.jpp --fixtures examples/fixtures/lifecycle.json --ledger-out pending.json
./jpp run examples/lifecycle.jpp --fixtures examples/fixtures/lifecycle-response.json --resume pending.json --ledger-out complete.json
./jpp run examples/lifecycle.jpp --fixtures examples/fixtures/lifecycle-response.json --replay complete.json
```

The first run reads `input.json`, generates a candidate, judges it, and pauses at
`ask` without writing `result.json`. The second receives a fixed approval, reuses
the completed input/generation/judgment records, and writes the result. The third
returns the same result without calling the client or repeating the file write.
The integration test removes the original input before resume and overwrites the
output with a marker before replay: both retained input and non-repeated output
are checked, rather than inferred from status messages.

Generation and judgment in this example are fixed test data, including synthetic
calibration. No paid requests or claims of model accuracy are involved. Resume
uses the existing core ledger and reconstructs methods from unchanged source;
it does not serialize arbitrary closures. Keep the entry file, imported files and
calibration stable across these runs. Recorded file reads are snapshots: use a new
ledger to process fresh input, or explicitly change the source action identity.

## Host interfaces

| Source / CLI | Meaning |
|---|---|
| `do("read_json", [path], seq)` | Read JSON relative to the process directory; core wraps the result as untrusted material with action provenance |
| `do("write_json", [path,value], seq)` | Write the source-computed JSON value, replacing the destination; core records completion |
| `do("record_check", [value], seq)` | Retained local check recorder |
| `--resume ledger.json` | Reuse completed effects and allow the fixed client/actions to continue |
| `--replay ledger.json` | Reuse completed effects; reject fresh client requests and action execution |

File action errors return the core's `Fail` value, testable with `is_fail`.
Unsigned JSON integers beyond signed 64-bit range are rejected rather than rounded.
Action completion and ledger-file persistence are not a distributed transaction;
this package does not claim crash-proof exactly-once file I/O. Budget accounting,
failures, pending state and effect identities remain owned by the core.

The existing fixture keys remain valid. New optional arrays are `generations`
(`prompt`, `retry_seq`, `output`) and `responses` (the same state/question/answer
fields as `observations`). Answer variants must match the operation (`test`/`Noul`,
`select`/`Choice`, `measure`/`Score`); incompatible records are fixture errors before
execution. Omit a response to leave `ask` pending. The fixed
generator matches the existing core's prompt/retry key; it does not model
context-sensitive generation or implement an algorithm in the CLI.

## Source versus direct core use

The existing [comparison](COMPARISON.md) remains executable. Since step 12d the
source is lowered directly to the IR; the separate core-AST builder path and its
`core_equivalence` test were removed with it, so there is one representation to
test. File imports only assemble declarations before lowering; they do not add a
second execution path.

```sh
cargo test --locked --workspace
cargo test --locked -p jpp --test library_lifecycle
```

The live JEV backend (`--backend live`, which requires a capability profile) and
the calibration workflow are described in [GUIDE.md](GUIDE.md). Broader library
migration and optimizer work remain in the engineering plan and are not declared
complete here.

[中文说明](METHODS-AND-LIFECYCLE.zh-CN.md)

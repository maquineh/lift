# Conservative last-use moves

[Português do Brasil](liveness.pt-BR.md) · [User guide](../README.md)

After semantic analysis succeeds, a private pass builds a control-flow graph for each function and for the entry script. It does not change syntax, accepted programs, parameter modes, alias diagnostics, or Truxt's value-copy behavior. It can remove a redundant copy when the original owned value is never needed again.

Each simple statement and each `if`/`while` condition has a graph point. Branches join at their actual continuation; `return` has no successor. Loops include the condition backedge and the zero-iteration exit. Backward liveness computes `live_out = union(live_in(successors))` and `live_in = uses ∪ (live_out − defs)` until a fixed point. Direct rebinding kills the previous value; a field/index mutation does not kill its root. Calls conservatively use every argument.

Only `Clone(Value, Local)` of an owned direct local can become an explicit `HirExprKind::Move`. The binding must occur once in that complete statement and be absent from the point's `live_out` on every path. Condition counters include their full if/while statement, including both branches/body; child statements have their own points and complete counters. Code generation simply emits the selected binding and does not infer moves again.

Owned bindings are local declarations and parameters passed `ByValue` or `OwnedMove`. `BorrowImm`/`BorrowMut` parameters are excluded. The pass keeps `ByClone` call arguments, borrowed-string/aggregate clones, fields/indices, grouped clone operands, multiple occurrences in one statement, and uncertain cases. It does not optimize unreachable statements, fold constant branches, or perform interprocedural alias analysis. Existing owned-return parameters may already be plain Local values in the validated HIR; this pass only replaces explicit qualifying clones.

The proof is at statement granularity. It does not prove general last use inside arbitrary expression evaluation or remove every possible clone. No existing unsupported ownership pattern becomes accepted. The new Move HIR variant also requires external exhaustive HIR consumers to handle that decision.

The Option increment observes payloads and both coalesce operands as possible uses, while leaving clones inside Some/Coalesce untouched. This preserves conservative liveness around a conditional fallback; see [Option](option.md).

Tests cover eligible copies in functions/scripts, future uses, if/else/else-if joins, terminal returns, loop fixed points/zero iterations, reassignment, repeated occurrences, borrowed parameters, projections, and preserved `ByClone`. The CLI compiles `examples/liveness.truxt` through check/build/run and verifies the native executable's output. These tests establish the exercised cases, not a general borrow checker proof.

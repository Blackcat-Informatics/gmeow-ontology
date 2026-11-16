# Issue #2812: Native engine refuses a rule head mixing plain and reified n-ary atoms; prove fails without naming the rule

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, cross-cutting, logic, rust

## Body

## Problem

A formula whose consequent is a conjunction of a unary or binary atom and an atom of arity three or more cannot be executed by the native engine, and `gmeow prove` ends in `execution-failure` (exit 5) with a message that names a variable and no rule:

```
run the native engine over the selection: reified n-ary head reifier "?A" is bound by the body (not existential) — an invented tuple's reifier node must be a fresh existential, never a frontier variable
```

The formula is valid. `LOGIC-IR.md` says a head conjunction mixing binary and reified n-ary atoms is one dependency. The compiler emits arity-1 and arity-2 head atoms plainly and reifies only atoms of arity three or more, so such a consequent becomes a mixed head. `reified_nary_head_groups` in `crates/logic/src/physical/chase.rs` requires every head atom to have an existential subject as soon as the head contains any `naryArg` atom, and refuses the mixed head from `PreparedChaseRule::new`. The same recogniser gives other messages when the plain atom has an existential or a constant subject.

Three further defects show on the same path:

1. The error names neither the rule nor its head relations, so an author cannot find the formula.
2. A shape the engine cannot carry should be a named drop and a `not-decided` verdict (exit 3); exit 5 is for broken invariants. Nothing checks the head shape before execution.
3. When the engine fails, the `prove` report is built blank: the drops already computed for the selection are discarded and the selection block is empty.

## Reproduction

A module with one fact `Src(a)` and one formula of the form `forall A. Src(A) -> (Dst(A) and r(A, A, A))` gives exit 5 and the message above. A head with only the ternary atom executes.

## Acceptance

- [ ] A head mixing plain and reified n-ary atoms executes: only subjects of `naryArg` atoms are treated as reifiers, every other head atom is an ordinary head atom, and the existing group-level refusals remain. A chase test covers the mixed head with a frontier subject, an existential subject and a constant subject on the plain atom.
- [ ] A head the chase still refuses is refused with the rule identity and the head relation IRIs in the diagnostic.
- [ ] The same shape check runs at lowering, so a head the chase would refuse is a named `UNSUPPORTED_CONSTRUCT` drop and the verdict is `not-decided`, never an execution failure.
- [ ] When the native engine fails, the `prove` report still carries the selection and the drops computed before the failure, marked incomplete.
- [ ] A `prove` CLI test runs the reproduction above and expects a decided or `not-decided` verdict, not exit 5.

Principles 7, 17.


## Comments (0)


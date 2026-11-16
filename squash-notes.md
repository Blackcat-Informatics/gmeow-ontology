fix(logic): execute rule heads that mix plain atoms with reified n-ary tuples; name a refused head (Principles 4, 7, 17)

A formula whose consequent joins a unary or binary atom with an atom of arity three or more compiles to a
rule head that mixes plain atoms with a reified n-ary tuple. The native chase refused any such head, and
`gmeow prove` ended in an execution failure with a message that named a variable and no rule.

Mixed heads now execute. The reifiers of a head are exactly the subjects of its `naryArg` atoms. A typing atom
belongs to a tuple only on such a subject; every other head atom is an ordinary atom of the same dependency,
grounded after every value null and reifier is bound, so one that mentions a reifier receives the minted
node. The tuple mint reads only the relation and the argument values, so it is unchanged.

Tuples are minted in dependency order: a tuple that takes another tuple's node as an argument is minted after
it, and tuples with no such constraint keep sorted order. Tuples that take each other's node, or a tuple that
takes its own, have no mint order and are refused.

Every refusal on this path carries a typed `RefusedRuleHead` payload with the rule identity and the head
relations. Formula lowering runs the chase's own head-shape check, so a head the chase would refuse is a named
`UNSUPPORTED_CONSTRUCT` drop before execution, and an authored existential-rule record with a refused head is
declined the same way in `prove`: a drop naming the rule, the native leg `refused`, verdict `not-decided`.

When an engine does fail, the report keeps the selection, the claims read and the drops named before the
failure, with `selection.complete` false, instead of a blank report. The README says what each kind of failure
sets.

One existing test asserted the old refusal of a mixed head. It now asserts that such a head prepares, that
the closed-list proof still refuses a cell minted by tuple identity, and that a tuple with no typing atom is
still refused.

Verified: independent review PASS at e063cd8df; the production diff read in full (the chase, the error
payload, the lowering call site and the prove command), the tests by name. At that head a gmeow-logic
selection ran 207 tests: 203 passed; two refuse for a missing produced fixture; two fail identically on main
at the same panic sites and are not on this change's path. `make producer-recipe` and `make lint` exit 0.
Pull request checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the
aggregate red as on main, with no error before the producer is killed.

Not shown: the prove command's own tests did not run at this head, here or on the hosted runner; they last
ran at an earlier head of the same production code. No producer pass, gate or `make check`. A comment in
`crates/logic/src/nary/tests.rs` still names a function this change removed.

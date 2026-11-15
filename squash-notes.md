feat(epistemics): evaluate a claim that exists as a reified statement, with the evaluator's reason (Principles 2, 9, 10, 17)

`gmeow:evaluates` takes a `gmeow:ClaimToken`, so a claim recorded only as an RDF 1.2 reified statement could
not be evaluated, and an evaluation had no agreed place for the evaluator's reason.

`gmeow:putForwardByClaim` links the reifier of a reified statement to the claim token, the assertion act, by
which that statement was put forward. The evaluation's target stays a claim token. The reifier stays a
reifier: the relation does not type it as a claim token, copies no annotation, entails no proposition and has
no display, ranking or lifecycle effect on either end. It is functional, carried by a
`logic:PropertyCharacteristicAssertion` and entered in the functional-carrier ledger, and deliberately not
inverse functional: one assertion act may put several statements forward. The bound is a precondition on the
data: each act to be evaluated gets a named reifier of its own, and a reifier shared by several sources is
never linked to a token.

The usage text of `gmeow:ClaimEvaluation` and `gmeow:evaluates` now says that an evaluation never suppresses,
ranks or replaces the evaluated claim, that the evaluator's reason goes in `gmeow:statedRationale` in its own
words, and that `gmeow:evaluates` points at the token and not at the reifier. Verdicts stay open.

Added with it: a worked example, a competency question that returns the evaluation beside the statement and
the reason, a counter-example in which `gmeow:evaluates` points at a reifier with its conformance cell, a
conforming cell for the example, two structural assertions, guide text, and French and Chinese catalog
entries with the reviewed-coverage golden extended by their keys. Two count pins in the slice-test crate are
updated for the new cells and now select their cells by IRI.

Verified: independent review PASS at e8f625ffa; the whole diff read. At that head: the four slice-test
parsing tests pass (4 run, 4 passed); `make lint`, Turtle parses and catalog compilation exit 0 at earlier
heads of the same content. Pull request checks: helpers, lint, producer-build and rust-prebuild green; the
three producer jobs and the aggregate red as on main, with no error before the producer is killed.

Not shown: nothing has executed the new ontology material. No producer pass, harness, gate or reasoning run,
so both conformance cells, the competency row, the structural assertions and the derived shapes are worked by
hand. The inverse-functional ban has no fail-witness fixture. A functional axiom makes a reasoner treat two
tokens linked from one reifier as one individual, while validation reports the second link; the sibling
`gmeow:evaluates` has the same form.

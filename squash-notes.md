feat(inquiry): absence records say why a question has no answer (Principles 4, 9, 10, 17)

A consumer needs to record, as attributed and dated data, that a question about a subject went unanswered and
why, without asserting a value or a negative. `gmeow:AbsenceRecord` is a sub-kind of `gmeow:Observation`: who
determined the absence, when, about which subject, and what was asked. `gmeow:absenceQuestion` and
`gmeow:absentProperty` name the question or the property; `gmeow:consultedSource` names the scope searched or
the source that could not be fetched; `gmeow:absenceReason` takes a value of the open vocabulary
`gmeow:AbsenceReason`: not found, retrieval failed, not published, not applicable. The definitions say what
each reason does claim; none gives a value for the asked property or negates it.

Nine `logic:Constraint` records hold a direct record to a reason, at most one reason, a vantage, a time, a
subject, at most one subject, a question or a property, and a consulted source when the reason is not-found
and when it is retrieval-failed. One reason per record is a validation constraint, not a functional axiom,
because a functional axiom would have a reasoner infer two reasons on one record to be the same individual.
Competing reasons are separate records from separate vantages and coexist. A later answer sits beside the
record and each keeps its own time. The slice now depends on `observations`.

Nothing is derived from a record: a competency question over the asserted example returns no value, denial or
refutation for an asked property, and structural assertions pin that no sub-property, inverse,
equivalent-property, chain or restriction axiom, no positive or negated rule body and no formula outside a
constraint involves the four absence properties, at module scope.

A worked example holds five records and a later answer; three competency questions and the structural
assertions cover the vocabulary; the slice guide says when to use each term; French and Chinese entries cover
every new literal.

Verified: independent review PASS at fb460b557 after five rounds; the module, manifest and example read, the
tests, guide and catalogs resting on the review. At that head `make lint`, the Turtle parses, the catalog
comparison and the guide's prose check pass. The slice's quality floors hold by arithmetic from copies of the
scoring code (prose 0.9167 against 0.8846, translation 0.6619 against 0.4074, documentation 0.75 against
0.6667). Pull request checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs
and the aggregate red as on main.

Not shown: no producer pass, harness, gate or reasoning run, so the competency questions, the structural
assertions, the lowering and running of the nine constraints, the ownership check on the new dependency and
the translation lint are unproven; everything was worked by hand. No reasoning run shows that nothing is
derived from a record; that rests on the structural pins. GMN-1 coverage is not measured.

Closes #2802.

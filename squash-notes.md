feat(memory-records): marks on an agent's important events, with watermark, frontier and give-up terms (Principles 4, 7, 9, 14, 17)

A memory service marks important events and a recorder keeps the traces around them. The slice could say
what a mark names, but not when marking is final. This adds the structural terms for that, in the
`memory-records` core slice, and leaves the order laws to the typed comparison they need.

Three declarations, each an information record that says what its declarer stated and nothing more:
`gmeow:TraceWatermark` (marking is final for every packet of one ledger at or below one named packet; it
belongs to a producer and must not pass any applicable late-producer frontier), `gmeow:TraceMarkFrontier`
(the committed point of a producer that marks events later; it declares no finality and makes no packet
eligible for discard) and `gmeow:TraceMarksGivenUp` (marks held only in memory were lost and are given up,
with the reason). Six roles carry the ledger, the producer, the cut packet, the committed point, the given-up
packets and the reason. The declarer is recorded through `wasAttributedTo` and is distinct from the producer
the declaration belongs to.

Three validation constraints, none comparing a time or a digest: a watermark names at most one cut packet; a
frontier names at most one committed point (both by term distinctness, not by a functional axiom, so a second
value is reported and not merged); and a watermark's cut packet must be listed by the watermark's own ledger,
a plain join of three roles.

Not authored, and said so where the terms are defined and in the guide and design note: that the ledger's
order is time then digest, that a watermark never moves backward, that every applicable late-producer
frontier is respected before a watermark advances, and that lost marks are given up before a watermark passes
their packet. Each needs a typed comparison of times and digests.

Controls: structural assertions for the declarations and required roles; five bans, each with a fail-witness
fixture (no comparison in a finality constraint, no derived finality role, no finality role in a rule body, no
formula outside a constraint, no functional finality role); two assertions for the membership constraint with
a violating fixture; three competency questions over the worked example. French and Chinese entries for every
English literal; the reviewed-coverage golden gains this slice's keys.

Verified: independent review PASS at f6c9de77a; the whole diff read apart from the two catalogs and the coverage golden, which rest
on the review's check that all 87 changed literals have byte-equal entries. At that head `make lint`,
`rapper -c` on the changed Turtle and `prose-check` exit 0. Pull request checks: helpers, lint, producer-build
and rust-prebuild green; the three producer jobs and the aggregate red as on main, with no error before the
producer is killed.

Not shown: nothing has executed the new material. No producer pass, harness, gate or reasoning run; the
constraints' lowering and evaluation, the assertions, the fail-witness runs and the competency questions are
worked by hand. One fixture uses an RDF 1.2 triple term in the form the shipped rules use, which the local
Turtle checker cannot read. The reviewed-coverage golden here is main's file plus this slice's keys; main's
copy is already out of date against other slices' catalogs, which this change does not repair. The guide's
sentence that the vocabulary "performs no join" predates the membership constraint and now reads loosely
beside it: it is about joining traces, not about the constraint.

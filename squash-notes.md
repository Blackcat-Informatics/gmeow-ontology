feat(sources): a source snapshot with its digest, retrieval time and immutable address (Principles 2, 4, 7)

A web source cited as evidence can change or vanish, so a citation needs to point at what was actually
retrieved. `gmeow:SourceSnapshot` is a retrieved copy of a source at one moment: a sub-kind of `gmeow:Item`,
one particular carrier of the manifestation it exemplifies, never a new work, expression or manifestation. A
second retrieval is a second snapshot.

`gmeow:retrievedAt` is when the bytes were fetched, kept apart from valid time, assertion time, source carrier
time and record time. `gmeow:immutableAddress` is the address of exactly the stored bytes; the live address
stays on the existing `gmeow:sourceLocation`. The digest reuses `gmeow:contentDigest`, and a citation names the
snapshot through `gmeow:citedEntity` with `gmeow:viaSelector`. Digest, retrieval instant, retrieval address and
immutable address are each required of a snapshot (`logic:allValuesFrom` plus a closed-world
`logic:ClosureEntry`), so a fetch that produced no bytes is not a snapshot. The slice now depends on
`creative-works`.

A worked example holds one claim, one citation and one snapshot; a competency question returns the evidence
chain; three structural assertions pin the placement, that a snapshot is not a work, expression or
manifestation, and the four requirements. The slice guide says when to use each term. French and Chinese
entries cover every new literal.

Verified: independent review PASS at 5cbfacdaf (second round); the module, manifest, example, tests and guide
read in full, the catalogs sampled and every `msgid` compared by the reviewer. At that head `make lint`, the
Turtle parses of the module, manifest and test files, and the catalog comparison pass. Pull request checks:
helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red as on
main.

Not shown: no producer pass, harness, gate or reasoning run, so the competency question, the structural
assertions, the lowering of the four required paths, the layering check on the new dependency and the
translation lint are unproven; the question and assertions were worked by hand twice, independently. The
example parses only with its RDF 1.2 triple term substituted in the parser used.

Closes #2801.

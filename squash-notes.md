fix(logic): do not validate a reification obligation declaration as an instance of its reifier class (Principles 4, 7, 17)

A schema-level reification obligation is declared as a classic `rdf:Statement` record typed with its reifier
class, so the declaration is itself an instance of that class and of each ancestor. The shapes derived for
those classes selected it as a focus node, and a recorded producer pass reported three missing-required-path
violations on declarations that are metadata, not reifier occurrences.

The shape derived for a reifier class, and for each named ancestor, now carries the sorted IRIs of the
obligation declarations recognized for it and lowers its target to a `sh:SPARQLTarget` selecting what
`sh:targetClass` selects minus exactly those IRIs. No other node is excluded: a record in the same classic
form, or a native `rdf:reifies` reifier, is still validated. The exclusion is part of the shape's content key
and enforcement key, and is ShEx residue. A blank-node declaration is refused at derivation with its subject
class and predicate, because a shape can exclude a node only by naming it.

A functional or inverse-functional property whose domain or range class lies outside the authoring namespaces
no longer mints a class shape named in a foreign namespace; the property-scoped cap still bounds every subject
or object.

`gmeow-dev conformance-bless <case>…` (and `make maint-conformance-bless CASES=…`, run through the producer
runner) refreshes the goldens of explicitly named conformance cases through the same `run_case` the producer
uses. It has no bless-everything form, refuses an unknown case, an oracle-backed case and a case that selects
shipped rules, and writes nothing if any named case fails; `--init` seeds a new case. The environment switches
that blessed from inside a test run are removed and the documents that named them corrected. The
`validation/reifier-shape` case names its declaration and is re-blessed with the command. The design notes
state the rule.

Verified: independent review PASS at 6026d22c3 after five rounds; the whole diff read. At that head, on a
clean tree: the branch's focused test selection, 328 passed and 0 failed; clippy clean on `gmeow-logic-compile`
and `gmeow-conformance`; `make producer-recipe` and `make helpers-hygiene` exit 0; `make maint-conformance-bless
CASES=validation/reifier-shape` built the producer in the `pipeline` profile and reported 0 expected files
differing. Pull request checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs
and the aggregate red as on main.

Not shown: no producer pass and no `make check`, so that the three recorded violations clear on the real
slices, which ancestor shapes move to the SPARQL target on the corpus, the regenerated shapes and bundle, and
the conformance stage's grading of the re-blessed case are unproven. `--init` is not exercised by a test or a
run. `make crate-check` was not run for the new `gmeow-dev-cli` to `gmeow-conformance` dependency. Clippy on
`gmeow-dev-cli` exits 101 under the local nightly on one `large_unopt_stack_frames` error at a line that is the
same on main. The refusals reuse the conformance crate's existing diagnostic kinds, one of which labels an
argument error as case anatomy.

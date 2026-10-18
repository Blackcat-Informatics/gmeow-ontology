# Issue #1805: logic: rule projection uses RDF 1.1 classic reification (rdf:Statement); move to RDF 1.2 triple terms

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Defect

The `logic:` program's RDF projection (`crates/logic-compile/src/projections/rdf.rs`, the "Rules as logic:Rule nodes" block) encodes every rule's head, body and negated-body atom with **RDF 1.1 classic reification**: `rdf:type rdf:Statement` plus `rdf:subject`, `rdf:predicate` and `rdf:object`. `.goals` requires RDF 1.2 only, with no RDF 1.1 shortcuts.

## Observed consequence

These atom nodes reach the `graph/logic` class world: 232 `rdf:type rdf:Statement` assertions such as `logic:rule/_000001/body/0000`. The OWL-Direct reverse mapping in purrdf 3.0.0 reports `rdf:Statement` as an `unrecognized-term` boundary, so that world's consistency verdict cannot be exact. A one-triple probe (`<s> rdf:type rdf:Statement`) reproduces it.

Probed on the same engine, the RDF 1.2 form `<b0> rdf:reifies <<( <x> <p> <y> )>>` decides with **no** boundary.

## Why this is not a mechanical swap

- Atom variables are encoded as plain literals (`"?x"`), including in subject position. An RDF 1.2 triple term cannot take a literal subject, so variables need a term representation that is valid in every triple-term position.
- The compiler frontend reads this projection back (`frontend.rs` reads `rdf:Statement` nodes), so the reader moves with the writer.
- Triple terms travel in `gmeow.gts`'s statement layer, not its base quads. A comment in `frontend.rs` gives that as the reason classic reification was chosen for one slice-authored obligation form. The rule encoding has to be carried through that layer losslessly.

## Requirement

Encode `logic:` rule atoms in RDF 1.2: triple terms reached by `rdf:reifies`, with variables as terms that are valid in every triple-term position. Round-trip byte-identically through the frontend, carry the encoding losslessly to `gmeow.gts`, and remove the RDF 1.1 reification form from the rule projection (greenfield, no dual encoding).


## Comments (6)

### paudley — 2026-10-15T18:40:28Z

# Native rule-atom projection

Issue 1805 requires RDF 1.2 rule atoms reached by rdf:reifies, legal resource-valued variable carriers in every triple-term position, byte-identical logical round trips, lossless GTS carriage, and removal of classic rule encoding. Principles 1, 2, 3, 4, 7, 17 and 18 govern this change. Scope remains the compiler and its actual first-party rule producers/consumers.

Existing mechanisms: TripleSink already owns RdfDatasetBuilder; graphutil default_graph_pattern includes the reifier/annotation layer; formula_reader::read_atomic_carrier admits typed variables and literals; logic:termDistinct already expresses a term inequality. Reuse those homes. Triple terms do not assert their contents. Variable carriers must not become logical domain assertions.

The live issue has no comments and no earlier implementation attempt. The brief found related ownership and projection issues and the combined production PR; requirements-candidates extraction found zero lines, so the entire issue body is the manual acceptance contract. No independent plan or completion review has been performed; unavailable collaboration tools are not a review pass.

## Task 1: One native rule writer and reader

Emit head, positive body, negated body and distinct guards as RDF 1.2 reifiers over triple terms. Carry variables through the existing term-carrier vocabulary, preserving literal strings that begin with a question mark as literals. Decode native statement-layer targets through the same carrier reader, rejecting missing/ambiguous targets or malformed guard relations. Preserve scope, polarity, aggregation, raw variable names and object datatypes/language/direction. Remove the classic rule writer and reader after migrating their first-party inputs; do not introduce a compatibility mode or dual writer.

## Task 2: Migrate first-party source and consumers

Migrate the repository-authored rules and affected test fixtures to the single native form. Update rule structural admission and ownership so internal carriers remain structural. Update directly affected annotation prose and its existing French/Chinese translations together. Any shared scoped-axiom emission helper must also emit native reifiers rather than retain a second classic emitter. Preserve unrelated foreign-data interpretation and dirty work.

## Task 3: Prove round trips and carriage

Use tiny synthetic programs to exercise variable/resource/blank/literal atoms, positive and negated bodies, distinct guards, aggregation and scope. Assert identical IR/content identity after projection and frontend parsing, no classic rule scaffold, no accidental assertion of quoted atoms, and exact source ownership. Exercise a small GMEOW-profile GTS bundle through its actual writer/reader with the full statement layer; tests must not produce or load the ontology corpus. Review intentional snapshot changes before acceptance.

Focused checks use the existing Make surface and standard workspace graph. Final producer/full-gate/cold-generation requirements remain on the combined production branch, after fixes settle. No compiler-profile, assertion, overflow, fixture or hook weakening.

## Task 4: Publish and integrate

Commit and push coherent verified units normally, publish actual results to the issue and combined PR, obtain fresh CodeRabbit and independent completion review where available, then qualify the combined source. Every merge uses ghprsq. No completion or merge-readiness claim before all issue acceptance and required gates are met.


### paudley — 2026-10-15T19:24:18Z

The native writer/reader and first-party rule inputs are migrated in the isolated worktree and applied to the combined production candidate. Head, positive-body and negated-body atoms now use `rdf:reifies` triple terms. Variables use typed resource carriers; question-mark literals remain literals. The classic rule encoding is removed rather than kept as a second grammar. Principles 2, 3, 4, 7, 17 and 18 apply.

One design correction from implementation evidence: native rule guards compare RDF term identity, whereas the existing procedural `logic:termDistinct` means SPARQL value inequality. Guards therefore use two ordered variable argument carriers, and their SHACL-AF projection uses `!sameTerm`. The five directly affected structural definitions and their existing French/Chinese translations move together. No comparison semantics is silently changed by the encoding migration.

The first native round-trip/scoped-quotation controls passed before the expanded migration. Combined lint passes. The expanded compiler/consumer run is in progress; it has not passed yet. Its GTS control routes a directional-literal rule with dynamic predicate, negation, guard and contextual scope through the carrier-rooting helper, the mandated GMEOW snapshot writer, and native GTS import. Compilation caught two ordinary test-code errors (checked-term construction and mandatory ingestion-report handling); both are fixed without suppressions. Empty or malformed typed carriers now fail closed in every triple position.

No full producer/gate result, completion audit, commit of the canonical migration, PR readiness or merge is claimed. The combined source must still pass its required full qualification.


### paudley — 2026-10-15T21:06:07Z

The native-rule candidate now includes the shared collection-home repairs and the corpus-purity correction discovered during validation. The old all-module list test compiled real source inside the test process; it now consumes a compact authenticated producer observation. The stronger scanner also moved authored goal-directed evaluation and translation-catalog parsing out of tests. Principles 4, 7, 12, 17 and 18 apply.

Current focused observation controls pass: 18/18, nextest e77e4757-aaa3-4e28-adc6-71c2a5ef9f9a. Corpus-purity, helper hygiene and final lint pass. The earlier native writer/reader, dialect metadata, atom-scope and actual mandated GTS write/import/read controls remain recorded separately; none establishes a full production verdict.

A fresh full make check is running on the settled combined source. It includes explicit cache-version changes for the affected producer artifact contracts. The first gate failed and its producer refused changed inputs, so this is the required reported retry. No new commit or issue-completion claim is made while qualification is pending. The published b797a315e CI head separately failed at stage-docs-render with a nonconclusive native reasoning verdict; the current-source result remains unknown.


### paudley — 2026-10-15T21:59:52Z

The qualification retry failed at the authored-default source boundary: its guard flattened valid native `rdf:reifies` bindings and mistook their quoted objects for unsupported ordinary base quads. This was a GMEOW boundary error, not evidence that PurRDF's native statement tables were lost.

The repair parses canonical RDF back into the complete native dataset before checking only its ordinary quad partition. Native bindings travel through the statement tables; unsupported ordinary quoted positions still fail closed and report the offending row. The two fixture ingestion helpers now share the corrected guard.

The expanded control first reproduced the exact producer failure. Its repaired run passed both cases: canonical authored-default plus named `graph/logic` through the actual SnapshotBuilder, mandated GTS profile, frame audit, GTS read/import and exact typed-rule consumer; and a negative ordinary quotation refusal. The selected rule retains its dynamic predicate, Arabic directional literal, negation, distinct guard and scope. Nextest 1b981437-983b-4041-88a9-efad97234113: 2 passed, 0.100s execution.

These are explicit synthetic inputs; the tests do not produce the corpus. Source-load's stage version is bumped. The repair is uncommitted, and the full retry remains FAILED. No third full gate has started while the known static, diagnostic and browser-asset prerequisites remain unresolved. Principles 2, 3, 4, 7, 18.


### paudley — 2026-10-16T00:37:14Z

The native rule migration also changed the original language module's byte
identity. The producer's shared source catalog emits that identity into the
native browser codebook, but both browser consumers still expected the previous
pin. Current original bytes hash to
`9127a8f94adc3dad1c8ca96fa54c6f6a995b6f737a62352ef99a2b2d0bbe2bb3`.

The combined candidate now refreshes that source pin and strengthens the existing
read-only `verify-wasm-codebook` operation: after authenticating the selected
producer artifact and comparing bytes, it invokes the exact native decoder used
by GMN and validation. This catches an authenticated packet that the compiled
browser consumers would refuse, retaining the original typed diagnostic.

Focused controls passed 4/4, including the original-byte identity check, complete
table round trip, wrong identity/tamper/inconsistent tables, trailing data and
size refusal. Source lint passed. Current-source producer-independent Clippy is
still running. The original standalone producer build was intentionally stopped
with exit 130 for this required source correction; the normal replacement build
is running. Neither invocation is being reported as a full qualification pass.

The authenticated codebook-verifier runtime, all six fresh browser assets, parity,
full corpus/gate acceptance, hosted cold generations, current-head review and
ghprsq merge remain required. These source corrections are local and uncommitted;
the published PR head is unchanged. No new PR or worktree was opened.


### paudley — 2026-10-16T20:38:07Z

The native RDF 1.2 rule migration was applied to the combined branch and is in main fb73513b8 through eca80ae00 / PR #1802. The isolated development copy predates the checked PurRDF APIs and newer canonical terms. Its complete staged/unstaged patches and a named stash are retained; the worktree fast-forwarded to main without reapplying the older copy.

Current-main verification passed all ten selected native statement/rule and production-profile GTS controls. This includes typed atoms and term-identity guards, full contextual scope, proposition versus assertion, named-graph boundaries, coexisting standpoints, invalid-scope refusal and GMEOW bundle carriage. The run used the existing producer-independent inventory in the 1792 worktree and produced no corpus.

The implementation is already merged and needs no duplicate PR. Full-corpus qualification is still unmet: PR #1830 passed the initial recovery/fixture defects and reached a valid but unsupported native result, then failed at the boolean documentation boundary. #1833 owns the truthful observation projection; the canonical reasoning capabilities and full gate remain required by #1757/#1748. This issue remains open rather than treating ten focused controls as a complete gate.



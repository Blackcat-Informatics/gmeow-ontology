# Native rule-atom projection

Issue 1805 requires RDF 1.2 rule atoms reached by rdf:reifies, legal resource-valued variable carriers in every triple-term position, byte-identical logical round trips, lossless GTS carriage, and removal of classic rule encoding. Principles 1, 2, 3, 4, 7, 17 and 18 govern this change. Scope remains the compiler and its actual first-party rule producers/consumers.

Existing mechanisms: TripleSink already owns RdfDatasetBuilder; graphutil default_graph_pattern includes the reifier/annotation layer; formula_reader::read_atomic_carrier admits typed variables and literals; The existing ordered argument-carrier grammar represents rule inequality guards without conflating RDF term identity with SPARQL value inequality. Reuse those homes. Triple terms do not assert their contents. Variable carriers must not become logical domain assertions.

The live issue has no comments and no earlier implementation attempt. The brief found related ownership and projection issues and the combined production PR; requirements-candidates extraction found zero lines, so the entire issue body is the manual acceptance contract. No independent plan or completion review has been performed; unavailable collaboration tools are not a review pass.

## Task 1: One native rule writer and reader

Emit head, positive body and negated body as RDF 1.2 reifiers over triple terms. Keep guards as ordered variable arguments: native rules compare RDF term identity, whereas logic:termDistinct compares SPARQL values. Carry variables through the existing term-carrier vocabulary, preserving literal strings that begin with a question mark as literals. Decode native statement-layer targets through the same carrier reader, rejecting missing/ambiguous targets or malformed guard argument lists. Preserve scope, polarity, aggregation, raw variable names and object datatypes/language/direction. Remove the classic rule writer and reader after migrating their first-party inputs; do not introduce a compatibility mode or dual writer.

## Task 2: Migrate first-party source and consumers

Migrate the repository-authored rules and affected test fixtures to the single native form. Update rule structural admission and ownership so internal carriers remain structural. Update directly affected annotation prose and its existing French/Chinese translations together. Any shared scoped-axiom emission helper must also emit native reifiers rather than retain a second classic emitter. Preserve unrelated foreign-data interpretation and dirty work.

## Task 3: Prove round trips and carriage

Use tiny synthetic programs to exercise variable/resource/blank/literal atoms, positive and negated bodies, distinct guards, aggregation and scope. Assert identical IR/content identity after projection and frontend parsing, no classic rule scaffold, no accidental assertion of quoted atoms, and exact source ownership. Exercise a small GMEOW-profile GTS bundle through its actual writer/reader with the full statement layer; tests must not produce or load the ontology corpus. Review intentional snapshot changes before acceptance.

Focused checks use the existing Make surface and standard workspace graph. Final producer/full-gate/cold-generation requirements remain on the combined production branch, after fixes settle. No compiler-profile, assertion, overflow, fixture or hook weakening.

## Task 4: Publish and integrate

Commit and push coherent verified units normally, publish actual results to the issue and combined PR, obtain fresh CodeRabbit and independent completion review where available, then qualify the combined source. Every merge uses ghprsq. No completion or merge-readiness claim before all issue acceptance and required gates are met.

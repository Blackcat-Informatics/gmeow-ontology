# Current validation

Scope: importer-owned model routing, strict schema target/documentation identities, shared source metadata, Pydantic stage cache identity and documentation. Plan: plan.md. Principles 4, 7, 17 and 18. Self-review only; no independent dispatcher is exposed.

| Requirement | Status |
| --- | --- |
| Production refusal reproduced | Three completed hosted jobs report the same imported-class documentation error; production-observation.md |
| Source ownership | Existing blood-pressure/datatype OPT namespace constants are shared by the actual producer and routed exporter; no suffix/absence heuristic or second parser |
| Imported types and actual renderer | PASS: tests-fixed.log, 21 tests; synthetic OPT-constraint lift → SHACL projection → native shape compilation → real routed Pydantic emission |
| Ontology documentation guard | PASS: ordinary and nested/near-import namespace missing-doc controls, existing owner/digest guards |
| Identity collision refusal | PASS: distinct targets sharing a key and target/documentation IRI mismatch controls |
| Formatting | make fmt passed, fmt.log |
| Source lint | PASS: make lint, lint.log; later test-only named-node conversions also passed formatting and normal commit hooks |
| Strict Clippy | Initial run FAILED: missing generated native_semantic_sources.rs build input and compiler-output capture failures, clippy.log. PASS on unchanged normal target: clippy-retry.log, exit 0, all producer-independent workspace targets with warnings denied; no source, compiler, wrapper or check setting changed for the retry |
| Normal signed commit/push hooks | PASS: normal signed commit and push, commit.log and push.log |
| Hosted CI | Will not block merge under the user's explicit instruction; no new hosted pass is claimed |
| Full corpus, generated package and Python import/schema acceptance | NOT RUN locally. Local optimized producer, corpus/fixture production, synchronization/full gate and new Python infrastructure are not authorized |

The broader 1646 and 1757 contracts remain open; this repair does not redefine or close them. Tests use tiny independent sources and may not produce a repository corpus.

The first local test build failed on the new tests' named-node construction, recorded in tests.log. The native node conversions were corrected without changing any expected result; tests-fixed.log is the final passing selection.

Integration assessment: fetched main has no advancement beyond the branch starting base; merge-tree completed cleanly, so the candidate contains the tested source.

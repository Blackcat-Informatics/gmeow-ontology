# Route imported template model definitions by their actual owner

The previous turn made authoritative progress: PR 1884 merged and issue 1657 closed. The full all-issue objective remains active, with 58 issues and no open PRs at intake. Completed cold generations and the following PR generation now reach model export but fail on Blutdruck-Diastolic-cardinality lacking term documentation. This is an openEHR-imported schema target, not an authored slice term. The registered GMEOW namespace prefix is too broad to distinguish those owners.

Read the complete issue and its nine comments. Existing routed emission, term metadata, README, strict ownership guards and the earlier validated-documentation-owner correction are merged and must be retained. Principles 4, 7, 17 and 18 apply. Preserve the full parent: actual package acceptance and the separately authorization-gated Python import/schema check remain required, with no new Python infrastructure authorized by this repair.

## Completeness contract

| Requirement | Acceptance |
| --- | --- |
| Existing importer namespace ownership is authoritative | Move the two existing source-specific OPT bases to shared compiler-owned constants; production lifting and export routing use the same declarations |
| Imported schema classes have a total route | Independently authored OPT constraint shapes compile through the actual shared native schema compiler and Pydantic renderer; every imported class goes to schema_helpers with its original class IRI and source artifact |
| No blanket missing-documentation fallback | Keep undocumented ordinary/nested ontology class targets refused; do not classify by missing prose or class-name suffix |
| Identity collisions remain hard | Reject distinct target IRIs sharing a schema key and reject a documented term whose IRI differs from the actual target with that key |
| Existing ontology metadata is preserved | Existing routed models, digests, prose, owners, README and real owner-collision/missing-metadata controls remain passing |
| Cache identity follows the correction | Advance the Pydantic stage implementation version; moving unchanged importer constants does not change compiler output |
| Qualified integration | Focused synthetic Make checks, strict producer-independent Clippy, source lint, normal signed hooks, current review, ghprsq and owned cleanup |

## Task 1: Correct the model ownership partition

Reuse the two explicit, already allocated OPT import namespaces and source paths from the compilation owner. These source-specific artifact types have no slice term coat; route them as imported schema helpers and retain sourceClass/sourceArtifact metadata without inventing ontology term digests or slice ownership. Keep ordinary helpers unchanged. Retain all ordinary ontology missing-documentation failures. Validate schema-key-to-target identity before attaching any documentation or source metadata.

## Task 2: Verify the actual importer-to-renderer path

Author tiny independent cardinality and quantity inputs, lift through the production OPT adapter, project SHACL, compile through the native shared schema compiler and emit the actual routed package. Test both importer namespaces, source metadata, stable rendering and an ordinary documented slice model together. Add negative controls for near/nested namespaces and target/documentation identity collisions. Preserve existing refusal tests. Tests must neither read nor build a repository corpus.

Run focused synthetic tests only after the complete patch settles, then strict producer-independent all-target Clippy, formatting/lint and normal hooks. Do not run local optimized producer, fixture/corpus generation, synchronization or full gate. Do not probe the host or impose resource caps. No independent reviewer dispatcher is available; reports are self-reviews.

## Task 3: Publish and integrate sequentially

Post actual results and limits, create one PR referencing 1646 and 1757 without closing either broader parent, inspect current feedback and merge with ghprsq while ignoring CI as instructed. Verify the landing/archive and remove only this owned branch/worktree. Preserve the GMN sibling and update the live shared queue. Completed hosted failures are observations, not a later production pass.

## Rejected alternatives

Do not remove imported shapes, fabricate term documentation/digests, route every missing term as a helper, infer ownership from suffixes, parse the source template a second time in the exporter, or weaken the source declaration/identity guards. No additional emitter, registry artifact, Python test surface or upstream engine change is needed for this ownership correction.

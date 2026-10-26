# Issue #1969: Use native JSON for term release-authority evidence

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, rust, complexity-hard

## Body

Part of #1776. Migrate the complete term release-authority JSON family to the existing native record codecs.

The owning implementation is `crates/pipeline/src/stages/term_manifest.rs`: private PriorTerm and ReleaseAuthority records, the required previous-release reader, and the explicit maintainer refresh writer. The synthetic fixed-point test has one matching writer. No Report metadata or other transport depends on these records.

Acceptance:

- Wire the actual reader and maintainer writer through native JSON records and remove this family's Serde derives and calls. Do not add a parallel parser or compatibility path.
- Preserve named fields and their output order, compact output plus one newline, sorted term keys, omitted empty changed_versions, missing-history default and null refusal. Unknown members remain ignored; duplicate declared fields are refused. Positional struct arrays are outside the authored authority format.
- Preserve all existing schema, release ordering, namespace, digest and history checks, atomic publication, bootstrap refusal, same-release mutation refusal and fixed-point no-op behavior.
- Qualify those behaviors using authored component bytes and tiny existing synthetic datasets, scoped strict production-consumer compilation, normal hooks and independent source review. No test builds repository corpus artifacts.
- Do not regenerate or advance the tracked authority, run a local optimized producer, alter release policy or claim full #1776 completion. Hosted corpus and installed acceptance remain with the existing parent contracts.

Principles 4 and 7 apply.


## Comments (0)


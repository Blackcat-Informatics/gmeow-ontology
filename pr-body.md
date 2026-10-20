Explicitly closed language and calendar-precision property ranges were silently omitted from generated validation shapes. The shared classifier also treated RDF language datatypes as classes, causing the wrong constraint when used on class restrictions.

Recognize ordinary and directional RDF language datatypes and canonical Datarange declarations through the existing classifier. Lower explicit property-range unions to ObjectsOf constraints, preserving open-world defaults and opt-outs. Reuse the native strict list walker; malformed, conflicting or unsupported selected ranges fail with the property as diagnostic focus. Empty and universal unions retain their meanings.

Adds focused compiler controls and an actual generated-SHACL/native-validator matrix for public/private tags, both text directions, instant/day/month/year literals and rejected machine values. Principles 4, 7, 11, 17 and 18 apply.

This is the shared projection prerequisite for the full literal migration. The property census/migration, digest/media contracts, precision migration and structured tool payload comment remain required under the original issue; it stays open. No corpus or optimized producer ran locally. Hosted CI is ignored for integration as instructed; normal signed hooks remain mandatory.

Refs #1807.

Validation: 73 focused tests passed, including generated-SHACL/native-validator acceptance. Strict producer-independent workspace/all-target Clippy passed with warnings denied. Source hygiene, static recipe admission and whitespace checks passed. Full producer/corpus qualification was not run locally.

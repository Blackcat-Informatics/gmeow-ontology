# Combined native bundle-reader and primer observation delivery

Two ready owner families share the docs-model/bundle-view/dev dependency graph, so they are settled sequentially in one exact checkout before combined root qualification. This avoids another broad workspace build and hook cycle without changing parent scope or acceptance requirements.

The primitive reader source froze first. A separate source lane then implemented only the five typed primer record/enum codecs, direct observation writer and two authenticated reader decode boundaries. Detailed seven-file inventory and nine authored controls are in implementation-primer.md and validation-primer.md. No source mutation is concurrent. Root reviewed primitive readers independently; review-primer.md independently reviews the primer family and records the signed-zero defect and repair.

Run one explicit synthetic selection for nine controls per family. Initial execution was 17 passed/1 failed on unsigned signed-zero admission; preserve its log, repair through the native unsigned-token API and rerun the same selection. Native set codecs and original error causes remain authoritative. Strict prebuild Clippy, helper hygiene, source-input admission, formatting and normal hooks follow settled source. No local optimized producer/corpus/sync/full gate. Actual authenticated bundle/primer/site/installed acceptance remains open.

## Original primitive owner plan

# Primitive bundled JSON readers — issue 1776

Base: `8d12c1afa`, after Card PR 1918. This is one bounded delivery under the
existing issue; the remaining Serde owners and full parent requirements stay open.

## Actual source path and ownership

`Bundle::from_snapshot` consumes the supplied snapshot. Existing representation
selection and blob decoding feed `schema()` through the schemas archive, and
feed `denied_cells()` directly through the denied representation. Only the two
JSON decoding closures change. Their production methods call private pure
helpers; there is no second bundle reader, native-to-Serde bridge, fallback,
new dependency, or change to archive/authenticated byte selection.

## Implementation

- Replace the free-form schema decoder with native `record::read_document`,
  whose owner explicitly resolves duplicate object names recursively last-wins.
  Retain sorted `$defs` key extraction and empty results for absent/non-object
  `$defs` and scalar/array/null documents.
- Deliberately admit native RFC numeric lexemes such as `1e400` in ignored
  schema values. No numeric identity conversion or schema validation is added;
  keys remain decoded strings. Document this admission difference.
- Replace denied-row decoding with native `record::from_slice` into
  `Vec<Vec<String>>`, retaining the exact three-string conversion and its
  explicit refusal. Preserve output types, ordering, duplicates and strings.
- Retain wheel-only optional absence, malformed-document hard failures, and the
  existing diagnostic kind. Keep `diag_nodes` and all remaining Serde ownership.
- Add nine independently authored `json_bundled_` controls. They construct no
  bundle, corpus, fixture, receipt or producer output.

## Exact source inventory

1. `crates/bundle-view/src/bundle_blobs.rs`
2. `crates/bundle-view/src/bundle_blobs/json.tests.rs` (new)
3. `crates/bundle-view/BUNDLED-JSON.md` (new)

Stage records are this plan and `validation.md`. No manifest or lock changes.
Root owns formatting, qualification, integration and publication. This source
unit does not complete issue 1776 or establish whole-bundle acceptance.

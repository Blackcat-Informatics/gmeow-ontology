# Validation

Source is frozen for root qualification. No formatting, builds, tests, host probes,
corpus/fixture/codebook/producer/model execution, lock edits, commits or posts were
performed in this lane. Root owns qualification. Existing real model golden and
produced site/search artifact acceptance remain unrun.

## Source inventory

- crates/docs/Cargo.toml: necessary direct purrdf-lex owner dependency; other JSON
  and template dependencies retained.
- crates/docs/src/render.rs: private SearchRecord ToJson and the actual shared
  sort/write adapter; public String API and record assembly unchanged.
- crates/docs/src/render.search_index.tests.rs: new external pure unit module.
- crates/docs/SEARCH-JSON.md: private wire contract and public-model behavior.

Stage plan.md and validation.md are process evidence. No Card, NavItem,
minijinja, Cargo.lock, source-policy, fixture, generated output or asset edits.
The selected native owner is purrdf_lex::json::Object plus record::ToJson and
write_pretty. Object preserves insertion order; Vec preserves item order and
Option::None emits null. The writer adds no newline. No local JSON macro,
parser/writer, compatibility layer or native-to-Serde translation was added.

## Focused controls (unrun)

Filter: package(gmeow-docs) and test(json_search_index_)

Eight tests use only independently authored SearchRecord and DocsModel values:
complete literal record bytes and field order; explicit null definition; omission
of empty vectors versus retention of vectors containing empty strings; populated
facet key/vector order; Unicode, control-character and quote/backslash escaping;
URL/kind/ID tie-breaking; empty adapter/public model; complete public concern
bytes; public child-document URL and full-prose retention without a docs.md child;
and public term advice plus sorted/deduplicated alignment facets. The public
path builds only an in-memory source-to-page map from supplied plain model values.
No SliceCatalog, fixture/cache loader, source compilation or producer is called.

## Caller and acceptance boundary

search_index_json(&DocsModel) -> String retains its public signature. It and the
site renderer's search_index_json_with_map call the same private adapter with the
original (url,kind,id) ordering. Facets are vectors, not maps; their original
Vec::is_empty omission rule is retained exactly. Source-document extraction and
page-map URL behavior are unchanged.

Existing render_golden::search_index_json_golden and synthetic-slice coverage
were not run or weakened. Their authenticated real inputs and full rendered-site
artifact parity remain separate acceptance. No corpus was constructed and no
golden was blessed. Keeping docs' remaining normal serde_json dependency is
intentional because the Card caller closure has not yet migrated.

## Root qualification and final inventory

Root ran make fmt (exit 0), git diff --check (exit 0), make nextest-synthetic NEXTEST_FILTER='package(gmeow-docs) and test(json_search_index_)' (exit 0; 8 executed and passed, 227 outside selection skipped), make clippy-prebuild (exit 0; 49.05 seconds), make helpers-hygiene (exit 0), and make producer-recipe (exit 0; source identity only). Raw outputs are retained beside this report. No optimized producer or corpus ran.

Final source inventory is the four files above plus Cargo.lock: normal resolution adds only the direct purrdf-lex dependency to the docs package; no pin change. Independent review in review-independent.md found no blocking source finding. Root inspected the implementation and all eight controls. Normal commit/push hooks remain mandatory.

Main advanced with finance-only annotations, translations, and FnO tests. Those changes do not touch docs, native JSON, dependencies, or the exercised caller closure; existing focused results remain applicable. Fresh merge-tree assessment will check integration. Card, NavItem, and CachedSite retain their existing JSON owners. Authenticated real-model golden, actual full rendered-site, installed consumer, and full gates remain unrun under the recorded campaign exception. Issue 1776 remains open.

Normal commit and push hooks completed with exit 0. Fresh merge-tree against main exits 0; finance-only base delta introduces no shared API, schema, dependency, generator, or runtime change affecting the docs closure. Pending CI recorded in pr-checks.txt and raw/pr-state.json is explicitly waived for this campaign. No source mutation followed qualification.

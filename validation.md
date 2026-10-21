# Validation

Source is frozen for root qualification. No formatting, builds/checks/tests, host
or resource probes, model/corpus/fixture/producer execution, lock edits, commits,
pushes or posts were performed in this lane. Root owns qualification.

## Exact source inventory

- crates/docs-model/src/card.rs: five native output records, direct JSON, sorted
  TOON working value and native authored schema; accurate omission/dependency docs.
- crates/docs-model/src/card.tests.rs: existing serializer expectations ported.
- crates/docs-model/src/card.json.tests.rs: new external pure owner controls.
- crates/docs-model/CARD-JSON.md: new public wire/capability contract.
- crates/docs/src/render.rs: both direct compact Card writer adapters.
- crates/docs/src/render.tests.rs: existing expected-Card serialization ported.
- crates/docs/src/render.card_json.tests.rs: new plain-model adapter control.
- crates/mcp/Cargo.toml: native direct dependency selected only by core.
- crates/mcp/src/lib.rs: actual private complete Card envelope adapter and caller.
- crates/mcp/src/tests.rs: existing real schema assertions consume schema bytes;
  native schema Display is the native compact writer, with no Serde translation.
- crates/mcp/src/card_json.tests.rs: new external pure envelope controls under core.
- crates/pipeline/src/stages/json_schema.rs: actual native Card schema publication.
- crates/pipeline/src/stages/json_schema.card_json.tests.rs: new external pure
  publication adapter check with complete independent literal schema bytes.

No Card decoder, schema derive/field bitmap, local JSON engine or compatibility
bridge was introduced. No CachedSite/NavItem/minijinja/receipt/other JSON owner,
source-policy, lock, fixture, artifact or attestation edits. Examples were audited:
docs' coverage refresh and pipeline's diagnostic example use unrelated JSON, so
all their dependencies remain. Docs/src/fixture.rs still requires serde_json;
no whole-crate dependency-removal claim is made.

## Focused controls (unrun)

Filter: (package(gmeow-docs-model) or package(gmeow-docs) or package(gmeow-mcp) or package(gmeow-pipeline)) and test(json_card_)

Thirteen independently authored pure tests: seven record/schema/TOON owner tests,
one docs model adapter, four complete MCP envelope adapter tests and one pipeline
schema publication adapter. Tests pin complete compact Card bytes for every field
and panel, pretty escaping/no-LF, required/present-empty retention, empty omission,
tier projection, sorted TOON root/table columns, numeric-looking string quoting,
complete MCP outer/nested ordering and literal UTF-8 byte/character-token costs,
Markdown payload and tier boundaries, actual single-term compact bytes, native
schema positive/negative payload controls and complete sorted pretty schema+LF.
The literal schema expectation is independent of the new schema helper; root's
read-only review confirmed its semantic equality and exact prior publication bytes.

Tests call no corpus/fixture loader, SliceCatalog, source compilation, producer,
MCP server/bundle, engine query or runtime model. Native schema validation runs
only over independently authored JSON values. Existing authenticated full MCP
Card/schema assertions are preserved at their byte boundary, not replaced by
the synthetic controls.

## Acceptance boundaries

All formatting/compile/test/Clippy/helper/source-identity checks are unrun in this
lane. Authenticated bundled MCP card/schema/tier behavior, full rendered site and
single-term emitted artifacts, installed describe JSON/TOON, actual pipeline
publication and browser core/reasoning feature images remain unrun. No producer
or corpus was run to fill those gaps. Pure source coverage is not their substitute.

## Root qualification in progress

After the narrow readability/prose repair and explicit refreeze, make fmt and git diff --check exited 0. The focused native run selects all eleven existing pure card::tests plus thirteen new controls across docs-model/docs/MCP/pipeline (expected 24). Actual terminal result is exit 0: 24 tests executed, 24 passed, 1525 outside selection skipped. Raw output is native-tests.log. Normal lock resolution adds only the direct native dependency edge to the MCP package; no pin changed. Independent review is clear; see review-independent.md and root source-audit.md. No optimized producer/corpus/full gate runs are authorized locally.

## Root final qualification

make nextest-synthetic NEXTEST_FILTER='(package(gmeow-docs-model) and test(card::)) or ((package(gmeow-docs) or package(gmeow-mcp) or package(gmeow-pipeline)) and test(json_card_))' exited 0: 24 executed, 24 passed, 1525 outside selection skipped. These are the eleven existing pure Card tests and thirteen new controls, not authenticated bundle tests.

make clippy-prebuild exited 0 (30.05 seconds); make helpers-hygiene exited 0; make producer-recipe exited 0 (source-input admission only). Root collected all actual terminal results. Formatting/diff checks passed after refreeze. No source change followed those checks. Normal Cargo resolution added the required MCP edge before the focused run. Final source inventory is thirteen files above plus Cargo.lock. No pin or other package changed. Normal signing/commit/push hooks are still required and will run fully.

The recorded campaign exception omits local optimized producer/corpus/sync/full-gate runs and allows sequential merges without CI. It does not establish authenticated MCP/site/schema/installed/browser or full issue 1776 acceptance. The independent review grants no additional waiver. No fixture, attestation or golden was refreshed.

Normal commit and push hooks completed with exit 0. Fresh main is unchanged from the branch base, and git merge-tree exits 0. No integration delta invalidates the focused results. Pending current CI is recorded in pr-checks.txt and raw/pr-state.json and covered by the explicit campaign exception; it is not reported as passed.

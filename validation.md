# Validation state

Initial synthetic selection failed to compile refresh-witness-mcp: two JSON reads still used the removed dev dependency. No tests executed. A narrow search found this sole remaining caller in the three crates. Migrate its existing envelope/payload reads to read_document, preserving validation and error propagation. Do not run the producer or write attestations. Retain the initial raw log as failed-build evidence.

Source frozen for root review. No formatting, builds, tests, host probes, model/corpus production, commits or posts by this lane. Cargo.lock untouched. Root recorded a successful pre-change source-only producer recipe before mutation; that baseline is not post-change acceptance.

## Exact source inventory

Only crates/query-wasm/Cargo.toml and tests/witness_query.rs; crates/mcp-core-wasm/Cargo.toml and tests/witness_core.rs; crates/mcp-wasm/Cargo.toml and tests/witness_mcp.rs changed. Each manifest replaces its dev-only serde_json dependency with purrdf-lex. No runtime source, requests, generated or committed attestations, bundle loaders, fixture selectors, pipeline policy or other source files changed.

All seven JSON reads now use record::read_document, preserving sorted/last-wins free-form objects. No public serde types or adapters cross this boundary. The native owner's numeric equality compares exact decimal values; the reasoning ID check still requires numeric value 1 and the unchanged full-frame attestation comparison independently requires exact wire bytes. No JSON output is regenerated or serialized by this unit.

## Actual helper ownership

- query-wasm decode_queries separates the existing committed-file read from decoding. The real query witness consumes its exact returned names/SPARQL. Unknown fields remain ignored and escaped text remains complete.
- mcp-core assert_core_answer/assert_deferred_answer contain the former conversion/routing JSON assertions. The existing real witness still authenticates one bundle, initializes one engine, checks deterministic real responses and pins both unchanged attestations.
- mcp-wasm judgment_from_answer contains the former protocol/product/proof-verdict JSON assertions and returns the decoded judgment String. The real witness retains full-vs-segmented engine byte equality, judgment control-character/RDF parse checks and exact unchanged attestation comparison.

## New pure controls (unrun)

Nineteen json_witness_* tests use independently authored literal JSON only. Query controls cover full quoted Unicode SPARQL/newline text, duplicate last-wins names and ignored fields, malformed/non-array documents and missing/wrongly typed required values. Core controls cover conversion/RDF-1.2 quoting and correct recall deferral, malformed outer/inner JSON, non-text contents, failed products, lost quoting and wrong tool/segment routing. Reasoning controls pin nested quoted Unicode judgment/newline decoding and reject malformed JSON, uncorroborated/incomplete products and missing judgment text. These controls never invoke init, mcp, a source/bundle loader, RDF parser, filesystem, producer or subprocess.

Initial root selection: make nextest-synthetic NEXTEST_FILTER='(package(gmeow-query-wasm) or package(gmeow-mcp-core-wasm) or package(gmeow-mcp-wasm)) and test(json_witness_)'. Formatting, compilation and runtime verdicts remain unrun.

## Real acceptance boundaries (unchanged, unrun)

- package(gmeow-query-wasm) and binary(witness_query) and test(native_query_results_match_the_witness_attestation): reads the committed small TriG/query corpus and frozen documentation attestation; no GTS required and no refresh allowed.
- package(gmeow-mcp-core-wasm) and binary(witness_core) and test(lean_core_native_witnesses_share_one_authenticated_import): requires already-produced authenticated bundle identity, executes the real core engine and checks conversion/deferral frames.
- package(gmeow-mcp-wasm) and binary(witness_mcp) and test(native_mcp_frame_matches_the_witness_attestation): requires authenticated bundle identity, executes full/segmented engines and validates real judgment RDF/attestation.

Do not admit the two bundle witnesses through nextest-synthetic or construct their corpus. Node/wasm parity remains unrun. This unit removes only three dev-dependency edges; the underlying production MCP JSON owner migration and full #1776 acceptance remain open.

## Independent source review

No blocking finding. Native record decoding preserves sorted, last-duplicate-wins free-form objects and decoded strings. Actual authenticated callers retain every extracted assertion, bundle/engine work, RDF parsing and unchanged byte attestation. Native numeric equality admits id 1.0 as the same numeric value as 1; actual byte comparison remains required and therefore still rejects any changed response bytes. No compilation or runtime claim follows from this source review. Root make fmt completed successfully.

## Root correction and identity result

Expanded source scope to the sole missed example caller: crates/mcp-wasm/examples/refresh-witness-mcp.rs, import plus two native reads only. Independent review confirms existing fallible validation, judgment RDF check, deterministic frame comparison and original-byte publication remain intact. Narrow search across all three crates finds no serde_json/serde uses. No producer execution or attestation mutation. Cargo resolved exactly the three selected dependency edges in Cargo.lock. make fmt EXIT0; make producer-recipe EXIT0 and cmp of before/after receipts EXIT0: optimized production identity unchanged. Corrected pure-control selection is running; initial failed compile terminal EXIT2 is retained in native-initial-failed.log and is not a test result.

## Corrected runtime result

make nextest-synthetic with the documented three-package/json_witness_ filter terminal EXIT0:19 executed,19 passed,16 outside-selection tests skipped. Build also compiled the corrected maintainer example; no example/producer execution. These literal tests exercise actual reader/assertion helpers only. Strict producer-independent Clippy is running. Consumer-inclusive Clippy, actual authenticated witnesses and browser parity remain unrun.

Strict make clippy-prebuild rerun terminal EXIT0 (2m53s), all selected workspace/all-target diagnostics with warnings as errors; same four embedded consumer exclusions as documented. No lint suppressions or weakened policy. The documented CARGO_REAL points at the unchanged configured managed nightly Cargo and retains the wrapper. Full consumer-inclusive Clippy remains unrun. Helpers-hygiene pending.

make helpers-hygiene terminal EXIT0 with unchanged debt and policy. Final source inventory is eight files: the three manifests, three actual witness tests, the sole MCP refresh example and normally resolved Cargo.lock. git diff --check PASS. All check writers complete. Commit/push hooks pending.

Normal commit and push hooks completed successfully with the same configured managed Cargo selected through the documented wrapper setting. No bypass flags or policy changes. Fresh origin/main delta is only PR1910 ledger retirement and guidance (two disjoint files); candidate merge-tree succeeds without conflicts. Nineteen JSON controls and strict prebuild Clippy/helpers remain applicable: no JSON owner, signature, dependency or witness path changed in that base delta. The before/after identity comparison was performed against the same pre-1910 base, proving this dev-only unit itself leaves producer identity unchanged; it is not a claim that the legitimate finance-ledger source change leaves global producer identity unchanged.

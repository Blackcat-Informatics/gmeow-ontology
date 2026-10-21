# Validation

Source is frozen. The source implementer ran no checks; root qualification is
recorded below. Actual producer, embedded bundle/codebook, wasm/Node and installed
acceptance remain unrun.

## Exact source inventory

- crates/lang-bridge/Cargo.toml
- crates/lang-bridge/src/gmn_validation.rs
- crates/lang-bridge/src/gmn_validation.tests.rs (new external pure unit module)
- crates/lang-bridge/JSON.md (new wire contract)
- crates/validate-wasm/Cargo.toml
- crates/validate-wasm/src/lib.rs
- crates/validate-wasm/README.md
- crates/pipeline/src/stages/conformance/wasm_gmn.rs
- crates/pipeline/src/stages/conformance/wasm_gmn.tests.rs (new external pure unit module)
- crates/gmn-wasm/Cargo.toml
- crates/gmn-wasm/src/lib.rs
- Cargo.lock (root resolved three package dependency edges normally)

The source implementer did not edit Cargo.lock. Other Serde/CBOR records, source
legend, attestations, generated assets, codebook bytes, source selection and
runtime validation remain unchanged.
The companion GMN wasm test reader was the crate's sole serde_json use; package
manifests, source, examples and Rust test targets were searched before removal.
The validate-wasm crate's complete serde_json dependency is also removed.
Pipeline keeps unrelated JSON dependencies and examples unchanged. The verdict
has only the two migrated production callers and no other typed embeddings.

## Focused pure controls (root PASS)

Filter: (package(gmeow-lang-bridge) or package(gmeow-pipeline)) and test(json_gmn_verdict_)

Eight owner tests cover literal compact success/failure bytes, Unicode/escaping,
all optional omission combinations, missing/null optionals, roundtrip, unknown
fields, known duplicates including equal/null repetitions, required boolean and
optional-string type checks, missing required field, object-only admission,
malformed and trailing JSON. One pipeline test exercises the actual producer
adapter over two independently authored verdicts and pins sorted outer paths,
declaration-order nested fields and no newline. It calls no SourceCatalog,
dictionary, codec, source reader, corpus loader, producer or runtime engine.

## Acceptance boundaries

validate-wasm's existing pure gmn_verdict_json_retains_success_and_typed_failure_fields
still exercises the actual wrapper with its independent tiny dictionary. Its
package also embeds generated/projections/lang/gmn-codebook.cbor at compile time;
compilation and the produced-codebook observations/digest check are not qualified
by the source-only owner tests. Real wasm/Node/export/package and installed checks
remain unrun, as does actual pipeline/wasm-gmn-verdicts.json byte parity.

The gmn-wasm source-legend observer retains the authenticated stage-conformance
selection and every existing assertion, including real token-cost comparison.
It was not executed. Neither reader reserializes authenticated witness bytes or
turns absent selected data into corpus/codebook production. No attestations or
assets were fabricated, weakened or refreshed.

Native owner source confirms declaration-order Object, String-keyed BTreeMap
ToJson, Record.required/optional duplicate/type law, and direct typed from_slice
admission without free-form last-wins normalization. The producer adapter uses
owned String path keys to satisfy that selected native JsonKey API. No new macro
or generic compatibility decoder is introduced. The object-only replacement of
incidental prior positional-array admission is explicit in user-facing JSON.md.

Root additional source audit: the exact pinned native Value implements both Display and Debug (crates/lex/src/json/value.rs:900,917). An initially suspected unported assertion format was valid; existing Display diagnostic strings were preserved. Both wasm source files and assertion/format callers were inspected against that pinned owner; no incompatible Value assumptions found. This source audit does not compile or execute excluded wasm consumers.

## Root qualification

| Command / review | Actual result and boundary |
| --- | --- |
| make nextest-synthetic with the exact filter above | Exited0;9executed9passed1317outside-selection skipped. Eight owner codecs plus actual producer-map adapter over authored values; no dictionary/corpus/engine execution |
| make clippy-prebuild | Exited0; strict workspace/all-target -D warnings over repository producer-independent inventory,31.81s; gmeow-cli/lsp/gmn-wasm/validate-wasm excluded by target. No full consumer-inclusive Clippy claim |
| make helpers-hygiene | Exited0; ownership/dependency/hash-domain controls, no debt or exception change |
| make fmt; git diff --check | Exited0 after preserving original wasm diagnostic text |
| make producer-recipe | Exited0; actual selected source-input admission only, no optimized build or corpus production |
| Independent review | review-independent.md: no blocking source finding; root additionally checked real writer/reader diffs and pinned Value owner audit |
| Normal commit/push hooks | Both exited0 without bypass; commit.log/push.log. Initial spelling failure and fixture repair below |

Only three package dependency edges changed in the lock; no pin or unrelated
graph changes. Twelve exact source files include three new files explicitly
inspected despite ordinary Git status hiding untracked files. Base delta1914 is
the independently qualified FnO reference-extension repair; it changes no GMN
verdict API or source contract. Fresh merge-tree integration will be inspected.
No local corpus/full gate and sequential ghprsq ignoring CI remain the user
exception; parent1776 and all actual produced/installed acceptance remain open.

The first normal commit attempt failed codespell on an intentionally malformed
JSON boolean token. Replaced it with uppercase TRUE, an equally invalid JSON
boolean, keeping the negative expectation and all other malformed/trailing cases.
No suppression or hook bypass. The same nine-test selection reran: exited0,
9passed1317outside selection skipped; native-tests.log is the final result and
native-tests.initial.log/commit.initial.log preserve the first evidence. Only the
test string changed, so the strict Rust analysis and source-input recipe result
remain applicable; normal commit hooks rerun fully.

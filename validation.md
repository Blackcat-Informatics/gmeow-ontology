# Native CachedSite JSON acceptance index

| Check / behavior | Actual result | Evidence / scope |
| --- | --- | --- |
| Combined authored/in-memory controls |15 executed,15 PASS,440 outside selection skipped | native-tests.log; eleven new json_cached_site_ plus four existing exact regressions |
| Strict producer-independent Clippy |make clippy-prebuild exit0,22.08s | clippy.log; workspace/all targets -D warnings, four embedded consumers excluded |
| Helper ownership |make helpers-hygiene exit0 after direct native-owner repair | helpers.log; original failure retained, no debt/config change |
| Producer source admission |make producer-recipe exit0 on final source | producer-recipe.log; source identity only, no optimized build or corpus production |
| Formatting / whitespace / lock |make fmt exit0; git diff --check exit0; Cargo.lock unchanged | fmt.log and explicit source inspection |
| Independent review |review-cached-site-json.md clear, including helper repair | Actual2reads/1writer, shared digest byte boundary, dependencies, failure paths, tests and string parity |

Exact selection: (package(gmeow-docs) and (test(json_cached_site_) or
test(cached_site_round_trips_to_identical_bytes) or
test(an_edited_site_envelope_is_refused) or
test(cached_site_rejects_non_utf8_files))) or
(package(gmeow-docs-model) and test(an_edited_model_envelope_is_refused)).
No broad fixture selection. These consume authored Site maps/JSON or a default
plain model only. They run no action store, renderer, discovery, prime, selected
corpus loader or producer; expected tamper/nonUTF8 panics remain expected panics.

Eleven new controls pin the complete digest/files compact envelope, independently
computed literal SHA1, sorted string-map/noLF bytes, Unicode/control/empty values,
strict required kinds and native pointers, known duplicate refusals, file-map
lastwins, ignored metadata/formatting invariance, edited payload/digest refusal,
malformed/trailing/UTF8/BOM/surrogate refusals and nonUTF8 Site bytes.
Root independently computed SHA1 with Python hashlib over exact authored bytes
domain+US+site+RS+compact {"index.html":"hi"}, noLF:
9eb22eb612249bac6ae424a0189a7db696d0faa3. No owner writer or corpus supplied it.
Pinned native COMPACT ShortForms and selected Serde1.0.151 escape tables match
for strings; literal tests cover quotes/backslashes/C0/rawslash/DEL/C1/
U2028/U2029/astral text in filenames and values, without a Serde runtime oracle.

The same shared SHA1 owner now consumes caller-supplied canonical payload bytes.
Model body serialization/panic is unchanged; site/book uses native string-map
bytes and retains label site. Both actual decoders retain prior selector/context/
receipt verification; into_site still verifies before reconstructing files.
Only docs production serde_json is removed; dev edge and model/receipt/template
Serde remain honest separate owners. No dependency revision, lock pin or authentication-policy change.

Initial15controls, strictprebuild29.84s/sourceadmission/fmt passed, then helper
hygiene found an isomorphic site_payload_bytes compact-byte wrapper matching
existing verdict_map_bytes. Original logs are retained with -before-helper-fix.
Pinned record.rs639 provides exactly compact(ToJson).into_bytes. Root deleted
the new wrapper and directly used json::record::to_vec at the producer writer
and two digest calls; three authored payload assertions call that same owner.
All literal expectations stayed unchanged. Final15 rerun and independent repair
review passed. No sibling GMN edit, generic bridge, suppression or policy weakening.

User retained no local optimized producer, corpus/fixture construction, sync or
full gate and sequential merges ignoring CI. Actual source-produced cache/receipt
selection, real site/book rendering, authenticated/installed/full consumers and
full1776 acceptance remain unrun. The four local exclusions are gmeow-cli,
gmeow-lsp, gmeow-gmn-wasm and gmeow-validate-wasm. Hosted1917 Pages build119228681872
now fails at stage-mappings broad schema.org exemption8>7; the valid math quantity
bridge and overly broad procedure family are owned by next1894 repair. This is
not cache production acceptance. Normal signing/commit/push hooks remain required.

Normal commit/push both exited0 with required hooks; logs retained. PR1923 created once despite stagectl JSON-parser error, recovered by forge lookup without retry.

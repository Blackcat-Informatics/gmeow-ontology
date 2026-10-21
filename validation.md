# Combined validation index

Scope: primitive schema-key/denied-cell readers plus the complete GMN primer observation JSON family. Exact seven-file primer inventory and source-only report: validation-primer.md. Primitive inventory: bundle_blobs.rs, new bundle_blobs/json.tests.rs and BUNDLED-JSON.md. Ten source/doc files total; no manifest, lock, dependency pin, artifact, fixture, golden or attestation change.

Root make fmt and git diff --check exited 0 before each focused run. First combined run exited 2 from Make after nextest failure: 18 executed, 17 passed, one signed-zero counter failure; native-tests-initial.log retained. No expectation changed. The two counter fields now use native Value.as_u64 plus checked usize conversion through Record.required_with, preserving field-located native DecodeError. HeldoutTask uses direct native set codecs; authenticated docs decode preserves the original native error through Diag::from. Independent repair review is clear.

The same combined selection now exited 0: 18 tests executed, 18 passed, 212 outside selection skipped. Root collected the terminal result; native-tests.log retains raw output. Signed-zero rejection and all other prior expectations are unchanged. make clippy-prebuild exited 0 (39.50 seconds), make helpers-hygiene exited 0, and make producer-recipe exited 0 (source-input admission only). Root collected the complete guarded command terminal. Mandatory normal commit/push hooks remain pending.

## Primitive source handoff (original limits)

# Validation — primitive bundled JSON readers

All executable checks below are **unrun** at source handoff. No formatter,
compiler, test, gate, producer, host probe or corpus action was invoked by the
implementer. Root owns sequential qualification.

## Pure controls authored

Suggested focused selection:
`package(gmeow-bundle-view) and test(json_bundled_)`.

| Control | Authored expectation | Status |
| --- | --- | --- |
| Schema keys | Decoded escapes/Unicode and exact sorted string identities | Unrun |
| Schema duplicates | Last `$defs` wins, including object-to-null/array transitions; duplicate definition names contribute one key | Unrun |
| Empty shapes | Scalar/null/array roots and absent/non-object `$defs` preserve empty extraction | Unrun |
| Native numeric admission | Ignored `1e400`/large values admitted; numeric-looking keys stay distinct; denied numeric member refused | Unrun |
| Schema malformed bytes | Syntax/trailing data/UTF-8/BOM/surrogate failures retain `BundleJson` and schema context | Unrun |
| Denied values | Exact decoded strings, empty strings/rows, order and duplicate rows retained | Unrun |
| Denied types | Strict outer/inner arrays and strings; nested error carries `/0/1` | Unrun |
| Tuple widths | Short/long rows retain `denied-cells row is not a 3-tuple` refusal | Unrun |
| Denied malformed bytes | No partial rows for malformed/trailing data, invalid UTF-8 or unpaired surrogate | Unrun |

These controls use literal authored JSON only and call the same private decoder
helpers as the actual methods. They do not prove snapshot, archive, receipt or
authenticated producer acceptance. Those paths are unchanged by source review.

## Required handoff checks and acceptance limits

- Formatting, focused pure tests, strict prebuild/Clippy and mandatory hooks:
  unrun; root to record exact results after qualification.
- Existing selected/authenticated bundle consumers, generated schema and
  installed/wheel consumer acceptance: unrun here; production inputs must be
  supplied by their authenticated producer boundary. No test may create them.
- Full corpus/producer/gates: unrun under the campaign's recorded execution
  exception; no whole-corpus or issue-completion claim.
- Remaining Serde surfaces, including canonical diagnostic nodes and the
  independent authenticated schema test reader, retain their current owner.

This source handoff grants no additional waiver and does not establish broader
acceptance.

## Final root scope and limits

The final ten source/doc files are the three primitive files listed above and seven primer files in validation-primer.md. No manifest or Cargo.lock change occurred. Original artifact names, source identity, manifest checks, selector assertion, OnceLock behavior and corpus assertions are unchanged. Native generic unsigned decoding admitted signed zero; the field-owned native unsigned-token API repair preserves the declared strict counter contract without a new parser or changed expectation. Direct native set codecs and original error propagation are in the tested source.

Root inspected all ten files, the initial failure and final run. Independent primitive review is review-bundled-readers.md; independent primer review and repair disposition are review-primer.md. No outstanding source finding remains. No production fixture, GTS, codebook, dictionary or corpus was built by tests or locally. The selected pure tests do not establish actual authenticated bundle/primer, installed consumers, actual producer output parity or full issue 1776 acceptance. Those remain open under the recorded campaign execution/CI exception.

Normal commit and push hooks completed with exit 0. Fresh main is unchanged from the branch base, and git merge-tree exits 0. No integration delta invalidates the focused results. Current CI helpers/lint passed; producer/prebuild and review are pending. Full current check state is retained in raw/pr-state.json; pending CI is covered by the recorded campaign exception, not reported as passed.

# Current qualification

Native replacement: PurRDF PR 573 merged via ghprsq as aa4ae5c133bb10203203fe98f061649cf31ed055; PR 574 corrected finite-space exhaustion and the stale unsupported-type fixture, merging as 298edd98d87641094c6b3e947894820748cf3e48. The final source pin is 2845023147548b7bc6a6264149d4239e2caf99ed, the preserved PR 574 head with the same source tree. Both native merges retain their complete evidence at refs/ghprsq/pr/573/stage and refs/ghprsq/pr/574/stage. Their worktrees/branches were removed after audit verification and native main was fast-forwarded. Issue 572 remains open for the downstream criterion.

GMEOW source removes ad-hoc anyURI membership, opaque equality, whitespace normalization, primitive recognition and capacity handling. Native XsdValue::facet_length also replaces GMEOW's string/binary counting copy. NativeValues preserves the untouched lexical before native anyURI parsing, so non-XML Unicode whitespace is not stripped. Both manifests and both lockfiles select the same native revision; each lockfile changed exactly 31 source lines with no unrelated package updates.

| Criterion | Current evidence | State |
| --- | --- | --- |
| NativeValues and DatatypeCache consumers | make nextest-synthetic with explicit gmeow-logic value/datatype filter: 12 passed, 2319 skipped, 0.051s; nextest-final-native-values.log | PASS for datatype cutover |
| Substrate source identity | Final corrected-pin selection passed: 19 tests, 2315 skipped, 2.754s; includes immutable-pin fixture, identity folding/target-profile controls, datatype cases and both native witnesses; nextest-fixed-substrate.log | PASS |
| Tiny closure/conjecture attestations | Explicit final make maint-refresh-reason-witness passed against 284502314, preserving seven closure facts and verified corroborated/refuted-in-standpoint verdicts; final-refresh-reason-witness.log | PASS |
| Native and WASM witness consumers | Both final native read-only witness cases passed. Four actual Node cases passed on implementation head 9d992189d with earlier pin 128dda6bf; reason-node-parity.log. Updated native primitive also passed its three actual WASM cases. Full final-pin browser replay remains hosted-CI pending | SCOPED REUSE; FINAL CI PENDING |
| Affected Rust warnings | make clippy-prebuild with gmeow-build-inputs, gmeow-logic and gmeow-reason-wasm, all targets and warnings denied: passed; clippy-affected.log | PASS |
| Formatting | make fmt and git diff --check passed | PASS |
| Full hosted corpus/gate | Not run locally; production stays hosted. No equivalent acceptance claimed | PENDING |
| Hooks and publication | Both normal configured commit hooks passed; implementation and final fixed-pin commits pushed in PR 1838 | PASS |

Historical failed execution is retained: nextest-native-values.log compiled the earlier ac4094981 pin, before its native facet_length extraction was published; it failed with E0599 at the downstream call. The pin was corrected to 128dda6bf and the actual 12 selected tests passed. This failure is not counted as a pass. The subsequent immutable-source provenance change needs the additional controls and refreshed witnesses recorded above; the earlier datatype pass does not establish that separate behavior.

The broader issue 1803 remains open. No local optimized GMEOW corpus producer, runtime hardware fingerprint, machine probes or resource caps have been used. No compatibility path or performance ratio is claimed. CodeRabbit reviewed the native semantic implementation with no actionable comments and rate-limited its mechanical length-method delta; downstream independent review has not yet occurred.

Hosted native CI found one stale boundary fixture after merge: it still used anyURI as unsupported. The owning issue 572 records run 39018704037/job 117356063207 (717 pass, one failure, one ignored). Native PR 574 preserves both supported-anyURI and genuinely unmodelled owl:real rows, and also corrects finite-space exhaustion. Its 17 affected data tests passed after the correction. The earlier failed CI job is not represented as a green run.

The native follow-up then also removed the inherited inexhaustible-character-space assumption. Its full XSD suite (413 unit tests plus integration/doctests), warnings-as-errors all-target clippy, three WASM cases and 17 native datatype tests passed on the corrected algorithm. The final GMEOW pin includes that fix. Historical paragraphs above describe their original stages; none replaces the final source selection recorded here.

Reuse assessment: all GMEOW Rust implementation code is unchanged after reviewed head 9d992189d. The final delta is the immutable upstream pin, lockfile source IDs and the two verified witness outputs. The previous Node execution proves the unchanged binding/portable identity mechanism; the updated upstream engine has its own actual WASM replay. It is not a claim that the final GMEOW browser binary was executed locally. The user explicitly authorized merge with subsequent CI findings tracked: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged." Final-pin browser/full-corpus CI remains pending, not passed. No implementation requirement or code path was cut.

Integration candidate is clean against unchanged origin/main 11c04b49d4fc4d1c594f70b2b7462d128c41f000, tree a95971591482cda75a9292613d736bb7cfd8e031. No base-side changes or conflicts invalidate the affected evidence.

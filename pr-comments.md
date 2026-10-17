# PR #1838 — comments and review threads

3 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1838?scope=[redacted]&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Bug Fixes**
  * Updated `xsd:anyURI` handling to use native datatype rules. XML-whitespace-normalized values compare as equal, while percent-encoded and non-breaking-space variants remain distinct.
  * Opaque `xsd:anyURI` values are no longer assumed to belong to the datatype or given a computed facet length. Their whitespace is preserved, and opaque values compare equal only when their RDF terms match.
  * Enumeration checks now reflect these rules; unrecognized foreign datatypes remain unknown.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The changes pin PurRDF to a new Git revision and add its source identity to the reasoning substrate identity. They update anyURI value handling and refresh the reason witness datasets and their documented refresh command.

**Changes: Reasoning substrate updates**

|Layer / File(s)|Summary|
|:---|:---|
|**Pin and embed substrate identity** <br> `Cargo.toml`, `fuzz/Cargo.toml`, `crates/build-inputs/src/*`, `crates/build-inputs/tests/native_substrate.rs`, `crates/logic/build.rs`, `crates/logic/src/reason/mod.rs`, `crates/logic/src/reason/mod.tests.rs`|The workspace and fuzz package pin PurRDF to a new Git revision. The build-inputs crate validates and exposes the source identity, the build script embeds it, and the reasoning substrate identity includes it.|
|**Update anyURI handling** <br> `crates/logic/src/reason/value.rs`, `crates/logic/src/reason/value.tests.rs`, `crates/logic/src/physical/seminaive/property/datatype/extent.rs`, `crates/logic/src/physical/seminaive/property/datatype.tests.rs`|anyURI values follow the shared datatype and facet logic. Parsing preserves their lexical input. Opaque anyURI values no longer receive special membership or equality handling, and tests update the expected membership results.|
|**Refresh reason witness records** <br> `crates/reason-wasm/tests/WITNESS.conjecture.nq`, `crates/reason-wasm/tests/WITNESS.reason.nq`, `crates/reason-wasm/tests/witness_conjecture.rs`|The witness datasets use updated identifiers, contract hashes, and Skolem identifiers. The test documentation and missing-attestation message use the updated refresh command.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal





**Estimated code review effort:** 3 (Moderate) | ~20 minutes

<!-- change_assessment_commit:"9d992189de1d06627d7e113756a796374ad08567" -->
**Change:** Refactor
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>9d992</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `9d992`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"9d992189de1d06627d7e113756a796374ad08567","coveredCommitId":"9d992189de1d06627d7e113756a796374ad08567","kind":"reviewed"} -->

No actionable merge-blocking issue is established by the reviewed changes; merge after the planned build and parity checks pass.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>
<!-- pre_merge_checks_walkthrough_start -->

<details>
<summary><strong>Pre-merge checks: <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> 5</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|     Docstring Coverage     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 93.75% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 16 functions across 10 files. (4 skipped: 4… |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise and accurately describes the main anyURI delegation change. It contains no agentic branding.                                                                                         |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description is mostly complete. It explains the change, motivation, validation results, scope, dependencies, and remaining CI work. It does not reproduce the template headings or checklist boxes, … |

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 93.75% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 16 functions across 10 files. (4 skipped: 4 unsupported.)

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description is mostly complete. It explains the change, motivation, validation results, scope, dependencies, and remaining CI work. It does not reproduce the template headings or checklist boxes, and it does not explicitly address every checklist item, but the required context is substantially provided.

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<!-- pre_merge_checks_walkthrough_end -->
<!-- finishing_touch_checkbox_start -->

<details>
<summary><strong>✨ Finishing Touches</strong></summary>
<dl>
<dd>

<details open>
<summary><strong>📝 Generate docstrings</strong></summary>
<dl>
<dd>

- [ ] <!-- {"checkboxId":"3e1879ae-f29b-4d0d-8e06-d12b7ba33d98"} --> Commit to this branch
- [ ] <!-- {"checkboxId":"7962f53c-55bc-4827-bfbf-6a18da830691"} --> Create a new PR

</dd>
</dl>
</details>
<details open>
<summary><strong>🧪 Generate unit tests (beta)</strong></summary>
<dl>
<dd>

- [ ] <!-- {"checkboxId": "6ba7b810-9dad-11d1-80b4-00c04fd430c8", "radioGroupId": "utg-output-choice-group-unknown_comment_id"} --> Commit to this branch
- [ ] <!-- {"checkboxId": "f47ac10b-58cc-4372-a567-0e02b2c3d479", "radioGroupId": "utg-output-choice-group-unknown_comment_id"} --> Create a new PR

</dd>
</dl>
</details>
<details open>
<summary><strong>✨ Simplify code</strong></summary>
<dl>
<dd>

- [ ] <!-- {"checkboxId": "9a4e3077-58f6-4eba-b7ee-62e936ea00ea", "radioGroupId": "simplify-output-choice-group-unknown_comment_id"} --> Commit to this branch
- [ ] <!-- {"checkboxId": "f120d606-b0e2-4b7d-8316-181794555b43", "radioGroupId": "simplify-output-choice-group-unknown_comment_id"} --> Create a new PR

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<!-- finishing_touch_checkbox_end -->

<!-- autopilot:start -->
- [ ] <!-- {"checkboxId":"2708ad07-9f24-4260-9c11-7dc76a49f2e3"} --> <strong title="Keep fixing CodeRabbit findings and required CI, and resolving merge conflicts">Autofix</strong> · Keep fixing CodeRabbit findings and required CI, and resolving merge conflicts
<!-- autopilot:end -->
<!-- tips_start -->






<!-- poem_footer_start -->
<sub>A rabbit checks the pinned source line,  <br>Then hops through URI space in time.  <br>New hashes settle in their place,  <br>Fresh witnesses mark the trace,  <br>And carrots crown the build just fine.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 2. paudley — 

refactor(logic): delegate anyURI semantics to the native substrate

Advances #1803. The broader reasoning ownership migration remains open.

Principles 4 and 17: native datatype mechanisms have one owner. PurRDF now supplies anyURI parsing, XML whitespace collapse, primitive identity, length facets and finite-space reasoning. Delete the GMEOW named-membership, opaque-equality, normalization, primitive-recognition and capacity special cases. Delegate string/binary/URI length measurement to the native XsdValue method as well. Preserve source lexical forms and non-XML Unicode whitespace before native anyURI interpretation; known distinct URI values now produce proved inequality while foreign datatypes stay unknown. No deprecated alias, old implementation or fallback remains for this cutover.

Bind the immutable upstream Git URL and full revision into the portable semantic identity alongside calculus/rule hashes. Datatype implementation changes can leave rule syntax unchanged. The build reads the declared source structurally and fails closed on a missing/mutable pin, without host discovery. Operation limits remain in their receipts and are not changed or folded into the portable semantic identity. Both manifests and lockfiles select corrected native head 2845023147548b7bc6a6264149d4239e2caf99ed, already merged in PurRDF PR 574. The two tiny attestations are regenerated through their one verified producer; stale references to the retired conjecture-only producer are removed.

Validation: all 19 final native synthetic tests passed on the corrected pin, including production datatype consumers, immutable-source selection, portable identity checks and both read-only witnesses. The explicit producer verified seven closure facts and corroborated/refuted-in-standpoint conjectures. Affected-package all-target clippy passed with warnings denied. Both normal configured commit hooks passed and both commits were pushed.

Four actual optimized Node parity cases passed on implementation head 9d992189d before the final upstream pin update. All GMEOW Rust implementation is unchanged after that head. The final upstream correction separately passed its complete XSD suite, exact finite-exhaustion regression, all-target clippy, three actual WASM cases and 17 native datatype tests. The old Node execution is reused only for the unchanged binding/identity mechanism; a final-pin browser binary was not executed locally. Full final-pin browser and corpus CI remain pending. The user explicitly authorized merge with later CI findings tracked: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged." No code requirement was cut; pending hosted qualification is not counted as a pass. No local full corpus producer or machine/resource probes ran.

CodeRabbit reviewed 9d992189d with no actionable merge-blocking finding and minimal reported risk; documentation coverage was 93.75% over 16 functions in 10 files with four unsupported skips. It explicitly rate-limited the final data/pin delta review. That delta contains only source IDs and the two verified attestations. The newly pinned native correction was independently reviewed through 284502314 without actionable comments. Actual coverage and limits are preserved in review-debt.md; no unavailable independent audit is fabricated.

Integration is clean against unchanged origin/main 11c04b49d4fc4d1c594f70b2b7462d128c41f000; candidate tree a95971591482cda75a9292613d736bb7cfd8e031. There are no conflicts or base-side changes. Native implementation/correction evidence is archived at refs/ghprsq/pr/573/stage and refs/ghprsq/pr/574/stage. Source ownership, other generic reasoning seams and full-corpus acceptance remain tracked by open issue 1803; this merge does not close that larger contract.

Authoritative plan and selected evidence: /home/paudley/Active/gmeow-ontology/.worktrees/1803-split-gmeow-purrdf-responsibilities/.stage/split-gmeow-purrdf-responsibilities

Defect-Class: none

## 3. paudley — 

Review disposition: keeping any_uri_enumeration_compares_collapsed_values. The two loops test one equivalence relation: XML whitespace aliases belong to the enumeration, while percent escapes and NBSP do not become equivalent through that normalization. The existing name describes both the positive and negative controls. The suggested broader rename was explicitly optional and unverified; no behavior or coverage is omitted by retaining the precise existing name.

The refresher concern is refuted: the single explicit producer writes and verifies both native attestations, and both final read-only witness cases passed. Remaining additional review entries are LGTM. The corrected-pin native selection passed all 19 tests. Full hosted qualification remains pending under the existing merge instruction.


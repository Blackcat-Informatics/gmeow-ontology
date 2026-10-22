# PR #1954 — comments and review threads

2 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 



<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1954?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Improvements**
  * Standardized JSON handling for build identity and validation observation data, with required fields and invalid values checked during decoding.
  * Validation reports now preserve result ordering, and violation messages are consistently extracted from matching results.
  * Malformed observation data is reported through diagnostic errors.

* **Documentation**
  * Added guidance on JSON data formats and decoding behavior used in expression and inference validation.

* **Tests**
  * Expanded coverage for serialization, round trips, optional fields, ordering, and invalid or ambiguous JSON input.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The change adds JSON codecs for `BuildIdentity` and conformance observation records. Selected pipeline stages and test readers use the native record codec or a caller-provided decoder.

**Changes: Native JSON transport**

|Layer / File(s)|Summary|
|:---|:---|
|**Shared inference observations and transport** <br> `crates/validate/src/inference_observation*`, `crates/validate/src/lib.rs`, `crates/pipeline/src/stages/conformance/inference_validation.rs`, `crates/validate/tests/source_observations/*`, `crates/validate/tests/gts_medium_axis.rs`|The validate crate exposes shared observation, control, and result types with JSON codecs. The inference-validation stage uses those types and the native record codec. The source-observation reader accepts a decoder callback, and related tests use the updated decoding paths.|
|**Expression-substrate JSON records** <br> `crates/pipeline/src/stages/conformance/expression_substrate*`, `crates/pipeline/src/stages/conformance/corpus_tests/expression_substrate.rs`, `crates/pipeline/src/stages/conformance/EXPRESSION-SUBSTRATE-JSON.md`|The conformance stage adds explicit codecs for `Profile`, `Observations`, `Scene`, and `Reasoned`. It uses the native record codec for observation output and corpus input. Tests and documentation cover field validation, optional values, and JSON encoding rules.|
|**Build identity JSON codec** <br> `crates/pipeline/src/cache.rs`, `crates/pipeline/src/cache/build_identity_json*`|`BuildIdentity` gains JSON serialization and deserialization for its five metadata fields. Tests cover round trips, unknown fields, and rejected input.|

<!-- change_assessment_start -->
**Priority:** ⬇️ Low



**Estimated code review effort:** 3 (Moderate) | ~25 minutes

<!-- change_assessment_commit:"af0cdc7d2e41dc3508602a3e72c633ff1df687dd" -->
**Change:** Refactor
<!-- change_assessment_end -->

**Possibly related issues**
- Issue 1776 requests a broader migration from Serde and `serde_json` to `purrdf_lex::json`. This change migrates selected cache, pipeline, and reader paths but retains other Serde uses.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>🟡 Moderate</strong> · up to <code>af0cd</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **🟡 Moderate** · up to `af0cd`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"af0cdc7d2e41dc3508602a3e72c633ff1df687dd","coveredCommitId":"af0cdc7d2e41dc3508602a3e72c633ff1df687dd","kind":"reviewed"} -->

The wasm build may fail until the native JSON module is gated. Fix that supported build path before merging.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>
<!-- pre_merge_checks_walkthrough_start -->



<!-- pre_merge_checks_override_start -->
> [!IMPORTANT]
> ## Pre-merge checks failed
> 
> Please resolve all errors before merging. Addressing warnings is optional.
<!-- pre_merge_checks_override_end -->

<strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" width="14" height="14" align="middle" alt="Failed" title="Failed"></picture></sup> Failed checks: 1 error</strong>

|     Check name     |                                            Status                                            | Explanation                                                                                                                                                                                               | Resolution                                                                         |
| :---------------- | :------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------- |
| Docstring Coverage | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | Docstring coverage is 37.14% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 35 functions across 16 files. (1 skipped:… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise and accurately describes the native JSON transport refactor. It contains no agentic branding.                                                                                        |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description provides a clear summary, rationale, validation results, reviewer context, open limitations, and issue reference. It does not use the template headings or explicitly mark each checklis… |

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description provides a clear summary, rationale, validation results, reviewer context, open limitations, and issue reference. It does not use the template headings or explicitly mark each checklist item, but the required information is mostly covered in prose.

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 37.14% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 35 functions across 16 files. (1 skipped: 1 unsupported.)

**Resolution**

Write docstrings for the functions missing them to satisfy the coverage threshold.

</dd>
</dl>
</details>

<!-- pre_merge_checks_walkthrough_end -->
<!-- finishing_touch_checkbox_start -->

<details>
<summary><strong>✨ Finishing Touches: 💡 1</strong></summary>
<dl>
<dd>

<!-- finishing_touch_suggestion:docstrings -->
<details open>
<summary><strong>📝 Generate docstrings: 💡</strong></summary>
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
<sub>A rabbit checks each record's shape,<br>Then follows fields through JSON’s gate.<br>Maps stay sorted, results in line,<br>Escaped words return just fine.<br>With codecs set and errors clear,<br>The rabbit hops along, ears near.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 2. paudley — 

Corrected the grounding reader to use the producer-owned typed RDF confidence literal, preserving the original decimal spelling in presentation. Both new literal controls passed; the existing corpus assertions are unchanged.

The portability finding is also fixed: only the inference JSON submodule is gated to match the native-only dependency. Shared records and the pure violations view remain portable. An independent source review verified the manifest boundary and actual callers. This establishes the source repair; wasm compilation remains unrun.

Ten transport controls and two confidence controls passed within their stated scopes. Refreshed strict compilation, helper/source checks, lint and normal commit/push hooks passed. The recorded campaign exception still excludes local corpus/full-gate runs and permits sequential merges ignoring CI. Actual authenticated corpus, wasm, excluded embedded/installed behavior and full dependency removal remain open. Principles 4 and 7; Refs #1776.


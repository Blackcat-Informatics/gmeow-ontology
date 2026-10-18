# PR #1849 — comments and review threads

2 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1849?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->

> [!WARNING]
> ## Review limit reached
> 
> Your organization has reached its usage spending cap. Adjust your spending cap in the [billing tab](https://app.coderabbit.ai/settings/billing?tab=usage&orgId=b268aba6-a194-4fd6-8ca5-f1a13a313209).
> 
> **Next included review available in 7 minutes.**
> 
> [Check out review usage here](https://app.coderabbit.ai/dashboard/review-capacity?orgId=b268aba6-a194-4fd6-8ca5-f1a13a313209).
> 
> <details>
> <summary><strong>View limit details</strong></summary>
> <dl>
> <dd>
> 
> **Limit details:** You’ve used the included review currently available. Your 96 included PR review attempts over the past 7 days set your current allowance at 1 review per hour.
> 
> [Learn how review limits work](https://docs.coderabbit.ai/management/rate-limits).
> 
> **Review configuration:**
> 
> <details>
> <summary><strong>⚙️ Run configuration</strong></summary>
> <dl>
> <dd>
> 
> - **Configuration used**: Repository UI
> - **Review profile**: ASSERTIVE
> - **Plan**: Team
> - **Run ID**: `99028148-f17f-415a-9dc4-3b8192f2a9a3`
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>📥 Commits</strong></summary>
> <dl>
> <dd>
> 
> Reviewing files that changed from the base of the PR and between eabb0f74c10c05051a66e4900fc55f2c2bd2fc29 and 2922b9313880396da99a7c20c0e73fa90feea67c.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>📒 Files selected for processing: 3</strong></summary>
> <dl>
> <dd>
> 
> * `crates/pipeline/src/scoreboards.rs`
> * `crates/pipeline/src/scoreboards.tests.rs`
> * `docs/validation-thresholds.md`
> 
> </dd>
> </dl>
> </details>
> 
> </dd>
> </dl>
> </details>

<!-- end of auto-generated comment: rate limited by coderabbit.ai -->

<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Documentation**
  * Clarified which validation thresholds apply to local checks and required CI lanes, including the 64.5% transpile recall floor and its 64.57% baseline.
  * Updated guidance on where acceptance checks and external-corpus sweeps run, and clarified that omitting a recall threshold does not disable the gate.
  * Corrected the documented source of truth for the enforced recall floor.
* **Tests**
  * Updated the expected count of default test exclusions and refined test selection for the conformance override.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The nextest filters and their contract test now reflect updated test selections. The validation guide describes a 64.5% acceptance recall floor and assigns acceptance to the required CI breadth lane.

**Changes: Gate and Validation Updates**

|Layer / File(s)|Summary|
|:---|:---|
|**Nextest filter selections** <br> `.config/nextest.toml`, `crates/gmeow-dev-cli/tests/nextest_override_contract.rs`|The default filter removes the `cli_parity::project_view_over_the_snapshot` exclusion. The maint-heavy profile no longer excludes `measurement_is_deterministic_across_two_runs`, and the conformance override matches `accepts_an_independent_user_corpus`. The test contract now requires 14 exclusion clauses.|
|**Acceptance threshold and lane documentation** <br> `crates/pipeline/src/scoreboards.rs`, `docs/validation-thresholds.md`|The recall-floor comment identifies Rust as the source of truth. The guide documents the 64.5% default floor, acceptance behavior when `--min-recall` is omitted, and acceptance through `make heavy` in the required CI breadth lane.|

<!-- change_assessment_start -->
**Priority:** ⬇️ Low









**Estimated code review effort:** 2 (Simple) | ~10 minutes

<!-- change_assessment_commit:"eabb0f74c10c05051a66e4900fc55f2c2bd2fc29" -->
**Change:** Other
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>🔵 Low</strong> · up to <code>eabb0</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **🔵 Low** · up to `eabb0`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"eabb0f74c10c05051a66e4900fc55f2c2bd2fc29","coveredCommitId":"eabb0f74c10c05051a66e4900fc55f2c2bd2fc29","kind":"reviewed"} -->

This change updates test lane filters and documents the 64.5% recall floor. No runtime behavior changes. Two documentation statements are inaccurate. The floor is the default and can be lowered by an explicit override. CI runs acceptance as its own job branch rather than through the aggregate `make heavy` target. Correcting this wording is low-effort and the change is otherwise safe to merge.
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

|      Check name     |                                            Status                                            | Explanation                                                                                                                                                                                               | Resolution                                                                                                                                                                                                                                        |
| :----------------- | :------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Linked Issues check | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | `#1644`’s recall-floor objective is met: `docs/validation-thresholds.md` documents the 64.5% floor, 64.57% recorded baseline, intentional 0.07 percentage-point margin, and required CI breadth lane. The … | Restore the named `maint-heavy` exclusion and rationale for `measurement_is_deterministic_across_two_runs`, unless code evidence establishes that it no longer rebuilds the corpus. Also schedule `every_real_slice_batch_matches_parallel_scrat… |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The nextest selector changes, lane-contract test update, and recall-floor documentation all relate to `#1644`’s lane-placement and recall-margin objectives. The diff shows no unrelated changes.           |
|     Docstring Coverage     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 100.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 1 functions across 2 files. (2 skipped: 2 … |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise, accurate, and describes the test-lane repairs and acceptance-recall change. It contains no agentic branding.                                                                        |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description clearly explains what changed, why it changed, validation performed, known limitations, and linked issue context. It does not use the template headings or checkbox format, and it does … |

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 100.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 1 functions across 2 files. (2 skipped: 2 unsupported.)

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description clearly explains what changed, why it changed, validation performed, known limitations, and linked issue context. It does not use the template headings or checkbox format, and it does not explicitly confirm every checklist item, but the required information is mostly present.

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Linked Issues check</strong></summary>
<dl>
<dd>

**Explanation**

`#1644`’s recall-floor objective is met: `docs/validation-thresholds.md` documents the 64.5% floor, 64.57% recorded baseline, intentional 0.07 percentage-point margin, and required CI breadth lane. The nextest changes also address stale selectors, and the remaining default exclusions retain numbered justifications and lane ownership. However, the diff removes the `maint-heavy` exclusion and rationale for `measurement_is_deterministic_across_two_runs`, while `#1644` says this test runs the production DAG and must stay unscheduled. The diff does not establish that the test’s behavior changed. The diff also does not change `every_real_slice_batch_matches_parallel_scratch`; `#1644` identifies it as ignored and absent from every lane. This leaves the issue’s no-test-excluded-from-every-lane requirement unmet.

**Resolution**

Restore the named `maint-heavy` exclusion and rationale for `measurement_is_deterministic_across_two_runs`, unless code evidence establishes that it no longer rebuilds the corpus. Also schedule `every_real_slice_batch_matches_parallel_scratch` in a lane, with ignored-test handling that makes the lane run it.

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
<sub>A rabbit checks the gates at dawn,<br>Then counts one fewer clause upon.<br>The recall floor is clearly shown,<br>The CI lane has claimed its own.<br>It hops away through fields of green.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 2. paudley — 

All three CodeRabbit findings are repaired. The constant and documentation now distinguish the default floor from explicit development overrides, and name the actual CI acceptance matrix branch.

The outside-diff correctness finding was valid: an absent addressable denominator returned 100%. The aggregate gate now rejects an unmeasured corpus even with a zero floor, and its structured corpus verdict agrees. A regression drives the real per-file round-trip gate over an OWL-linkage-only dataset, then checks both aggregate verdicts at the default and zero floors.

Nine focused native contracts passed. Strict producer-independent all-target Clippy passed with warnings denied, as did all applicable normal commit hooks. Fixes are committed and pushed. Full hosted pipeline qualification is still running; no local corpus producer or current-corpus recall measurement was invoked. Principles 4, 7 and 18 apply.


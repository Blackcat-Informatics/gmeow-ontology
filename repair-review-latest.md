<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1830?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Reliability**
  * Goal-directed workflow results now persist between runs, helping preserve validated observations.
  * Lossy correspondences are checked against their declared recovery guarantees: mappings that lose source detail are accepted as lossy, but not as exact recoveries.
* **Validation**
  * Added checks that authenticated pipeline stages use stable identifiers and persistent caching.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The changes add explicit legs and recovery checks for a lossy correspondence. They also change the goal-directed stage’s cache policy to persistent and add a fixture test for stage stability and cache policy.

**Changes: Correspondence Recovery**

|Layer / File(s)|Summary|
|:---|:---|
|**Recovery legs and gate validation** <br> `imports/supersession-correspondences.ttl`, `crates/logic/src/correspondence_exec.tests.rs`|The writing-system split correspondence declares get and candidate put legs. A test checks that missing executable legs fail, lossy recovery remains a violated section obligation, and gates reject a `SectionRetraction`/`mnemomorphic` overclaim.|

**Goal-Directed Stage Persistence**

|Layer / File(s)|Summary|
|:---|:---|
|**Persistent admission and fixture contract** <br> `crates/pipeline/src/stages/goal_directed.rs`, `slices/core/pipeline/module.ttl`, `crates/pipeline/src/fixture.tests.rs`|The goal-directed stage changes from `Recompute` to `Persistent`, and its implementation version changes. The pipeline declaration uses the persistent-contribution cache disposition. A test checks that selected fixture stages use `StablePrefix` stability and `Persistent` cache policy.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 3 (Moderate) | ~20 minutes

<!-- change_assessment_commit:"ff3cfe1c680bb5b15c27c68acc5925028ec840ef" -->
**Change:** Bug fix · **Severity of issue fixed:** Medium
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>🔵 Low</strong> · up to <code>ff3cf</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **🔵 Low** · up to `ff3cf`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"ff3cfe1c680bb5b15c27c68acc5925028ec840ef","coveredCommitId":"ff3cfe1c680bb5b15c27c68acc5925028ec840ef","kind":"reviewed"} -->

The change appears mergeable with a bounded test-coverage gap: add a recoverable control to confirm the new legs are exercised before relying on this regression test.
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

<strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" width="14" height="14" align="middle" alt="Failed" title="Failed"></picture></sup> Failed checks: 2 errors</strong>

|      Check name     |                                            Status                                            | Explanation                                                                                                                                                                                               | Resolution                                                                                                                                                                                                                        |
| :----------------- | :------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linked Issues check | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | The implementation addresses the requested repairs. `GoalDirectedStage` uses `CachePolicy::Persistent`, has the new implementation identity, and keeps proof-checked production. The fixture regression … | Run the hosted fixture-prefix producer and downstream consumers for `#1829`. Run both hosted generation lanes and dependent validation for `#1828`. Provide passing results with the exact admitted identities and recovery evidence. |
|  Docstring Coverage | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | Docstring coverage is 45.45% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 11 functions across 3 files. (2 skipped: … | Write docstrings for the functions missing them to satisfy the coverage threshold.                                                                                                                                                |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 3 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise and accurately describes the pipeline fixture and lossy recovery changes. It contains no agentic branding.                                                                           |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description provides a clear summary, rationale, validation results, scope, limitations, and linked issues. It does not use the template's explicit Summary, Checklist, and Notes headings, and it d… |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The changed stage policy, implementation identity, canonical pipeline metadata, correspondence legs, recovery refutation, and focused tests all implement objectives in `#1829` or `#1828`. No unrelated cha… |

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description provides a clear summary, rationale, validation results, scope, limitations, and linked issues. It does not use the template's explicit Summary, Checklist, and Notes headings, and it does not list each validation checkbox separately, but the required information is mostly present.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Out of Scope Changes check</strong></summary>
<dl>
<dd>

**Explanation**

The changed stage policy, implementation identity, canonical pipeline metadata, correspondence legs, recovery refutation, and focused tests all implement objectives in `#1829` or `#1828`. No unrelated change is established by the available PR evidence.

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

The implementation addresses the requested repairs. `GoalDirectedStage` uses `CachePolicy::Persistent`, has the new implementation identity, and keeps proof-checked production. The fixture regression checks `StablePrefix` and `Persistent` for every selected stage. The Writing System correspondence has explicit get and candidate put paths. Its `LossyLens` declaration and executable refutation remain present. The focused regression covers missing legs and false full-recovery claims. However, `#1829` still requires successful fixture-prefix production and downstream consumers with exact identities. `#1828` still requires both generation lanes and dependent validation to pass. The PR explicitly reports that these hosted qualifications remain undone.

**Resolution**

Run the hosted fixture-prefix producer and downstream consumers for `#1829`. Run both hosted generation lanes and dependent validation for `#1828`. Provide passing results with the exact admitted identities and recovery evidence.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 45.45% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 11 functions across 3 files. (2 skipped: 2 unsupported.)

**Resolution**

Write docstrings for the functions missing them to satisfy the coverage threshold.

</dd>
</dl>
</details>

<!-- pre_merge_checks_walkthrough_end -->

- [ ] <!-- {"checkboxId":"585bb3f6-faf5-4dbf-96d2-74e382adf19a"} --> Fix all pre-merge checks with AI
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
<sub>A rabbit checks the mapping legs,<br>Then tests the gate where detail begs.<br>The goal-directed stage can stay,<br>Its graph and observation saved this way.<br>I nibble clover, pleased with proof,<br>And thump beneath the cached roof.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

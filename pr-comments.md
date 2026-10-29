# PR review evidence

Source payload: review-evidence.json
Observed items: 1; PR head: 3505bc7ad593d4bc47dab67e6fc220a4fc8a208d.
Every item needs an explicit disposition; bot comments count.
Dispositions apply to the recorded item revision; edited feedback is reconsidered.

Coverage:
- comments: complete
- reviews: complete
- inline_comments: complete
- threads: complete

## github:comment:IC_kwDOSwdn1s8AAAABgvff2w — coderabbitai (comment)
https://github.com/Blackcat-Informatics/gmeow-ontology/pull/2040#issuecomment-6492250075
state=None resolved=None path=None




<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/2040?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->


> [!WARNING]
> ## Review limit reached
> 
> Your organization has reached its usage spending cap. Adjust your spending cap in the [billing tab](https://app.coderabbit.ai/settings/billing?tab=usage&orgId=b268aba6-a194-4fd6-8ca5-f1a13a313209).
> 
> **Next included review available in 31 minutes.**
> 
> [Check out review usage here](https://app.coderabbit.ai/dashboard/review-capacity?orgId=b268aba6-a194-4fd6-8ca5-f1a13a313209).
> 
> <details>
> <summary><strong>View limit details</strong></summary>
> <dl>
> <dd>
> 
> **Limit details:** You’ve used the included review currently available. Your 131 included PR review attempts over the past 7 days set your current allowance at 1 review per hour.
> 
> [Learn how review limits work](https://docs.coderabbit.ai/management/rate-limits).
> 
> **Review configuration:**
> 
> <details>
> <summary><strong><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/cog-dark.svg"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/cog.svg" width="16" height="16" align="absmiddle" alt=""></picture> Run configuration</strong></summary>
> <dl>
> <dd>
> 
> - **Source mode**: Auto
> - **Repository**: Unavailable
> - **Branch**: Unavailable
> - **File**: No configuration file from this repository applied
> - **Commit**: Unavailable
> - **Layers**: Repository UI (base), Organization UI (inherited)
> - **Review profile**: ASSERTIVE
> - **Plan**: Advanced
> - **Run ID**: `d8805e5c-a93b-4c5d-94d3-f218a89566dc`
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/git-commit-horizontal-dark.svg"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/git-commit-horizontal.svg" width="16" height="16" align="absmiddle" alt=""></picture> Commits</strong></summary>
> <dl>
> <dd>
> 
> Reviewing files that changed from the base of the PR and between 48abcf4b6dcb67ba0fd4e6c1aa004dcc44be7fe1 and 3505bc7ad593d4bc47dab67e6fc220a4fc8a208d.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/file-plus-corner-dark.svg"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/file-plus-corner.svg" width="16" height="16" align="absmiddle" alt=""></picture> Files selected for processing: 1</strong></summary>
> <dl>
> <dd>
> 
> * `crates/gmeow-dev-cli/src/dev_shapes.tests.rs`
> 
> </dd>
> </dl>
> </details>
> 
> </dd>
> </dl>
> </details>



<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>



## Summary by CodeRabbit

* **Bug Fixes**
  * Raw SPARQL report extraction now accounts for shape ownership and deactivation metadata, and avoids producing results for unsupported projected forms.
* **Tests**
  * Expanded coverage for extraction involving property bodies, conditional deactivation, matching deactivation states, and differences in native metadata.


`raw_sparql_block_key` now resolves a shape’s native owner definition and uses its data to build the key. It rejects unsupported property bodies and conditional deactivation forms. Tests cover those rejections and matching unconditional shape deactivation.

**Changes: Native-definition SPARQL identity**

|Layer / File(s)|Summary|
|:---|:---|
|**Resolve and validate definition-based block keys** <br> `crates/gmeow-dev-cli/src/dev_shapes/query_identity.rs`, `crates/gmeow-dev-cli/src/dev_shapes.tests.rs`|The key reads constraints, annotations, severity, messages, and box roles from the native definition. It rejects unsupported forms and combines shape- and definition-level unconditional deactivation. Tests cover unsupported forms and matching shape deactivation.|

<!-- change_assessment_start -->
<strong>Priority: <picture><img src="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/priority-normal-16.svg" width="16" height="16" align="absmiddle" alt=""></picture> Normal</strong>



**Estimated code review effort:** 2 (Simple) | ~10 minutes

<!-- change_assessment_commit:"48abcf4b6dcb67ba0fd4e6c1aa004dcc44be7fe1" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**

- `Blackcat-Informatics/gmeow-ontology#2039` — This change resolves native shape roots through their owned definitions for SPARQL equivalence clearance. It does not cover the issue’s math result identity encoding.

**Suggested reviewers:** `erinaudley`

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/risk-unassessed-16.svg" width="16" height="16" align="absmiddle" alt=""></picture> Minimal</strong> · previous review <code>48abc</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `48abc`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"48abcf4b6dcb67ba0fd4e6c1aa004dcc44be7fe1","coveredCommitId":"48abcf4b6dcb67ba0fd4e6c1aa004dcc44be7fe1","kind":"reviewed"} -->

Native-definition SPARQL matching has no verified blocking defect. The full PR diff could not be checked because its comparison base was unavailable.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/risk-low-16.svg" width="16" height="16" align="absmiddle" alt=""></picture> Low</strong> · up to <code>48abc</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🔵 Low** · up to `48abc`
-->

The reviewed path retains exact matching and conservative rejection of unsupported cases. No introduced security defect was established. However, the complete before-and-after comparison and end-to-end removal behavior were not verified.

**Retained concerns**
No architecture-level concerns identified.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _inferred_ — The security-relevant outcome is acceptance of dataset-provided replacement evidence for legacy validation obligations. Through pruning, that decision can affect validation blocks in files processed by the CLI; the scoped source does not establish a new network or cross-tenant entrypoint.

**Trust Boundaries and Controls**
- _observed_ — Missing ownership or definition resolution fails explicitly. Unsupported targets, rules, property bodies, conditional deactivation and non-SPARQL constraints cannot produce a comparable key. Eligible keys retain query identity, effective severity and messages, result annotations, deactivation and box roles rather than comparing query text alone.

**Resilience and Maintainability Implications**
- _observed_ — Migration requires apply mode to write and deduplicates exact appended statements. Pruning writes per file, continues after reported failures and returns a nonzero result on errors. The inspected path provides no transaction-wide rollback or writer serialization; these adjacent limitations were not established as introduced or worsened by this change.

</dd>
</dl>
</details>
<!-- architecture_review_end -->

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
| Docstring Coverage | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | Docstring coverage is 33.33% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff with cyclomatic complexity of at least 2. Documenta… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise, accurate, and describes the main change. It contains no agentic branding.                                                                                                           |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description explains what changed and why, records validation limits and test status, identifies related issues, and provides reviewer context. It does not use the template headings or explicitly … |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description explains what changed and why, records validation limits and test status, identifies related issues, and provides reviewer context. It does not use the template headings or explicitly confirm every checklist item, but it is mostly complete and relevant.

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

Docstring coverage is 33.33% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff with cyclomatic complexity of at least 2. Documentation-only edits and deletions within surviving functions are included; unchanged diff context and fully deleted functions are excluded. Analyzed 3 functions across 2 files.

**Resolution**

Write docstrings for the functions missing them to satisfy the coverage threshold.

</dd>
</dl>
</details>

<!-- pre_merge_checks_walkthrough_end -->
<!-- finishing_touch_checkbox_start -->

<details>
<summary><strong>Finishing Touches: 💡 1</strong></summary>
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
<sub>A rabbit checks the shape’s own root,<br>Then follows definitions to the route.<br>Unsupported forms are left aside,<br>Matching deactivation keys coincide.<br>The SPARQL blocks hop into view.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->


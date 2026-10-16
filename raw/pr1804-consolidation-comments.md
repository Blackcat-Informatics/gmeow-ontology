# PR #1804: fix(slices): repair the corpus inconsistencies native reasoning exposed

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Comments (1)

### coderabbitai — 2026-10-05T17:45:52Z

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1804?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=2" alt="Review in Change Stack →" width="220" height="32"></a>

Navigate logical layers of code changes, visualize relationships, and explore their blast radius.

<!-- review_stack_entry_end -->
<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->

> [!WARNING]
> ## Review limit reached
> 
> This review is too large to run within your organization's remaining usage spending cap. Raise or remove your spending cap in the [billing tab](https://app.coderabbit.ai/settings/billing?tab=usage&orgId=b268aba6-a194-4fd6-8ca5-f1a13a313209), then retry.
> 
> **Next included review available in 41 minutes.**
> 
> [Check out review usage here](https://app.coderabbit.ai/dashboard/review-capacity?orgId=b268aba6-a194-4fd6-8ca5-f1a13a313209).
> 
> <details>
> <summary>View limit details</summary>
> 
> **Limit details:** You’ve used the included review currently available. Your 93 included PR review attempts over the past 7 days set your current allowance at 1 review per hour.
> 
> [Learn how review limits work](https://docs.coderabbit.ai/management/plans#rate-limits).
> 
> **Review configuration:**
> 
> <details>
> <summary>⚙️ Run configuration</summary>
> 
> - **Configuration used**: Repository UI
> - **Review profile**: ASSERTIVE
> - **Plan**: Team
> - **Run ID**: `cfc7b70a-ee42-47c0-9a38-b1fb0369818f`
> 
> </details>
> 
> <details>
> <summary>📥 Commits</summary>
> 
> Reviewing files that changed from the base of the PR and between f06f59e5c5b088b064344f11ca8e453a88c7c352 and a794c70b3cba86870c41a302815fe92c7a4e09c7.
> 
> </details>
> 
> <details>
> <summary>📒 Files selected for processing (8)</summary>
> 
> * `slices/core/affect/module.ttl`
> * `slices/core/agentic/module.ttl`
> * `slices/core/calendar/module.ttl`
> * `slices/core/gts/module.ttl`
> * `slices/core/norms/module.ttl`
> * `slices/extensions/images/module.ttl`
> * `slices/grounding/lang/module.ttl`
> * `slices/grounding/math/module.ttl`
> 
> </details>
> 
> </details>

<!-- end of auto-generated comment: rate limited by coderabbit.ai -->

<!-- recent_review_start -->

No actionable comments were generated in the recent review. 🎉

<details>
<summary>ℹ️ Recent review info</summary>

<details>
<summary>⚙️ Run configuration</summary>

- **Configuration used**: Repository UI
- **Review profile**: ASSERTIVE
- **Plan**: Team
- **Run ID**: `9f76f9a2-38bb-4cb4-8aa9-250058f8835b`

</details>

<details>
<summary>📥 Commits</summary>

Reviewing files that changed from the base of the PR and between a631f73e6e39b4493d295bcd83b4deec80637f54 and f06f59e5c5b088b064344f11ca8e453a88c7c352.

</details>

<details>
<summary>📒 Files selected for processing (2)</summary>

* `slices/grounding/logic/module.ttl`
* `slices/vocabulary.ttl`

</details>

<details>
<summary>🔗 Linked repositories identified</summary>

CodeRabbit considers these linked repositories for cross-repo context during reviews:

- [`Blackcat-Informatics/purrdf`](https://github.com/Blackcat-Informatics/purrdf) _(auto-detected)_
- [`Blackcat-Informatics/gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) _(auto-detected)_

</details>

**Included review availability:** This review used your included allowance. 0 included reviews remain after this review. Your included PR review attempts over the past 7 days set your current allowance at 1 review per hour.

</details>

<details>
<summary>📜 Recent review details</summary>

<details>
<summary>⏰ Context from checks skipped due to timeout. (3)</summary>

* GitHub Check: producer-build
* GitHub Check: rust-prebuild
* GitHub Check: lint

</details>

<details>
<summary>🧰 Additional context used</summary>

<details>
<summary>📓 Path-based instructions (3)</summary>

<details>
<summary>Source excerpt: So GMEOW does not author shapes on the SHACL or ShEx surface.</summary>


**📄 CodeRabbit inference engine (slices/grounding/logic/design/LOGIC-VALIDATION.md)**

**Files:**
- `slices/vocabulary.ttl`
- `slices/grounding/logic/module.ttl`

</details>
<details>
<summary>Source excerpt: Author the backing axiom in the **owning slice's `module.ttl`**; `derive_validation_shapes` (`crates/logic-compile/src/frontend.rs`) reproduces it into `validation-shapes.ttl`.</summary>


**📄 CodeRabbit inference engine (docs/MIGRATING-SHAPES-TO-LOGIC.md)**

**Files:**
- `slices/grounding/logic/module.ttl`

</details>
<details>
<summary>Refer to the project-root `AGENTS.md` for available `make` targets and developer commands, such as `make check` and `gmeow-dev sync`.</summary>


**📄 CodeRabbit inference engine (.github/copilot-instructions.md)**

**Files:**
- `slices/vocabulary.ttl`
- `slices/grounding/logic/module.ttl`

</details>

</details><details>
<summary>🔀 Multi-repo context Blackcat-Informatics/purrdf, Blackcat-Informatics/gmeow-gts</summary>

## Linked repositories findings

### Blackcat-Informatics/purrdf

- No references to `logic:message` or `GroundingCorrespondence` appeared in the search. PurRDF does explicitly admit `x-gmeow-english` as a language tag, including through its RDF literal intern path (`crates/rdf-core/src/model.rs:469`; `crates/rdf-core/tests/literal_shapes.rs:76`). This is relevant RDF-term context, but does not indicate a consumer of the changed constraints. [::Blackcat-Informatics/purrdf::]

### Blackcat-Informatics/gmeow-gts

- No references to the changed vocabulary appeared. GTS’s language-tag discipline vector retains `x-gmeow-english` on a canonical payload literal, and its test verifies that the tag survives reading (`python/src/gts/vectors.py:481-501`; `python/tests/test_gts.py:664-674`). This shows generic GTS transport preserves language-tagged literals; it does not identify a consumer of these constraint messages. [::Blackcat-Informatics/gmeow-gts::]

</details>

</details>

<details>
<summary>🔇 Additional comments (2)</summary><blockquote>

<details>
<summary>slices/grounding/logic/module.ttl (1)</summary><blockquote>

`7319-7319`: LGTM!



Also applies to: 7330-7330, 7341-7341, 7352-7352, 7363-7363, 7374-7374, 23492-23492, 23509-23509, 24062-24062, 24079-24079

<!-- cr-comment:v1:be1d531aff5ea7769c507c52 -->

</blockquote></details>
<details>
<summary>slices/vocabulary.ttl (1)</summary><blockquote>

`248-248`: LGTM!



Also applies to: 259-259, 270-270, 279-279, 300-300, 326-326, 335-335

<!-- cr-comment:v1:143d07d75412f5474cddda00 -->

</blockquote></details>

</blockquote></details>

</details>

---



<!-- recent_review_end -->
<!-- walkthrough_start -->

<details>
<summary>📝 Summary</summary>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Updates**
  * Validation messages keep their existing wording, while their English language labels have been removed. Constraint behavior and validation results are unchanged.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
## Walkthrough

Constraint message literals in two Turtle files no longer have the `@x-gmeow-english` language tag. The message text, constraint definitions, and validation formulas remain unchanged.

### Changes

**Constraint message literals**

|Layer / File(s)|Summary|
|:---|:---|
|**Remove message language tags** <br> `slices/grounding/logic/module.ttl`, `slices/vocabulary.ttl`|Grounding and vocabulary constraint messages are now plain string literals. Their text and validation formulas are unchanged.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal





**Estimated code review effort:** 2 (Simple) | ~5 minutes

<!-- change_assessment_commit:"f06f59e5c5b088b064344f11ca8e453a88c7c352" -->
**Change:** Bug fix
<!-- change_assessment_end -->

</details>

<!-- walkthrough_end -->
<!-- final_review_risk_start -->
**Merge Risk:** _⚪ Minimal_ · up to `f06f5`
<!-- final_review_risk_coverage:{"sourceCommitId":"f06f59e5c5b088b064344f11ca8e453a88c7c352","coveredCommitId":"f06f59e5c5b088b064344f11ca8e453a88c7c352","kind":"reviewed"} -->

The messages retain their text, and inspected repository consumers do not require their language tags. No user-facing regression is identified; the change is mergeable after normal checks.
<!-- final_review_risk_end -->
<!-- pre_merge_checks_walkthrough_start -->

<details>
<summary>🚥 Pre-merge checks | ✅ 5</summary>

<details>
<summary>✅ Passed checks (5 passed)</summary>

|         Check name         | Status   | Explanation                                                                                                                                                                                               |
| :------------------------: | :------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|     Docstring Coverage     | ✅ Passed | No functions found in the changed files to evaluate docstring coverage. Skipping docstring coverage check. Docstring coverage is scoped to functions touched by this diff. Analyzed 0 functions across 0… |
|     Linked Issues check    | ✅ Passed | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
| Out of Scope Changes check | ✅ Passed | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
|         Title check        | ✅ Passed | The title accurately describes the corpus inconsistency fix and contains no agentic branding.                                                                                                             |
|      Description check     | ✅ Passed | The description gives a detailed summary of the changes and rationale, and reports parsing and pre-commit checks. It does not use the template’s Validation, Checklist, or Notes headings, but the main … |

</details>

</details>

<!-- pre_merge_checks_walkthrough_end -->
<!-- finishing_touch_checkbox_start -->

<details>
<summary>✨ Finishing Touches</summary>

<details open>
<summary>🧪 Generate unit tests (beta)</summary>

- [ ] <!-- {"checkboxId": "6ba7b810-9dad-11d1-80b4-00c04fd430c8", "radioGroupId": "utg-output-choice-group-unknown_comment_id"} --> Commit to this branch
- [ ] <!-- {"checkboxId": "f47ac10b-58cc-4372-a567-0e02b2c3d479", "radioGroupId": "utg-output-choice-group-unknown_comment_id"} --> Create a new PR

</details>
<details open>
<summary>✨ Simplify code</summary>

- [ ] <!-- {"checkboxId": "9a4e3077-58f6-4eba-b7ee-62e936ea00ea", "radioGroupId": "simplify-output-choice-group-unknown_comment_id"} --> Commit to this branch
- [ ] <!-- {"checkboxId": "f120d606-b0e2-4b7d-8316-181794555b43", "radioGroupId": "simplify-output-choice-group-unknown_comment_id"} --> Create a new PR

</details>

</details>

<!-- finishing_touch_checkbox_end -->

<!-- autopilot:start -->
- [ ] <!-- {"checkboxId":"2708ad07-9f24-4260-9c11-7dc76a49f2e3"} --> <strong title="Keep fixing CodeRabbit findings and required CI, and resolving merge conflicts">Autopilot</strong> · Keep fixing CodeRabbit findings and required CI, and resolving merge conflicts
<!-- autopilot:end -->
<!-- tips_start -->

---




<!-- poem_footer_start -->
<sub>A rabbit checks each message line,<br>The words stay put, the tags decline.<br>Plain strings now rest in Turtle’s place,<br>No formulas change their course or pace.<br>The rabbit hops through fields of green,<br>And leaves the literals clean.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->


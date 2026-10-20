# PR #1873 — comments and review threads

1 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1873?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **New Features**
  * Added support for language-tagged question wording, including directional and custom language tags.
  * Questions can now provide multiple equivalent wordings while preserving each wording’s language metadata.
  * Added validation that flags question text without a language tag.

* **Documentation**
  * Expanded guidance on question wording, memory records, decoded content, identifiers, and media objects.
  * Added examples and competency queries for multilingual question text.
  * Added French and Chinese translations for question-text terminology and validation guidance.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The change adds language-tag evaluation and validation for `gmeow:questionText`, with examples, tests, and translations. It also documents distinctions between claim memory and objectual remembering, and clarifies identifier, extent, and decoded-content models.

**Changes: Question wording**

|Layer / File(s)|Summary|
|:---|:---|
|**Language-tag predicate and projection** <br> `crates/logic-compile/src/ir/term_kind.rs`, `crates/logic-compile/src/projections/shapes.rs`, `crates/logic-compile/src/projections/shapes.procedural_tests.rs`|The compiler recognizes `termHasLang` and matches only literals with nonempty language tags. Its projection requires a literal and a nonempty tag. Tests cover language-tagged literals and non-literal terms.|
|**Question wording property and validation** <br> `slices/core/inquiry/module.ttl`, `crates/logic/src/reason/enactment/term_kind.tests.rs`, `slices/core/inquiry/tests/structural.ttl`|The inquiry module defines the non-functional `questionText` property and a constraint that requires each supplied value to satisfy `termHasLang`. Integrity and structural tests cover the constraint and its value rules.|
|**Question wording example and usage** <br> `slices/core/inquiry/examples/question-wording.ttl`, `slices/core/inquiry/tests/competency.ttl`, `slices/core/inquiry/docs.md`, `slices/core/inquiry/i18n/*.po`|The example and competency fixture include multiple wordings. Documentation and French and Chinese translations describe language tags, direction, validation, and question-text semantics.|

**Memory model documentation**

|Layer / File(s)|Summary|
|:---|:---|
|**Claim memory and objectual remembering** <br> `slices/core/ai/docs.md`, `slices/core/cognition/docs.md`|The documentation describes claim-based memory records separately from objectual remembering assertions. It gives examples of explicit connections and audit queries, and describes recall and display policies.|

**Identifiers, extent, and decoded content**

|Layer / File(s)|Summary|
|:---|:---|
|**Identifier, extent, and BodyValue guidance** <br> `slices/core/documents/docs.md`, `slices/extensions/email/docs.md`|The documentation distinguishes media-object identifiers from generic entity identifiers, describes extent semantics and measurements, and defines `gmeow:BodyValue` as decoded content separate from literals and transport carriers.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 3 (Moderate) | ~20 minutes

<!-- change_assessment_commit:"121a5412cb03a047d79d44a0c0ce845976007bbc" -->
**Change:** Feature
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>🔵 Low</strong> · up to <code>121a5</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **🔵 Low** · up to `121a5`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"121a5412cb03a047d79d44a0c0ce845976007bbc","coveredCommitId":"121a5412cb03a047d79d44a0c0ce845976007bbc","kind":"reviewed"} -->

The question-wording validation and model documentation look sound. One sentence in the email guidance tells consumers that any file attachment is a MediaObject, which could lead them to mistype PDFs or archives. Narrowing that sentence to image, audio and video attachments is a small fix; otherwise the change is mergeable.
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
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title concisely describes the main changes: documentation for question wording, media metadata, and memory roles. It contains no agentic branding.                                                    |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description explains what changed, why it changed, and reports validation results and limitations. It is mostly complete, though it does not include the template’s explicit Summary, Validation, Ch… |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | `#1818` asks for consumer guidance on question wording, media metadata and content, and claim-based versus objectual memory. The inquiry documentation and example define `gmeow:questionText` separately … |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The changes support `#1818`'s requested guidance. The question-text implementation, language-predicate and projection changes, tests, translations, competency coverage, and example support the new wordi… |
|     Docstring Coverage     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 100.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 10 functions across 4 files. (11 skipped: … |

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description explains what changed, why it changed, and reports validation results and limitations. It is mostly complete, though it does not include the template’s explicit Summary, Validation, Checklist, and Notes headings or checklist items.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Linked Issues check</strong></summary>
<dl>
<dd>

**Explanation**

`#1818` asks for consumer guidance on question wording, media metadata and content, and claim-based versus objectual memory. The inquiry documentation and example define `gmeow:questionText` separately from `rdfs:label`; the property, language constraint, native predicate support, and tests implement and verify the documented language requirement. The documents guidance explains identifier and extent use, and the email guidance clarifies `gmeow:BodyValue` provenance and its distinction from literals and carriers. The AI and cognition guidance keeps `gmeow:MemoryItem` separate from `gmeow:remembers` and documents an explicit query join without an inferred bridge. The changes meet the linked coding and documentation requirements.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Out of Scope Changes check</strong></summary>
<dl>
<dd>

**Explanation**

The changes support `#1818`'s requested guidance. The question-text implementation, language-predicate and projection changes, tests, translations, competency coverage, and example support the new wording contract. The media and memory documentation address the issue's other questions. The PR does not widen domains or add an inferred memory bridge, and the summary keeps the broader `#1807` migration separate. No unrelated change is established by the supplied change summary.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 100.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 10 functions across 4 files. (11 skipped: 11 unsupported.)

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
<sub>A rabbit reads each tagged line,<br>And checks the wording, neat and fine.<br>It hops through claims and memories,<br>Then sorts decoded content with ease.<br>Three soft thumps mark the change complete.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->


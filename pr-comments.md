# PR #1836 — comments and review threads

3 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1836?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- recent_review_start -->

No actionable comments were generated in the recent review. 🎉

<details>
<summary><strong>ℹ️ Recent review info</strong></summary>
<dl>
<dd>

<details>
<summary><strong>⚙️ Run configuration</strong></summary>
<dl>
<dd>

- **Configuration used**: Repository UI
- **Review profile**: ASSERTIVE
- **Plan**: Team
- **Run ID**: `09741884-7db9-4989-9e6c-142a9fe13312`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between 8ba5c655d681c3eb43020c8bb4cef88079f8b8de and 3aba8f7a87c5d05718b30b14ae766ef9bff60164.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 1</strong></summary>
<dl>
<dd>

* `slices/grounding/logic/design/LOGIC-SEMANTICS.md`

</dd>
</dl>
</details>

**Included review availability:** This review used your included allowance. 1 included review remains after this review. Your included PR review attempts over the past 7 days set your current allowance at 3 reviews per hour.

</dd>
</dl>
</details>


<!-- review_details_start -->

<details>
<summary><strong>📜 Recent review details</strong></summary>
<dl>
<dd>

<details>
<summary><strong>🧰 Additional context used</strong></summary>
<dl>
<dd>

<details>
<summary><strong>📚 Code guidelines: 2</strong></summary>

```
docs/GROUNDING.md — auto-discovered
.github/copilot-instructions.md — unknown
```

</details><details>
<summary><strong>📓 Path-based instructions: 2</strong></summary>

<details>
<summary><strong>Applicability reference: AGENTS.md: # AI Developer Agent Guide (AGENTS.md) **Grounding ownership (Principles 17 & 19)**: All semantic grounding to external formalisms is authored in `slices/grounding/lang`, `slices/grounding/math`, or `slic...</strong></summary>


**📄 CodeRabbit inference engine (docs/GROUNDING.md)**

**Files:**
- `slices/grounding/logic/design/LOGIC-SEMANTICS.md`

</details>
<details>
<summary><strong>Refer to the project-root `AGENTS.md` for available `make` targets and developer commands, such as `make check` and `gmeow-dev sync`.</strong></summary>


**📄 CodeRabbit inference engine (.github/copilot-instructions.md)**

**Files:**
- `slices/grounding/logic/design/LOGIC-SEMANTICS.md`

</details>

</details>

</dd>
</dl>
</details>

<details>
<summary><strong>🔇 Additional comments: 1</strong></summary><blockquote>

<details>
<summary><strong>slices/grounding/logic/design/LOGIC-SEMANTICS.md: 1</strong></summary><blockquote>

`262-266`: LGTM!

<!-- cr-comment:v1:5aac46feba7dd06c998ce6f7 -->

</blockquote></details>

</blockquote></details>

</dd>
</dl>
</details>

<!-- review_details_end -->





<!-- recent_review_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Behavior Changes**
  * Probabilistic settings can be declared without a catalogue model, but probabilistic queries now require an explicitly selected model. A model elsewhere in the graph no longer enables evaluation.
  * Without a selected model, probabilistic facts return an unknown result without marginals.
  * Modal translations now preserve the original relation arguments within a contextual application.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The frontend now retains probabilistic configuration declarations without requiring a probability model in the source graph. Probabilistic evaluation requires a model selected by the query. Compiler tests, evaluation tests, and documentation reflect these distinctions.

**Changes: Probabilistic Contract Handling**

|Layer / File(s)|Summary|
|:---|:---|
|**Declaration admission and facet support** <br> `crates/logic-compile/src/frontend.rs`, `crates/logic-compile/src/frontend/source_admission.rs`, `crates/logic-compile/src/frontend/source_admission/classes.rs`, `crates/logic-compile/src/frontend/source_admission/contracts.rs`, `crates/logic-compile/src/graphutil.rs`, `crates/logic-compile/src/frontend/tests.rs`|The frontend no longer rejects probabilistic declarations based on a graph-wide model check. Admission tracks facet-support statements as definition support and processes rules and owners before later source content. Tests cover declaration retention and updated modal-translation expectations.|
|**FOF projection and owner checks** <br> `crates/logic-compile/src/tptp/contracts.tests.rs`, `crates/logic-compile/src/tptp/owners.tests.rs`|Tests cover retaining probabilistic declarations while excluding configuration terms from FOF problems, preserving unrelated facts, and blocking projection without selected object-theory facts. The owner test checks an `UNSUPPORTED_CONTRACT` diagnostic.|
|**Query-time model requirement** <br> `crates/logic/src/probabilistic.tests.rs`, `crates/logic-compile/src/compat.rs`, `slices/grounding/logic/design/LOGIC-CONTRACT.md`, `slices/grounding/logic/design/LOGIC-SEMANTICS.md`, `slices/grounding/logic/i18n/*`, `slices/grounding/logic/module.ttl`|The evaluation test checks that a world-stored model does not replace query model selection. Design text, rule descriptions, translations, and compatibility comments describe the execution-time requirement and the result when no model is selected.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 3 (Moderate) | ~25 minutes

<!-- change_assessment_commit:"3aba8f7a87c5d05718b30b14ae766ef9bff60164" -->
**Change:** Bug fix
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>3aba8</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `3aba8`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"3aba8f7a87c5d05718b30b14ae766ef9bff60164","coveredCommitId":"3aba8f7a87c5d05718b30b14ae766ef9bff60164","kind":"reviewed"} -->

The semantics now accurately explain query-selected models and the no-model result. No concrete merge-blocking risk remains in the reviewed changes.
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
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise and accurately describes the main change. It contains no prohibited agentic branding.                                                                                                |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description clearly explains what changed and why, documents validation results, identifies updated tests and documentation, and records scope, limitations, and follow-up work. It does not reprodu… |
|     Docstring Coverage     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 90.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 30 functions across 9 files. (1 skipped: 1 … |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description clearly explains what changed and why, documents validation results, identifies updated tests and documentation, and records scope, limitations, and follow-up work. It does not reproduce the template headings or checklist boxes, but it includes the required information in prose.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 90.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 30 functions across 9 files. (1 skipped: 1 unsupported.)

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
<sub>A rabbit checked the model at query time,  <br>Then watched the facts return in line.  <br>The catalog kept its settings neat,  <br>While FOF left those terms off its sheet.  <br>“Select a model,” the evaluator said,  <br>And hops of certainty followed ahead.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 2. paudley — 

# Configuration declarations retain their execution boundary

PR 1836: https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1836
Implementation d89debcda; canonical description/translation completion 8ba5c655d.

Removed graph_declares_probability_model, its obsolete extraction error and the now-unused graphutil predicate helper. A contract or preset declaration preserves its native facets without selecting a probabilistic query. Static incompatible facets and malformed owner diagnostics remain hard failures. The actual native probabilistic evaluator remains the one selected-model authority; its model guard is unchanged.

Successfully admitted declarations record their exact selected facet-type support in the existing native definition-support index before source statement admission. Required syntax is not reasserted as object-theory facts. Domain typings and properties of the same resource remain present. No broad vocabulary exclusion, second cache or per-statement configuration scan is added. An empty catalogue remains refused. Two modal expectations now verify all context/relation/original-argument coordinates, retaining source-world checks. The malformed-owner fixture uses a genuinely incompatible static combination and explicitly checks its hard compiler diagnostic.

The canonical RuleProbabilisticRequiresModel definition, French/Chinese coats, contract design and compatibility API documentation now describe the native query-selected requirement. Both PO keys exactly match the source definition and both translations are nonempty. No term or generated file is added, no materialization is claimed and no locale floor is changed. Principles 4, 7, 12, 17, 18 and 19.

Validation:
- The real frontend red control failed with the canonical preset's missing-model error: probabilistic-preset-before-fix.log.
- First expanded selection: 215/218 passed; preset-tests.log retains stale modal expectations and reasserted facet support failures.
- The first support-set attempt failed compilation because QuadIds lacks Ord; preset-final-tests.log is retained. Use the existing typed SourceStatementBinding index instead.
- Broader run: 286/287 passed; preset-settled-tests.log retains the old owner-error fixture failure. Its unsupported-contract preservation assertion remains and now uses an actual static contradiction.
- The identical final selection passed all 287 tests in 0.252s: preset-qualified-tests.log. It includes source/FOF admission, exact support/document/context handling, all malformed owner controls and real native probabilistic queries. A model merely present in the world yields no marginal without explicit query selection; explicit full independence yields 0.5. No test produces corpus.
- Strict producer-independent workspace/all-target Clippy passed with warnings denied in 5m27: preset-settled-clippy.log. Later changes only complete prose/translations and the fixture's static bad-combination input.
- Full source lint and complete helper hygiene passed: preset-final-lint.log and preset-final-helpers.log. Both normal signed commits passed all hooks. Push succeeded.

Initial lint refusal is retained in preset-lint.log. Stage setup exposed generated process receipts to authored-source scanning; only the selected Stage directory is locally excluded and retained for separate ghprsq archival. No source gate or hook is bypassed. stagectl pr-create created PR 1836 but failed parsing its own gh output; fresh forge lookup confirmed the existing PR, so no duplicate creation was attempted.

New-head hosted production/consumer qualification and independent review remain pending. Main CI 38994838112 had helpers/lint passed and both builds live at last readback. Old pipeline run 38988869011 is terminal cancelled, not qualification; only build artifacts exist. No local optimized producer, fingerprint or host probe ran.

This is concrete progress on the probability/configuration part of the full plan, not proof of zero complete-canon blockers. Remaining class/context/owner/binding semantics, prepared native canon, installed blinded proof behavior, production measurements and full qualification remain active under issues 1770/1768/1803 and the full goal.

## 3. paudley — 

Addressed the remaining probability-model paragraph in 3aba8f7a8. It now requires the query-selected model, explains that a model merely present elsewhere in the graph cannot authorize the query, and states the public result as `unknown` with no marginals.

The native result also really does carry `evaluation = unsupported` and `information = not-evaluated`: `prob_result(ProbStatus::Unknown)` in `crates/logic/src/probabilistic.rs` sets both fields, and `prob_unknown_is_unsupported_not_evaluated` asserts them. That control and the actual no-model query controls are included in the 287 passing tests. Those typed fields remain in the paragraph with the public/typed distinction made explicit; removing them would misdescribe the native output.

This last change is prose only. Normal signed commit hooks passed and the commit is pushed. The reviewed implementation at 8ba5c655d is unchanged. Full hosted production/consumer qualification remains pending; the full canon issues are not closed.


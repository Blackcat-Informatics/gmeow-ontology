# PR #1943 — comments and review threads

1 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 



<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1943?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

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
- **Run ID**: `d1cc5104-d618-4a43-9bde-72f92987cd42`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between d387e97bdc3faf0bf278d8f820d9f1238477cc46 and 57e255c68f52640eb111bf4af5a5290b22f2cd31.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 2</strong></summary>
<dl>
<dd>

* `slices/grounding/math/queries/competency/lifted-onnx-tensor-graph-and-parameter-space.rq`
* `slices/grounding/math/tests/fixtures/lifted-onnx.ttl`

</dd>
</dl>
</details>

<details>
<summary><strong>🔗 Linked repositories identified</strong></summary>
<dl>
<dd>

CodeRabbit considers these linked repositories for cross-repo context during reviews:

- [`Blackcat-Informatics/gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) _(auto-detected)_

</dd>
</dl>
</details>

**Included review availability:** This review used your included allowance. 0 included reviews remain after this review. Your included PR review attempts over the past 7 days set your current allowance at 1 review per hour.

</dd>
</dl>
</details>


<!-- review_details_start -->

<details>
<summary><strong>📜 Recent review details</strong></summary>
<dl>
<dd>

<details>
<summary><strong>⏰ Context from checks skipped due to timeout: 4</strong></summary>
<dl>
<dd>

* GitHub Check: rust-prebuild
* GitHub Check: helpers
* GitHub Check: lint
* GitHub Check: producer-build

</dd>
</dl>
</details>

<details>
<summary><strong>🧰 Additional context used</strong></summary>
<dl>
<dd>

<details>
<summary><strong>📚 Code guidelines: 1</strong></summary>

```
AGENTS.md — auto-discovered
```

</details><details>
<summary><strong>📓 Path-based instructions: 1</strong></summary>

<details>
<summary><strong>Source excerpt: **Grounding ownership (Principles 17 & 19)**: All semantic grounding to external formalisms is authored in `slices/grounding/lang`, `slices/grounding/math`, or `slices/grounding/logic`.</strong></summary>


**📄 CodeRabbit inference engine (AGENTS.md)**

**Files:**
- `slices/grounding/math/queries/competency/lifted-onnx-tensor-graph-and-parameter-space.rq`
- `slices/grounding/math/tests/fixtures/lifted-onnx.ttl`

</details>

</details><details>
<summary><strong>🔀 Multi-repo context Blackcat-Informatics/gmeow-gts</strong></summary>

## Linked repositories findings

### Blackcat-Informatics/gmeow-gts

- GTS is ontology-independent and transports RDF 1.2 graphs; this repository contains no documented ONNX-specific consumer or API coupling. [::Blackcat-Informatics/gmeow-gts::README.md:56-60]
- The documented boundary is reading/folding a graph and projecting it to N-Quads, so the updated canonical RDF fixture remains compatible with the transport’s graph model. [::Blackcat-Informatics/gmeow-gts::README.md:399-421]
- GTS artifacts are content-addressed: deterministic CBOR frame content determines frame IDs, and folded output unions segment graphs. Any future packaging of this witness as a `.gts` artifact would therefore require refreshed byte/hash expectations. [::Blackcat-Informatics/gmeow-gts::README.md:799-823]

</details>

</dd>
</dl>
</details>

<details>
<summary><strong>🔇 Additional comments: 5</strong></summary><blockquote>

<details>
<summary><strong>slices/grounding/math/tests/fixtures/lifted-onnx.ttl: 4</strong></summary><blockquote>

`4-6`: LGTM!



Also applies to: 16-16, 19-19

<!-- cr-comment:v1:5034b4e4b1e7415ee974730c -->



`51-85`: LGTM!



Also applies to: 89-89, 93-93, 97-97

<!-- cr-comment:v1:7788ca8f77b50715d8d95b4c -->



`110-110`: LGTM!



Also applies to: 115-115, 121-121, 126-126, 132-132

<!-- cr-comment:v1:5854a1db8ed2f7e336975af7 -->



`199-221`: LGTM!



Also applies to: 254-289, 295-295, 302-302

<!-- cr-comment:v1:720d90b439aaa7df52e8ee81 -->

</blockquote></details>
<details>
<summary><strong>slices/grounding/math/queries/competency/lifted-onnx-tensor-graph-and-parameter-space.rq: 1</strong></summary><blockquote>

`7-7`: LGTM!

<!-- cr-comment:v1:cb1d101083b53f12f00d6881 -->

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

* **Documentation**
  * Clarified the description of the canonical ONNX graph and updated its reported codomain count.
* **Data Updates**
  * Expanded the ONNX graph reference with tensor axes, variable domains, expression types, and tensor value spaces.
  * Updated tensor shape and type relationships, including links between weight tensors and their shapes.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The ONNX fixture now describes a canonical graph-parity witness and contains updated tensor axes, shapes, variable domains, expression types, and value spaces. A query comment now identifies the fixture as the lifter’s canonical graph.

**Changes: ONNX tensor graph witness**

|Layer / File(s)|Summary|
|:---|:---|
|**Tensor dimensions and variable domains** <br> `slices/grounding/math/tests/fixtures/lifted-onnx.ttl`, `slices/grounding/math/queries/competency/lifted-onnx-tensor-graph-and-parameter-space.rq`|The fixture description identifies a canonical N-Quads graph-parity witness. It updates the codomain count to 59, records the non-mnemomorphic correspondence, replaces tensor-axis and shape resources, and adds variable domains. The query comment calls the fixture the lifter’s canonical graph.|
|**Expression types and value spaces** <br> `slices/grounding/math/tests/fixtures/lifted-onnx.ttl`|The graph adds tensor value spaces and expression types linked to value spaces, element types, and shapes. Expression records gain links to the updated type resources. Weight tensors link to the updated shape resources.|

<!-- change_assessment_start -->
**Priority:** ⬇️ Low

**Estimated code review effort:** 2 (Simple) | ~10 minutes

<!-- change_assessment_commit:"57e255c68f52640eb111bf4af5a5290b22f2cd31" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**
- Issue 1902 — The fixture updates tensor identities, value spaces, and variable domains described in the issue’s objectives.
- Issue 1897 — The updated fixture supports the issue’s tensor-grounding parity scope, but does not cover metrics or full production acceptance.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>57e25</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `57e25`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"57e255c68f52640eb111bf4af5a5290b22f2cd31","coveredCommitId":"57e255c68f52640eb111bf4af5a5290b22f2cd31","kind":"reviewed"} -->

No actionable issue is identified in the witness update. Complete the planned hosted parity check as part of normal acceptance.
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
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise, accurate, and describes the refreshed ONNX parity witness. It contains no agentic branding.                                                                                         |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description clearly explains the change, rationale, reconciliation details, validation status, scope, and follow-up requirements. It does not use the repository template headings or explicit check… |
|     Docstring Coverage     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | No functions found in the changed files to evaluate docstring coverage. Skipping docstring coverage check. Docstring coverage is scoped to functions touched by this diff. Analyzed 0 functions across 0… |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                                                                                                  |

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description clearly explains the change, rationale, reconciliation details, validation status, scope, and follow-up requirements. It does not use the repository template headings or explicit checklist boxes, and it does not state the CLA status, but the required information is mostly present.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

No functions found in the changed files to evaluate docstring coverage. Skipping docstring coverage check. Docstring coverage is scoped to functions touched by this diff. Analyzed 0 functions across 0 files. (2 skipped: 2 unsupported.)

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
<sub>A rabbit checks the tensor rows,<br>Where every axis clearly shows.<br>New value spaces join the chart,<br>Expression types take their part.<br>The canonical graph is set,<br>With shapes no hare can forget.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->


# PR #1886 — comments and review threads

2 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1886?cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- This is an auto-generated comment: review in progress by coderabbit.ai -->

> [!NOTE]
> Currently processing new changes in this PR. This may take a few minutes, please wait...
> 
> <details>
> <summary><strong>⚙️ Run configuration</strong></summary>
> <dl>
> <dd>
> 
> - **Configuration used**: Repository UI
> - **Review profile**: ASSERTIVE
> - **Plan**: Team
> - **Run ID**: `8b6d401d-436b-4308-abd2-85c0d22119f3`
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
> Reviewing files that changed from the base of the PR and between 7a39a5f58231d8b2b16d363e263fd34c654f12f3 and 7f94b54fd344f400b615a782613676ef2d544ed2.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>📒 Files selected for processing: 6</strong></summary>
> <dl>
> <dd>
> 
> * `crates/pipeline/src/stages/compile_logic/validation_fixtures.tests.rs`
> * `docs/claim-lifecycle.md`
> * `slices/core/rights/tests/conformance-fixtures/forget-anonymous-requester.ttl`
> * `slices/core/rights/tests/counter-examples/forget-blank-right.ttl`
> * `slices/core/rights/tests/counter-examples/forget-literal-requester.ttl`
> * `slices/core/rights/tests/example-conformance.ttl`
> 
> </dd>
> </dl>
> </details>
> 
> ```ascii
> +--------------------------------------------------+
> | Are you a real coder or just a pretty committer? |
> +--------------------------------------------------+
>   \
>    \     /)/)
>         ( '')
>         /  |
>       o(___)
> ```

<!-- end of auto-generated comment: review in progress by coderabbit.ai -->

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
- **Run ID**: `8c08a0d3-0bbf-4a1c-9862-96eabc2b201c`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between 39891b15742cf24b57a1254923debbebdd698f60 and 7a39a5f58231d8b2b16d363e263fd34c654f12f3.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 7</strong></summary>
<dl>
<dd>

* `crates/logic-compile/src/frontend.rs`
* `crates/logic-compile/src/ir/validation.rs`
* `crates/logic-compile/src/ir/validation.tests.rs`
* `crates/logic-compile/src/projections/shapes.tests.rs`
* `crates/pipeline/src/stages/compile_logic.rs`
* `slices/core/standpoint/i18n/fr.po`
* `slices/core/standpoint/i18n/zh.po`

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

<details>
<summary><strong>💤 Files with no reviewable changes: 2</strong></summary>
<dl>
<dd>

* slices/core/standpoint/i18n/fr.po
* slices/core/standpoint/i18n/zh.po

</dd>
</dl>
</details>

**Included review availability:** This review used your included allowance. 0 included reviews remain after this review. Your included PR review attempts over the past 7 days set your current allowance at 3 reviews per hour.

</dd>
</dl>
</details>


<!-- review_details_start -->

<details>
<summary><strong>📜 Recent review details</strong></summary>
<dl>
<dd>

<details>
<summary><strong>⏰ Context from checks skipped due to timeout: 2</strong></summary>
<dl>
<dd>

* GitHub Check: producer-build
* GitHub Check: rust-prebuild

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
CLAUDE.md — auto-discovered
```

</details><details>
<summary><strong>📓 Path-based instructions: 1</strong></summary>

<details>
<summary><strong>Source excerpt: The regeneration pipeline is governed by [`docs/PIPELINE_SPINE.md`](./docs/PIPELINE_SPINE.md) — the in-memory carrier spine, the single `gmeow.gts` terminal, and the post-pipeline fanout.</strong></summary>


**📄 CodeRabbit inference engine (CLAUDE.md)**

**Files:**
- `crates/pipeline/src/stages/compile_logic.rs`

</details>

</details><details>
<summary><strong>🧠 Learnings: 1</strong></summary>

<details>
<summary><strong>📓 Common learnings</strong></summary>

```
Learnt from: CR
Repo: Blackcat-Informatics/gmeow-ontology

Timestamp: 2026-10-20T19:15:40.905Z
Learning: Applicability reference:
AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 1. Project Overview & Architecture
Every design decision, code modification, and schema change is governed by the nineteen principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 6. PR Lifecycle: Integrate, Review, Push
### Pull review feedback
Read both automated (CodeRabbit, Gemini) and human reviews. Treat actionable automated feedback as binding unless it contradicts the ontology design principles in [CONSTITUTION.md](./CONSTITUTION.md).

Source excerpt:
# The GMEOW Constitution

## 7. Verified by construction

> **A generated artifact is trustworthy only if it is round-trip-checked and guarded by a
> no-drift `--check` in CI.**
```

```
Learnt from: CR
Repo: Blackcat-Informatics/gmeow-ontology

Timestamp: 2026-10-20T19:15:40.905Z
Learning: Applicability reference:
AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 1. Project Overview & Architecture
Every design decision, code modification, and schema change is governed by the nineteen principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 6. PR Lifecycle: Integrate, Review, Push
### Pull review feedback
Read both automated (CodeRabbit, Gemini) and human reviews. Treat actionable automated feedback as binding unless it contradicts the ontology design principles in [CONSTITUTION.md](./CONSTITUTION.md).

Source excerpt:
# The GMEOW Constitution

## 17. The logic itself is canonical — OWL is a projection of it, not its ceiling

> **The logical core is authored in a maximally expressive, RDF 1.2-native logic (`logic:`);
> OWL, Datalog, SHACL, Prolog, N3, and the gUFO / BFO / DOLCE upper ontologies are generat...
```

```
Learnt from: CR
Repo: Blackcat-Informatics/gmeow-ontology

Timestamp: 2026-10-20T19:15:40.905Z
Learning: Applicability reference:
AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 1. Project Overview & Architecture
Every design decision, code modification, and schema change is governed by the nineteen principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 6. PR Lifecycle: Integrate, Review, Push
### Pull review feedback
Read both automated (CodeRabbit, Gemini) and human reviews. Treat actionable automated feedback as binding unless it contradicts the ontology design principles in [CONSTITUTION.md](./CONSTITUTION.md).

Source excerpt:
# The GMEOW Constitution

## 14. Grounded agent memory and claim provenance are the flagship

> **An LLM output is a claim, not a truth — stored with provenance, evidence, confidence, and
> standpoint; recalled with filters; revised by suppression, never deletion. An agent's memory
> is a po...
```

```
Learnt from: CR
Repo: Blackcat-Informatics/gmeow-ontology

Timestamp: 2026-10-20T19:15:40.905Z
Learning: Applicability reference:
AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 1. Project Overview & Architecture
Every design decision, code modification, and schema change is governed by the nineteen principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

AGENTS.md: # AI Developer Agent Guide (AGENTS.md)
## 6. PR Lifecycle: Integrate, Review, Push
### Pull review feedback
Read both automated (CodeRabbit, Gemini) and human reviews. Treat actionable automated feedback as binding unless it contradicts the ontology design principles in [CONSTITUTION.md](./CONSTITUTION.md).

Source excerpt:
# The GMEOW Constitution

## 10. Suppression for revision; governed erasure

> **A superseded label — a deadname, a former gender — is recorded with `gmeow:displayable
> false`: retained for ordinary revision, never displayed. Governed erasure is a
> separately authorized operation on explic...
```

</details>

</details><details>
<summary><strong>🔀 Multi-repo context Blackcat-Informatics/gmeow-gts</strong></summary>

## Linked repositories findings

### Blackcat-Informatics/gmeow-gts

- GTS has no references to the new lifecycle vocabulary (`ForgetRequest`, `succeedsClaim`, `correctsClaim`, or `retractsClaim`), so these ontology terms do not currently map to GTS APIs. [::Blackcat-Informatics/gmeow-gts::]
- GTS implements “deletion” only as additive suppression; bytes remain hash-linked and recoverable (`docs/GTS-SPEC.md:1261-1282`, `rust/README.md:526`). This does not provide the PR’s governed-erasure behavior of physically removing selected objects and the request. [::Blackcat-Informatics/gmeow-gts::]
- Term suppression applies value-wise to quads containing the term (`docs/GTS-SPEC.md:1284-1291`), which is broader than erasing one explicitly selected object while leaving other references dangling. [::Blackcat-Informatics/gmeow-gts::]
- The Rust and Python memory APIs model revision as appending suppression frames, with suppressed claims hidden by default but available when explicitly requested. This remains compatible with ordinary revision suppression, but no purge/delete operation exists. [::Blackcat-Informatics/gmeow-gts::]

</details>

</dd>
</dl>
</details>

<details>
<summary><strong>🔇 Additional comments: 5</strong></summary><blockquote>

<details>
<summary><strong>crates/logic-compile/src/frontend.rs: 1</strong></summary><blockquote>

`2196-2197`: LGTM!

<!-- cr-comment:v1:cc19672679be99ef02987877 -->

</blockquote></details>
<details>
<summary><strong>crates/logic-compile/src/ir/validation.rs: 1</strong></summary><blockquote>

`133-178`: LGTM!



Also applies to: 464-464, 674-674, 946-946

<!-- cr-comment:v1:2c815ca53800876abbfab878 -->

</blockquote></details>
<details>
<summary><strong>crates/logic-compile/src/ir/validation.tests.rs: 1</strong></summary><blockquote>

`6-55`: LGTM!



Also applies to: 57-97

<!-- cr-comment:v1:7347f26f65a79805b79e20c1 -->

</blockquote></details>
<details>
<summary><strong>crates/logic-compile/src/projections/shapes.tests.rs: 1</strong></summary><blockquote>

`15-74`: LGTM!

<!-- cr-comment:v1:97c9aa077f7745a245c99a60 -->

</blockquote></details>
<details>
<summary><strong>crates/pipeline/src/stages/compile_logic.rs: 1</strong></summary><blockquote>

`503-503`: LGTM!

<!-- cr-comment:v1:f9c1e6170f881e323ef2b004 -->

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

- **New Features**
  - Added governed erasure requests requiring an identified requester, selected named targets, and a governing right.
  - Added claim lifecycle support for succession, correction, and withdrawal with standpoint- and time-aware semantics.
  - Clarified that revision retains records while suppressing display; authorized erasure removes only selected objects and the request, without cascading.
  - Unspecified modality now remains unspecified rather than implying a default force.

- **Documentation**
  - Updated lifecycle, governance, identity, standpoint, and multilingual guidance.

- **Bug Fixes**
  - Improved handling of combined validation constraints.

- **Tests**
  - Added validation, conformance, lifecycle, and GMN round-trip coverage.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The changes define claim succession, correction, and withdrawal semantics; clarify that absent modality is unspecified; add a validated forget-request contract; distinguish revision suppression from authorized erasure; and normalize conjunctive node-kind constraints.

**Changes: Claim Lifecycle and Governed Erasure**

|Layer / File(s)|Summary|
|:---|:---|
|**Claim lifecycle and modality model** <br> `slices/core/standpoint/*`, `crates/lang-bridge/tests/gmn1_roundtrip.rs`, `docs/claim-lifecycle.md`|The standpoint ontology adds lifecycle relations, distinguishes speech-act retraction from lifecycle withdrawal, and treats absent modality as unspecified. Examples, structural assertions, translations, documentation, and round-trip tests cover these semantics.|
|**Forget-request contract and validation** <br> `slices/core/rights/*`, `crates/pipeline/src/stages/compile_logic/validation_fixtures.tests.rs`|The rights slice defines requester, named erasure targets, and invoked-right properties. Constraints and tests check required fields and named IRI references.|
|**Erasure doctrine and slice integration** <br> `CONSTITUTION.md`, `governance/constitution.ttl`, `docs/*`, `slices/core/gender/*`, `slices/core/names/*`, `slices/extensions/software/*`|Constitutional text and scope documentation distinguish retained suppression from separately authorized erasure. They describe selected-object removal, non-cascading references, and succession versus correction.|
|**Conjunctive node-kind normalization** <br> `crates/logic-compile/src/ir/validation.rs`, `crates/logic-compile/src/ir/validation.tests.rs`, `crates/logic-compile/src/projections/shapes.tests.rs`, `crates/pipeline/src/stages/compile_logic.rs`|The logic compiler intersects node-kind constraints at qualified, property, and node levels. Disjoint kinds become empty `In` constraints, and tests cover IR, SHACL, and ShEx behavior.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal



**Estimated code review effort:** 4 (Complex) | ~45 minutes

<!-- change_assessment_commit:"7a39a5f58231d8b2b16d363e263fd34c654f12f3" -->
**Change:** Feature · **Severity of issue fixed:** Medium
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>7a39a</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `7a39a`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"7a39a5f58231d8b2b16d363e263fd34c654f12f3","coveredCommitId":"7a39a5f58231d8b2b16d363e263fd34c654f12f3","kind":"reviewed"} -->

The reviewed changes are ready to merge after normal checks; no actionable issue remains.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>
<!-- pre_merge_checks_walkthrough_start -->

<details>
<summary><strong>Pre-merge checks: <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> 4 · <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" width="14" height="14" align="middle" alt="Failed" title="Failed"></picture></sup> 1</strong></summary>
<dl>
<dd>

<strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" width="14" height="14" align="middle" alt="Failed" title="Failed"></picture></sup> Failed checks: 1 warning</strong>

|     Check name    |                                              Status                                              | Explanation                                                                                                                                                                                               | Resolution                                                                                                                                                                                                                                        |
| :--------------- | :----------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Description check | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | The description provides a detailed summary, rationale, validation caveats, scope, and issue reference. However, it does not use the required Summary, Validation, Checklist, and Notes headings, and it… | Restructure the description using the repository template. Add the required Summary, Validation, Checklist, and Notes sections. Record the status of each applicable validation command and checklist item, including skipped commands, test upd… |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title accurately summarizes the claim lifecycle and governed erasure changes. It is concise and contains no agentic branding.                                                                         |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Issue `#1815` is directly linked and has coding requirements. The PR updates Principle 10 and the lifecycle guidance so revision suppresses retained history, while authorized erasure removes exactly the… |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The constitution, ontology, translations, examples, and lifecycle tests implement Issue `#1815`. The node-kind intersection change supports the new named-IRI validation and prevents invalid duplicate fa… |
|     Docstring Coverage     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 80.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 20 functions across 7 files. (2 skipped: 2 … |

<details>
<summary><strong>Full details: Linked Issues check</strong></summary>
<dl>
<dd>

**Explanation**

Issue `#1815` is directly linked and has coding requirements. The PR updates Principle 10 and the lifecycle guidance so revision suppresses retained history, while authorized erasure removes exactly the request and named targets without cascades, tombstones, receipts, or purge history. `ForgetRequest` defines the requester, target, and invoked-right roles, and requires named IRIs for targets and rights. The lifecycle documentation defines external pre-store repair receipts, succession, correction, unspecified absent modal force, and `Activity/retractsClaim` as the canonical withdrawal encoding. Fixtures, counter-examples, structural assertions, round-trip tests, and validation tests cover these requirements.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Out of Scope Changes check</strong></summary>
<dl>
<dd>

**Explanation**

The constitution, ontology, translations, examples, and lifecycle tests implement Issue `#1815`. The node-kind intersection change supports the new named-IRI validation and prevents invalid duplicate facets before projection; its nested-shape and projection tests are supporting regression coverage. The reviewed changes do not add a store deletion endpoint or demonstrate unrelated feature work.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 80.00% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 20 functions across 7 files. (2 skipped: 2 unsupported.)

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description provides a detailed summary, rationale, validation caveats, scope, and issue reference. However, it does not use the required Summary, Validation, Checklist, and Notes headings, and it does not provide the required checklist confirmations.

**Resolution**

Restructure the description using the repository template. Add the required Summary, Validation, Checklist, and Notes sections. Record the status of each applicable validation command and checklist item, including skipped commands, test updates, documentation updates, term metadata, scope, secrets, and CLA requirements.

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
<sub>A rabbit reads each line,<br>The patch grows clear beneath the moon,<br>Small changes hop in place,<br>Tests guard the garden path,<br>Reviews bloom before the dawn.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 2. paudley — 

Addressed the missing coverage from the outside-diff review: the independently authored native fixture now rejects a blank right reference and a literal requester, and positively accepts a typed anonymous requester. Matching source conformance fixtures are registered for all three cases. The existing named-agent success and target/right IRI controls remain.

The proposed requester IRI constraint is not part of the production contract and is not adopted. requestingAgent has range Agent; named or anonymous agents can fill that role. Only requestedErasureTarget and requestUnderRight require named IRIs. The doctrine now states this distinction explicitly, with consuming authorization policy remaining separate. Requiring a global requester IRI here would add an identity restriction that the issue did not request (Principles 7, 9 and 17).

The source-derived conformance judgments remain registered for the authenticated hosted producer; no test rebuilds the corpus. The earlier seven-test run passed, and the affected native request test plus strict Clippy are being rerun for these added controls.


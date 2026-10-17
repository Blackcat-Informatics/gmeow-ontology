<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1835?scope=ghh_OusAJGA4HG8ljoOV0wJvVx8NmnZIjc2DdjmxOu0xZWA&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- This is an auto-generated comment: skip review by coderabbit.ai -->

> [!IMPORTANT]
> ## Review skipped
> 
> Auto reviews are disabled on base/target branches other than the default branch.
> 
> 
> 
> Please check the settings in the CodeRabbit UI or the `.coderabbit.yaml` file in this repository. To trigger a single review, invoke the `@coderabbitai review` command.
> 
> <details>
> <summary><strong>⚙️ Run configuration</strong></summary>
> <dl>
> <dd>
> 
> - **Configuration used**: Repository UI
> - **Review profile**: ASSERTIVE
> - **Plan**: Team
> - **Run ID**: `d7b37583-7d4a-4ab3-bab9-526ec89156a0`
> 
> </dd>
> </dl>
> </details>
> 
> You can disable this status message by setting the `reviews.review_status` to `false` in the CodeRabbit configuration file.
> 
> Use the checkbox below for a quick retry:
> - [ ] <!-- {"checkboxId":"e9bb8d72-00e8-4f67-9cb2-caf3b22574fe"} --> 🔍 Trigger review

<!-- end of auto-generated comment: skip review by coderabbit.ai -->

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
- **Run ID**: `1e5e97e3-5f81-4dfe-854d-3435ae73290f`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between 12380e2dfeb42eac126b3669d3ab64be43db4ed7 and f033b9f3a84cabc6319ca05453afc216c5037cb5.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 13</strong></summary>
<dl>
<dd>

* `crates/affect-ingest/src/lib.rs`
* `crates/build-inputs/tests/inventory.rs`
* `crates/errors/src/diag.rs`
* `crates/errors/src/diag.tests.rs`
* `crates/errors/src/error.tests.rs`
* `crates/gmeow-cli/src/commands/reasoning_input.rs`
* `crates/gmeow-cli/src/commands/reasoning_report.rs`
* `crates/gmeow-cli/src/error.rs`
* `crates/logic-compile/src/frontend.rs`
* `crates/logic-compile/src/tptp/contracts.tests.rs`
* `crates/logic/src/reasoner_services.tests.rs`
* `crates/math-lift/src/sink.tests.rs`
* `slices/grounding/logic/i18n/zh.po`

</dd>
</dl>
</details>

<details>
<summary><strong>🔗 Linked repositories identified</strong></summary>
<dl>
<dd>

CodeRabbit considers these linked repositories for cross-repo context during reviews:

- [`Blackcat-Informatics/purrdf`](https://github.com/Blackcat-Informatics/purrdf) _(auto-detected)_
- [`Blackcat-Informatics/gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) _(auto-detected)_

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
<summary><strong>⏰ Context from checks skipped due to timeout: 3</strong></summary>
<dl>
<dd>

* GitHub Check: producer-build
* GitHub Check: rust-prebuild
* GitHub Check: lint

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
.github/copilot-instructions.md — unknown
```

</details><details>
<summary><strong>📓 Path-based instructions: 2</strong></summary>

<details>
<summary><strong>Ensure Rust code is formatted with `cargo fmt` and passes `cargo clippy` with warnings treated as errors.</strong></summary>


**📄 CodeRabbit inference engine (.github/copilot-instructions.md)**

**Files:**
- `crates/build-inputs/tests/inventory.rs`
- `crates/errors/src/error.tests.rs`
- `crates/gmeow-cli/src/commands/reasoning_report.rs`
- `crates/errors/src/diag.tests.rs`
- `crates/math-lift/src/sink.tests.rs`
- `crates/logic-compile/src/tptp/contracts.tests.rs`
- `crates/logic-compile/src/frontend.rs`
- `crates/logic/src/reasoner_services.tests.rs`
- `crates/errors/src/diag.rs`
- `crates/gmeow-cli/src/error.rs`
- `crates/gmeow-cli/src/commands/reasoning_input.rs`
- `crates/affect-ingest/src/lib.rs`

</details>
<details>
<summary><strong>Refer to the project-root `AGENTS.md` for available `make` targets and developer commands, such as `make check` and `gmeow-dev sync`.</strong></summary>


**📄 CodeRabbit inference engine (.github/copilot-instructions.md)**

**Files:**
- `crates/build-inputs/tests/inventory.rs`
- `crates/errors/src/error.tests.rs`
- `crates/gmeow-cli/src/commands/reasoning_report.rs`
- `crates/errors/src/diag.tests.rs`
- `crates/math-lift/src/sink.tests.rs`
- `crates/logic-compile/src/tptp/contracts.tests.rs`
- `crates/logic-compile/src/frontend.rs`
- `crates/logic/src/reasoner_services.tests.rs`
- `crates/errors/src/diag.rs`
- `crates/gmeow-cli/src/error.rs`
- `crates/gmeow-cli/src/commands/reasoning_input.rs`
- `slices/grounding/logic/i18n/zh.po`
- `crates/affect-ingest/src/lib.rs`

</details>

</details><details>
<summary><strong>🔀 Multi-repo context Blackcat-Informatics/purrdf, Blackcat-Informatics/gmeow-gts</strong></summary>

# Linked repositories findings

### Blackcat-Informatics/purrdf

- `x-gmeow-english` is explicitly accepted by `RdfLiteral` validation and tested as a concrete-syntax language tag. `[::Blackcat-Informatics/purrdf::]` (`crates/rdf-core/src/model.rs:479-505`)
- GTS views reject only language tags the RDF grammar cannot read; callers assembling graphs bypassing parsers are still validated at view construction. `[::Blackcat-Informatics/purrdf::]` (`crates/rdf/src/gts_view.rs:227-259`)
- `frame_le_with` remains the shared exact length-prefix framing primitive. `[::Blackcat-Informatics/purrdf::]` (`crates/hash/src/frame.rs:138-144`)
- No consumers of `RdfSink` or `f64_decimal_lexical` were found.

### Blackcat-Informatics/gmeow-gts

- Canonical sections may carry private-use tags, while projection/docs sections reject them. Affect-ingest output using `x-gmeow-english` must therefore target the canonical payload path. `[::Blackcat-Informatics/gmeow-gts::]` (`python/src/gts/writer.py:63-79`, `491-509`)
- Native codec tests preserve numeric lexical forms such as `0.70`, `.5`, and `1.0e0`; the shared formatter should not normalize already-parsed source lexicals. `[::Blackcat-Informatics/gmeow-gts::]` (`rust/tests/rdf_codecs.rs:461-505`)
- Long private-use tags are required to round-trip through Turtle and N-Triples codecs. `[::Blackcat-Informatics/gmeow-gts::]` (`rust/tests/rdf_codecs.rs:590-615`)
- No consumers of the new Rust helper names were found.

</details>

</dd>
</dl>
</details>

<details>
<summary><strong>🔇 Additional comments: 11</strong></summary><blockquote>

<details>
<summary><strong>crates/errors/src/diag.rs: 1</strong></summary><blockquote>

`941-952`: LGTM!

<!-- cr-comment:v1:c1285555d2c7a5b52828e79d -->

</blockquote></details>
<details>
<summary><strong>crates/gmeow-cli/src/commands/reasoning_input.rs: 2</strong></summary><blockquote>

`39-70`: LGTM!

<!-- cr-comment:v1:06898ee45912c36aaf1ba3f4 -->



`139-180`: LGTM!

<!-- cr-comment:v1:bd7c006fd3c1c24427581ba6 -->

</blockquote></details>
<details>
<summary><strong>crates/gmeow-cli/src/error.rs: 1</strong></summary><blockquote>

`111-111`: LGTM!



Also applies to: 118-118, 181-181, 195-203

<!-- cr-comment:v1:783ea2c1ae31cb8c59973db0 -->

</blockquote></details>
<details>
<summary><strong>crates/errors/src/diag.tests.rs: 1</strong></summary><blockquote>

`15-15`: LGTM!



Also applies to: 21-21, 28-28, 42-61

<!-- cr-comment:v1:076d18a0e602bb928dfa6f52 -->

</blockquote></details>
<details>
<summary><strong>crates/errors/src/error.tests.rs: 1</strong></summary><blockquote>

`9-9`: LGTM!



Also applies to: 23-31

<!-- cr-comment:v1:5ea21980a1ca6e101886cb2d -->

</blockquote></details>
<details>
<summary><strong>crates/build-inputs/tests/inventory.rs: 1</strong></summary><blockquote>

`980-1000`: LGTM!

<!-- cr-comment:v1:3bae1d1eaed9eb293dbf7f39 -->

</blockquote></details>
<details>
<summary><strong>slices/grounding/logic/i18n/zh.po: 1</strong></summary><blockquote>

`22918-22918`: LGTM!

<!-- cr-comment:v1:aed9fa87300bea05660d5ef6 -->

</blockquote></details>
<details>
<summary><strong>crates/logic/src/reasoner_services.tests.rs: 1</strong></summary><blockquote>

`81-81`: LGTM!



Also applies to: 101-101

<!-- cr-comment:v1:f68cb18abc6621b2daad9219 -->

</blockquote></details>
<details>
<summary><strong>crates/math-lift/src/sink.tests.rs: 1</strong></summary><blockquote>

`8-8`: **🎯 Functional Correctness**

The concern is refuted. `purrdf@7734b013d19b68600ad24c4e8aa5a40955a8e89e` records a relative-IRI violation during `intern_iri` instead of returning it. `freeze()` reports that stored violation, so `invalid.string(...).unwrap()` succeeds and `invalid.serialize().is_err()` checks the rejection point correctly.

<!-- cr-comment:v1:dce229208cebc809d244fc47 -->

</blockquote></details>
<details>
<summary><strong>crates/gmeow-cli/src/commands/reasoning_report.rs: 1</strong></summary><blockquote>

`150-151`: **🗄️ Data Integrity & Integration**

The identity-byte preservation claim remains undecidable because the specified merge-base revision `b25d5c510a68d59a54a6eda69da7a6595f863b06` could not be resolved. The previous helper was not inspected, so no safe correction can be proposed.

<!-- cr-comment:v1:da71e1590605730179185d82 -->

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

* **Bug Fixes**
  * RDF output preserves language tags on labels and claim text, with consistent decimal formatting.
  * Invalid contract facets now produce diagnostics that identify the affected contract and conflicting values.
* **Improvements**
  * Diagnostic reports retain severity, failure classification, and original causes.
  * Program identity digests distinguish field boundaries, including for empty, Unicode, and null-containing values.
  * Service refusals now carry classifications distinguishing no-model, malformed-input, and execution failures.
* **Documentation**
  * Added French and Simplified Chinese guidance for service refusal and boundary classifications.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The change adopts shared RDF and hash-framing helpers, adds reusable diagnostic registration, updates contract-facet error reporting, and associates logic service diagnostics with a new Direct Semantics refusal class.

**Changes: Shared helper ownership**

|Layer / File(s)|Summary|
|:---|:---|
|**Shared RDF emission and affect ingestion** <br> `crates/math-lift/src/sink.rs`, `crates/math-lift/src/sink.tests.rs`, `crates/math/src/lib.rs`, `crates/affect-ingest/src/lib.rs`, `helpers-census-debt.json`|The math crates expose the RDF sink and decimal lexical formatter. Affect ingestion uses these shared primitives and emits labels as language-tagged literals. The census removes the recorded sink duplication group.|
|**Diagnostic registration macro and callers** <br> `crates/errors/src/diag.rs`, `crates/errors/src/diag.tests.rs`, `crates/errors/src/error.tests.rs`, `crates/build-inputs/src/modules.rs`, `crates/build-inputs/tests/inventory.rs`, `crates/gmeow-cli/src/commands/reasoning_input.rs`, `crates/gmeow-cli/src/error.rs`|The new macro registers metadata for existing diagnostic payload types. CLI diagnostic kinds use it. Tests cover metadata, causes, macro-input ownership, and uniqueness; uniqueness checks use `FixedState`.|
|**Reasoning identity framing and debt records** <br> `crates/gmeow-cli/src/commands/reasoning_report.rs`, `helpers-census-debt.json`|Program identity uses shared length-framing APIs. Tests cover empty, Unicode, and NUL-containing inputs and boundary changes. The census removes recorded parser and default-constructor debt groups.|

**Contract facet diagnostics**

|Layer / File(s)|Summary|
|:---|:---|
|**Owner-associated facet errors** <br> `crates/logic-compile/src/frontend.rs`, `crates/logic-compile/src/tptp/contracts.tests.rs`|Facet routing reports conflicting values and unknown facet classes directly to diagnostics associated with the contract owner. The test checks the owner and both conflicting values.|

**Direct Semantics service refusal**

|Layer / File(s)|Summary|
|:---|:---|
|**Service-refusal class and diagnostic association** <br> `slices/grounding/logic/module.ttl`, `slices/grounding/logic/i18n/fr.po`, `slices/grounding/logic/i18n/zh.po`, `crates/logic/src/error.rs`, `crates/logic/src/reasoner_services.tests.rs`|The logic catalog adds the service-refusal class and service boundary, with French and Chinese entries. `DlService` diagnostics and related tests use the refusal failure class.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal













**Estimated code review effort:** 3 (Moderate) | ~25 minutes

<!-- change_assessment_commit:"f033b9f3a84cabc6319ca05453afc216c5037cb5" -->
**Change:** Bug fix · **Severity of issue fixed:** Medium
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>f033b</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `f033b`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"f033b9f3a84cabc6319ca05453afc216c5037cb5","coveredCommitId":"f033b9f3a84cabc6319ca05453afc216c5037cb5","kind":"reviewed"} -->

The change shares RDF and hashing helpers and diagnostic registration without any identified behavior regression. Hosted full-check qualification is still pending per the PR description, which is normal process rather than a specific defect.
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

|         Check name         |                                            Status                                            | Explanation                                                                                                                                                                                               | Resolution                                                                                                                                                                                                               |
| :------------------------ | :------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Out of Scope Changes check | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | The PR contains changes outside [`#1834`]. `logic-compile/src/frontend.rs` changes contract-facet diagnostic routing. `logic/src/error.rs`, `reasoner_services.tests.rs`, `slices/grounding/logic/module.t… | Move the contract-facet and direct-semantics refusal changes, including related tests, ontology data, and translations, to a separate PR linked to the applicable issue. Keep this PR limited to the [`#1834`] objectives. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|      Check name     |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :----------------- | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linked Issues check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The PR satisfies the coding objectives in [`#1834`]. `affect-ingest` uses the shared `gmeow_math::RdfSink` and `f64_decimal_lexical`. `impl_diag_kind!` shares diagnostic registration while preserving cu… |
|  Docstring Coverage | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 90.70% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 43 functions across 16 files. (1 skipped: 1… |
|     Title check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise, accurate, and describes the main changes to helper ownership and typed diagnostic failures. It contains no agentic branding.                                                        |
|  Description check  | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description clearly explains the changes, rationale, validation results, known limitations, and linked issue context. It does not reproduce the template's explicit Checklist and Notes headings or … |

<details>
<summary><strong>Full details: Linked Issues check</strong></summary>
<dl>
<dd>

**Explanation**

The PR satisfies the coding objectives in [`#1834`]. `affect-ingest` uses the shared `gmeow_math::RdfSink` and `f64_decimal_lexical`. `impl_diag_kind!` shares diagnostic registration while preserving custom display and source behavior for `ReasoningInputFailure` and `ProverFailed`. `ProgramIdentity::selected` uses shared `Domain` and `frame_le_with` framing. The debt update removes obsolete helper groups, including the affect-ingest, correspondence, and `WorldStore::default` entries. The PR reports no ceiling increase or scanner weakening. The added inventory, identity, failure-path, and helper/diagnostic controls cover the changed behavior.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 90.70% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 43 functions across 16 files. (1 skipped: 1 unsupported.)

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description clearly explains the changes, rationale, validation results, known limitations, and linked issue context. It does not reproduce the template's explicit Checklist and Notes headings or checkbox items, but the required information is mostly covered.

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Out of Scope Changes check</strong></summary>
<dl>
<dd>

**Explanation**

The PR contains changes outside [`#1834`]. `logic-compile/src/frontend.rs` changes contract-facet diagnostic routing. `logic/src/error.rs`, `reasoner_services.tests.rs`, `slices/grounding/logic/module.ttl`, and the French and Chinese catalogs add and validate the `DirectSemanticsServiceRefusal` failure class. The PR description attributes these changes to remaining source-seal findings in `#1762`. They are not required for the [`#1834`] helper ownership, debt, scanner, RDF, framing, or diagnostic-registration objectives.

**Resolution**

Move the contract-facet and direct-semantics refusal changes, including related tests, ontology data, and translations, to a separate PR linked to the applicable issue. Keep this PR limited to the [`#1834`] objectives.

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
<sub>A rabbit checks the triples in a row,<br>And tags each label so its tongue can show.<br>It frames a hash, then hops through code,<br>While typed refusals mark the road.<br>“Good changes,” says the rabbit, ears aglow.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

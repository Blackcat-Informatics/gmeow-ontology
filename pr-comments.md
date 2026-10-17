# PR #1835 — comments and review threads

12 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. paudley — 

# Shared helper ownership repair

Principles 4, 6, 7, 12, 17 and 18 govern one owner per operation, preservation of
semantic evidence and removal of superseded paths. The issue body and all zero
comments were retrieved by brief; its prior-art coverage is retained separately.
No independent-agent API is available. Analysis and implementation proceed locally;
independent plan/task/completion judgments remain unverified, never fabricated.

## Task 1: Consolidate duplicate producers and diagnostics

Use the existing math/lift dependency chain to share the RDF sink; retain affect
language literals explicitly and preserve deterministic Turtle and recovery.
Remove the superseded sink and decimal implementation. Extend the diagnostic
owner to support existing payload types without changing their source chains,
codes, grades, evidence or display. Make generated and handwritten payloads use
the same registration implementation. Verify focused sink, affect and diagnostic
failure-path tests and strict Clippy, then commit/push normally.

## Task 2: Restore the non-increasing debt ledger

Route framing through purrdf's byte-identical owner, use registered Domain values
and fixed hash states at affected callers. Prove preimage identity and error
behavior with synthetic controls. Verify retired parse-helper body groups and
the strictly smaller default cohort, then lower recorded debt only. Never add
an unsanctioned group, raise a count or weaken the scanner. Acceptance is the real
make helpers-hygiene gate plus affected synthetic tests.

## Task 3: Publish and integrate

Commit with all normal hooks, push, open the issue-linked PR, collect CodeRabbit
feedback, execute required hosted qualification and use ghprsq when prerequisites
are satisfied. Keep full native semantic and performance acceptance separate.
No local producer may probe the host under the user's explicit ban.

## 2. coderabbitai — 

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
> - **Run ID**: `2b29a9b4-7166-4dfc-af27-b1f351159b47`
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
- **Run ID**: `b8f6c3a0-8ccf-476b-a988-eb9e4704bb0c`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between f033b9f3a84cabc6319ca05453afc216c5037cb5 and bc70ca583d86f911294ffa7cbcd8dbad8f7de206.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 3</strong></summary>
<dl>
<dd>

* `.github/workflows/ci.yml`
* `crates/gmeow-dev-cli/tests/make_gate_contract.rs`
* `helpers-census-debt.json`

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
<summary><strong>⏰ Context from checks skipped due to timeout: 4</strong></summary>
<dl>
<dd>

* GitHub Check: generation (a)
* GitHub Check: generation (b)
* GitHub Check: fixture-prefix
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
.github/copilot-instructions.md — unknown
```

</details><details>
<summary><strong>📓 Path-based instructions: 2</strong></summary>

<details>
<summary><strong>Ensure Rust code is formatted with `cargo fmt` and passes `cargo clippy` with warnings treated as errors.</strong></summary>


**📄 CodeRabbit inference engine (.github/copilot-instructions.md)**

**Files:**
- `crates/gmeow-dev-cli/tests/make_gate_contract.rs`

</details>
<details>
<summary><strong>Refer to the project-root `AGENTS.md` for available `make` targets and developer commands, such as `make check` and `gmeow-dev sync`.</strong></summary>


**📄 CodeRabbit inference engine (.github/copilot-instructions.md)**

**Files:**
- `helpers-census-debt.json`
- `crates/gmeow-dev-cli/tests/make_gate_contract.rs`

</details>

</details><details>
<summary><strong>🪛 zizmor (1.30.1)</strong></summary>

<details>
<summary><strong>.github/workflows/ci.yml</strong></summary>

[info] 318-318: workflow or action definition without a name (anonymous-definition): this job

(anonymous-definition)



[info] 327-327: action functionality is already included by the runner (superfluous-actions): use `rustup` and/or `cargo` in a script step

(superfluous-actions)

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
<summary><strong>🔇 Additional comments: 3</strong></summary><blockquote>

<details>
<summary><strong>helpers-census-debt.json: 1</strong></summary><blockquote>

`2756-2756`: LGTM!

<!-- cr-comment:v1:08bab220db94ca88f6441a2c -->

</blockquote></details>
<details>
<summary><strong>.github/workflows/ci.yml: 1</strong></summary><blockquote>

`317-341`: LGTM!



Also applies to: 1726-1726, 1748-1748, 1760-1760, 1769-1769

<!-- cr-comment:v1:69e82adce105662b518a327a -->

</blockquote></details>
<details>
<summary><strong>crates/gmeow-dev-cli/tests/make_gate_contract.rs: 1</strong></summary><blockquote>

`80-117`: LGTM!



Also applies to: 195-210, 331-341, 353-354, 532-532, 540-540, 707-707, 957-957, 1481-1481, 1570-1576, 1804-1804

<!-- cr-comment:v1:d2089c20489bf2c8da9de3b6 -->

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
  * Invalid contract facets now produce diagnostics identifying the affected contract and conflicting values.
* **Improvements**
  * Diagnostic reports retain severity, failure classification, and original causes.
  * Program identity digests distinguish field boundaries, including for empty, Unicode, and NUL-containing values.
  * Service refusals carry classifications for no-model, malformed-input, and execution failures.
* **Documentation**
  * Added French and Simplified Chinese guidance for service refusal and boundary classifications.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
The change reuses shared RDF and hash-framing helpers, adds a diagnostic-registration macro, updates contract-facet and Direct Semantics refusal diagnostics, and adds helper-hygiene checks to the CI quality gate.

**Changes: Shared helper ownership**

|Layer / File(s)|Summary|
|:---|:---|
|**Shared RDF emission and affect ingestion** <br> `crates/math-lift/src/sink.rs`, `crates/math-lift/src/sink.tests.rs`, `crates/math/src/lib.rs`, `crates/affect-ingest/src/lib.rs`, `helpers-census-debt.json`|The math crates expose the RDF sink and decimal lexical formatter. Affect ingestion uses these shared primitives and emits labels as language-tagged literals. The census removes the recorded sink duplication group.|
|**Diagnostic registration macro and callers** <br> `crates/errors/src/diag.rs`, `crates/errors/src/diag.tests.rs`, `crates/errors/src/error.tests.rs`, `crates/build-inputs/src/modules.rs`, `crates/build-inputs/tests/inventory.rs`, `crates/gmeow-cli/src/commands/reasoning_input.rs`, `crates/gmeow-cli/src/error.rs`|The new macro registers metadata for existing diagnostic payload types. CLI diagnostic kinds use it. Tests cover metadata, causes, macro-input ownership, and uniqueness; uniqueness checks use `FixedState`.|
|**Reasoning identity framing and debt records** <br> `crates/gmeow-cli/src/commands/reasoning_report.rs`, `helpers-census-debt.json`|Program identity uses shared length-framing APIs. Tests cover empty, Unicode, and NUL-containing inputs and boundary changes. The census removes recorded parser and default-constructor debt groups and updates the fixed-hasher count.|

**Contract facet diagnostics**

|Layer / File(s)|Summary|
|:---|:---|
|**Owner-associated facet errors** <br> `crates/logic-compile/src/frontend.rs`, `crates/logic-compile/src/tptp/contracts.tests.rs`|Facet routing reports conflicting values and unknown facet classes directly to diagnostics associated with the contract owner. The test checks the owner and both conflicting values.|

**Direct Semantics service refusal**

|Layer / File(s)|Summary|
|:---|:---|
|**Service-refusal class and diagnostic association** <br> `slices/grounding/logic/module.ttl`, `slices/grounding/logic/i18n/fr.po`, `slices/grounding/logic/i18n/zh.po`, `crates/logic/src/error.rs`, `crates/logic/src/reasoner_services.tests.rs`|The logic catalog adds the service-refusal class and service boundary, with French and Chinese entries. `DlService` diagnostics and related tests use the refusal failure class.|

**Helpers quality gate**

|Layer / File(s)|Summary|
|:---|:---|
|**Helper hygiene in CI** <br> `.github/workflows/ci.yml`, `crates/gmeow-dev-cli/tests/make_gate_contract.rs`|CI runs `make helpers-hygiene` in a standalone job and includes its result in the quality verdict. Contract tests check the helper job and update related quality-gate expectations.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal







**Estimated code review effort:** 3 (Moderate) | ~25 minutes

<!-- change_assessment_commit:"bc70ca583d86f911294ffa7cbcd8dbad8f7de206" -->
**Change:** Bug fix · **Severity of issue fixed:** Medium
<!-- change_assessment_end -->

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>bc70c</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `bc70c`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"bc70ca583d86f911294ffa7cbcd8dbad8f7de206","coveredCommitId":"bc70ca583d86f911294ffa7cbcd8dbad8f7de206","kind":"reviewed"} -->

No actionable merge-blocking issue is identified in these changes. Normal qualification should still complete before merge.
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

|         Check name         |                                            Status                                            | Explanation                                                                                                                                                                                               | Resolution                                                                                                                                                                                                                        |
| :------------------------ | :------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Out of Scope Changes check | ![Error](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-error.svg) | The PR includes changes outside [`#1834`]. `logic-compile/src/frontend.rs` changes contract-facet diagnostic routing. `logic/src/error.rs`, `reasoner_services.tests.rs`, `slices/grounding/logic/module.t… | Move the contract-facet routing changes and the Direct Semantics refusal class changes, including their tests, ontology data, and translations, to a separate PR linked to the applicable issue. Keep this PR limited to [`#1834`]. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|      Check name     |                                             Status                                             | Explanation                                                                                                                                                                                               |
| :----------------- | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linked Issues check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The PR satisfies the coding requirements in [`#1834`]. `affect-ingest` now uses the shared `gmeow_math::RdfSink` and formatter. `impl_diag_kind!` centralizes diagnostic registration while preserving cus… |
|  Docstring Coverage | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Docstring coverage is 92.45% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 53 functions across 17 files. (2 skipped: 2… |
|     Title check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The title is concise, accurate, and describes the main changes. It contains no agentic branding.                                                                                                          |
|  Description check  | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | The description clearly explains the changes, reasons, validation results, remaining qualification work, and reviewer caveats. It does not reproduce the template headings or checklist format, but it p… |

<details>
<summary><strong>Full details: Linked Issues check</strong></summary>
<dl>
<dd>

**Explanation**

The PR satisfies the coding requirements in [`#1834`]. `affect-ingest` now uses the shared `gmeow_math::RdfSink` and formatter. `impl_diag_kind!` centralizes diagnostic registration while preserving custom payload and cause behavior. `ProgramIdentity::selected` uses shared `Domain` and `frame_le_with` framing. The debt file removes retired groups and lowers the recorded helper count without raising ceilings. The PR adds source-input ownership controls and makes `make helpers-hygiene` a required independent CI job. The reported identity, failure-path, scanner, and helper-gate checks support the required invariants.

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 92.45% which is sufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 53 functions across 17 files. (2 skipped: 2 unsupported.)

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Description check</strong></summary>
<dl>
<dd>

**Explanation**

The description clearly explains the changes, reasons, validation results, remaining qualification work, and reviewer caveats. It does not reproduce the template headings or checklist format, but it provides the required information in prose and is substantially complete.

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

The PR includes changes outside [`#1834`]. `logic-compile/src/frontend.rs` changes contract-facet diagnostic routing. `logic/src/error.rs`, `reasoner_services.tests.rs`, `slices/grounding/logic/module.ttl`, and the French and Chinese catalogs add and validate the `DirectSemanticsServiceRefusal` failure class. These changes address source-seal findings attributed to `#1762`, not the shared-helper ownership, debt, scanner, RDF, framing, or diagnostic-registration requirements in [`#1834`].

**Resolution**

Move the contract-facet routing changes and the Direct Semantics refusal class changes, including their tests, ontology data, and translations, to a separate PR linked to the applicable issue. Keep this PR limited to [`#1834`].

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

- [ ] <!-- {"checkboxId": "6ba7b810-9dad-11d1-80b4-00c04fd430c8", "radioGroupId": "utg-output-choice-group-6227488385"} --> Commit to this branch
- [ ] <!-- {"checkboxId": "f47ac10b-58cc-4372-a567-0e02b2c3d479", "radioGroupId": "utg-output-choice-group-6227488385"} --> Create a new PR

</dd>
</dl>
</details>
<details open>
<summary><strong>✨ Simplify code</strong></summary>
<dl>
<dd>

- [ ] <!-- {"checkboxId": "9a4e3077-58f6-4eba-b7ee-62e936ea00ea", "radioGroupId": "simplify-output-choice-group-6227488385"} --> Commit to this branch
- [ ] <!-- {"checkboxId": "f120d606-b0e2-4b7d-8316-181794555b43", "radioGroupId": "simplify-output-choice-group-6227488385"} --> Create a new PR

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
<sub>A rabbit checks the labels bright,<br>And frames each hash with lengths right.<br>New errors keep their causes near,<br>Facet conflicts name owners clear.<br>The helper gate joins the queue,<br>While carrots wait for CI too.<br>One hop, one test, and all checks through.</sub>
<!-- poem_footer_end -->

<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 3. paudley — 

# Diagnostic macro source-input ownership

Hosted producer-recipe rejected the new shared impl_diag_kind macro before
compilation, because the authenticated source-input traversal only recognized
define_diag_kind. The failure is retained in producer-build-failure.log.

Register the syntax-only shared diagnostic implementation macro alongside its
definition macro. Both forward their complete argument tokens to the existing
recursive traversal. No unknown macro, source-producing declaration, included
file or compile-time input is silently accepted or skipped.

The focused control calls the real InputInventory::collect with a synthetic
source file. It verifies nested included bytes are retained and changing them
changes the inventory; an unknown nested input capability must still fail. The
existing lexical-scope, standard-output nested-input and negated-keyword controls
are selected alongside it. No producer recipe, host probe or corpus production
is invoked by these tests.

All four selected ownership controls passed (macro-ownership-tests.log), including
lexical scope, output-macro inputs and keyword negation. The refreshed real
helpers-hygiene gate passed (macro-ownership-helpers.log). Strict producer-independent
workspace/all-target Clippy passed with warnings denied in 4m50s
(macro-ownership-clippy.log). Commit 425eb65de passed all normal hooks and is pushed
normally to PR 1835. Hosted producer-recipe admission must qualify the new source.
The earlier 156 runtime tests and
helper gate retain their original coverage; they did not exercise producer source
capability admission and are not evidence that the failed hosted job passed.

## 4. paudley — 

# Helper review documentation repair

CodeRabbit comment 6224649194 covered c74f5e804 with no inline or actionable source
finding. Its outer success status still carried a failed documentation-coverage
precheck: 65.52% across 29 touched functions, below the required 80%. The review
also kept authenticated program-identity continuity unverified pending consumer
qualification. Raw evidence: bot-review-current.json and inline-review-current.json.

Commit c3123db1a adds only nine documentation lines across eight source/test files.
They explain the non-cloneable cause fixture, nested-input ownership, exact
diagnostic registration and identity receipts, byte framing, native freeze refusal
and score lexical contracts. No runtime body or verification policy changed.
Every normal commit hook passed. The commit is pushed normally; updated CodeRabbit
coverage is required rather than inferred from the number of comments.

The earlier 156 runtime tests, four macro-ownership controls, refreshed helper gate
and strict Clippy retain their original implementation coverage; documentation
does not establish the still-required hosted producer and consumer acceptance.

## 5. paudley — 

# Diagnostic source-seal repair

The current source-only repository guard found two defects while qualifying the
already-integrated diagnostic migration: route_facet_value returned Result<(),
String>, and logic.dl-service had no canonical failure class. The original run
executed 29 cases: 28 passed and live_repo_static_passes failed on those two
findings. See diag-current-qualification.log.

Contract facet routing now appends directly to the source compiler's existing
Diagnostic observations. The obsolete string-return path and caller conversion
are removed. Conflicting singleton values retain INVALID_CONTRACT_FACET, Error
severity, the exact contract owner and both conflicting values; source admission
continues to reject them. Repeated values and set-valued facets retain their
previous semantics. This is the compiler observation channel, not a new runtime
error abstraction or a suppressed refusal.

DlService now names DirectSemanticsServiceRefusal as its canonical failure class.
Its existing code, grade, typed no-model/malformed/execution disposition and
original upstream error remain intact. The owning logic slice declares that
operational class and its enforcing boundary, with all seven English prose
fields and paired French/Chinese translations. No authored shape or external
semantic authority was introduced. Principles 4, 6, 7, 17 and 18 apply.

Commit 12380e2df is pushed in PR 1835 and the source tree is clean. All 45 focused
cases passed in 17.824s after 7m34s compilation, including the live repository
source seal. Strict producer-independent workspace/all-target Clippy passed with
warnings denied in 4m11s. Every normal signed commit hook passed. Logs:
diag-repair-qualification.log, diag-repair-clippy.log, diag-repair-commit.log and
diag-repair-push.log. The new head has live producer/lint/Rust-prebuild CI.

Hosted ontology/translation and full consumer qualification remain pending.
No local producer, corpus production, full gate or machine probe ran.
The prior helper repair's hosted optimized producer, lint and Rust prebuild all
passed on c3123db1a. Its downstream generation failed with the known recovery-leg
error; generation-current.log retains that evidence and PR 1830 owns the repair.

No full gate or translation/ontology corpus validation is inferred from the
source seal. This repairs two current diagnostic source-seal defects without
claiming full issue 1762 qualification or full native semantic completeness.

## 6. paudley — 

# Review remediation

CodeRabbit reviewed 12380e2df and found one actionable translation error: native was translated as local-machine (本机). Commit f033b9f3a uses the established 原生 terminology. Its remaining source edits document evidence and registration contracts and place test documentation before attributes. No runtime body, admission policy, grade, identity bytes, dependency or threshold changed. All normal commit hooks passed, push succeeded, and the source tree is clean.

The actual review failed documentation coverage at 68.42% across 38 functions. Missing function contracts are now documented; the next review must establish the measured result. The prior 45 tests, strict Clippy and final helpers-hygiene result retain their runtime scope. No full gate is claimed.

The Out of Scope precheck recommends a separate PR for the diagnostic source-seal repairs. That split is declined: the user authorized the entire logic issue/PR queue and challenged the 17 open PRs and worktree/branch sprawl. This PR explicitly links 1834 and 1762. Its shared diagnostic macro must qualify through the real repository source seal; this repair fixes the two actual remaining seal findings while preserving the diagnostic model. It does not claim all of 1762 complete. The PR title and description state the combined scope. Keeping this already-reviewed increment together avoids another branch/PR and an unresolved source seal.

The translation finding is fixed and its thread resolved. New-head hosted production, consumer/ontology checks and review coverage remain pending. The review on 12380e2df is archived in submitted-reviews-current.json and bot-review-after-diagnostic-repair.json.

## 7. paudley — 

The existing proof branch now includes its actual helper-repair base, which itself includes the pipeline-repair base. Published head: 9086140cf; helper head: 589ba27a2; pipeline head: bc230b3f6. No new branch, worktree or PR was created. Ordinary actual-base synchronization was clean; final PR integration still uses ghprsq.

The shared changes were inspected: the native certificate API and proof-checking preparation retain the helper branch's canonical DL refusal class and cause controls. Snapshot canon-admission publication coexists with the sink's retained native Reasoning reader, without a second parser or a new reason-carrier retention edge. The obsolete CertifiedAnswer, RawDerivation and explanation parser/formatters remain deleted. No compatibility wrapper is restored.

Strict producer-independent Clippy passed on the combined source in 1m38; combined-repair-clippy.log. Combined synthetic qualification is running in combined-repair-focused-tests.log. The first selection, combined-repair-tests.log, was interrupted during compilation before any tests ran because it included unrelated Cargo-resolution cases that read the environment. The replacement explicitly selects only the relevant source-ownership cases. It covers the source-only repository seal, contract admission, source provenance, native certificate/proof binding, canon admission publication/reader, retained native panels, native reader tampering controls and snapshot/sink boundaries. Helpers-hygiene is running in combined-repair-helpers.log. Hosted CI must qualify the actual consumers and authenticated corpus; no local optimized producer, fingerprint or full gate runs.

The prior full generation and fixture failures are recorded, not waived by stacking. Neither full native semantics nor prepared-canon completion is claimed. All three PRs remain open pending actual acceptance and final integration.

## 8. paudley — 

# Combined repair qualification

The existing three-PR stack is pushed: pipeline bc230b3f6, helper 213e0bd2a, proof 9cd02cd77. Actual-base synchronization was clean. No new branch, worktree or PR was created, and no final integration or main push occurred.

The replacement deletes CertifiedAnswer, RawDerivation, the explanation Turtle parser and its three obsolete formatters. Production callers preserve the complete native certificate and selected proof; native question preparation supports independent checking. The sink reads the authenticated Reasoning payload already retained by the snapshot, preserving modal worlds and nested/directional terms without another parser or longer original-carrier retention.

Combined qualification: 136/136 focused synthetic tests passed in 31.540s, including the live source-only repository seal. Strict producer-independent workspace/all-target Clippy passed in 1m38. The complete helpers-hygiene target passed on the final combined head. Exact logs: combined-repair-focused-tests.log, combined-repair-clippy.log, combined-repair-helpers-final.log. Corpus was not produced. The earlier overly broad selection was interrupted during compilation and is not counted as a pass.

Deleting the parser reduced fixed-hasher copies from 702 to 701. The authoritative helper ledger records that decrease only. A pipeline-only ledger regeneration would add or raise other unresolved helper debt, so that generated change was discarded; the helper repairs remain a required part of final integration. No scanner, exemption, dependency or semantic admission was weakened. A misplaced untracked review report was moved into Stage and its identical root reply duplicate removed; neither entered source. Normal commit hooks passed after that correction. Pushes succeeded.

CodeRabbit explicitly reviewed executable-source head 9086140cf, reporting no actionable findings and 88.71% documentation coverage. The only subsequent source delta is the measured one-line ledger decrease. Linked-issue/scope checks were skipped; they do not establish full issue completion. The separate pipeline review request was rate limited and is recorded as unavailable.

Hosted final-source production, authenticated consumers and full acceptance remain pending. No local optimized producer, fingerprint, full gate or machine probe ran. All PRs remain open; final integration uses ghprsq after actual acceptance. Full canon semantics, prepared canon, installed proof composition and measured production performance remain open under the standing plan. Principles 4, 6, 7, 17 and 18 apply.

## 9. paudley — 

# Required helper coverage in the first CI wave

The local CHECK_DAG requires helpers-hygiene as a source-only ROOT task, but the current CI workflow contains no invocation. It is not a legitimate CI_JOB_COVERED alternative. The existing every_check_dag_target_is_exercised_by_ci test is running against the unchanged workflow to capture its actual finding; this is not yet a passing or failed executed result.

Add the real make helpers-hygiene target to the independent lint job. Rust is already installed there. Reuse a dedicated Cargo cache for its native helper-census leaf, without depending on producer artifacts, generated outputs, fixtures or test compilation. Required quality already depends on lint, so a helper failure will block the aggregate gate in the initial wave. No check is removed, skipped, softened or run twice by another job. Both cold generations and every existing producer/consumer lane remain unchanged.

Validate the existing CI/check-DAG structural contract, the complete make-gate contract suite, managed YAML/action/hook hygiene and the real helper target. Commit and push in the existing helper branch and propagate the actual base to the proof stack without rewriting history. This closes a required hosted coverage hole and shortens its dependency path; no measured overall speedup is claimed. Principles 4, 6, 7, 15, 17 and 18 apply.

## 10. paudley — 

# Required helper gate in the first scheduling wave

The existing CHECK_DAG coverage test failed against the unchanged workflow, naming helpers-hygiene as the sole missing CI target. The workflow now runs the real Make target in an independent helpers job, without producer, generated corpus, fixture or workspace-test dependencies. Required quality waits for that job and checks its actual result. The source-only lint boundary, both cold generations, authenticated fixture transfers and every existing producer/consumer lane remain intact.

The first lint-job placement was rejected by the full structural suite because lint must not own a Cargo cache. That placement was removed. The independent helper job owns only its native checker cache and is not an alternate corpus producer. No coverage exemption or conditional success was introduced.

The full suite also found stale expectations for the existing helper DAG root, the inert standalone-lint prerequisite, the static fixture download name and the root-aware producer verification signature. Tests now check the current behavior while retaining exact selector identities, verification-before-execution and no test-side production. The required quality-job list has one shared constant instead of two copied tables; helpers is required rather than exempted.

All 20 make-gate contract tests passed in 0.025s; ci-helper-coverage-final-tests.log. Full source hygiene, including YAML, Actions, security, corpus-purity, formatting and provenance hooks, passed; ci-helper-inventory-lint.log. The real helpers-hygiene target passed; ci-helper-coverage-real-gate.log. All normal commit hooks and push passed for edef801c6; ci-helper-coverage-commit.log and ci-helper-coverage-push.log. The proof branch then synchronized this actual base cleanly at 6870f384d: strict Clippy, all 20 contracts and the complete helper target passed on the combined source too. Those exact context-helper-combined logs live in the proof Stage. The before-fix and intermediate failed runs remain evidence, not passes.

The change shortens the helper check dependency path to the initial wave and restores mandatory hosted coverage. No measured whole-pipeline speedup, current hosted success or final merge is claimed. Existing source/failure-class work and its review dispositions remain part of this branch. Principles 4, 6, 7, 15, 17 and 18 apply.

Hosted follow-through: run 38986600675 started the new helper job immediately and its self-tests passed. Job 117259515409 then failed the offline workspace census because the checker-only build had not fetched adler2 v2.0.1 from the locked graph. Exact evidence: ci-helper-hosted-failure.log. Producer-build and lint passed; generation jobs remained live. This job failure is not a whole-run terminal result or a pass.

CI now explicitly fetches the complete locked dependency graph before the offline helper target. The checker retains --offline and fails closed; no local gate acquires a new network dependency or fallback. The contract verifies the locked fetch and its order. All 20 contracts passed again (0.026s), full source hygiene, normal hooks and push passed for bc70ca583; ci-helper-fetch-tests.log, ci-helper-fetch-lint.log, ci-helper-fetch-commit.log and ci-helper-fetch-push.log. Fresh hosted helper job 117261693343 in run 38987283073 completed SUCCESS on that head, including the fetch and complete real gate; ci-helper-fetch-hosted-job.json. Full hosted production/consumers and updated review remain required.

## 11. paudley — 

@coderabbitai review

Please review current head bc70ca583: the required helper check runs in an independent first-wave job, stays mandatory in quality, and explicitly acquires locked dependencies before the unchanged offline census. The source-only lint boundary and both cold generations remain intact. All 20 gate contracts, source hygiene and normal commit hooks pass. Hosted helper job 117261693343 has now passed, including dependency acquisition and the full real target. Please also assess the corrected fixture/verifier contract assertions and single required-quality-job inventory. Full production and consumer acceptance remain pending.

## 12. coderabbitai — 

<!-- This is an auto-generated reply by CodeRabbit -->
<!-- CodeRabbit review command invocation: v2:e5fbe77b7effdc08d7f07010492f7512872bf00545fb5a25d26d2dad10982370 -->
<details>
<summary>✅ Action performed</summary>

Review finished.

> Note: CodeRabbit is an incremental review system and does not re-review already reviewed commits. This command is applicable only when automatic reviews are paused.

</details>


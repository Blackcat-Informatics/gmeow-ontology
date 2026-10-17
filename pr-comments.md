# PR #1841 — comments and review threads

2 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1841?cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

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
> - **Run ID**: `a591c6f2-d11a-48a8-a312-3b8d7f95ccbb`
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
> Reviewing files that changed from the base of the PR and between 96f49762c586180bcc4ad954dc48e7fc633d57b3 and ee262a12f66ba3a11c8dad42007a61307ea40573.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>📒 Files selected for processing: 15</strong></summary>
> <dl>
> <dd>
> 
> * `crates/logic-compile/src/projections/shapes.procedural_tests.rs`
> * `crates/reason-wasm/tests/WITNESS.conjecture.nq`
> * `crates/reason-wasm/tests/WITNESS.reason.nq`
> * `crates/validate/tests/conformance_cases/conformance_profiles.rs`
> * `slices/core/documentation/docs.md`
> * `slices/core/documentation/module.ttl`
> * `slices/core/documentation/shapes.ttl`
> * `slices/core/documentation/tests/structural.ttl`
> * `slices/core/profiles/docs.md`
> * `slices/core/profiles/module.ttl`
> * `slices/core/profiles/tests/structural.ttl`
> * `slices/grounding/lang/module.ttl`
> * `slices/grounding/lang/tests/structural.ttl`
> * `slices/grounding/logic/module.ttl`
> * `slices/grounding/logic/tests/structural.ttl`
> 
> </dd>
> </dl>
> </details>
> 
> ```ascii
> +------------------------------------------------+
> | Bugs in your code are closer than they appear. |
> +------------------------------------------------+
>   \
>    \     /)/)
>         ( '')
>         /  |
>       o(___)
> ```

<!-- end of auto-generated comment: review in progress by coderabbit.ai -->

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






<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->

## 2. paudley — 

fix(logic): preserve annotation metadata through integrity constraints

Advances #1803; the issue remains open.

## What changed

Keep metadata annotation properties as annotations. Their invalid class-role restrictions prevented native DL admission. Replace those restrictions with ten canonical integrity records for requiredness, resource/IRI kind and single resource values across profiles, documentation, logic and lang. Retain existing flagship wiring and failure identities. The companion Profile label requirement is also integrity, so literal labels are not forced into an object class.

Delete the replaced docOwnerSlice legacy shape block after actual generated-validation equivalence passed. The tiny control exercises the frontend, authored reader view, native admission and proof replay, and compares generated SHACL against an independent required/single-IRI fixture. It checks missing, multiple, blank and literal values, distinct IRIs related by owl:sameAs, and an unrelated-class control. Four producer-owned slice assertions and three authenticated profile cases cover actual source wiring after fresh production.

Principles 4, 7, 17 and 18: one canonical integrity source, generated validation, native mechanism ownership and original metadata semantics. No native parser relaxation, source filtering, alias or weaker fallback is added.

All 13 distinct selected synthetic, identity and witness cases have passing coverage. Strict producer-independent workspace all-target Clippy passed with warnings denied, as did formatting and normal commit/push hooks. Historical witness mismatches are retained: actual source identity was checked, the explicit verified tiny producer refreshed both attestations, dev/test computations matched, and final read-only consumers passed. The origin of older identity drift is not inferred from these observations.

Fresh corpus and full hosted acceptance remain pending. The user explicitly instructed: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged." CodeRabbit is processing and no substantive review is yet available; no independent approval is claimed. Later hosted/review findings remain active work. Two older hosted jobs ended with runner shutdown signals, not a demonstrated OOM or timeout; actual logs/timings are archived and attached to existing issue 1757.

Class delegation/deletion and its proof/stop/identity/shared-store boundaries remain unfinished under 1803. Native trace inspection exposes private term IDs and retained legacy reachability trust assumptions that must not be laundered into source-derived local heads. Full prover and logic acceptance is not established by this increment.

Authoritative plan: .stage/split-gmeow-purrdf-responsibilities/plan.md. Selected evidence: /home/paudley/Active/gmeow-ontology/.worktrees/1803-split-gmeow-purrdf-responsibilities/.stage/split-gmeow-purrdf-responsibilities. validation.md and completion-audit.md describe the exact scope and remaining acceptance.

## Commits squashed (1)

- `ee262a12f` fix(logic): preserve annotation metadata through integrity constraints

## Files

```
.../src/projections/shapes.procedural_tests.rs     |  94 +++++++++
 crates/reason-wasm/tests/WITNESS.conjecture.nq     | 228 ++++++++++-----------
 crates/reason-wasm/tests/WITNESS.reason.nq         |   2 +-
 .../conformance_cases/conformance_profiles.rs      |  33 +++
 slices/core/documentation/docs.md                  |   5 +
 slices/core/documentation/module.ttl               |  72 ++++---
 slices/core/documentation/shapes.ttl               |   8 -
 slices/core/documentation/tests/structural.ttl     |  17 ++
 slices/core/profiles/docs.md                       |   4 +
 slices/core/profiles/module.ttl                    |  41 +++-
 slices/core/profiles/tests/structural.ttl          |  21 ++
 slices/grounding/lang/module.ttl                   |  26 ++-
 slices/grounding/lang/tests/structural.ttl         |  17 ++
 slices/grounding/logic/module.ttl                  |  36 +++-
 slices/grounding/logic/tests/structural.ttl        |  16 ++
 15 files changed, 452 insertions(+), 168 deletions(-)
```

## Conflicts

No conflicts. The fetched base remains the branch base. The clean integration prediction is tree 63e3f87f8b08de46c1e6a0011c42074b72caebd5, with no base-only behavior delta that invalidates the focused checks.

---

Defect-Class: none


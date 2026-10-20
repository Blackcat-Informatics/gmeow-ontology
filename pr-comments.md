# PR #1885 — comments and review threads

2 comment(s). **Every one needs a disposition** in
`review-debt.md`: addressed (with the commit hash) or declined (with the
reason posted in reply). Bot comments count.

## 1. coderabbitai — 

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1885?cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

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
> - **Run ID**: `82f9a110-c4be-4c57-a48a-6e2d793633e1`
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
> Reviewing files that changed from the base of the PR and between 591f8f6a7fb1c6797b8d747463d79387dc3e6d25 and 5bbb43a03be1cf16bf882589c6fdfa7392882138.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>📒 Files selected for processing: 5</strong></summary>
> <dl>
> <dd>
> 
> * `crates/pipeline/src/stages/compile_logic.rs`
> * `crates/pipeline/src/stages/pydantic.rs`
> * `crates/pipeline/src/stages/pydantic.tests.rs`
> * `crates/pipeline/src/stages/pydantic/topology.rs`
> * `docs/schema-projections.md`
> 
> </dd>
> </dl>
> </details>
> 
> ```ascii
> +---------------------------------------------------------+
> | This PR is 10% code and 90% emotional support comments. |
> +---------------------------------------------------------+
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

fix: route imported template model types

Refs #1646 and #1757. Both broader issues remain open.

Completed cold generations stopped at model export because the imported Blutdruck-Diastolic-cardinality class had no slice term documentation. The exporter treated the source-specific OPT namespace as ordinary vocabulary ownership. Share the actual importer's existing namespace declarations with model routing, place imported types in schema_helpers, and preserve their original class IRI and source template path. Keep ontology models' real owners, documentation and definition digests.

Refuse distinct target IRIs that collapse to one schema key, and refuse documentation naming a different target IRI. Preserve all constraints and the shared native schema compiler/renderer. Advance the Pydantic stage cache identity and document the imported-type metadata. Principles 4, 7, 17 and 18.

Validation: 21 focused synthetic tests passed, including actual OPT-constraint lift, SHACL projection, native schema compilation and routed model emission for both template sources; near/nested namespace refusals, target/documentation collisions, existing metadata/owner controls and stable artifact rendering also passed. Source lint, strict producer-independent all-target Clippy with warnings denied, and normal signed commit/push hooks passed. The initial Clippy missing generated build input cleared on the unchanged normal target retry, without source or toolchain changes.

No local optimized producer, corpus/fixture production, synchronization, full gate, generated package or live Python import/schema check ran. The authenticated artifact consumer remains registered and unchanged. Full package/pipeline acceptance remains on the original issues; no requirement is closed or moved to a replacement ticket.

Integration: fetched main did not advance beyond the starting base; merge-tree is clean and the candidate contains the tested source. No conflicts were resolved. Fresh review submissions, inline comments and threads are empty; the bot processing notice is informational. This is self-review because no independent dispatcher is exposed. Hosted CI and automated review are pending, not approval. Merge follows the explicit instruction: "merge them one by one, ignoring CI."

Authoritative plan: .stage/schemas-recover-the-developer-surface/plan.md.
Selected Stage directory: /home/paudley/Active/gmeow-ontology/.worktrees/1646-schemas-recover-the-developer-surface/.stage/schemas-recover-the-developer-surface.

Defect-Class: model-ownership


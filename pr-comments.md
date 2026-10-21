# PR #1934: fix: ground literal projections and separate schema value guards

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Comments (1)

### coderabbitai — 2026-10-21T21:40:57Z



<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/Blackcat-Informatics/gmeow-ontology/pull/1934?cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

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
> - **Run ID**: `21f8bae6-ee6c-4df9-802c-ecfdf94714fd`
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
> Reviewing files that changed from the base of the PR and between 186c4ce0e302619cd218ddc35c4daac5bcdf9034 and d42fc820db2bd314cc75551a26bfad2c7f52f601.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>⛔ Files ignored due to path filters: 1</strong></summary>
> <dl>
> <dd>
> 
> * `Cargo.lock` is excluded by `!**/*.lock`
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>📒 Files selected for processing: 110</strong></summary>
> <dl>
> <dd>
> 
> * `crates/bundle-view/src/bundle_blobs.rs`
> * `crates/docs-model/src/fixture.rs`
> * `crates/docs-model/src/i18n.rs`
> * `crates/docs-model/src/i18n_compile.rs`
> * `crates/docs-model/src/i18n_compile.tests.rs`
> * `crates/docs-model/src/lib.rs`
> * `crates/docs-model/src/model.rs`
> * `crates/docs-model/src/ontology_catalogs.rs`
> * `crates/docs-model/src/ontology_catalogs.tests.rs`
> * `crates/docs-model/src/reviewed_coverage.rs`
> * `crates/docs-model/src/reviewed_coverage.tests.rs`
> * `crates/docs/src/lib.rs`
> * `crates/docs/tests/fixtures/reviewed_coverage_golden.json`
> * `crates/gmeow-cli/src/commands.rs`
> * `crates/gmeow-cli/tests/slice_quality_bundle.rs`
> * `crates/gmeow-dev-cli/src/dev_slice_quality.axis_floor_diagnostics_tests.rs`
> * `crates/gmeow-dev-cli/src/dev_slice_quality.edge_reason_merge_tests.rs`
> * `crates/gmeow-dev-cli/src/dev_slice_quality.relocation_gate_tests.rs`
> * `crates/gmeow-dev-cli/src/dev_slice_quality.rs`
> * `crates/gmeow-dev-cli/src/dev_transpile.rs`
> * `crates/logic-compile/Cargo.toml`
> * `crates/logic-compile/src/ingest/dataset.rs`
> * `crates/logic-compile/src/projections/correspondence.rs`
> * `crates/logic-compile/src/projections/correspondence_frontend.rs`
> * `crates/logic-compile/src/projections/correspondence_frontend/test_support.rs`
> * `crates/logic-compile/src/projections/correspondence_frontend/tests.rs`
> * `crates/logic-compile/src/projections/correspondence_soundness/tests.rs`
> * `crates/logic-compile/src/projections/edoal.rs`
> * `crates/logic-compile/src/projections/edoal.tests.rs`
> * `crates/logic-compile/src/projections/fno.tests.rs`
> * `crates/logic-compile/src/projections/get_leg.rs`
> * `crates/logic-compile/src/projections/grounding_catalog.rs`
> * `crates/logic-compile/src/projections/mod.rs`
> * `crates/logic-compile/src/projections/profile_program.rs`
> * `crates/logic-compile/src/projections/put_derivation.rs`
> * `crates/logic-compile/src/projections/selected_templates.rs`
> * `crates/logic-compile/src/projections/sparql.rs`
> * `crates/logic-compile/src/projections/sparql/hygiene.rs`
> * `crates/logic-compile/src/projections/sparql/tests.rs`
> * `crates/logic-compile/src/projections/sparql_put.tests.rs`
> * `crates/logic-compile/src/projections/sssom.rs`
> * `crates/logic-compile/src/projections/sssom.tests.rs`
> * `crates/pipeline/src/catalog_families.rs`
> * `crates/pipeline/src/catalog_families.selected.tests.rs`
> * `crates/pipeline/src/cli_ops/okf_import.rs`
> * `crates/pipeline/src/docs_distribution.rs`
> * `crates/pipeline/src/docs_distribution.tests.rs`
> * `crates/pipeline/src/lib.rs`
> * `crates/pipeline/src/projection_execution.rs`
> * `crates/pipeline/src/projection_execution/tests.rs`
> * `crates/pipeline/src/projections.rs`
> * `crates/pipeline/src/projections.tests.rs`
> * `crates/pipeline/src/scoreboards.rs`
> * `crates/pipeline/src/stages/archive_blobs.rs`
> * `crates/pipeline/src/stages/carrier.rs`
> * `crates/pipeline/src/stages/conformance/grounding_catalogs.rs`
> * `crates/pipeline/src/stages/conformance/reviewed_translations.rs`
> * `crates/pipeline/src/stages/correspondence_lower.rs`
> * `crates/pipeline/src/stages/correspondence_lower/statistics.tests.rs`
> * `crates/pipeline/src/stages/correspondence_soundness.rs`
> * `crates/pipeline/src/stages/lang_docs_rendering.rs`
> * `crates/pipeline/src/stages/lang_docs_rendering.selected.tests.rs`
> * `crates/pipeline/src/stages/lang_glossary.rs`
> * `crates/pipeline/src/stages/lang_translation.rs`
> * `crates/pipeline/src/stages/mappings.rs`
> * `crates/pipeline/src/stages/research_objects.rs`
> * `crates/pipeline/src/stages/substrate_graph.rs`
> * `crates/pipeline/src/transform.rs`
> * `crates/pipeline/src/transform.tests.rs`
> * `crates/slice-quality/Cargo.toml`
> * `crates/slice-quality/src/axes.rs`
> * `crates/slice-quality/src/counting.rs`
> * `crates/slice-quality/src/counting.selected_templates.tests.rs`
> * `crates/slice-quality/src/counting.tests.rs`
> * `crates/slice-quality/src/lib.residue_text_carrier_tests.rs`
> * `crates/slice-quality/src/lib.rs`
> * `crates/slice-quality/src/lib.test_support.rs`
> * `crates/slice-quality/src/profile.tests.rs`
> * `crates/slice-quality/src/projection_debt.rs`
> * `crates/slice-quality/src/projection_debt.tests.rs`
> * `crates/slice-quality/src/report.rs`
> * `crates/slice-quality/tests/common/mod.rs`
> * `crates/slice-quality/tests/rdf_projection.rs`
> * `crates/slice-quality/tests/render_goldens.rs`
> * `crates/slice-quality/tests/score_from_file_map.rs`
> * `crates/validate/src/grounding_catalog.rs`
> * `crates/validate/src/lib.rs`
> * `crates/validate/src/repo_static.rs`
> * `docs/GROUNDING.md`
> * `docs/PROJECTION-VOCABULARY-RATCHET.md`
> * `dsl/mappings/catalog-families.ttl`
> * `dsl/mappings/i18n/fr.po`
> * `dsl/mappings/i18n/zh.po`
> * `dsl/mappings/projections/schema-org.ttl`
> * `dsl/mappings/vocabulary.ttl`
> * `slices/core/organization/manifest.ttl`
> * `slices/core/organization/module.ttl`
> * `slices/core/slice-quality-rubric/i18n/fr.po`
> * `slices/core/slice-quality-rubric/i18n/zh.po`
> * `slices/core/slice-quality-rubric/module.ttl`
> * `slices/extensions/images/manifest.ttl`
> * `slices/extensions/images/module.ttl`
> * `slices/grounding/lang/design/LANG-TRANSLATION.md`
> * `slices/grounding/lang/i18n/fr.po`
> * `slices/grounding/lang/i18n/zh.po`
> * `slices/grounding/lang/mappings/literal-value-bridge.ttl`
> * `slices/grounding/lang/module.ttl`
> * `slices/grounding/logic/i18n/fr.po`
> * `slices/grounding/logic/i18n/zh.po`
> * `slices/grounding/logic/module.ttl`
> 
> </dd>
> </dl>
> </details>
> 
> ```ascii
> +--------------------------------------------------------------------------------------------+
> | It is easier to change the specification to fit the program than vice versa. - Alan Perlis |
> +--------------------------------------------------------------------------------------------+
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


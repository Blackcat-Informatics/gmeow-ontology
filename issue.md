# Issue #2831: Constraints over reified statements never see RDF 1.2 data on the validation paths

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, documentation, cross-cutting, quality, logic, rust, complexity-hard

## Body

## Problem

Several `logic:Constraint` records are written over the lowered components of an RDF 1.2 reified statement: `logic:reifiedStatementSubject`, `logic:reifiedStatementPredicate` and `logic:reifiedStatementObject`. Those edges are produced only by `statement_lowering::lower_reifiers` (`crates/logic/src/statement_lowering.rs:83-111`), and its only caller is `cli_world_dataset` in `crates/logic/src/operator.rs:566`, the operator CLI's reasoning world.

The validation paths never add them. `gmeow validate` builds a flat data view (`data_dataset_flat` / `flatten_to_default_graph`, `data_validate.rs:899-933`) that keeps base statements, reifiers and annotations and adds no component edge. The conformance `validate()` path (`conformance_support/mod.rs:183-187, 534-537`) and the pipeline fixture validation do the same. A constraint with no `gmeow:enforcesFailureClass` is enforced only through its derived SHACL, which runs on that flat view.

So for data written the way these constraints' own definitions prescribe (`ex:a gmeow:moreEntrenchedThan ex:b ~ ex:rankAB {| gmeow:accordingTo ex:ranker ; gmeow:validFrom … |}`):

- `moreEntrenchedThanRankerOrderConstraint` and `moreEntrenchedThanRankerClosureConstraint` never match: a reversed pair or a missing closure is never reported.
- `moreEntrenchedThanAttributionConstraint` reports every correctly attributed ranking as a Violation, because its consequent finds no component edge.

The same form is used by constraints in other slices (affect, dreaming, record-metadata and others), which are affected the same way.

This is established by reading the code at `cd0a85620` (and the same code on `main`), not by a run. The existing cases pass because their fixtures write the lowered edges by hand, a form no validation entry point produces from RDF 1.2 input.

## Acceptance

- [ ] One run confirms or refutes the reading: a correctly attributed RDF 1.2 ranking validated through the consumer path, with the findings recorded.
- [ ] The lowered component edges are present in every validation view that derived SHACL runs on (the consumer validate path, the conformance path, the pipeline fixture validation), from the one lowering, not a second implementation.
- [ ] Every constraint that reads lowered components is listed, by slice, and each has a control written as RDF 1.2 reified statements: one conforming, one violating.
- [ ] No correctly formed reified statement draws a finding from such a constraint.
- [ ] Fixtures that hand-write lowered edges are replaced by, or paired with, their RDF 1.2 form.

Principles 2, 7, 17.


## Comments (1)

### coderabbitai — 2026-11-16T00:44:11Z

<!-- coderabbit-bot-generated:v1 -->

<!-- This is an auto-generated issue plan by CodeRabbit -->

<details>
<summary>⚠️ Possible Duplicate Issue(s)</summary>
<dl>
<dd>
<br>

- https://github.com/Blackcat-Informatics/gmeow-ontology/issues/2807
</dd>
</dl>
</details>
<details>
<summary>🔗 Related PRs</summary>
<dl>
<dd>
<br>

Blackcat-Informatics/gmeow-ontology#2688 - docs(standpoint): add a per-ranker partial order between standpoints [merged]
</dd>
</dl>
</details>

---
<details>
<summary>📝 Issue Planner</summary>
<dl>
<dd>
<br>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId":"8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</dd>
</dl>
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>
<dl>
<dd>
<br>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</dd>
</dl>
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!


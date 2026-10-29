# Issue #1446: EPIC: Closed-world constraint canonicalization — blanket shape-purity seal, zero legacy

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, epic, foundational, cross-cutting, logic, rust, complexity-hard

## Body

# EPIC — Closed-world constraint canonicalization: blanket shape-purity seal, zero legacy

Canonicalize the entire closed-world constraint corpus so every SHACL/ShEx shape is a
projection of slice constraint axioms, land a **blanket** shape-purity seal, and delete the
whole hand-authored shape corpus + the structural `write_shex` export. Single source of truth.

Driven by `.goals`/`.baseline` (no deferrals, no compromises, no optionality, keep no legacy):
the gate is blanket, and a blanket gate is green only when the corpus is clean — so the migration
is executed to completion, not softened to a "detector".

## Foundation (landed on `paudley/1193-shape-subsumption-blanket-seal`)
- Subsumption engine (`≡`/`⊑` over the `ConstraintComponent` lattice).
- Equivalence oracle + comparison-only SHACL→IR reader + non-vacuous cross-check.
- Lift = the projector's Galois adjoint, certified against the real forward derive.
- Blanket declarative purity gate (proven over the live corpus; activates at the terminal increment).
- Typed `Shacl`/`Shex` lookaside sidecars into `gmeow.gts`.
- `conformance/validation/` projection-correctness category.
- `gmeow-dev shape-equivalence` — the per-increment verifier and the **live coverage ledger**
  (exits non-zero until every authored shape is grounded; this is the epic done-condition, no
  staleable committed snapshot).
- Proving slices `inference` + `inhabitation` grounded end-to-end (14 classes), oracle-verified.

## Remaining increments (per-slice; each: lift → author axioms in the owning slice → oracle `EQUIV` → delete legacy block)
~160 legacy shapes remain, by owning slice:
- `slices/extensions/music` (~33)
- `shapes/gmeow-shapes.ttl` — re-home ~78 to owning slices
- `slices/core/epistemics` (~8), `slices/core/ai` (~5)
- `shapes/mapping-dsl-shapes.ttl` (~12), `shapes/test-dsl-shapes.ttl`
- `slices/extensions/{model-serving,graphrag,agentic}`, `slices/core/{temporal,gts,standpoint,pipeline,notation,kernel,concepts,learning,diagnostics}`
- tail singletons

Run `gmeow-dev shape-equivalence --path <slice>` to see a slice's current verdicts.

## Residue-bearing classes (no faithful shape projection — carried in the canon, ledgered)
Some legacy shapes carry `sh:or` type-disjunction, value-keyed `sh:SPARQLTarget`, or nested
`sh:node` conditions the dataset-derive cannot reproduce (e.g. `AttackShape`, `SupportShape`,
`AbductionCommitmentShape`, `InhabitationTenureShape`). These are grounded either by extending the
derive or by authoring the condition as a full-FOL `logic:` constraint (projecting to the SHACL
constraint surface) — never deleted with their check dropped.

## Terminal increment
When the ledger reaches 0: wire the blanket gate active into `check_repo_static`; delete all
remaining hand-authored `**/shapes.ttl` + the structural `write_shex`/`dist/gmeow.shex`; repoint
consumers to the generated surface. This is where #1193 closes (`Closes #1193, #1194`) — its
headline gate reaches production with `make check` green.


## Comments (3)

### coderabbitai — 2026-07-08T04:45:56Z


<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#52 - feat: standpoint / contested-claims facility — coexisting claims, no winner (`#43`) [merged]
Blackcat-Informatics/gmeow-ontology#608 - feat(shacl): Python seam swap to gmeow_shacl behind a dual-run cross-check (`#578`) [closed]
Blackcat-Informatics/gmeow-ontology#1232 - feat(1195): openEHR constraints axis — validation-shape IR + SHACL projection + OPT→logic: lift with u∘d=id law [merged]
Blackcat-Informatics/gmeow-ontology#1283 - feat(logic): SHACL-as-projection consolidation — 9 FOL axioms single-sourced (H8) [merged]
Blackcat-Informatics/gmeow-ontology#1387 - feat(logic): derive the full closed-world validation-shape fragment from slice constraint axioms [merged]
</details>

---
<details>
<summary>📝 Issue Planner</summary>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!

### paudley — 2026-07-09T17:01:57Z

Epic re-shaped by the 2026-07-09 backlog triage: the per-slice-group grounding children (#1447/#1450/#1451) are closed as consolidated into re-scoped #1449 (shape-grounding axis + docs + skill) and the continuous background uplift lane (#1476). Remaining tracker scope for this epic: #1449 (axis), #1448 (gmeow-shapes.ttl re-home, bounded), #1452 (terminal: blanket gate + legacy deletion, closes #1193/#1194).

### paudley — 2026-07-20T04:13:09Z

## Binding architecture correction — “author axioms” now means logic-only

For every remaining increment, the cycle is:

`lift legacy obligation → author realized logic validation/constraint node + formula in the owning slice → generate projection → oracle proves equivalence → delete legacy block`.

The wording “author axioms in the owning slice” must not be implemented as new OWL/RDFS/gUFO semantic axioms. The lift/oracle may read those legacy surfaces as evidence, but the sole new semantic source is `logic:`; all external formalism surfaces are generated and loss-ledgered.

The terminal zero must therefore mean zero legacy shapes and zero obligations backed only by OWL/RDFS/gUFO. Required RDFS annotation metadata remains unchanged. Principles 4, 17, and 19 apply.



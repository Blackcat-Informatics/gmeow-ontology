# Issue #1656: docs-coverage and translation debt on recently minted vocabulary

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, cross-cutting, quality, complexity-hard

## Body

Drained from the local `.deficiencies` ledger (recorded 2026-07-14 under the math shape-grounding packet, and under #764, #1374, #1546). Re-verified against current `main`.

Blocked by: #1530 — the slice authoring factory owns the machinery (briefs, fuzzy-gated two-stage translations, distinctiveness guard) that makes this authorable without producing gamed boilerplate.
Relates to: #1642 (the four axis floors this debt currently caps).

## Problem

Four separate landings deliberately shipped vocabulary without its full documentation coat or translations, each with explicit approval at the time. The debt is now the binding constraint on several slice-quality axes, and nothing tracks burning it down.

### 1. The math slice (the largest block)

Excluded from the shape-grounding PR by explicit user direction ("Everything but the translations, we'll punt those for now"):

- ~**691 uncoated terms** across 8 annotation fields.
- ~**1,383 untranslated literals** across 2 languages.

The math slice therefore sits at tier Registered: translation coverage measures **0.41**, information coverage ~**0.60**. The floor was deliberately kept at 0.333 rather than pinned at 0.41, because the sanctioned coat authoring will expand the localizable denominator and a 0.41 pin would manufacture a guaranteed future gate red.

### 2. GMN math-symbology terms in the lang slice

The GMN symbology plane added ~95 new sign-inventory terms to `slices/grounding/lang/module.ttl` (number-type classes, set/order operator families, the extended real line and its poles, and their Form/Denotation records). Only **19** received authored fr + cmn translations — the number sufficient to restore the lang `axisTranslationCoverage` floor after the additions widened the localizable set. **~76 carry English labels/definitions with no translation.**

The `.po` `msgctxt "term|predicate"` keys in `slices/grounding/lang/i18n/{fr,zh}.po` are the authoring seam.

*Note:* the glyph-enrichment branch (PR 1626) is live in these same files and may absorb this.

### 3. The abductive-schema family

The 14 `logic:` abductive-schema terms (`logic:AbductiveSchema`, `logic:AbductiveRepairStrategy`, the four `logic:Strategy*` individuals, `logic:repairsDiscipline`, `logic:repairStrategy`, `logic:completenessFormula`, and the five `*AbductiveSchema` instances) carry a full `rdfs:label` + `skos:definition` and complete fr/zh translations, but raised the coverage ratchet baseline by **+5 on each of nine facets**: `missing-example`, `missing-fixture-pair`, `missing-worked-instance`, `missing-usage-advice`, `missing-scope-note`, `missing-competency-rationale`, `missing-annotation-coat`, `missing-prose-quality`, `missing-test-reach`.

The vocabulary is fully functional and demonstrated (the abductive wing fires on the real `gmeow validate` CLI over an under-specified A-Box) — the shortfall is documentation completeness, not capability.

### 4. The projection-ratchet registry

Adding the registry (2 new `gmeow:` properties + ~154 vocab/ceiling individuals) raised `docs/missing-annotation-coat` by +1 and `docs/missing-competency-rationale` by +15. Recorded at the time as "the annotation debt on the new terms is owed and should be burned down".

## Work

Per family, author the missing dimensions in the term's own home. For the abductive family specifically: `gmeow:example` / worked-instance fixtures, usage advice, scope notes, and competency rationale.

**Do not chase the ratchet number.** Every axis here is an objective intrinsic measure; the goal is real documentation of real terms, and the baseline burning down is the consequence, not the target.

## Acceptance

- The math slice's translation and information coverage rise on measured content, and its floors ratchet up behind them.
- The GMN symbology terms carry fr + cmn.
- The nine facets the abductive family regressed are covered for those 14 terms.


## Comments (3)

### coderabbitai — 2026-07-27T00:19:08Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#1318 - feat(lang): rendering, translation & paraphrase — translation as logic:Correspondence [merged]
Blackcat-Informatics/gmeow-ontology#1479 - feat(lang,logic,mentation): GMN-1 lossless witness, Cyc lineage, factored security lattice, mentation coverage (Closes `#1475`) [merged]
Blackcat-Informatics/gmeow-ontology#1486 - docs(lang): follow-through the landed graft framing; verify `#1440` acceptance [merged]
Blackcat-Informatics/gmeow-ontology#1494 - feat(slice-quality): honest en+fr+cmn translation-coverage measure + re-anchorable floors [merged]
Blackcat-Informatics/gmeow-ontology#1581 - feat(1534): fuzzy-gated translation coverage on one unified PO parser [merged]
Blackcat-Informatics/gmeow-ontology#1619 - feat(1493): complete logic fr/cmn translation coverage; remove parquet export [merged]
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

### paudley — 2026-10-21T17:01:37Z

PR #1927 adds a manual hosted path for the existing reviewed-translation golden generator. It checks out the dispatch source revision explicitly, runs the canonical Make target and uploads the produced JSON for review. Read-only permissions and disabled checkout credentials prevent source writes; a failed refresh cannot publish the old tracked golden.

Repository lint, including Actions/YAML/security checks, independent source review and normal commit/push hooks passed. No local full catalog refresh, corpus construction, sync or full gate was run. This changes one workflow file and leaves the native generator and authenticated frozen-golden consumer unchanged.

This enables the pending #1894/#1717 golden refresh after the whole canonical source and translations settle. Actual hosted generation, complete semantic diff review and a normal commit of produced bytes remain required. It does not claim content/facet completion, measured coverage improvement, floor ratcheting or completion of this issue. Principles 4 and 7.


### paudley — 2026-10-21T18:18:55Z

PR #1929 adds distinct usage, scope and worked-triple guidance for eight existing linear-algebra classes, paired with French and Chinese labels, definitions and guidance. Form versus basis-dependent representation, symmetry versus positive-definiteness, and exact rational inputs versus potentially irrational norm values are explicit. Existing axioms, definitions and bridges remain unchanged. Principles 4 and 7.

Repository lint, independent source review and both normal signed commit/push hook runs passed. The existing native generator succeeded in the [hosted reviewed-coverage refresh](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39648315997). Independent review reconciled its complete delta since the August golden against actual source history; the committed golden is byte-identical to that produced artifact. No local corpus construction or full gate was run under the recorded exception.

The refresh exposes older missing Chinese label/definition/how-to entries for 15 quality-profile/applicability terms: 45 contexts already present in French. These remain owned by this issue and are the next bounded translation unit. This PR does not claim measured coverage/floor ratcheting, complete translation quality, authenticated corpus acceptance or the remaining Math, GMN, abduction and registry facets of #1656.



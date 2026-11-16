PASS at b72f57a3f

# Delta recheck: issue 1896, branch paudley/1896-ground-evidence-and-trust-strengths

Written 2026-11-16 00:58 CST. HEAD is b72f57a3f, one commit over c803c4792, with 13 files changed.
This recheck covers the five REQUIRED findings in review-1896-at-c803c4792.md and any fault the
delta adds. Unchanged history was not re-judged. The issue body and comments were re-read with
`stagectl issue 1896`. The scope is unchanged: the issue closes on its terms content.

## Checks run (read-only)

- `git diff c803c4792..b72f57a3f`, all hunks read.
- `rapper -c`: provenance module.ttl (373 triples) and tests/structural.ttl (21) parse. The witness
  parses to exactly `gmeow:confidence logic:instantiatesFramework logic:EpistemicFramework`.
- The provenance module, converted to N-Triples, has 0 `gmeow:confidence logic:instantiatesFramework`
  triples.
- `msgfmt -c` on epistemics fr/zh, provenance fr/zh and standpoint fr/zh: exit 0 for all six. The
  only warnings are the existing missing header fields.
- Exact-line `grep -cxF 'msgid "<module literal>"'` gives 1 in fr and 1 in zh for each changed
  literal: the confidence definition, the confidence scope note and the supportStrength scope note.
  The msgctxt keys are unchanged, so reviewed_coverage_golden.json needs no change.
- The old wording ("No range is declared", "documented here and not asserted", "no probabilistic
  framework", "mapping to a probability model") is gone from both modules, both guides and all
  four catalogs. No reference to the old witness filename or the old assertion IRI is left.
- `prose-check` exits 0 for both docs.md guides, the commit message and the added Turtle lines.

## Findings 1 to 5

1. **Framework: fixed.** The `logic:instantiatesFramework logic:EpistemicFramework` triple is deleted.
   The definition, the scope note ("no framework is declared for this term, as for logic:confidence")
   and provenance/docs.md 85-86 now say so, and that is true: `logic:confidence` (logic/module.ttl
   2908-2913) declares no framework.

2. **Machine-readable link: fixed, and it cannot red a gate.** `logic:formalizes gmeow:confidence`
   now sits on `logic:confidence` (logic/module.ttl:2912). It is the right edge:
   - Its definition (module.ttl:18112) says: subject a `logic:` term, object a `gmeow:` term,
     authored in the grounding slice, an annotation with no DL or EL weight.
   - Precedent exists for a property-level or individual-level target, for example
     `logic:InfoSupported logic:formalizes gmeow:supportSupported`.
   - The logic manifest already declares `sliceDependsOn` provenance, so no dependency edge is
     undeclared.

   I traced every production reader of the edge:
   - **Projection-vocabulary ratchet** (slice-quality counting.rs:380): it only subtracts residue.
     The `logic:confidence` block contains no construct from a guarded vocabulary. Only
     `logic:DatatypeProperty`, labels and the definition are there.
   - **Harvest and advice coverage axes** (axes.rs 2057, 2084, 2333): they count only the subjects
     typed `logic:Constraint` with a severity, `logic:Formula` or `logic:AdviceGuidance`. A
     DatatypeProperty adds no credit.
   - **Advice-binding obligation** (obligations.rs:863): it also requires
     `logic:adviceSourceField`, so the new triple does not match.
   - **Guidance index**: it keys rule terms only for `gmeow:ValidationRule` subjects.
   - **Shape-grounding ledger**: it scans the projected constraint surfaces only.
   - **logic-compile** classifies `formalizes` as a reference target, not an axiom.
   - **docs-model** (model.rs 2266, 3708) renders "Formalized by logic:confidence" on
     `gmeow:confidence`. That is the purpose the edge's definition states.

   The term-content authority (term_manifest.rs) records a changed digest as a "Definition changed"
   release note, not as a failure. No snapshot pins the `logic:confidence` block. The scope note,
   docs.md 87-89 and both catalogs now state the link truthfully.

3. **Structural assertion and witness: fixed.** `ex:saConfidenceDeclaresNoFramework` (mustNot,
   scopeModule) now asks `gmeow:confidence logic:instantiatesFramework ?framework`.
   - Against the module alone the ASK is false, because no such triple exists, so the assertion
     passes.
   - Unioned with confidence-framework.ttl the ASK is true, so the teeth check trips.
   - The witness path is registered through `gmeow:saFailWitness`, which `registered_fail_witnesses`
     in repo_static.rs reads. The old file is deleted.
   - The rationale matches the pattern.

4. **supportStrength: fixed.** The module, docs.md and fr/zh now say "No interval is declared for its
   values". That is true beside `rdfs:range xsd:decimal`.

5. **Translations: fixed, and faithful.**
   - zh credence in epistemics and in both standpoint entries: 分级信念程度.
   - fr trust level: "niveau de confiance interpersonnelle accordé à un agent".
   - fr/zh of the new definition, scope note and supportStrength wording render the English
     faithfully, including "porté par logic:formalizes sur logic:confidence" and
     "由 logic:confidence 上的 logic:formalizes 承载".
   - zh now uses 担保 for "warrant", which also settles the old OPTIONAL-4.

No fault in behaviour or in a stated rule was introduced.

## REQUIRED

None.

## OPTIONAL

- provenance/docs.md 86-88: the reflowed lines run past the guide's usual wrap width (formatting only).
- OPTIONAL-1 from the prior review (name `math:declaredConfidenceMapping` in howToUse) is still open,
  as before.

## Not shown (unexecuted)

No cargo, make, producer, slicetest, reasoner, slice-quality or docs render ran. The ASK verdicts and
the gate-impact analysis come from reading the code by hand.

VERDICT: PASS

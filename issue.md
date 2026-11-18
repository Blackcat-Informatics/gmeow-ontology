# Issue #2916: Repair native catalogue technical-text classification and inconsistent labels

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, quality, terms

## Body

The native catalogue check reports nine errors on the current source. Six are
copied-English findings on preserved technical text: two Turtle examples in
French and Chinese and the R-hat notation label in both languages. Three are
real glossary inconsistencies: AbsenceReason/absenceReason in French and
Chinese, and ModelFormula/modelFormula in French.

The failures predate the PCA-output guidance batch. Its new eighty-four
translation pairs are outside every reported context. Apply Principles 4, 7,
11 and 19. Keep this repair separate from the unchanged PCA guidance contract.

Acceptance:

- Preserve RDF terms, machine-readable examples and mathematical notation.
  Correct technical-invariant classification through the existing native
  translation-integrity owner; do not translate code, add a term-specific
  suppression, weaken copied-prose detection or loosen configuration.
- Resolve the three genuine label inconsistencies with accurate, consistent
  French/Chinese terminology. Preserve unrelated catalogue entries and all
  canonical English terms, axioms and runtime profiles.
- Extend existing integrity behavior cases for the actual technical text and
  retained copied-English refusals. Add no parallel test target or broad matrix.
- Qualify the affected native checker and actual command. Run the native
  catalogue check with an authenticated producer for the changed implementation;
  it must report zero errors. Report remaining warnings accurately.
- Preserve current corpus, generated artifacts and measured floors. No test
  produces a corpus, no fixture selector is fabricated, and no CI or full
  producer-runtime success follows from a catalogue pass.

Observed contexts:

- core/gts: evidenceCorpusStale example in fr/zh.
- core/pipeline: flowReadsHandle example in fr/zh.
- grounding/math: RHat label in fr/zh; ModelFormula/modelFormula labels in fr.
- core/inquiry: AbsenceReason/absenceReason labels in fr/zh.

The current command reports nine errors and twenty-four warnings. The warnings
have separate stale/orphaned-entry causes and are not the errors counted here.


## Comments (0)


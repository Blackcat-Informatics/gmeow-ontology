# Issue #2024: Use native JSON for the complete correspondence axes report

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Replace the complete terminal correspondence axes report with native PurRDF JSON, preserving its existing public byte API and exact wire behavior. This is a complete bounded child of the native JSON migration in issues 1776 and 1777; parent consumer and production acceptance remains open. Principles 4 and 7.

Scope and acceptance:

- Route SourceAxes::report and its actual CompileLogicStage publication of generated/logic/correspondence-axes.json through the native fallible pretty writer.
- Cover both inline report shapes, every AxisResult variant and the complete CompletionFrontier record. Reuse existing native owners for full derived quads, preservation evidence, numeric literals and RDF terms. Retain shared Serde owners where still used.
- Preserve declaration order in every record and enum, ordered map/set entries, vector order, status tags, complete numeric literal metadata, raw strings, two-space indentation and no terminal newline. Do not apply recursive key sorting.
- Capture complete original report bytes before production edits. Extend the existing independently authored axes control to compare the full output and a bounded complete projection atlas literally, covering every result arm and nonempty frontier fields. Preserve its existing claim, derivation and repeated-output assertions. No witness hydration or semantic preparation change.
- Verify current callers and nested owners, independent source review, focused strict compilation of actual production consumers, the existing authored control, helper hygiene, static producer recipe and all normal signed commit hooks. Record exact scope and limitations.

Standing execution limits remain: no local optimized producer, canonical corpus or fixture construction, sync or full gate. Tiny independently authored controls are permitted. No fabricated corpus, installed assets or receipt. CI is explicitly waived by the user and must not be represented as passing. This issue does not close parent installed consumer, authenticated corpus, browser, release or complete migration acceptance.


## Comments (0)


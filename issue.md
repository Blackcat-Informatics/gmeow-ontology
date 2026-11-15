# Issue #2802: [term] Reasons a question has no answer: not found, retrieval failed, not published, not applicable

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, foundational, cross-cutting, module-enhancement, complexity-hard, terms

## Body

## Request

A consumer needs to record why a question has no answer, as data, without asserting a negative. Cases:

1. not found in the searched sources, with the scope that was searched;
2. retrieval failed (the source could not be fetched);
3. not published (the answer exists but its holder has not made it public);
4. not applicable (the question does not apply to this subject).

"Withheld" is already covered by `gmeow:DisclosureOutcome`.

## What exists

Absence is doctrine throughout the model (an absent value is unknown, never false), but there is no vocabulary for the reason. The nearest terms are each scoped to another domain: `gmeow:StructuredNull` (a source's own explicit null), `gmeow:recallVerdictNotFound` (an asker's verdict on a recall answer), `gmeow:lossValueMissing` (a query-input loss), `gmeow:dispositionTypedAbsence` (a pipeline stage's typed nothing), `gmeow:qualityNotApplicable` (a slice-quality axis), and `gmeow:verdictUndetermined`.

## Required

- A record of an unanswered question: which subject and which property (or question) it concerns, the reason, who determined it and when, as an attributed, dated observation and never a bare fact. It asserts no value and no negation, and nothing may derive falsity from it.
- A small set of reason individuals covering the cases above, each with a definition that distinguishes it from the others and from withholding. "Not found" carries the scope searched; "retrieval failed" can point at the attempted source. Whether "not published" and "not applicable" belong is to be decided on the merits: "not applicable" is a claim about the subject, which the others are not.
- The relation to the existing domain-scoped absence terms is stated, and where one of them is the same notion it is grounded in the new term instead of duplicated.
- A later answer does not contradict an earlier absence record: both are retained, each with its time.
- Worked examples for each reason, a competency question listing unanswered questions for a subject with their reasons, counter-examples showing that an absence record does not entail a negative, and translations.

Principles 4, 9.


## Comments (1)

### paudley — 2026-11-15T01:49:57Z

Blocks: #2795 (Release 0.4.0), by the maintainer's decision.


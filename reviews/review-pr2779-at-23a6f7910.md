# Independent review of PR 2779 at `23a6f7910` (delta `fdca3d769..23a6f7910`; coverage `04aa28a7e..23a6f7910`)

Saved by the primary session from the report of reviewer `a43ef2e63acffa3c2` (2026-11-14), condensed in
wording only; the full report is in the session transcript. By reading only; nothing built or run.

## Scope

Read: the full delta of `frontend.rs`, `meta_findings.rs`, `meta_findings.tests.rs`, the logic slice module
and both catalogs; `frontend/native_statement_tests.rs` from line 586 to the end; `ir.rs` (`LogicModality`),
`graphutil.rs`, `frontend/emission.rs`; `docs/PIPELINE_SPINE.md` 205-235. Consumer audit by search at the
head: every consumer of `owner_lowerings()`, `axiom_sources()`, `formula_lowerings()`,
`OwnerDisposition::Emitted`, of `.rules` in the pipeline, validate, the two command-line tools, the language
server and conformance, and of `rule_iri`. With the first report the readings cover the whole range.
Not read: the TPTP writer, the Datalog lowering, the runtime beyond `rule_iri` and `lower.rs`; the first 585
lines of the test file at this head.

## Evidence (summary)

R1 fixed: `meta_findings.rs` 344-382 keeps every emitted index per source node and refuses a node with more
than one before any rule is chosen; the new test fails without the fix (the old collect returns success and
the test unwraps the error). The audit holds: no other consumer picks one of several lowered rules or
claims. R2: confidence and modality validated per spelling, then counted by parsed value; `-0.0` folded;
`total_cmp`; the derived order of the modality; `term_identity` total, deterministic, independent of the
store; debug text a sound tiebreak while `Debug` stays derived. R3: the new tests pin what they name and can
fail without the fixes, except the two noted below. The default-modality asymmetry is the old single-value
behaviour applied per value and is right. The ambiguity message and the usage note with its catalog entries
are true and faithful.

## Findings

REQUIRED 1. `docs/PIPELINE_SPINE.md` 223-224 still says the lowered claims are ordered by the values'
canonical term text; the parsed-value counting made that false.

FOLLOW-UP: (2) the numeric-order test uses spellings whose text order equals their numeric order; (3) the
split-table test pins one placement only; (4) an IRI and a literal of one text are two values but one scope
text, so a node yields identical scopes and can be refused as ambiguous for no distinct scope; (5) untested:
literals differing in datatype or language, blank-node scope, a rule atom with spelling-equal confidence,
admission of `none` beside another modality on a rule node, invalid-value withholding on the classic
carrier; (6) an early return drops a second diagnostic on a node; (7) rule units collapse per node while
axiom units stay one per index; (8) the runtime rule identifier comes from provenance only, so rules fanned
on another coordinate share one identifier (untraced whether scoped rules reach execution); (9) a comment on
the debug-text tiebreak.

VERDICT: BLOCKED 23a6f7910

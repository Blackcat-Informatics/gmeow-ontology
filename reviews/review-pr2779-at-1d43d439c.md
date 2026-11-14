# Independent review of PR 2779 at `1d43d439c` (delta `23a6f7910..1d43d439c`; coverage `04aa28a7e..1d43d439c`)

Saved by the primary session from the report of reviewer `a43ef2e63acffa3c2` (2026-11-14), condensed in
wording only; the full report is in the session transcript. By reading only; nothing built or run; the tests
named below were not executed by the reviewer.

## Scope

Read: the full delta of `crates/logic-compile/src/frontend.rs`, `frontend/native_statement_tests.rs` and
`docs/PIPELINE_SPINE.md`. The surrounding functions were not re-read at this head; the call-site context is
the `scope_from_node` body read at `fdca3d769` and `23a6f7910`. With the two earlier reports the readings
cover `04aa28a7e..1d43d439c` (three commits; the test file's lines before 586 were read at `fdca3d769` and no
later hunk touches them).

## Evidence (summary)

The rule (values of a coordinate are distinct exactly when the scopes they produce are distinct) holds for
every case: `scope_values` still returns every distinct term in (kind, text, debug) order, so confidence and
modality validate each; `first_per_text` runs on standpoint, time, provenance and module before the
multi-valued count and the fan-out, and the shadowing `let` makes the count, the fan-out and the
two-coordinate refusal see the reduced vectors. An IRI and a literal of one text collapse (the IRI survives:
kind 0); literals of one lexical form and different datatype or language collapse (first by debug text,
fixed); blank nodes of one label collapse; both authoring orders give one result; a second multi-valued
coordinate beside them fans out and is not refused. Confidence and modality are not reduced by text: an IRI
"0.5" beside a literal "0.5" is still refused as an invalid confidence. Which term survives is not observable
in the scope (only the text enters it); it affects fan-out position only, which is fixed for a term set and
independent of the store.

Tests: the replaced test fails without the behaviour through its second half (provenance of one text beside
two standpoints was refused as ambiguous at `23a6f7910`); its first half would pass anyway because axiom
de-duplication hides identical scopes. The numeric-order test is now strong (text order would give the
reverse). The split-table test now covers both placements; one residual: both iterations intern `a` before
`b`, so if the store merges tables by term id the unsorted result is the same in both placements and the
test could not fail (store read order not confirmed by the reviewer).

Statements: the three doc comments and the document sentence are true, with no absolute the code does not
keep.

## Findings

REQUIRED: none.

FOLLOW-UP: (a) assert on a rule node that provenance given as an IRI and a literal of one text yields one
scope (the claim assertion is masked by axiom de-duplication); (b) intern `b` before `a` in one placement of
the split-table test; (c) test the collapse of literals differing in datatype or language and of blank nodes
of one label; (d) still open from the earlier lists: the early return that drops a second diagnostic on a
node, rule units collapsing per node while axiom units stay one per index, and the runtime rule identifier
derived from provenance only. Earlier follow-ups 2, 3 (but for (b)), 4 and 9 are resolved.

VERDICT: PASS 1d43d439c

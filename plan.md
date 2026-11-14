# Plan: lower a scope node that carries several values of one coordinate (issue 2777, PR 2779)

## Behavior

A statement reifier or rule node carrying several values of ONE scope coordinate is lowered to one scoped
claim or rule per distinct value, in an order that does not depend on the store. Values of a coordinate are
distinct exactly when the scopes they produce are distinct (text for standpoint, time, provenance, module;
parsed number for confidence; parsed kind for modality). Two or more multi-valued coordinates on one node are
refused, naming the node. A rule atom takes one value per coordinate; several are refused. A diagnostic
meta-rule node that would lower to several rules is refused in the pipeline's meta-findings stage.

## Dependencies

Base `main` at `04aa28a7e`. Files: `crates/logic-compile/src/frontend.rs`, `frontend/source_admission.rs`,
`frontend/native_statement_tests.rs`, `crates/pipeline/src/stages/meta_findings.rs` and its tests,
`docs/PIPELINE_SPINE.md`, `slices/grounding/logic/module.ttl` with `i18n/fr.po` and `i18n/zh.po`.

## Exclusions

The classical profile still does not decide a scoped claim (evidence coordinates and module scope are later
units). Rule atoms are not fanned out. No product of two multi-valued coordinates.

## Acceptance (from the issue)

1. The pinned refusal becomes a pair: two values yield two scoped claims; both orders give the same result.
2. A case for each scope field, and one for two multi-valued fields on one node.
3. A full pass of this repository's slices produces the same findings as before the change. NOT SHOWN by a
   pass of this head: see validation.md. The issue stays open for this line until a full pass of a head
   that contains the change shows it.

## Risk

Shared front end of the logic compiler; one consumer in the pipeline keyed rule lowerings by source node
(found by review, fixed). Open items found by review are tracked in issue 2785.

## User rulings

Verbatim, with dates, in `/home/paudley/Active/gmeow-ontology/.stage/STATUS.md` under "User rulings"
(rulings 1 to 24). Those that bind this merge:

- 2026-11-13: "You are forbidden from skipping skips, breaking trust, commiting fraud, lying about your work,
  hacking around quality gates - all things your predecessor did."
- 2026-11-13, merge rule chosen by Patrick: "No new red (Recommended)": every hosted check finished; each
  red check red on main for the same failing lines; CodeRabbit has completed and its comments are answered;
  focused local validation passed; anything beyond text gets an independent review.
- 2026-11-13: "A note about model capability ... if things require a deep check, or complex logic and your
  agents are not doing it, check it yourself using your higher tier model."

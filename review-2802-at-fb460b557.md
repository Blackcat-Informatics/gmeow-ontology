PASS at fb460b557

# Delivery recheck: issue 2802, absence records (inquiry slice)

## Scope

- Delta reviewed: 8f3ff6132..fb460b557. It is `slices/core/inquiry/tests/structural.ttl` only, +3/-3:
  - the rule-body path in `ex:saAbsenceNoRuleBody` is now `(logic:body|logic:negatedBody)`;
  - that assertion's rationale names both bodies;
  - `ex:saAbsenceNoFormulaOutsideConstraint` matches `?formula logic:relation ?p` without the type pattern.
- module.ttl is byte-unchanged since 8f3ff6132 (`git diff --quiet` succeeded). The worktree is clean and
  HEAD is fb460b557.
- The verdict covers the whole branch diff, origin/main (862638ad8) to fb460b557. Every required finding
  from the four earlier rounds (at 2c859ff0a, 71d81642b, 129c3e138 and 8f3ff6132) is resolved, and nothing
  outside this file changed in the delta.

## Required findings

None.

## Hand evaluation of the two assertions on the committed module

- **What the module scope reads.** For a non-grounding slice, a structural cell with `scopeModule` reads
  `module.ttl` only (`crates/slicetest/src/paths.rs:111-126`), plus its OWL/RDFS reader-view projection.
  Examples, mappings (`mappings/equivalences.ttl`) and the catalogs are not in scope, so an example or
  mapping cannot trip either assertion.
- **`saAbsenceNoRuleBody` (mustNot).**
  - The module contains no `logic:Rule`, `logic:body` or `logic:negatedBody` (grep count 0), so the ASK is
    false and the assertion passes.
  - Failing input: `ex:r a logic:Rule ; logic:negatedBody [ rdf:reifies <<( _:r gmeow:absenceReason _:x )>> ]`
    matches the alternation path and the triple-term pattern, so the ASK is true and the assertion fails.
    The same holds with `logic:body`.
  - The triple-term form is the one the repository's rules use for both kinds of body (refinement-cues
    module.ttl:1298 for `logic:negatedBody`). The harness evaluates it with a variable predicate in existing
    competency queries.
- **`saAbsenceNoFormulaOutsideConstraint` (mustNot), now untyped.**
  - I dumped the committed module to N-Triples and listed every subject with `logic:relation` set to one of
    the four absence properties. There are 9: absRAtom, absNGuard, absNAtom, absFGuard, absFAtom,
    absAqQuestionAtom, absAqPropertyAtom, absRur1 and absRur2.
  - All 9 are reachable from a `logic:integrity` root of a `logic:Constraint` along forall, exists, not, and,
    or, antecedent and consequent (0 unreached). No other node in scope (no rule atom, no law, no
    definition) carries such a `logic:relation`, so the ASK is false and the assertion passes.
  - Failing input: an untyped `ex:law logic:relation gmeow:absenceQuestion` reached from no constraint makes
    the ASK true, so the assertion fails. Dropping the type closes the gap noted as O2 last round.
- **Texts against checks.** `docs.md:313-317` ("no rule body and no formula outside a constraint … a rule or
  axiom authored in another slice is outside their reach") and the rationales at `structural.ttl:376` and
  `:398` now name exactly the routes checked, at module scope, and nothing more.

## Evidence

- The `r5-2802-*.log` files record HEAD fb460b557 with empty `status --porcelain` sections, at
  03:50:13-03:50:14. `structural.ttl` was last written at 03:47:35, before those runs.
  - `make lint` EXIT=0.
  - rapper 5/5 EXIT=0.
  - msgid: fr/zh 116 matched, 62 added literals, 0 problems.
  - prose-check EXIT=0.
- My own reruns at the committed head: `rapper -q -i turtle -c tests/structural.ttl` EXIT=0, and the module
  N-Triples dump EXIT=0, used for the reachability walk above.

## Optional findings (carried; none blocks the contract)

- The cross-slice relations are stated only in `docs.md`, which does not ship in `gmeow.gts`.
- The retrieval-failed source is mandatory where the issue says "can point at".
- The `directType` guards leave a future subclass of `gmeow:AbsenceRecord` outside all nine constraints.
- `saAbsenceReasonAtMostOne` mirrors the uniqueness constraint's own formula shape node by node.
- The assertions are module-scoped and cannot see a rule or axiom on these properties authored in another
  slice. This is stated in the texts.

## Not verified

- No harness, `make validate`, producer, gate or reasoning run (operator ruling; execution evidence cannot be
  obtained in this stage). The competency results, the structural ASK results and the constraint projections
  were worked by hand and from source, not executed by the slice harness or the pipeline.

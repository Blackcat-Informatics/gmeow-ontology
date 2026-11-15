# Delivery recheck: evaluating a claim that exists as a reified statement

- Head reviewed: `5b14a825df104607b025994d104f91917ec2d7ab` (`git rev-parse HEAD` in
  `.worktrees/2726-evaluate-a-reified-claim`); previous review at `a08f86c98`
  (`.stage/reviews/review-2726-at-a08f86c98.md`).
- Delta: `git diff a08f86c98 5b14a825d`, one commit, six files, 55 lines added and 40 removed, no
  axiom changes.
- Started 2026-11-14T21:13:23-06:00; written 2026-11-14T21:15:01-06:00 (`date -Is`).
- Mode: read-only. No make, cargo, build or test runs. `prose-check` on `docs.md` and the worked
  example: exit 0, no output (`.stage/reviews/prose-2726-5b14.log`).

## Coverage

| File | Lines | Commit | Read |
|---|---|---|---|
| slices/core/epistemics/module.ttl | full delta (684-697, 1264-1278) | 5b14a825d | yes |
| slices/core/epistemics/docs.md | 185-256 (whole section, delta included) | 5b14a825d | yes |
| slices/core/epistemics/examples/evaluating-a-reified-statement.ttl | delta (header 9-14); rest from the earlier review, unchanged | 5b14a825d | yes |
| slices/core/epistemics/tests/competency.ttl | delta (rationale); query and expected row unchanged and rechecked | 5b14a825d | yes |
| slices/core/epistemics/i18n/fr.po | full delta (French read in full) | 5b14a825d | yes |
| slices/core/epistemics/i18n/zh.po | full delta (structure and clauses checked) | 5b14a825d | yes |
| crates/pipeline/src/stages/statements.rs | 150-175, 320-390 (to confirm the shared minted reifier and that nothing rejects duplicates) | 5b14a825d | yes |
| Unchanged files: structural.ttl, example-conformance.ttl, counter-example, manifest | not reread; unchanged in the delta (`git diff --stat`) | n/a | not reread |
| The compiler-guard branch | none | n/a | NOT read: not in this worktree |

## (1) Earlier REQUIRED finding 1: resolved

Checked each statement against the code and the unchanged axioms.

- **The shared minted reifier.** The new text says that when a statement names no reifier, the
  compiler mints one from the triple alone, so two sources share it and it carries both sources'
  annotations. This is true: `statements.rs:361-364` calls `mint_reifier(&triple)` and
  `provenance.rs:444-450` derives it from the triple alone. `compile_statements`
  (statements.rs:163-168) sorts the cells by reifier and emits them all. It never rejects two cells
  that share one reifier, so both annotation sets land on the same node.
- **The precondition** (each act to be evaluated gets one named reifier; a shared or minted
  reifier is never linked) appears consistently in five places: the definition
  (`module.ttl:690`), the avoidWhen (:693), the howToUse (:695), the functionality record (:1269),
  and the docs paragraph (`docs.md:213-220`). It agrees with the unchanged functional axiom,
  because the bound now constrains only reifiers that meet the precondition.
- **The stated limit is accurate.** A statement that has only the compiler-minted shared reifier
  cannot be evaluated per source through this bridge until each act gets its own named reifier
  (`docs.md:217-219`: "do not expect to evaluate one source's claim through it: that cannot be done
  per source. Give each act ... its own named reifier"). RDF 1.2 does allow several reifiers for
  one triple term, so the remedy is real.
- **"Evaluating a token judges the whole act"** is stated in the definition, the howToUse, the
  record, the docs table and paragraph (`docs.md:211`, :222-224), and the competency rationale. It
  follows from `evaluates` targeting a ClaimToken and from the property not being inverse
  functional. The record no longer says the opposite.
- **Competency rationale.** It now says one row per linked statement and that the example's act
  put forward one statement. This matches the unchanged query: it returns one row per
  `(evaluation, statement, triple, reason)`, which is exactly one for the example. The expected
  row is unchanged and still correct.
- **Example header.** "The reifier is named here and belongs to Alice's act alone" is true:
  `ex:skyStatement` is named and carries one `accordingTo`.
- **Contradictions.** None found. `docs.md:201-206` (mint a token for the act and link the reifier)
  is followed straight away by the precondition, so the section reads consistently.

## (2) Catalogs: correct

- The four replaced literals each have one fr and one zh entry. Running `grep -F` on each
  `msgid` finds every one, byte for byte, as an `@x-gmeow-english` literal in `module.ttl`. That
  holds for all 17 entries in each catalog.
- The old entries for the four replaced literals were removed, so no orphans remain. Both catalogs
  have 140 `msgctxt` lines and no duplicate context.
- The French carries every clause of all four: precondition, shared and minted reifier, the
  remedy of a separate reifier per act, coexistence, not inverse functional, and the whole-act
  judgment.
- The Chinese carries the same clauses with the slice's term 具体化者 for reifier.

## (3) Earlier REQUIRED finding 2 (the shape in the W3C namespace) and merge ordering

At this head the axiom pair is unchanged (`logic:domain rdf:Statement` plus the functional
record), so the unguarded compiler would still emit `rdf:Statement-shape`. Nothing in this branch
depends on that shape:

- `git grep "Statement-shape"` over `slices`, `crates` and `tests` at 5b14a825d finds nothing.
- The conformance cell pins `gmeow:ClaimEvaluation-shape`, which the guard does not touch.
- The structural cells test the `logic:` declarations, not derived shapes.
- The scopeNote says the bound is carried by the functionality record. That stays true.
- The at-most-one bound remains enforced by the property-level
  `gmeow:putForwardByClaim-domain-shape`, which the guard leaves in place
  (frontend.rs:3828-3842).

Once the guard is on `main` and merged into this branch, I expect no test or text here to fail.
If this branch merged first, it would ship the foreign-namespace shape. So the ordering is the
only remaining condition. I did not read the guard branch, so whether it guards both the
functional cap and the inverse-functional cap is not verified here.

## (4) Rest of the delta and findings

Findings:

- **OPTIONAL O1.** The definition (`module.ttl:690`) says "A reifier shared by several sources,
  including the content-addressed reifier the statement compiler mints". This reads as if a minted
  reifier is always shared. With one source it is not shared yet, but its triple-only identity
  means a later source would join it. The avoidWhen's "such as" is the more precise form. The rule
  ("never link it") is sound either way.
- **OPTIONAL O2.** The record (`module.ttl:1269`) says "the assertion act behind a reified
  statement is never ambiguous". "A reifier" would be exact, since one triple may have several
  reifiers, each linked to a different act.
- **OPTIONAL O3.** Nothing can detect the precondition in data. A shared reifier linked to only
  one token passes every check. A reifier that carries `putForwardByClaim` and more than one
  `accordingTo` or `wasAttributedTo` value could get a warning constraint. The contract does not
  require this.
- **OPTIONAL O4 (inherited, unchanged).** The record's useWhen says "names exactly one value"
  where the property means at most one. The French has "lorsque une", which should elide. Both
  are boilerplate shared with the 18 neighbouring records.
- Earlier OPTIONAL F3 to F5 (the retained displayable sentence, which belongs to the display work;
  closed-world-only refusal; `rdf:Statement` typing) are unchanged and remain optional.
- The commit message is accurate. The added lines contain no process references.

No REQUIRED findings in this delta. Earlier REQUIRED finding 1 is resolved. Earlier REQUIRED
finding 2 remains, by agreement, on the compiler guard branch.

## What only a producer pass can prove

The same items as before: the two conformance cells, the competency row and structural verdicts
under the real harness, purrdf parsing of the triple-term files, and, after the guard merges, that
the generated SHACL holds no `rdf:Statement-shape`.

VERDICT: PASS CONDITIONAL ON the compiler guard merging first, at 5b14a825df104607b025994d104f91917ec2d7ab

VERDICT: PASS c5053878f8806982311ab330a0be2247faf6dee9

# Review of PR 2689 at c5053878f (issue 2684: a frame relation between standpoints that does not narrow)

Read-only review from `.worktrees/2684-vocabulary-a-frame-relation-between` (HEAD equals the stated head; merge base with origin/main `f71169a78`, so the three-dot diff is the PR's own change: `module.ttl` +14, `docs.md` +11, `i18n/fr.po` +24, `i18n/zh.po` +24). Nothing was built or run.

## Requirement and delivery

Issue 2684: a frame relation between a standpoint and the context its readings were made within, which does not narrow the standpoint; proposed term `madeWithinContextOf`, relating a standpoint that exists for reading works to the frame whose context its readings were made within; explicitly not a sharpening (the subject does not become a case of the object); for standpoints that exist for reading works, not for a person who holds claims about other things as well.

| Requirement | State |
|---|---|
| Term `madeWithinContextOf` between a reading standpoint and its frame | Met. `module.ttl:174-186`; `logic:domain` and `logic:range gmeow:Standpoint`. No class for a reading standpoint exists (grep for Reading/Reader/Interpret/Critic/Reception classes finds none; `gmeow:ReadingOrder` at `slices/core/documents/module.ttl:455` is a different notion, as `howToUse` says), so the restriction to reading standpoints is carried by prose. The issue does not ask for a class. |
| Does not narrow; not a sharpening; subject does not become a case of the object | Met. No `logic:subPropertyOf gmeow:sharpens`, no characteristic, definition and `avoidWhen` say so; docs.md row and anchor agree. |
| Not for a person who holds claims about other things | Met in prose (definition, `avoidWhen`, docs.md anchor), with `gmeow:accordingTo` named as the right relation for such a holder. |
| Two standpoints may share a context without one sharpening the other | Met: nothing entails a `sharpens` edge. |

## Prior REQUIRED finding (`review-pr2689-at-2b358d964.md`)

R1 (authored `rdfs:domain`/`rdfs:range` instead of `logic:`): fixed. `module.ttl:176` is `logic:domain gmeow:Standpoint`, `:179` is `logic:range gmeow:Standpoint`; the grep of added lines for `rdfs:domain`, `rdfs:range`, `owl:`, `sh:`, `rdfs:subClassOf`, `rdfs:subPropertyOf` returns nothing. No other REQUIRED finding existed.

## Review feedback

None on this pull request: the only comment is the automated reviewer's "review limit reached" notice and there are no inline comments (checked with the API).

## Ontology rules

- Term has `rdfs:label`, `skos:definition`, `rdfs:isDefinedBy`, plus `skos:example`, `useWhen`, `avoidWhen`, `howToUse`, `graphBoxRole`.
- No shape, no `owl:`/`rdfs:` authoring spelling, no cardinality, no subsumption triple added (so the projection-vocabulary ratchet should not move; not run).
- No single-winner preference or ordering is introduced; `avoidWhen` forbids reading two standpoints with the same object as ordered or equal.
- No issue or PR numbers or process wording in any added line (grep).
- Definition, `avoidWhen`, `howToUse`, `useWhen`, the example and docs.md agree on the meaning, the non-narrowing rule, the undeclared characteristics and the reading-standpoint restriction. They agree with what the axioms do (there are none beyond domain and range).

## Translations

Read-only comparison (script outside the repository) of every added `@x-gmeow-english` literal against both catalogs: 6 literals (label, definition, example, `avoidWhen`, `howToUse`, `useWhen`); each has an entry in `fr.po` and `zh.po` whose `msgctxt` names the term and whose `msgid` equals the English literal exactly, each with a non-empty `msgstr`; no catalog `msgid` lacks an English literal. The `msgstr` text follows the English clause for clause (read in full for both languages). "门" for "gate" is the catalog convention.

## OPTIONAL findings

- O1. `module.ttl:182` (and the French/Chinese texts): the definition says "it is not transitive ... not symmetric" as facts while it also says no characteristic is declared; an undeclared characteristic does not make a property non-transitive in an open world. "is not declared transitive or symmetric, and a reader should not close or mirror it" would stop a reader taking it for an axiom. The same applies to docs.md ("not transitive, symmetric or irreflexive").
- O2. The definition is long and mixes meaning, design rationale and usage advice that `avoidWhen` and `howToUse` already carry; the non-transitivity sentence ("a frame that supplied the context for one set of readings does not thereby supply it for the readings made within that frame's own context") is hard to follow. A chain form (A read within B, B within C: nothing says A was read within C) is clearer.
- O3. "a gate must not reject" in the irreflexivity sentence refers to a mechanism the term does not use; say what withholding irreflexivity means for a consumer.
- O4. `fr.po` `useWhen`: "sans que ni l'un ni l'autre soit un cas plus affiné" omits "ne" before the subjunctive ("ni l'un ni l'autre ne soit"); changing it needs no English change.
- O5. No structural assertion or example protects the central rule (no `subPropertyOf gmeow:sharpens`, no transitive characteristic); `tests/structural.ttl` has MUST-NOT patterns that could carry it. The issue asks for none and the terms-only practice applies.
- O6. With a `Standpoint` domain a person wrongly used as subject is entailed to be a `Standpoint`; nothing in the slice declares `Standpoint` disjoint from `Agent`, so only the prose catches it.

## Limits (only a full producer run can show)

Turtle parse in the pipeline parser (the PR cites `rapper`, 1307 triples); the annotation-completeness gate; the derived shapes for `logic:domain`/`logic:range`; reasoning consistency (the entailment that subject and object are `gmeow:Standpoint`, interaction with `gmeow:ReadingOrder` as a subclass); slice-quality floors; the translation gates (`msgfmt -c`, keying); hosted generation jobs, which fail on main as well.

## Coverage

Read at the head (worktree `.worktrees/2684-vocabulary-a-frame-relation-between`):
- `/home/paudley/.claude/skills/rdf-work/SKILL.md` 1-111; `/home/paudley/.claude/skills/rust-work/SKILL.md` 1-90 (no Rust in this PR).
- Issue 2684 (body, no comments); PR 2689 body, top-level comment, inline-comment list (empty).
- `.stage/recovery/reviews/review-pr2689-at-2b358d964.md` (all).
- `git diff origin/main...HEAD` for `slices/core/standpoint/module.ttl` (the whole added block, lines 174-186), `docs.md` (added lines 16-45), and the `+msgstr` lines of `fr.po` and `zh.po`.
- `slices/core/documents/module.ttl` 455-467 (`gmeow:ReadingOrder`); grep of classes by name and of disjointness sets across slices.
- AGENTS.md, CLAUDE.md, `.goals`, `.baseline` as loaded in the session context; `CONSTITUTION.md` Principles 9 and 10 (lines 155-215, read for the sibling PR).
- Script `/tmp/review-scripts/po_check.py` (outside the repository).

Not read: the rest of `docs.md` and `module.ttl`; `tests/structural.ttl` beyond a grep; the French and Chinese catalogs beyond the added entries; CI logs; generated artifacts.

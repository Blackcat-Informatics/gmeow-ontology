# Requirement candidates — issue #1652

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 3 | modal | Blocked by: #1642 — which owns the adjacent question of whether the ratchet should net a relocation across its source and destination cells. |
| B2 | body | 15 | scope-out, question | Worth revisiting now the bootstrap is past: are domain/range and declarative OWL genuinely out of scope, or were they excluded to get the ratchet landed? |
| B3 | body | 27 | checkbox | - [ ] Decide whether `rdfs:domain`/`range` join the counted set now the bootstrap is resolved. |
| B4 | body | 28 | checkbox | - [ ] Decide whether declarative OWL gets its own count kind or stays with `reason-verify`. |
| B5 | body | 29 | checkbox | - [ ] Triage the 124 seeded cells: which burn down by grounding, which need re-homing, which are permanent. |
| B6 | body | 30 | checkbox, modal | - [ ] Give the shippable surface a measured/headroom view, or state why measurement must stay repo-anchored. |
| C1.1 | comment 1 (coderabbitai, 2026-07-27T00:17:43Z) | 18 | checkbox | - [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan |
| C2.1 | comment 2 (paudley, 2026-07-27T07:10:55Z) | 1 | modal | Adjacency worth naming so these two do not get worked as one: #1201 generalizes the **projection-purity gate** (no hand-authored construct that belongs to a generated surface), while this issue widens the **projection-vocabulary ratchet** (which predicates count toward a slice's ungrounded-residue ceiling). Different mechanisms — a purity violation is categorical, a ratchet breach is quantitative — but they cover overlapping vocabulary, so whoever takes either should read the other first. In particular, a decision on #1201 about which generated-formalism constructs are authored-forbidden constrains what this issue can sensibly count. |
| C3.1 | comment 3 (paudley, 2026-09-05T03:05:23Z) | 5 | modal | > Blocked by: #1642 — which owns the adjacent question of whether the ratchet should net a relocation across its source and destination cells. |

## Comments that matched no rule

- comment 4 (paudley, 2026-09-06T02:51:21Z)

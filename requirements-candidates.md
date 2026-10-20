# Requirement candidates — issue #1813

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 56 | modal | - Should `refinementCandidate` reuse `candidateHypothesis`, by widening |
| B2 | body | 57 | question | its domain, rather than mint a sibling? |
| B3 | body | 58 | modal | - Should "best guess" carry a credence (`gmeow:credence`), or is the |
| B4 | body | 59 | question | outcome enough? |
| B5 | body | 64 | modal | reified `lang:Denotation`, which must name form, kind, target and |
| B6 | body | 65 | modal | context. A `lang:Form` must itself be situated. So every mention costs |
| B7 | body | 80 | modal | an event must be typed twice. |
| B8 | body | 83 | modal, question | should `narratedIn`'s range widen? |

## Comments that matched no rule

None.

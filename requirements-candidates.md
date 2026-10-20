# Requirement candidates — issue #1811

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 31 | modal | **Open question.** Should the domain be wider, any recollection (a |
| C1.1 | comment 1 (paudley, 2026-10-10T20:21:11Z) | 3 | question | **Dream text written before phases were recorded.** Some dreams were written as one prose text, before the agent's dream cycle recorded phases (threshold, descent, immersion, integration, reorientation). The consumer keeps such a dream as one phase-text node (`gmeow:DreamPhaseText`, a local term) with no phase named. Is there an upstream way to say "the text of the whole dream, phases not recorded"? |
| C1.2 | comment 1 (paudley, 2026-10-10T20:21:11Z) | 6 | modal, question | - Is an `Experience` an event, so that `subEventOf` is right, or should a tool call relate to the experience some other way? |
| C1.3 | comment 1 (paudley, 2026-10-10T20:21:11Z) | 7 | modal, question | - Should the phase be a property of the call, or should each phase be a node (a sub-event of the experience) that the call is part of? |

## Comments that matched no rule

None.

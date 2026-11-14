# Review feedback and dispositions, PR 2779

## Independent reviewer (four rounds; files in `reviews/`)

| Head | Verdict | Required findings | Disposition |
|---|---|---|---|
| `fdca3d769` | BLOCKED | meta-findings stage picked one of several lowered rules; values counted by text; named behaviours untested | fixed in `23a6f7910` |
| `23a6f7910` | BLOCKED | pipeline document sentence on ordering false after the fix | fixed in `1d43d439c` |
| `1d43d439c` | PASS | none | |
| `c31bd05a1` | BLOCKED | rule-atom diagnostic contradicted the corrected usage note | fixed in `46902b851` |
| `46902b851` | PASS | none | merging head |

Open follow-ups from all rounds: issue 2785.

## Coordinating reviewer

Read `fdca3d769`, then each code delta to `46902b851`. Required at `fdca3d769`: count confidence and
modality by parsed value (fixed in `23a6f7910`). Required at `23a6f7910`: values distinct exactly when
their scopes are (fixed in `1d43d439c`). Statement at `46902b851`: every production line read at the
merging head; nothing further owed before the merge.

## Review bot

One review, at `fdca3d769`, two inline comments; none at any later head (rate limit). Both answered in
their threads.

| Comment | Subject (my words) | Disposition |
|---|---|---|
| 4574210567 | the two-coordinate refusal told rule authors to write statement reifiers | fixed in `23a6f7910` ("one reifier or rule per intended combination") |
| 4574210575 | the usage note told authors to move a rule atom's values to the rule node | accepted; note fixed in `c31bd05a1`, diagnostic brought into line in `46902b851` |

The bot has not reviewed the merging head. Under the merge rule that condition is Patrick's to waive.

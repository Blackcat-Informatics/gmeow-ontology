# Referenced artifacts — issue #1796

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `slices/…/finance/module.ttl` | NO | repo | — | — | — |
| `crates/logic/src/physical/seminaive/joint.rs` | yes | repo | file | 1776 | SPDX: AGPL-3.0-only |
| `LOGIC-REFERENCES.md` | NO | repo | — | — | — |
| `LOGIC-PERFORMANCE.md` | NO | repo | — | — | — |
| `/finance/module.ttl` | NO | absolute | — | — | — |

### History: `crates/logic/src/physical/seminaive/joint.rs`

- `a0b0deb02 Execute native node-kind integrity checks`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `96b1ac63a fix(1757): name the obstruction that withholds completed native reads`
- `61fc549fb fix(1757): admit definition writes that only re-derive asserted statements`
- `3238773a2 fix(1757): admit clash commitments under a source-bound contextual grammar`
- `bce830888 fix(1757): name the write that changes a selected definition grammar`
- `12e5c4f8a feat(1796): apply Datalog-first to every chase executor and producer`
- `0e9a889a4 feat(1796): run the restricted chase Datalog-first`

## Identifiers (files containing each at HEAD)

- `module.ttl`: 555
- `Posting`: 38
- `JournalEntry`: 40

## Workspace packages named

None.

## URLs

None.

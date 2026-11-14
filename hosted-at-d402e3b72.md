
## Hosted checks at d402e3b72 (the head that merges; main merged in after the sibling standpoint term landed)

Green: `helpers`, `lint`, `producer-build`, `rust-prebuild`.

Red, each red on `main` for the same cause (the producer does not finish on the hosted runner):

| Check | This head | `main` |
|---|---|---|
| `generation (a)` | The operation was canceled | f71169a78: The operation was canceled |
| `generation (b)` | The operation was canceled | f71169a78: Process completed with exit code 143 |
| `fixture-prefix` | The operation was canceled | 04aa28a7e: runner lost in "Produce optimized corpus fixtures" |
| `quality` | a quality job did not succeed (skipped) | the same line on 04aa28a7e |

Independent review: PASS at 98bf53bad (the change) and PASS at this exact head (the merge of main, which
adds only the sibling term).

Not shown: no producer run, so whether the three constraints lower and fire, and the pinned count of 24
characteristic shapes in the fixture-backed test, are unconfirmed. No counter-example exercises the
constraints.

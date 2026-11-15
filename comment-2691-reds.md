Hosted checks at head `c75d1c6e4`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`, CodeRabbit.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner):

| Check | This head (`c75d1c6e4`) | `main` (`c5317f3bb`, the latest commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `generation (b)` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `fixture-prefix` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`: the competency questions this change adds and edits have not been executed by any run. Their expected rows were worked by hand in the independent review.

Independent review: PASS at `c75d1c6e4`. The "Hosted checks" section of the description above describes earlier heads; this comment is the record for the merging head.

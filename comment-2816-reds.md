Hosted checks at head `2f72db97d`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed or runs out of time before it finishes):

| Check | This head (`2f72db97d`) | `main` (`af68f2dde`, a commit with a finished run) |
|---|---|---|
| `generation (a)` | cancelled: "The job has exceeded the maximum execution time of 2h0m0s" | failure: "The operation was canceled." |
| `generation (b)` | failure: "The operation was canceled." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`.

The review bot skipped this pull request and left no comment. Independent review: PASS at `c0bc3c10e`; this head is that commit merged with `main`, with no conflict.

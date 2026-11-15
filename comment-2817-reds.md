Hosted checks at head `417fd45d7`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`417fd45d7`) | `main` (`af68f2dde`, a commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `generation (b)` | failure: "The hosted runner lost communication with the server." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`: no hosted run has executed the Rust test shards, so the tests of this change have only the local results in the description.

The review bot skipped this pull request and left no comment. Independent review: PASS at `417fd45d7`.

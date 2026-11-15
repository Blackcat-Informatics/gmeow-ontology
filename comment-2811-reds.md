Hosted checks at head `5cbfacdaf`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`5cbfacdaf`) | `main` (`af68f2dde`, the latest commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `generation (b)` | failure: "The hosted runner lost communication with the server." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`: no run has executed the competency question, the structural assertions or the lowering of the four required paths.

The review bot skipped this pull request and left no comment. Independent review: PASS at `5cbfacdaf`.

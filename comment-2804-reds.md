Hosted checks at head `22d0a0ffb`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`22d0a0ffb`) | `main` (`c5317f3bb`, a commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `generation (b)` | cancelled: "The job has exceeded the maximum execution time of 2h0m0s" | failure: "The operation was canceled." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`: no run has executed the structural assertions, the competency question, the example validation or the lowering of the revised constraint at this head. They were worked by hand in the independent review.

Independent review: PASS at `22d0a0ffb`.

Hosted checks at head `9154edf9d`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner, so the generation jobs die and everything after them is skipped):

| Check | This head (`9154edf9d`) | `main` (`c5317f3bb`, the latest commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `generation (b)` | cancelled: "The job has exceeded the maximum execution time of 2h0m0s" | failure: "The operation was canceled." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The three producer jobs end with different runner messages from run to run (cancelled, time limit, lost communication); in each case the job is killed before the producer finishes, which is the failure on `main`. The annotations do not carry a failing source line for either commit. This change touches only `AGENTS.md`, which no job reads.

Not shown: nothing downstream of the generation jobs ran (they are skipped here and on `main`).

Independent review: PASS at `9154edf9d`.

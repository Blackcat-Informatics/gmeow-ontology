Hosted checks at head `f807002e4`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed or runs out of time before it finishes):

| Check | This head (`f807002e4`) | `main` (`af68f2dde`, a commit with a finished run) |
|---|---|---|
| `generation (a)` | cancelled: "The job has exceeded the maximum execution time of 2h0m0s" | failure: "The operation was canceled." |
| `generation (b)` | failure: "The operation was canceled." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`: no run has executed the competency questions or structural assertions, compiled the eight alignment cells, or scored the slice.

The review finding (value links belong in the mapping source) is done in `fec97ae99` and answered in its thread. Independent review: PASS at `f807002e4`.

**Slice-quality floors** for the evidence slice, computed from shell copies of the scoring code that each reproduce a committed floor exactly at a known commit, not from a gate run: prose quality 0.8333 against a floor of 0.8, translation coverage 0.4082 against 0.3333, documentation maturity 0.75 against 0.75 (no margin, as on `main`). GMN-1 coverage needs the codec and is not measured.

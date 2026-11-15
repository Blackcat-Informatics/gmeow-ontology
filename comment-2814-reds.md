Hosted checks at head `fb460b557`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`fb460b557`) | `main` (`af68f2dde`, a commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `generation (b)` | failure: "The hosted runner lost communication with the server." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`: no run has executed the competency questions, the structural assertions or the lowering of the nine constraints.

The review bot skipped this pull request and left no comment. Independent review: PASS at `fb460b557`.

**Slice-quality floors**, computed from shell copies of the scoring code that each reproduce a committed floor exactly at a known commit, not from a gate run: the inquiry slice holds every floor that can be computed by reading: prose quality 0.9167 against a floor of 0.8846, translation coverage 0.6619 against 0.4074, documentation maturity 0.75 against 0.6667. GMN-1 coverage needs the codec and is not measured.

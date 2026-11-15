Hosted checks at head `651cb3ff3`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`651cb3ff3`) | `main` (`c5317f3bb`, a commit with a finished run; `b2693bb12` shows the same jobs failing so far) |
|---|---|---|
| `generation (a)` | failure: runner lost communication with the server | failure: "The operation was canceled." |
| `generation (b)` | failure: runner operation was canceled | failure: "The operation was canceled." |
| `fixture-prefix` | failure: runner operation was canceled | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`, so no run has executed the slice specifications, conformance or the Rust test shards at this head.

Independent review: PASS at `651cb3ff3`.

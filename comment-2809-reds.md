Hosted checks at head `6fc9f8142`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`6fc9f8142`) | `main` (`af68f2dde`, the latest commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `generation (b)` | failure: "The hosted runner lost communication with the server." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The operation was canceled." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit. Nothing downstream of the generation jobs ran, here or on `main`, so no run has shown that the nine pin-agreement findings clear.

The review bot left a review summary and no inline finding. The change is one line of prose.

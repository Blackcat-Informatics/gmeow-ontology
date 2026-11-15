Hosted checks at head `8cd4e7e46`, all finished.

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the hosted runner; the job is killed before it finishes):

| Check | This head (`8cd4e7e46`) | `main` (`af68f2dde`, a commit with a finished run) |
|---|---|---|
| `generation (a)` | cancelled: "The job has exceeded the maximum execution time of 2h0m0s" (at `13dccbeb7`: runner lost communication) | failure: "The operation was canceled." |
| `generation (b)` | failure: "The operation was canceled." | failure: "The hosted runner lost communication with the server." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line. Nothing downstream of the generation jobs ran, here or on `main`.

**What the runs measured.** Two hosted samples of this change (runs 43672812964 and 43693277023) agree: 43 returns per job cost about 5 s in total and give back 33 to 36 GiB; after `stage-mappings` resident memory falls from about 12.4 GiB to 5.6; after `reason-all` from about 10.5 to 6.6. Both generation jobs now run past reasoning, which they did not before, and are lost in `stage-snapshot`: `materialize-carrier-with-verdicts` ends at about 13.3 GiB live and `canonicalize-coherence-carrier` starts, then the runner is gone. So this change moves the failure point and does not make the pass fit; the next reductions are in the snapshot stage.

Independent review: PASS at `6752772c3`. The two commits since then (`13dccbeb7` and `8cd4e7e46`) change only comments and the optimization guide, each answering a review-bot finding in its thread.

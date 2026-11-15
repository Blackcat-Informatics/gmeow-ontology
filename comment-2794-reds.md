Hosted checks at head `b17c4b8e9`, all finished (run 43650867006).

**Pass here and on `main`:** `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`. `producer-build` failed at the earlier head `d1ec274f9` for a cause that was this branch's own (the source-input scanner refused a macro); it passes at this head.

**Red here, and red on `main` for the same cause** (the producer pass does not fit the 16 GiB hosted runner):

| Check | This head (`b17c4b8e9`) | `main` (`c5317f3bb`, the latest commit with a finished run) |
|---|---|---|
| `generation (a)` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `generation (b)` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `fixture-prefix` | failure: "The hosted runner lost communication with the server." | failure: "The operation was canceled." |
| `quality` | failure: "a quality job did not succeed (skipped)" | failure: "a quality job did not succeed (skipped)" |

The annotations carry no failing source line for either commit; in each case the producer job is killed before it finishes.

## What the run measured

The per-stage memory record did its job: the runner died, the upload step never ran (so there is no resource artifact), and the records are in the job log, written as they were made. 360 records from `generation (a)`, 295 from `generation (b)`.

`generation (a)`, stage `stage-reason`:

| Phase | Start (UTC) | End | Resident at start | Resident at end | Lifetime peak |
|---|---|---|---|---|---|
| `assemble-object-edb` | 23:29:09 | 23:29:34 | 3.94 GiB | 4.68 GiB | 5.88 GiB |
| `canonicalize-edb` | 23:29:34 | 23:29:42 | 4.68 GiB | 4.68 GiB | 5.88 GiB |
| `prepare-reasoning-input` | 23:29:43 | 23:29:50 | 4.69 GiB | 6.21 GiB | 6.21 GiB |
| `reason-all` | 23:29:50 | 00:36:14 | 4.35 GiB | 13.82 GiB | 14.66 GiB |
| `project-inferred-closure` | 00:36:15 | 00:36:25 | 13.82 GiB | 14.02 GiB | 14.66 GiB |
| `project-explanations` | 00:36:25 | (job lost at 00:36:26) | 14.02 GiB | | |

So on the hosted runner `reason-all` runs for 66 minutes and finishes, holding 13.8 GiB with a peak of 14.7 GiB, and the job dies seconds later while projecting the reasoning artifacts from that result. `generation (b)` was lost inside `reason-all`, 47 minutes in, with no boundary record after its start (4.38 GiB).

The early release of the asserted dataset is visible: resident memory falls from 6.21 GiB to 4.35 GiB between the end of `prepare-reasoning-input` and the start of `reason-all`.

Not shown by this run: anything after `stage-reason` (the snapshot and every later stage never ran), and resident memory inside `reason-all` between its two boundaries.

Independent review: PASS at `b17c4b8e9`.

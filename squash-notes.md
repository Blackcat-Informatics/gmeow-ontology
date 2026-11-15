perf(pipeline): record memory per stage from a killed producer run and remove three wastes (Principles 4, 7)

A producer run now writes one memory record at each stage's dispatch, publication and carrier release, and
at each measured phase boundary: process id, resident size, lifetime peak resident size, and the product's
size. When `GMEOW_STAGE_MEMORY_LOG` names a file, each record is one unbuffered append to it, whatever the
stderr log filter selects, so a run that is killed still leaves every record made before the kill. A log
file that cannot be opened is a typed error and a non-zero exit. The records are observations only: they
enter no product, key, receipt or digest.

Three wastes are removed without changing any emitted byte. The reasoning stage computes the expression
identity edges before the engine runs and drops the asserted dataset then, instead of holding it through
reasoning. The reasoning receipt text is encoded once and reused where it was encoded again. The gate
verdict stage reads the composed view instead of materializing its own copy.

Verified: independent review PASS at b17c4b8e9, with the production diff read in full; the named tests,
strict clippy on the touched crates and the source-input scanner passed locally before the last change, and
the scanner passes at this head. Pull request checks: helpers, lint, producer-build and rust-prebuild green;
the three producer jobs and the aggregate red as on main, because the producer does not fit the hosted
runner. The hosted run is the first per-stage measurement: on the 16 GiB runner `reason-all` runs 66 minutes
and ends holding 13.8 GiB with a peak of 14.7 GiB, and the job is lost seconds later while projecting the
reasoning artifacts; resident memory falls from 6.2 GiB to 4.4 GiB when the asserted dataset is dropped.

Not shown: no producer pass has completed with this change, so byte identity of the produced bundle rests
on the review's reading and on the receipt comparison test, not on a produced bundle. No test checks that
records reach the memory log under a `warn` stderr filter.

Closes #2702.

perf(pipeline): return freed memory when a stage completes, and record what each return gave back (Principles 4, 7)

The producer's resident size stayed near its high-water mark after a stage freed its working memory, because
the allocator kept the freed pages. The scheduler now returns freed pages to the system once after each
stage's publication and the releases it caused, and the reasoning stage does so after its input is prepared
and after `reason-all`. Returns take turns, since the return is process-wide. Each makes a `reclaim` memory
record with the resident size immediately before and after and the time it took, in the log and in
`observations.stage_memory` of the timings JSON, beside the allocator's own totals.

The optimization guide and the record's documentation say what the figures are and are not: the two resident
readings bracket other threads' activity, so their difference is net of it; the allocator totals and the
resident size measure different scopes; and the resident size less the allocator's in-use bytes is an
indication of resident memory held unused, not a bound, because glibc counts chunks freed into per-thread
caches as in use.

Measured on the hosted 16 GiB runner, two samples that agree (runs 43672812964 and 43693277023): 43 returns
per generation job cost about 5 s in total and give back 33 to 36 GiB; resident memory falls from about
12.4 GiB to 5.6 after `stage-mappings` and from about 10.5 to 6.6 after `reason-all`. Both generation jobs now
run past the reasoning stage, which they did not before, and are lost in `stage-snapshot` at about 13.3 GiB of
memory in use, where a return cannot help. The transient peak inside `stage-mappings` (14.7 GiB) is unchanged.

Verified: independent review PASS at 6752772c3; the production diff read at 9fb7090f5 and each later commit,
which change comments and the guide only, written or read by the merging session and answering a review
finding. Pull request checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs
and the aggregate red as on main.

Not shown: no producer pass has completed with this change, so nothing after the snapshot stage has a hosted
measurement and byte identity of the generated outputs is unobserved. Local clippy with warnings as errors
exits 101 on both touched crates with every error in a file this change does not touch, under a newer local
toolchain than the one pinned in CI.

Part of the work on the hosted runner's memory limit; the issue stays open.

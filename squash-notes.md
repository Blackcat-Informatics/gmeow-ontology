perf(pipeline): pin a restored handle graph without its canonical text; record dataset payload bytes (Principles 4, 7)

Cache restore pinned each handle graph by canonicalizing it and hashing the returned document. It now takes
the same SHA-256, under the same policy, from PurRDF's digest entry, so the canonical text of a handle graph
is not handed back to this crate. The pin is byte-identical: a test compares the two routes on a two-graph
RDF 1.2 carrier with blank nodes, a reifier with a triple term, and language-tagged, directional and typed
literals.

Stage memory records now carry `dataset_payload_bytes` on publication and release records, in the log line
and in `observations.stage_memory` of the timings JSON: the dataset's own accounting of its term text and
record tables, a lower bound on what the dataset holds and not its resident size. A phase-start record
brackets the stratum canonicalization. The optimization guide describes the field and the record.

The digest of the coherence certificate, the phase the hosted runner is killed in, is not changed: the pinned
PurRDF version has no entry that returns that digest without building the document.

Verified: independent review PASS at c0bc3c10e; the whole diff read; the merged head is that commit merged
with main, no conflict. The extended timings JSON test passes. A focused selection at the commit before a
test-only change: 189 run, 186 passed, 3 failed, none in changed code (two refuse without produced fixtures,
one fails inside the validate stage before any cache replay). `make producer-recipe` exit 0. Pull request
checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red
as on main.

Not shown: the memory saved is unmeasured, and the cache-restore path is not the one the hosted runner dies
in. No producer pass, gate or `make check`; nothing re-run after merging main. Clippy with warnings as errors
exits 101 on the two touched crates with every error in a file this change does not touch, under a newer
local toolchain than CI's.

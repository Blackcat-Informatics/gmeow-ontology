# Native cost-artifact ledger and soak

Bounded continuation at b4165c0b6 after the performance-report delivery. Own only
the cost portion of stages/bench.rs, new external pure controls and
COST-BASELINE-JSON.md. Preserve all timing/perf-merger source, manifests, locks,
committed baselines and engine measurements.

The13private reader records migrate to native FromJson/Record, with strict
u64/u32/i128 token decoders and a cost-owned exact four-slot tuple decoder using
native items_with and field/index pointers. Both actual renderers share
parse_cost_artifact; known duplicate fields refuse, unknown fields are ignored.
No native-to-Serde bridge, new generic framework or unused output codec.

Producer source confirms five fragments, exactly answer_count for backward and
derived_count for the other four; incremental profiles supply scratch/join/parity
and grounding additionally supplies grounding plus scratch probes/rules. Require
selected evidence, never render missing counts/parity as zero/false. Preserve
genuine max_proof_height absence as an unavailable cell.

Validate the actual cost/1 schema and case_count, nonempty case/ledger inventories,
unique nonempty corpus/case identities and existing ruleparallel/gap constraints.
Preserve sorted report order, arithmetic, integer values, gap refusal and exact
soak digest framing. Do not invent globally mandatory optional panels or alter
measurement gates.

Pure controls use independently authored tiny complete artifacts, native parser
and both actual renderers with temporary inputs. Pin literal simple Markdown/
soak outputs and exercise numeric/tuple/profile/refusal boundaries. No real
baseline, engine, benchmark, corpus, source compiler or producer calls.
Root owns formatting, builds, checks, lock resolution and publication.

Root addresses the independent review's concrete inherited nonvacuity gap in
this same delivery: require nonempty engine/ledger/finding identities, positive
ledger comparison counts and exact case-corpus/ledger-key scope. Actual producer
proof in review-cost-json.md establishes these without equating raw case counts
and comparison counts. Two additional authored controls join the original13.

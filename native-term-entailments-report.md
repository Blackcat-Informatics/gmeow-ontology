# Native term-entailment publication repair

SUPERSEDED lifetime assessment: hosted generation (b) in run 38981058280 failed
because documentation executes before snapshot assembly. The snapshot-owner
fixture below was incorrect. See native-panels-phase-report.md for the actual
production caller, correction and current qualification. Historical checks and
failed logs below are preserved; they do not establish production acceptance.

Hosted run 38964521632 completed with failure on a6c9c9986. Generation jobs
117196544677 and 117196544589 and fixture job 117196544651 reached stage-gts-sink
and rejected the same canonical modal logic/evidence identity under an assumed
gmeow/derivation namespace. Exact logs are generation-a-current.log,
generation-b-current.log and fixture-prefix-current.log. Generation (a) recorded
stage-reason at 4,763,339ms. Builds/lint passed; dependent qualification was skipped.
This is a new terminal failure, not a still-running or passing producer run.

The repair removes the serialized explanation parser and RawDerivation schema.
Term panels now borrow the original pinned native Reasoning result. The existing
reader is shared with documentation verdicts and retains owner, graph pin, complete
native commitment, payload-kind, valid-input and result-invariant checks. Serialized
explanations cannot substitute for a missing native handle. No namespace whitelist,
parser fallback, source exclusion or relaxed semantic grading is introduced.

Native inferred rows supply their original firing rule, conclusion and premises.
Modal premises use their actual source worlds; conclusions and premises display
world scope explicitly. Native quoted conclusions retain recursive term matching
and directional literal identity without RDF serialization/reparse. Asserted rows
are excluded and source/head joins remain deterministic and deduplicated. The
sink implementation key advances to v17 to invalidate older presentation products.

Obsolete parser-only Turtle spelling/IRI-prefix tests are removed with the dead
parser. Product controls cover authenticated native inputs, multiple heads and
premises, modal evidence identity/worlds, quoted/directional values, missing native
source despite serialized bytes and missing firing identity. Existing shared-reader
controls still cover wrong owners/arms/pins, changed native payloads and incomplete
or undecided observations. All fixtures are tiny explicit inputs, not corpus builds.

Published as bc230b3f6. All 19 focused checks passed in
native-term-entailments-settled-tests.log, and strict producer-independent Clippy
passed in 3m19 in native-term-entailments-settled-clippy.log. Normal commit hooks
and push passed. Hosted qualification is running in run 38981058280. No local
optimized producer/fingerprint, full gate or machine probe ran. No pipeline speedup
is claimed from deleting the round-trip; native semantic completion and the measured
79-minute reasoning phase remain required by the full logic/performance scope.

Lifetime/caching assessment: the sink reads the retained stage-snapshot owner,
which already carries the original native Reasoning payload. The original reason
carrier remains releasable at its real last reader. The three existing carrier
inputs are unchanged; no new large-carrier retention edge is added. Controls keep
the original reason carrier released, require the snapshot, and refuse a different
retained owner rather than probing a fallback.

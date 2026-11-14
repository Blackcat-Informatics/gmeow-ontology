# Independent review of PR 2755 at `4dac0f9b8` (range `e447d5f70..4dac0f9b8`)

Saved by the primary session from three reports of reviewer `afebdb792b5643d1d` (2026-11-14), condensed in
wording only; the full reports are in the session transcript. By reading git objects only; nothing was built
or run. The reviewer's report at `492245596` is `review-2755-at-492245596.md` (scope and evidence for the code
delta are there).

## Scope

The readings together cover the whole range `e447d5f70..4dac0f9b8`:

- `e447d5f70..492245596`: the code and test delta (7 files), reviewed in full. Verdict then: BLOCKED on three
  statements the delta made false; "the R1 requirement holds in the code".
- `492245596..49b8cff1a`: text only, 3 files (+13 -9). All three `scheduler.rs` hunks are doc lines, the
  `run.rs` hunk is comment lines, the `module.ttl` hunk is one definition string. REQ-B and both parts of
  REQ-C resolved. Verdict then: BLOCKED on the new last sentence of the `gmeow:laneAllArtifacts` definition,
  which said the consumer keeps the producer's carrier resident while it runs: false when the carrier was
  already released at production.
- `49b8cff1a..4dac0f9b8`: one file, one line, the wording the reviewer accepted after checking it against
  the code for: producer whole with carrier readers pending; the last carrier reader finishing while the
  consumer runs; producer released at production; the retain-all retention; the recording policy; a
  consumer that fails.

Also read for contradictions: the `laneCarrier`, `laneDataset`, `laneIdentity`, `flowReadsLane` and
`flowReadsHandle` definitions; `docs/PIPELINE_SPINE.md` 312-340; `docs/GATE-AND-PIPELINE.md` (every "carrier"
statement); the doc comments in `scheduler.rs`, `node.rs`, `bundle.rs`, `run.rs`.

Not covered by the verdict: compilation, tests, clippy; the cold enforcing producer run; peak memory against
`main`. `prose-check` was not run on the new sentences.

## Findings

REQUIRED: none. "The code at 492245596 met R1 by reading: no byte copy at dispatch, the same observations
for passing runs, and a swap only when the carrier readers and the in-flight views are both zero. The false
statements I raised (REQ-A, REQ-B, REQ-C) are now true at this head."

FOLLOW-UP:

1. `node.rs` `artifacts()` (about 520-525) deep-copies every lane entry before filtering while the producer
   is whole; transient, no larger than `main`. Issue 2770.
2. `node.rs` `artifact_refs()` (about 555-560) validates unserved entries, so a corrupt unserved internal
   entry fails only while the producer is whole; passing runs unaffected. Issue 2758.
3. No test pins the deferred swap. Issue 2770.
4. Identity-only edges have no separate pointer test; the pointer test covers `artifact()`, not
   `artifact_refs()`.
5. Cases that do not occur in the shipped graph: recording with drop-after-last-consumer; a span-table strip
   while a lane view of the source-load product is in flight; "no more than `main` at every instant" rests on
   graph order for the sink and the dictionaries stage.
6. Carried: cached stages (2753); read-comparison gaps (2758); release copies and retention (2770);
   fixture-backed tests (2773).

VERDICT: PASS 4dac0f9b8

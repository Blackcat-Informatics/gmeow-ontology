VERDICT: PASS

# Review of PR 2755 at head e447d5f70

## Scope

- Read: the one commit e447d5f70 on top of 70557e49a (`git diff 70557e49a e447d5f70`): `crates/pipeline/src/node.rs`, 8 insertions and 5 deletions, one hunk.
- Builds on my earlier reports, all in this directory:
  - `review-2755-at-85cef27fe.md`
  - `review-2755-at-dbbd77181.md`
  - `review-2755-at-68272e7e4.md`
  - `truth-sweep-2755-at-68272e7e4.md`
  - `review-2755-at-70557e49a.md`
- Method: git objects and the checkout at that revision, read-only, nothing built or run. The delta touches only `node.rs`, so every scheduler line cited below is unchanged since the 70557e49a review.

## State of my one finding (`review-2755-at-70557e49a.md`, REQUIRED 1)

FIXED. New text of the `carrier_released` field doc, `node.rs:159-169`:

> "...in two cases. Under [`CarrierRetention::DropAfterLastConsumer`](crate::scheduler::CarrierRetention), once the LAST stage whose dataflow edge from this one declares a carrier read has run (the edge reads declared in the pipeline slice, see [`crate::reads`]). Under the enforcing read policy, whatever the retention, when the product is handed to a consumer whose edge declares no carrier read; the recording policy hands every product over unreleased."

Checked against `scheduler.rs`:
- Case 1 (drop after the last carrier reader): `if carrier_retention == CarrierRetention::DropAfterLastConsumer {` at line 961. Inside it, the loop at 962-977 decrements `remaining_carrier_readers` and releases when `*remaining == 0 && !retained_carriers.contains(&producer)`. Lines 978-991 release a producer that has no carrier reader (`remaining_carrier_readers ... == 0`) once it is logically consumed. TRUE.
- Case 2 (release when handed over): lines 741-747. `whole = read_policy == Record || carrier_inputs.contains(producer)`; otherwise `product.into_carrier_released_keeping(&kept_for(producer))`. There is no `carrier_retention` condition anywhere in the expression. So it is TRUE that the release is under the enforcing policy whatever the retention (every `RunContext` constructor defaults to `RetainAll`), and not under the recording policy. TRUE.
- Agreement with `bundle.rs:162-166`: "when a product's exact remaining carrier-reader count reaches zero, and also, under the enforcing read policy, when it hands a product to an artifact-only consumer (one whose edge declares no carrier read)". The two docs now state the same two cases. AGREES.

## The ten statements

1. `node.rs:375-380`, `dataset()` `# Panics` doc ("The read is recorded BEFORE the assertion, so the scheduler, which catches the stage's panic, reports it as an undeclared read of this producer with the declaration to add"): TRUE. `record(Dataset)` is on line 382, before the `assert!` at 383. The worker `catch_unwind` turns the panic into an `Err`, and the `Err` arm at `scheduler.rs:833-838` pushes `undeclared_reads(&dispatched_edges)` under `Enforce`. The edge declares no dataset read in exactly the case where the carrier was released at hand-over, since a declared `Dataset` makes the producer a carrier reader.
2. `node.rs:382-388`, the `dataset()` panic text ("attempted to read a dataset after its carrier was released; its dataflow edge must declare the dataset read"): TRUE. Declaring `laneDataset` (or the whole carrier) makes the edge a carrier read, so the carrier is not released at hand-over.
3. `node.rs:394-401`, `dataset_arc()` (`# Panics` "As [`Self::dataset`]" and the identical text): TRUE. The code is the same record-then-assert.
4. `node.rs:721-722`, `carrier_consumes` ("A dataset read of a released carrier panics at the read, through [`StageProduct::carrier_released`]; a handle or blob read returns nothing."): TRUE. Only `dataset` and `dataset_arc` assert on the flag. `handle` and `rep_blob` read the released residue, which has no handles or blob records. The test `a_failing_stage_still_reports_its_undeclared_reads` exercises the handle case.
5. `scheduler.rs:829-832` ("A failing stage's own error is kept, and so is every read it made that its edges do not declare: a read on a released product often IS the cause of the failure ... Declared reads it never reached are not reported: it stopped early."): TRUE. The `Err` arm calls only `undeclared_reads`, not `mismatches`, and keeps `first_error`.
6. `stages/archive_blobs.rs:707-711` (the "carrier was released before `{rep}` was read; the consumer's dataflow edge from {STAGE_ID} in the pipeline slice must declare the whole carrier (`gmeow:flowReadsLane gmeow:laneCarrier`)" error): TRUE. The next line is `archive_bytes(product.bundle(), rep)`, which records `Read::Carrier`; `Carrier` is the lane that covers it.
7. `stages/gts_sink.rs:125-127` ("Which of these edges keep a live carrier for the terminal, and which internal artifacts it reads off the released products, is declared on the sink's `gmeow:BuildDataFlow` edges in the pipeline slice, not here."): TRUE. The fields and overrides were removed, and `BoundStage` derives both from the slice.
8. `tests.rs:511` ("How a stage that consumes `source` through an identity-only edge misbehaves"): TRUE. The `spec()` helper declares `Read::Identity` for each consumed producer, so the diamond edges are identity-only and the producer is released at hand-over.
9. `tests.rs:603` ("A handle read of a released product returns nothing and the stage then fails with its own message; the undeclared read is reported beside that message, never lost with it."): TRUE. This matches item 4 and the `Err` arm of item 5.
10. `slices/core/pipeline/module.ttl:311`, the `flowReadsArtifact` definition: TRUE. An internal artifact is dropped on release unless some consumer in the run declares it (`kept_internal` is a union over every executed stage); it is kept "until the run ends, whether or not the declaring consumer has run". A committed artifact survives release but is an undeclared read unless named or covered by `laneAllArtifacts`, which excludes internal paths (`covers`).

## Is the delta that one doc comment and nothing else?

Yes. One hunk, one file, inside the doc comment of the `carrier_released` field. No code, string, test or slice change.

## Required findings

None.

## Optional (not blocking, not required)

- `node.rs:159-169` says "in two cases". The first case reads "once the LAST stage ... declares a carrier read has run" and does not state the degenerate form (a producer with no carrier reader is released immediately after production, `scheduler.rs:978-991`), nor the `retained_carriers` exemption (the explicit keep set, line 970/984). Neither is false, and `bundle.rs` covers the first through "count reaches zero". It would be clearer with those two clauses.
- "the recording policy hands every product over unreleased" describes the hand-over release. Combining `Record` with `DropAfterLastConsumer`, which no shipped path does (`record_run` uses `RetainAll`), would still run case 1. The statement is true for every shipped use.
- The version-history comment at `gts_sink.rs:174-177` (v10, "declare the exact three carrier-lane inputs independently from the artifact-only dependency edges") is unchanged from main and describes history; it is not stale.

## Follow-ups carried forward (none block)

- Cached stages, 2753: a stage served from the stage cache is not compared, so a changed declaration is not checked until a miss runs the stage (stated in `GATE-AND-PIPELINE.md` item 4 and the slice).
- Read-comparison gaps, 2758:
  - F3: the recorder cannot discover an internal artifact reached through the whole artifact lane; it is now stated honestly in six places.
  - F4: dictionary names are authored by hand; no registry pairing test.
  - F5: no scheduler-level span-strip test; `parse_edge_reads` refusals untested.
  - F6: `undeclared_reads` should be `pub(crate)`.
  - F7: the typed inner error is stringified when read differences exist.
  - A duplicated `reads.insert` in `attach_drift_honors_typed_entity_inputs`.
- Release copies and retention, 2770: F2, dispatch-time `kept_for(producer)` deep-copies internal artifacts the consumer cannot see (peak memory).
- Fixture-backed tests, 2773: D2, D3, D12, N8 and N9 from my earlier reports.
- F1, a panic in `progress.stage_end` outside the catch can still stall the coordinator and a poisoned global resource lock persists after a caught panic: I do not know which issue owns it. 2770 is the nearest.
- Process: the PR description's "Changes made after the restore" list should name the commits after 68272e7e4 (2e140d4a0, 70557e49a, e447d5f70) and the producer-build failure (R4).

## What the verdict does not cover

These cannot be settled by reading. I did not determine them.
- The cold enforcing producer run, and whether it passes.
- Peak memory against main.
- The comparison of the two runs' findings.
- Anything that needs compiling or running: compilation, tests, clippy, and the `make check` gates, including the ratchet, i18n and prose-check gates and any gate that pins the content of the changed slice annotations.

## Not determined

- The claim in `run.rs` that "the producer's own authentication covers the file".
- Everything listed in my earlier reports as not determined that this commit does not touch.

## Verdict

PASS on the text. The prior required finding is fixed and I found no new false statement in the delta or the ten statements checked. With every earlier REQUIRED item fixed (R1 to R4, the sweep findings, and REQUIRED 1 of 70557e49a), this head has no open required finding that reading can settle.

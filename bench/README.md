<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# `bench/` — the committed perf reference run

`baseline.json` is the **committed reference run**: a flattened snapshot of a
criterion benchmark pass, one entry per `"<group>/<bench>"` with its `mean_ns`
and `median_ns` point estimates in **integer nanoseconds**.

It is the single source of truth for the committed perf leaderboard
(`generated/bench/leaderboard.md`, the `stage-export-bench` generator) and the
baseline the report-only regression scoreboard compares a live run against.

## Why integer nanoseconds

Timings are non-deterministic, so the committed artifacts must not encode raw
`f64`s that re-serialize differently across runs. The baseline rounds every
estimate to an integer ns at the emit boundary, so both `baseline.json` and the
rendered `leaderboard.md` are formatting-stable and survive the strict `sync`
drift gate. The drift gate only *reads* this file — it never runs benchmarks.

## Refreshing the baseline (maintainer only)

There is **one** producer — the Rust `bench-compare --emit-baseline` path:

```sh
make maint-bench-baseline   # runs `make bench`, then emits bench/baseline.json
git add bench/baseline.json generated/bench/leaderboard.md
```

Refreshing is a deliberate, hand-committed act — never auto-drift. Numbers are
machine-specific evidence; record exactly what ran and do not hand-edit them.

## Report-only regression scoreboard

The off-gate `suite-quality` CI lane runs `make bench-compare`, which prints a
`live run vs baseline` scoreboard (`ok | watch | regressed`) to the job summary.
It is advisory: runner jitter is expected and it **never** fails a PR
(Principle 18 — the authoritative gate stays native-first and Docker-free).

# `cost-baseline.json` — the committed deterministic engine-cost reference run

`cost-baseline.json` is the **committed deterministic cost/agreement baseline**:
the `gmeow-bench-engines --emit-cost` artifact over the committed mini corpora
(`conformance/logic/cases/bench/`). Unlike `baseline.json` (criterion timings),
every value here is an **integer count or a fingerprint or a boolean verdict** —
per `(corpus, case, engine)` the sorted cost-vector tuples, `consumed_steps`, the
derived / answer counts, the deterministic `peak_live_bytes` allocation scalar,
the verdict-agreement tokens, and the per-corpus divergence-ledger tally. It
carries **NO** wall-clock, **NO** peak-RSS, and **NO** total-allocation scalars
(those are report-only in the harness), so the bytes are a pure function of
`(engine version, corpus)` — byte-identical across runs.

It is the single source of truth for the committed cost ledger
(`generated/bench/cost-ledger.md`, the `stage-export-cost-ledger` generator), a
drift-gated projection reproduced byte-for-byte from this file without ever
running a benchmark. The strict `sync` gate only *reads* this file.

## Refreshing the cost baseline (maintainer only)

There is **one** producer — the Rust `gmeow-bench-engines --emit-cost` path:

```sh
make maint-bench-cost-baseline   # emits bench/cost-baseline.json (offline; twice-diffed for byte-stability)
make check-sync SYNC_MODE=update   # re-projects generated/bench/cost-ledger.md
git add bench/cost-baseline.json generated/bench/cost-ledger.md
```

Refreshing is a deliberate, hand-committed act — never auto-drift. The counts are
attributable to the pinned engine revisions recorded in the artifact's
`engine_pins`; do not hand-edit them.

## Cost-regression finding (richer honesty surface)

`gmeow-bench-engines --check-cost bench/cost-baseline.json` compares a FRESH cost
run against the committed baseline and, on **any** deterministic-count divergence
(a changed count/fingerprint/verdict, a dropped case, or a case absent from the
baseline), emits a `reason.divergence.corpus-only` `gmeow:Finding` routed through
the shared divergence ledger (`divergence_diag_ledger` — content-addressed
`finding_iri` + anchor + antecedents) and hard-fails. The primary on-gate gate is
the strict `sync` drift check on `cost-ledger.md`; this mode is the richer finding
surface behind it.

# `medium-baseline.json` — the committed dictionary winner table

`medium-baseline.json` is the **committed dictionary sweep artifact**: for every
**measurable** shipped `gmeow:CompressionDictionary`, the whole
`(strategy × target-length)` grid, the cell that won it, and the two-part code
that decided the winner — plus one global `(codec × level)` grid pricing the
mandated Rule 6 chain. Every value is an **integer byte count**, a token, or a
fixed six-decimal fraction, so the bytes are a pure function of
`(sources, dictionary grid)` and survive the strict `sync` drift gate.

`stage-medium-dictionaries` **consumes the committed winners**: it trains each
dictionary at the committed `(strategy, target length)` rather than searching per
build, so the shipped dictionary bytes are a deterministic function of the
repository instead of a function of how much CPU the build machine had. The
committed table ↔ the measurable registry is a **bijection**, hard-failing in both
directions (an unmeasured dictionary would be trained at a guess; a stale row would
steer a trainer for a dictionary the bundle no longer ships).

## The criterion: a two-part code, with no threshold

```text
two_part_code(d) = Σ_f |enc_d(f)|  +  |dict_d|         (the frames, then the model)
baseline(d)      = Σ_f |enc_base(f)|                   (gmeow:mediumProfileBaselineL12)
d ships  ⟺  two_part_code(d) < baseline(d)             (strictly; a tie is a loss)
```

Both arms run the **mandated** `zstd-rsyncable` chain at the media's declared
level through the same encoder `emit_gts` writes frames with — never a cheaper
plain-`zstd` proxy, because a number produced by a codec the bundle does not write
would describe bytes nobody ships. There is no tolerance band: charging a
dictionary its own in-band bytes is the entire non-vacuity argument.

## Two declared populations

* **`emitted-blob-frames`** — the blob frames the terminal writes into
  `gmeow.gts`, grouped by the dictionary their `gmeow:PayloadSchema` selects. The
  **snapshot frame is excluded by declaration**: the measurement is folded into the
  snapshot payload, so that frame's compressed length is a function of the very
  numbers being recorded. `measurement_evaluated_frame_count` is emitted so the
  population is visible rather than implied.
* **`runtime-store-segments`** — append-only runtime store files, replayed through
  the real `Memory::store` path over a declared, bundle-derived corpus of at most
  `REPLAY_RECORD_COUNT` canonical statement-layer lines (a **ceiling**: the statement
  layer is currently shorter, so the effective extent is `evaluated_frame_count` on the
  row, which is why every reading records it). The dictionary's in-band
  cost is charged **once per segment header**, not once per file, because that is
  what a store actually pays: whether a store dictionary wins is a pure function of
  the record count, so the corpus cardinality is recorded beside the result. These
  numbers are committed here (and projected into
  `generated/medium/dictionary-effect.ttl`) rather than measured per build, because
  a store's records carry a **wall clock** and are therefore not a function of the
  build; the live gate over freshly written store files is
  `crates/pipeline/tests/medium_bundle.rs`.

## Honesty: this is NOT a held-out evaluation

For the archive-backed dictionaries — `gmeow-core-v1` over `cells-archive`,
`gmeow-logic-v1` over `axioms-archive` — the training corpus is that archive's
**members**, and the frame the numbers are taken over is the **tar of those very
members**. Train and test are therefore not merely correlated: on the dominant
representation they are the **same bytes**. Nothing in this artifact is a held-out
measurement and none of it may be described as one, in a commit message, a release
note, or a `skos:definition`.

What the two-part code buys is **non-vacuity**, not generalization. Charging the
dictionary its own in-band bytes means "memorize the corpus" stops winning the moment
the memorized bytes cost more than they save — which is exactly what retired three
dictionaries below. It says nothing about whether a dictionary would still help on
material the build has never seen, and no experiment here could: the population a
bundle dictionary primes IS the bundle's own frames.

The runtime-store readings (`gmeow-memory-hot-v1`) are the one partial exception: the
replay corpus is drawn from the statement layer while the dictionary trains on the
statement + authoring-brief material, so the overlap there is between *related* corpora
rather than identical bytes. That is a weaker overlap, not an absence of one.

The same caveat, and the snapshot exclusion, are carried in the measurement's own
`skos:definition` in `generated/medium/dictionary-effect.ttl` — a consumer reading
the bundle sees them without reading this file.

## Where the "does it pay for itself?" refusal lives, and why not at the emitter

The gate that refuses to build against evidence saying a shipped dictionary loses is
`sweep::check_dictionaries_pay_for_themselves`, called by **`stage-medium-dictionaries`**
over this committed artifact. It is deliberately **NOT** at the emission site
(`serialize_snapshot` / the terminal sink), and that placement is a decision rather than
an omission:

* the emitter also serializes **fixture-scale folds** — a few hundred bytes of synthetic
  carrier in a focused unit test — where *no* dictionary of any size can pay for itself.
  A refusal there would red on artifacts the criterion was never about, and the only way
  to keep those tests green would be a size threshold, which is exactly the escape hatch
  the criterion refuses to have;
* the committed evidence is a **deterministic artifact about the real deliverable**, so
  checking it costs nothing per build and cannot be confused by scale.

The LIVE half — the same criterion over the whole DAG's real output, on the bytes the
terminal actually wrote — is `crates/pipeline/tests/medium_bundle.rs`. So there are two
gates and neither is the emitter: a cheap one over the committed evidence on every build,
and an expensive one over a real emission. If you came here looking for a missing check at
the emission site, this is why it is not there.

## The mandated codec chain: `mandated_is_argmin` is `false`, and the chain is KEPT

The `codec_sweep` block prices the whole `(codec × level)` grid over the primed frame
population and records, as data, whether the **mandated** Rule 6 chain
(`zstd-rsyncable` @ 12) is the grid's argmin. **It is not**, and the committed artifact
says so. The grid is committed in full beside the flag, so the size of the gap is legible
rather than a boolean: on the swept corpus the mandated cell is roughly **2×** the plain
`zstd` cell at the same level, and plain `zstd` @ 19 is cheaper still.

The chain is kept. This section is the record of that decision and of what a future
reader would have to establish to reopen it. It is **evidence, not an argument for an
outcome**.

**Why the grid cannot settle the question.** The grid prices **SIZE ONLY**. GTS §8.4
rsyncable framing does not exist to make the artifact small; it exists to make the
artifact **delta-transferable** — a local edit perturbs a bounded region of the encoded
stream instead of re-randomizing everything after it, so an `rsync`/CDN/incremental
consumer re-fetches a fraction of the file. No size grid can see that property, so
"cell A is smaller than cell B" is not an answer to "which chain should the bundle
mandate". Changing it is also a **normative Rule 6 change** in `docs/gts-narrow-waist.md`
with reader-capability consequences (`gmeow:requiresReaderCapability "zstd-rsyncable"`),
which is a doctrine edit and not a benchmark's to make.

**Two facts that keep the tradeoff live rather than academic.** Both are established on
this branch and both weaken the delta-transfer side of the case:

1. **The chunking is at a FIXED offset, not a content-defined one.**
   `purrdf`'s `encode_zstd_rsyncable` is `data.chunks(RSYNCABLE_BLOCK_SIZE)` with
   `RSYNCABLE_BLOCK_SIZE = 65_536` — the cut points are at fixed multiples of 64 KiB of
   *uncompressed* offset, not at boundaries derived from the content. So the delta
   property survives an **equal-length in-place edit** (a changed digest, a re-rendered
   fixed-width field) and essentially nothing else: any insertion or deletion shifts
   every subsequent byte across every subsequent boundary and re-encodes the whole tail,
   exactly as a single-frame encoding would. A content-defined chunker would restore the
   property in general; this one does not have it in general.
2. **`generated/dist/gmeow.gts` is git-ignored and never committed.** The original
   framing of the tradeoff — "rsyncable framing buys smaller git deltas on a committed
   binary" — has lapsed: there is no committed binary. The remaining beneficiary is a
   consumer fetching successive releases over the network, which is a real but different
   (and unmeasured here) population.

**What would settle it.** A measurement of the property the size grid cannot see: the
transferred-bytes cost of upgrading between two consecutive real bundles, under both
chains, over a realistic edit distribution — plus, separately, whether the fixed-offset
chunker or a content-defined one is what §8.4 should mandate. Until someone runs that,
`mandated_is_argmin: false` stands in the artifact as the honest statement that the
mandated chain costs size and the grid cannot price what it buys.

The sweep lane therefore **reports** this on every run and does not exit non-zero for it.
It was raised as a stop-and-ask once and answered; a lane that re-raised a settled
question every refresh would fail forever and teach a maintainer to ignore its exit code.
The only remaining stop-and-ask is a dictionary that does not pay for itself.

## Retiring a dictionary: the rule three measurements agree on

Three `gmeow:CompressionDictionary` drafts have been retired by this lane. All three were
drafted from **slice names** and all three failed the same criterion:

> A `gmeow:CompressionDictionary` is justified by the **FRAME SET** it primes, and it must
> **pay for its own in-band bytes on that set**. Slice topology is not frame topology, and
> topical importance is not a justification.

| retired | how it failed | numbers |
| --- | --- | --- |
| `gmeow-math-v1` | primed **zero** frames — the mathematical named graphs are unioned into the snapshot payload, which is one frame already primed in full by `gmeow-core-v1`, and a second dictionary cannot prime part of one frame | trained, measured, pinned and projected onto a `.zdict` no payload ever cited |
| `gmeow-claims-v1` | primed **one ~9 KB frame**; no cell of the grid can pay for a dictionary's own bytes over a population that small | best cell 12,020 B vs an 8,953 B no-dictionary baseline |
| `gmeow-lang-ast-v1` | primed **three frames** and still lost — a real saving, far too small against its own in-band bytes | lost by 3,684 B at its best cell |

Retirement is a **correction, not a descope**, and the distinction is enforced: every rep
the retired dictionaries primed (`yaml-ld-archive`, `lang-projections-archive`,
`lang-surface-blob`) is now primed by `gmeow-core-v1`, so **not one frame lost
compression** and nothing is left citing an id the bundle no longer trains.
`no_rep_is_primed_by_a_retired_dictionary` in `crates/pipeline/tests/medium_bundle.rs`
asserts that in every direction a retired id could survive — the declaration, the
per-rep selection, the segment header's in-band `"dct"` map, and the projected
`generated/medium/*.zdict` set.

Because absorbing those reps **changes `gmeow-core-v1`'s population**, the surviving
dictionaries were **re-swept over the widened frame set** rather than assumed to survive
it. The committed table is that re-sweep.

## Every declared dictionary has a row

There is **no exempt dictionary**. `gmeow-memory-compact-v1` used to have none:
`mcp::compact_store` could pass purrdf only a dictionary *name*, so purrdf derived
pack-local bytes and labelled them with that id — the compacted pack was genuinely
primed, but with bytes it derived rather than the bytes this bundle trained, and a
`(strategy, target length)` sweep would have been measuring a knob that steered
nothing.

`compact_store` now hands purrdf the **shipped** bytes as
`DictStrategy::Pinned` (used verbatim: no training, no corpus derivation, no
truncation), resolved out of the loaded bundle's in-band `"dct"` map exactly as the
hot lane resolves `gmeow-memory-hot-v1`. So the training point steers real bytes, the
carve-out is gone, and `gmeow-memory-compact-v1` is swept over the same
**population-B runtime-store replay** as the hot dictionary — the only faithful way to
price a store dictionary, because its in-band cost is paid per segment header rather
than once per artifact.

`every_declared_dictionary_primes_an_emitted_frame` in
`crates/pipeline/tests/medium_bundle.rs` asserts the consequence as a byte equality:
the dictionary the compaction lane pins **is** the bundle's header entry, so one
`gmeow:dictionaryId` resolves to exactly one byte sequence.

## Refreshing the sweep (maintainer only)

There is **one** producer — the Rust `medium-sweep --emit-baseline` path:

```sh
make maint-medium-sweep     # runs the real DAG, then the full grid
make check-sync SYNC_MODE=update   # re-projects generated/medium/dictionary-effect.ttl
git add bench/medium-baseline.json generated/medium/dictionary-effect.ttl
```

The lane exits **non-zero** on the one declared stop-and-ask condition — a dictionary
does not pay for itself at its best cell — and writes the artifact first, because the
evidence is the point. (That the mandated chain is not the grid's size argmin is
*recorded*, not a stop: see the codec section above.) It also *reports* (without acting
on) a dictionary whose **authored**
`gmeow:dictionaryTargetLength` / `gmeow:dictionaryStrategy` is not the measured
argmin: the declaration is never silently overwritten, and
`the_declared_training_points_are_the_committed_winners` stays red until a human
reconciles the slice with the evidence.

# The identity gates — a zstd-compressed claim is the same claim

The winner table above answers *does a dictionary pay for itself?*. It says nothing
about whether the priming changed what the bundle **says**, and that is the claim the
medium axis actually rests on. Three gates carry it; two run on every `make check`, the
third is a maintainer lane.

## On-gate: `medium_identity_gate`

`crates/pipeline/tests/medium_identity_gate.rs` runs the real DAG **once** and emits the
same carrier **twice** — under the authored assignment (whose primed blob reps all name
`gmeow:mediumProfileDistL12`) and under the DECLARED `gmeow:mediumProfileBaselineL12`.
The counterfactual is a **named medium**, never an empty registry and never
`MediumPlan::undicted`: both would be the legacy no-dict mode this axis removes, and
neither would leave anything on the artifact saying which medium it is. The baseline
emission still **pins** every declared dictionary — the pack is that family's
distribution channel — and primes no frame with any of them, so the two emissions differ
in priming and in nothing else. Then:

1. **the fold is byte-identical** — the same RDFC-1.0 canonical N-Quads per named graph
   and the same reconstructed bytes for every committed path, with the one difference
   confined to the `gmeow:MediumEnvelope` subgraph and characterized exactly: only
   `gmeow:envelopeMedium` and `gmeow:envelopeDictionary` may move, and they **must** —
   an envelope that did not move is projecting an intention rather than the wire;
2. **every `gmeow:contentDigest` is recomputed** from the bytes actually decoded off the
   wire (through the frame's own declared transform primed by the header's own pinned
   dictionary, so the documentation-scale payloads are covered too) and matched against
   the frame's in-band `pub.digest`;
3. **the delta-transfer property is measured**, not asserted — `zstd_block_layout` walks
   each frame's payload without decompressing it, and the rsyncable block count and the
   uncompressed cut points must be identical across the two emissions. The snapshot frame
   is DECLARED out of that comparison, for the same reason it is declared out of the
   dictionary-effect population: its payload carries the very envelopes that name the
   medium.

`crates/pipeline/tests/medium_codec_composition.rs` proves the same law for the MEDIUM
rather than for one build: `decode ∘ encode = id` for every chain the registry declares,
over inputs that straddle the 64 KiB rsyncable cut grid plus the repository's committed
frozen corpora, primed and unprimed — and that a mis-primed decode never silently
returns the payload.

## On-gate: zero model-facing change (legs 1, 2, 4)

Leg 1 rides the identity gate (the GMN-dialect artifact set, derived from the emitted
bundle, must reconstruct byte-identically from both emissions). Legs 2 and 4 live in
`crates/pipeline/tests/model_facing_invariance.rs`: the branch diff may not touch a
GMN-dialect producer, and the `llms.txt`-family **shape** is frozen against the merge
base while term entries and the MCP resource list may grow by an exact enumerated delta.
Each leg has a targeted red fixture beside it.

## Off-gate: `make maint-medium-model-facing-diff`

The merge base carries **zero** medium axis, so this branch's test binary cannot run
there — which is why the cross-branch comparison is a maintainer lane rather than an
on-gate test. It checks the merge base out into a temp worktree, runs **its own**
`make check-sync SYNC_MODE=update` with **its own** toolchain (this branch's sources are never overlaid onto
it), and compares the GMN-dialect artifact set of the two materialized trees: first as
sets, then byte for byte. Both trees are classified by the same read-only predicate
(`cargo run --bin gmn-dialect-paths`), because two predicates would compare nothing.

```sh
make check-sync SYNC_MODE=update    # materialize THIS branch's generated/ tree
make maint-medium-model-facing-diff  # regenerate the base and diff
```

It needs `origin/main` fetched and disk for a second full checkout plus target
directory, and exits non-zero on any set or byte difference, printing the offending
paths.

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Pipeline Spine — Carrier, Terminal, and Fanout

> **Genre.** A normative architecture spec for the regeneration pipeline. The
> declarative present tense is normative — "the carrier *is* the sole transport"
> means "a conforming realization makes it so" — not a claim that every line of the
> current build already conforms. The conformance gate (§7) is what establishes it.
>
> **Audience.** Anyone working on `crates/pipeline`, the GTS terminal, or any stage
> that produces a committed artifact under `generated/`.
>
> **Scope.** How data moves from the authored slices to the shipped `gmeow.gts` and
> back out to the flat consumer tree. It does not redefine the ontology, the `logic:`
> core, or the GTS file format — only the dataflow that assembles and unpacks them.

## 1. Thesis

The build has one spine and one exit. Authored slices flow through a chain of
stages that each **contribute to a single in-memory carrier**; exactly one
**terminal** stage presents that carrier as the `gmeow.gts` bundle; and a separate,
post-pipeline **fanout** projects the individual files back out of the bundle.

```text
slices/  ─▶  stage₁ … stageₙ  ─▶  terminal  ─▶  gmeow.gts  ─▶  fanout  ─▶  generated/
            (each ATTACHES to        (carrier ▶                (gmeow.gts ▶
             the one carrier)          bundle)                  flat files)
```

Two directions, exactly as the project's standing dataflow demands: everything is
authored **from the slices**, and everything ships **to `gmeow.gts`**. The bundle
and the `gmeow` CLI are the deliverables; every other artifact is a view of the
bundle. This is Principle 4 (one canonical source; everything else a generated
projection) applied to the build itself, and it is the project's *maximal
information flow* maximum made structural: information is carried from source to
bundle at full fidelity, and trimmed only at the exit a consumer asks for.

## 2. The carrier is the spine

There is **one** internal transport: an in-memory bundle value (the
`PipelineBundle` carrier, `crates/rdf-core`) threaded through the whole run. It
holds:

- the **dataset** — every named graph the build accumulates (authored default,
  statement layer, import closure, alignments, the reasoned closure, diagnostics,
  documentation, provenance, …);
- a content-addressed **blob store** — opaque byte payloads (archives, rendered
  text trees, serialized side formats) keyed by digest;
- the **provenance** sidecar — per-quad attribution.

The carrier is the spine for the whole run: every stage hands the next stage a
richer carrier, never a serialized file and never a second parallel assembly. In
particular the GTS form is **exit-only** — the bundle is serialized to `gmeow.gts`
once, by the terminal, and never re-parsed back into the pipeline as transport.
There is no `dataset → gts → dataset` round-trip inside the spine.

**Pin-on-attach (the integrity invariant).** When a stage attaches a typed result
to a named graph, the carrier records the graph's canonical digest and rejects any
later attachment whose payload disagrees with its backing graph. A handle that
contradicts its graph can never attach — fail-closed, no silently-stale view. This
is the construction half of "verified by construction" (Principle 7).

## 3. The stage contract

A stage is a function from carrier to carrier. It **reads** what upstream stages
contributed and **attaches** its own contribution — one or more named graphs and/or
blobs. Three rules bind every stage:

1. **No out-of-band writes.** A stage never writes a file under `generated/`, never
   reads one back as input, and never opens a side channel to a later stage. Its
   only output is what it attaches to the carrier. (Reading authored *source* —
   `slices/`, `dsl/`, `imports/` — is how stages begin; that is input, not
   transport.)
2. **Transform once (the razor).** Each transformation from one form to another
   happens at most once per run. A closure, a projection, or a rendering is computed
   a single time and attached; downstream consumers read the attached result rather
   than recomputing it. Reasoning in particular runs **once**, materializing the
   reasoned closure as a carrier graph that every consumer — the bundle, the
   committed closure artifact, the documentation — reads from.
3. **Declare contributions.** A stage names the graphs and blobs it contributes, so
   the dependency order is derivable and the superset gate (§7) can map every output
   to its producer.

4. **Read only what you declared.** The scheduler hands a stage exactly the products
   of the ids in its `dataflowConsumes`. There is no ambient access to a sibling's or
   an ancestor's carrier; an undeclared read is not "unsupported", it is
   *unreachable*.

Stage kinds (source-load, transform, reason, validate, docs-render) differ only in
*what* they contribute and *what they read*, never in *how* they deliver it: all
deliver by attaching to the carrier.

**The carrier's lifetime is bounded (drop-after-last-consumer).** Rule 4 is what
makes a stage's carrier *provably dead* once the last stage declaring it has run:
nothing can still read it. The whole-repository build therefore **releases** it at
that point — the dataset, the typed handles, the blob records, the provenance, and
the internal `pipeline/`-prefixed byte artifacts are freed — keeping only the
committed byte artifacts the post-run reconcile still owes, and the product's
`digest` verbatim. Peak residency is then the live frontier plus the run's outputs,
not the **sum** of every stage's cumulative carrier snapshot over the DAG; the
latter grows with both the corpus and the stage count, and is a build that dies as
the ontology grows rather than one that scales with it.

The release is invisible: it never changes a produced byte, and the run's
order-independent `combined_digest` is identical with or without it. A reader that
reaches for a released carrier — necessarily an out-of-band whole-run consumer, never
a stage — HARD-fails on the released marker rather than seeing an empty dataset. Such
a consumer selects full retention explicitly; that selection is a profile, not a
degradation, because both profiles produce the identical products.

## 4. One terminal

Exactly **one** stage writes bytes. The terminal takes the fully-accumulated
carrier and **presents** it as the `gmeow.gts` bundle. It assembles nothing: it does
not load sources, union datasets, re-canonicalize graphs, or recompute any view —
those are the stages' work, already in the carrier. The terminal is the sole
serialization boundary in the build, and the bundle it emits is the single
content-addressed, signable artifact the project ships (Principles 14, 16).

A build with two writers — one that serializes and another that re-emits — has two
terminals, and is non-conforming. Presentation and writing are one stage.

## 5. The superset law

> **`gmeow.gts` is a superset of every build output.** For every committed artifact
> `o` under `generated/`, `o` is byte-reconstructible from `gmeow.gts` alone — either
> as a fold of one of its named graphs or as an extraction of one of its inline
> blobs.

This is not an aspiration; it is the definition of correctness for the spine. The
project is *a superset by design* and pursues *maximal utility*: the bundle is the
one place that holds everything, so the bundle must in fact hold everything. An
artifact that exists on disk but is **not** reconstructible from the bundle is a
defect — the build produced something the canonical deliverable does not carry, and
the one-direction-to-`gmeow.gts` dataflow is broken. When a stage's output is not in
the bundle, the fix is to make the stage **attach** it (§3), not to special-case the
file.

The law follows directly from Principle 4 (the bundle is the canonical source, the
flat files its projections) and Principle 5 (maximal superset). It also makes the
bundle *self-sufficient*: a consumer with only `gmeow.gts` can reconstruct every
view without the repository (Principle 13).

## 6. Fanout

Reconstructing the flat consumer tree is a **separate phase that runs after the
pipeline ends**. Fanout is pure projection: it reads `gmeow.gts` and writes files,
performing **no** computation, reasoning, or assembly. Because each extraction is
independent and reads the same immutable bundle, fanout is embarrassingly parallel
and is driven as ordinary build targets.

Every committed output is therefore the meeting of two halves:

- a **carrier contribution** — the producing stage attaches the bytes (or the graph
  they fold from) to the bundle during the pipeline; and
- a **fanout extraction** — the post-pipeline phase projects those bytes back to
  their path under `generated/`.

Fanout is built on the existing bundle-introspection surface — the `gmeow export`
consumer views, and the GTS structural verbs (`fold` for named graphs, `extract`
for a blob by digest, `unpack` for a files-profile archive). No output requires a
bespoke generator at fanout time; an output that cannot be produced by extraction
alone signals a §5 violation upstream, not a need for computation downstream.

### 6.1 Worked instance — the GMN-1 ecosystem projections

The GMN-1 (Grounded Model Notation) ecosystem is the two-halves law at work over a
family of related outputs, every one a projection of `gmeow.gts` (§5) and none
authored on disk. Two producers contribute to the carrier:

- **The lang projection producer** (inside `stage-mappings`) attaches the
  graph-derived GMN notation surfaces. Fanout extracts them under
  `generated/projections/lang/`: the formalism grammars `ebnf/gmn.ebnf` and
  `abnf/gmn.abnf`, and — keyed by the graph-resolved dialect major under
  `gmn1/v<major>/` — the constrained-decode grammars `gbnf/gmn.gbnf` and
  `lark/gmn.lark`, the math-grounded `token-metrics.ttl` (a `gmeow:Measurement`
  7-vector with a byte-fallback compression gate), the `verbalizations.ttl`
  GMN↔controlled-NL `lang:translationCorrespondence` pairs, and the per-example
  `*.gmn` witnesses.
- **`stage-gmn-training-corpus`** — a **new registered generator stage**, the
  first dedicated `lang:` generator stage (the `lang:` sibling of
  `stage-math-producers`) — consumes `stage-compile-logic` (the typecheck/prover
  lane) and `stage-mappings` (the glyph registry), enumerates well-typed GMN terms,
  rejection-samples each through five deterministic verifiers, and attaches the
  proof-carrying corpus (plus its typed rejections) as the bundle-internal named
  graph `graph/gmn-training-corpus` (dual-carriage, exactly like
  `graph/goal-directed`).

The ~500-token GMN-1 teachability primer is not a separate file: `stage-docs-render`
folds it into the `llms.txt` / `llms-full.txt` surfaces, and the MCP server serves
the identical bytes off the bundle alone as the `gmeow://ontology/gmn1-primer`
resource. Whole-ecosystem tamper-evidence is folded into `pack_root`; the superset
gate (§7) keeps every one of these paths byte-reconstructible from the bundle.

### 6.2 Worked instance — the medium dictionaries

The shipped zstd dictionaries are the two-halves law at work over a family whose
canonical form is **not a file at all**, which is what makes it the sharper worked
instance. Two producers contribute to the carrier:

- **`stage-archive-blobs`** folds the by-reference TAR archives once. It is the
  upstream half: the archive-rep corpus selectors resolve against *its* product, so
  a dictionary is trained over the same in-memory bytes the bundle is about to
  carry rather than over a previous build's copy on disk.
- **`stage-medium-dictionaries`** — the single producer of the bundle's
  dictionaries — trains each declared `gmeow:CompressionDictionary` over its
  declared `gmeow:DictionaryCorpus` selectors, measures each into a
  `gmeow:CompressionDictionaryRealization` (content digest, byte length, zstd
  `Dictionary_ID`, measured strategy, measured target length), and attaches the
  result as the build-time named graph `graph/medium-registry`. Its trained bytes
  ride an **internal `pipeline/` byte lane**, not a committed `generated/` file.
  The terminal then reads that product to pin every dictionary in the shipped
  segment header's in-band `"dct"` map and to seal one `gmeow:MediumEnvelope` per
  emitted frame.

**The in-band bytes are the canonical form, carried exactly once.** A dictionary's
shipping channel is the segment header a consumer primes from — that is where a
runtime store obtains one without a second artifact, a network fetch, or a repo
checkout. Routing the same bytes through the generated-opaque archive as well would
carry one high-entropy blob twice: it would re-fold a blob the snapshot already
carries (Constitution §18) and feed incompressible bytes to a compressor. So the
fanout family for these paths is neither `rdf-fanout` nor `opaque` but a third one,
**`header-dict`**: the committed path is reconstructed as the *verbatim bytes of one
entry of the header's `"dct"` map*.

Fanout therefore extracts, under `generated/medium/`:

- `generated/medium/gmeow-core-v1.zdict`
- `generated/medium/gmeow-logic-v1.zdict`
- `generated/medium/gmeow-memory-compact-v1.zdict`
- `generated/medium/gmeow-memory-hot-v1.zdict`
- `generated/medium/gmeow-prooftrace-v1.zdict`

— one `header-dict` row per shipped dictionary — plus one path that is **not** a
header dictionary and travels as RDF because it *is* RDF (§5):

- `generated/medium/dictionary-effect.ttl`, the `rdf-fanout` fold of
  `graph/medium-measurement` re-rooted into its `graph/fanout` twin: the measured
  two-part code of every shipped dictionary, taken at the terminal because that is
  the one point at which the emission's whole blob frame set exists.

The gate keys the `header-dict` family on the `.zdict` suffix, so a `.ttl` under the
same prefix falls to `rdf-fanout` by construction rather than by exception.

**Superset-gate coverage (§7) is a bijection per family.** Every entry the shipped
segment header pins resolves to exactly one authored `header-dict` row, and every
authored `header-dict` row is claimed by exactly one pinned entry — a dictionary
added to the registry without its fanout row (or a row left behind after a
dictionary is retired) is a hard failure, not a smaller expectation. The
family-scoped bijection is what keeps the three families from vouching for each
other, and a separate clause hard-fails any `generated/medium/*.zdict` path that
also appears as a generated-opaque archive member, which is the one way the
"carried exactly once" law could be broken while every other assertion still held.

## 7. The conformance gate

The superset law (§5) is machine-checked, not trusted. The gate maps every committed
path under `generated/` to its carrier representative — a named graph or an inline
blob — and reconstructs it from `gmeow.gts`. A path with no representative, or whose
reconstruction does not match the committed bytes, is a hard failure: no skips, no
optional coverage, no degraded pass (the project's low/no-optionality, hard-fail
stance). The gate is the drift check that keeps the bundle honest as a superset, the
same way the existing drift gates keep the projections honest (Principle 7).

## 8. Consequences and non-goals

- **Reason once, project many.** The reasoned closure is a single carrier graph. The
  committed closure file, the bundle's reasoning graph, and any documentation of
  inferred axioms are all projections of that one graph — never independent
  reasoning passes. Two artifacts that claim to be "the closure" but were reasoned
  separately are a razor (§3.2) violation, even if they happen to agree.
- **One serialization.** The carrier is serialized exactly once, by the terminal.
  Intermediate `gmeow.gts` emissions inside the pipeline are non-conforming; a side
  format a blob needs is produced from the in-memory carrier, not by emitting and
  re-parsing a temporary bundle.
- **Determinism.** Stage completion order does not affect the bundle: contributions
  fold by a stable key, so the emitted bytes are identical regardless of scheduling.
- **Not a format spec.** The on-the-wire layout of `gmeow.gts` (segments, blob
  encoding, signatures) is the GTS specification's domain, referenced from the
  README documentation map. This document governs only what the build puts *into* the
  bundle and how it comes back *out*.

## 9. Grounding

| This spec | Canon |
| --- | --- |
| Carrier is the sole transport; trim only at exit | *Maximal information flow*; Principle 4 |
| Bundle ⊇ every output; superset by design | *Maximal utility*; Principle 5 |
| Authored from slices; shipped to `gmeow.gts` | The standing one-direction dataflow |
| One terminal; one signed single-file bundle | Principles 14, 16 |
| Bundle self-sufficient for consumers | Principle 13 |
| Pin-on-attach; the conformance gate | Principle 7 |
| Flat files are projections of the bundle | Principle 4; Principle 17 (canon → views) |
| Hard-fail gate, no optionality | Low/no-optionality, hard-fail stance |

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The narrow waist: GTS is the only exit

Every projection of GMEOW data leaves the repository through one artifact:
**`generated/dist/gmeow.gts`** — the committed, byte-deterministic,
drift-gated GTS snapshot of the canonical sources.

```text
ontology/ + slices/          statements rdf12        SSSOM mappings
        \                         |                       /
         \                        |                      /
          +----------- gts generator (compile_gts) ----+
                              |
                    generated/dist/gmeow.gts
                    (default graph | gmeow:graph/statements
                     | gmeow:graph/alignments
                     | gmeow:graph/imports
                     | gmeow:graph/metadata)
                              |
            +---------+------+--------+----------+
            |         |               |          |
         exports   metadata        schemas      lpg
        (CSV/MD/  (VoID/DCAT)   (LinkML → 4   (Cypher/
        JSONL/                     targets)    GraphML/CSV)
        llms.txt)
```

## The rules

1. **One producer.** `gmeow-dev compile-gts` — the terminal sink of the Rust
   pipeline (`crates/pipeline/src/stages/gts_sink.rs`) — is the only code that
   reads the canonical sources for export purposes. It canonicalizes blank
   nodes, content-sorts the term table, and partitions sources into named
   graphs; the emitted bytes are a pure function of the inputs (cross-hash-seed
   tested). The carrier it serializes is assembled once, in memory, and is
   never re-assembled at the terminal.
2. **Many projections, one fold.** Every export leaf consumes the carrier
   **dataset** off the in-memory snapshot the pipeline already built, and no
   leaf re-reads or re-parses the canonical source tree. An intermediate
   `gmeow.gts` emission inside the pipeline is non-conforming for the same
   reason: it would be a second fold, and two folds can disagree.
3. **Sealed by Rust gate.** `crates/validate/src/repo_static.rs` runs through
   `make crate-check` and statically proves the exporter does not reach around
   the waist. Three seals do the work, and the third is the one the medium axis
   added: **Seal A** — `purrdf::gts_compose::emit_gts` has exactly one
   production caller, the `gmeow-gts-profile` leaf crate; **Seal B** — zero
   production callers, outside that crate, of *any* purrdf entry point that
   mints a header or returns GTS bytes (`gts_write::to_gts`, a bare `Writer`,
   the `files`/`from_tar` packers, `compact_streamable`), which Seal A alone
   cannot see; **Seal C** — every production site that authors GTS bytes has a
   `gmeow:GtsProducer` row declaring exactly one `gmeow:producerMedium`, and
   every row names a real call site. The static seals also keep the public CLI
   from resurrecting retired GTS subcommands.
   Generator ordering remains registry-owned and outside this static seal:
   consumers must express dependencies through declared inputs — never by hand.
4. **Equivalence before deletion.** Each re-point proved value-equivalence
   against the old implementation before the old path was deleted —
   no compatibility shims survive.
5. **Reproducible without rebase pain.** `generated/dist/gmeow.gts` is a
   git-ignored local/release product, never committed: there is no
   `.gitattributes` merge driver and no binary file to resolve during a merge
   or rebase. `make install`/`make check` materialize it from canonical
   sources; after a merge or rebase, re-run `make check` to bring the bundle
   back in step rather than resolving anything by hand.
6. **One mandatory frame profile, and the dictionary is a parameter of it.**
   Every payload-bearing frame authored by GMEOW production code uses exactly
   one transform: `zstd-rsyncable`, at zstd compression level **12**. This
   applies to small and large blob frames, the snapshot frame, transformed
   consumer output, and signed release bundles; no size threshold may fall back
   to plain `zstd`, `gzip`, or `identity`. The GTS header is not a frame, and a
   signed bundle's transport-key metadata frame has no payload bytes to
   compress.

   The transform's **dictionary** is a parameter of that one codec, not a
   second transform and not a second chain. It travels **in band**: the segment
   header's `"dct"` map carries the dictionary bytes verbatim, the catalog entry
   the frame references names which entry primes it, and the shipped registry
   declares what that name is. Because a GTS codec catalog carries one entry per
   `(codec, dictionary)` pair (spec §5), a dictionary-primed pack legitimately
   declares **several** `zstd-rsyncable` catalog ids — the unprimed one plus one
   per pinned dictionary. **The mandate is on the chain, not on the arity**:
   every declared entry must be the mandated codec at the mandated level, and
   every payload frame must reference one of them. Requiring exactly one entry
   would have made priming unrepresentable.

   The rep→medium assignment is **total**: every registered
   `gmeow:PayloadSchema` resolves to exactly one declared `gmeow:Medium`, and
   "no dictionary" is itself a named medium (`gmeow:mediumProfileBaselineL12`),
   never a hole a frame falls into. An **undeclared or unresolvable dictionary
   is a hard failure** — `gmeow:MediumUndeclaredDictionary` or
   `gmeow:MediumUnknownDictionary` — and never a dictionary-less decode. There
   is no such fallback to have: priming changes the *code*, not the framing, so
   a payload written through a primed medium is not readable at lower fidelity
   without its dictionary, it is not readable **at all**.

   The `gmeow-gts-profile` LEAF crate centralizes production authorship behind
   three doors — `emit_gmeow_gts` (snapshot bundles), `dataset_to_gmeow_gts`
   (the `convert --to gts` exit), and `GmeowGtsWriter` (append-only
   `ai-package` segments) — and compile-time asserts the upstream dist level
   remains 12. `validate_mandated_frames` audits every payload frame of a bundle
   OR of a multi-segment append-only file, and each producer runs it over its
   own output on-gate. That universal rule is deliberately **registry-free**, so
   it still binds the many GMEOW artifacts that carry no medium registry at all
   (the feedback / music / math bundles, `convert --to gts` output, the runtime
   stores). The dictionary half is the separate **declared-media** audit
   (`gmeow-dev medium-gate`), which dispatches on the artifact's own
   `gmeow:mediumSourceKind`. Making the universal rule registry-dependent
   instead would leave two escapes — a red gate, or "a registry-less bundle
   skips the medium check" — and the second is exactly the silent degradation
   the medium axis exists to forbid. Seals A and B in Rule 3 keep the profile
   crate the only door.

7. **The bundle's declared reader-capability set.** GTS §8.4 classifies
   `zstd-rsyncable` as **non-baseline**, so the shipped bundle already demanded
   a non-baseline reader before any dictionary existed; dictionary priming
   raises the contract again. Principle 13 makes the reader contract a property
   of the **deliverable**, so it is declared rather than discovered mid-decode —
   each medium carries its demands on `gmeow:requiresReaderCapability`:

   | medium | source kind | declared reader capabilities |
   |---|---|---|
   | `gmeow:mediumProfileBaselineL12` | whole-artifact | `zstd-rsyncable` |
   | `gmeow:mediumProfileDistL12` | per-rep | `zstd-dictionary`, `zstd-rsyncable` |
   | `gmeow:mediumProfileStoreL12` | header-dict | `zstd-dictionary`, `zstd-rsyncable` |

   `generated/dist/gmeow.gts` is written through `gmeow:mediumProfileDistL12`,
   so **the shipped bundle's reader-capability set is
   `{zstd-dictionary, zstd-rsyncable}`**, and an append-only runtime store
   primed from it inherits the same set. A reader that holds neither must
   surface the region as a `gmeow:OpaqueFrame` with
   `gmeow:opacityUnknownCodec` rather than decode it. `gmeow-dev medium-gate`
   compares the set an artifact's wire actually demands against the set its
   declared medium publishes and hard-fails on any difference in either
   direction: under-declaring hides a demand, over-declaring turns readers away
   from bytes the artifact would in fact serve.

## Two axes: dialect and medium

Two different things can change about a body of GMEOW data without changing which
repository it came from: **which sign system** states it, and **how the bytes that
carry it are encoded**. Both are modelled the same way — as a
`logic:Correspondence` with named endpoints, an explicit morphism class, and an
explicit preservation judgment — and they are told apart by **that judgment**,
never by which one feels more like "just an encoding".

| crossing | correspondence | morphism class | preservation | what is recovered |
|---|---|---|---|---|
| **medium** — `gmeow:mediumCorrespondence` | one per `gmeow:Medium` | `logic:SectionRetraction` | `logic:ExactPreservation` | the **bytes**, exactly: `dec ∘ enc = id` on the declared content domain |
| **GMN-0 ↔ GMN-1** — `gmeow:gmnCorrNormalToGmn` | one per dialect crossing | `logic:SectionRetraction` | `logic:ExactPreservation` | the **model**, up to notation: a codebook-witnessed isomorphism that need not recover the exact GMN-0 byte serialization |
| **GMN-1 → GMN-2** — `gmeow:gmnCorrGmnToCompacted` | one, get-leg only | `logic:BridgeView` | `logic:ValidationOnly` | **nothing** — cognitive compaction is not preservation-preserving |

Read across the rows and the axes separate cleanly:

- A **medium** is an identity on content. Encoding and decoding compose to the
  identity function on the declared domain, so the bytes that come back out are
  the bytes that went in — bit for bit. *A zstd-compressed claim is the same
  claim.* Which dictionary primed the frame, at which level, in which codec is a
  **coordinate** of the medium, never its identity; the identity is the law.
  That is why a medium is an individual on an open axis (Principle 9) rather
  than a subclass, and why there is no `lossless: true` flag anywhere on it —
  "this medium round-trips" is a judged crossing with named endpoints and an
  executed discharge, exactly like a translation crossing.
- A **dialect** is a change of sign system. GMN-0 ↔ GMN-1 still recovers
  everything, but it recovers the **model** rather than the octets: the
  round-trip is discharged at claim granularity against the RDFC-1.0 normal
  form, so each canonical-subject group inverts independently, and the only
  non-image (`lang:GmnUncoveredTerm`) is a hard fail rather than a silent drop.
- **GMN-2 cognitive compaction is on neither footing.** It is get-leg-only, and
  its output is a **new claim about older claims** (`gmeow:GmnCompaction`) —
  provenance-linked, held under its own standpoint, carrying its own
  confidence. It never overwrites its sources — every compacted claim stays
  reachable through `gmeow:gmnCompacts` — precisely because there is no leg that
  would bring it back.

**GMN-0 is the existing normal form; media are encodings.** The narrow waist is
a statement about GMN-0: `generated/dist/gmeow.gts` is that normal form, folded
once, and every rule above governs what goes into it and how it comes back out.
A medium changes only the octets a frame carries — so adding, retiring, or
re-parameterizing one can never move a claim, and the medium identity gate
proves exactly that by emitting the whole carrier twice (once under
`gmeow:mediumProfileDistL12`, once under `gmeow:mediumProfileBaselineL12`) and
comparing the folded, canonicalized results. A dialect change is a different
kind of change entirely, and it is judged on its own correspondence rather than
smuggled in as "a different serialization".

## Box Roles In The Package

GTS keeps one transport waist, but the graphs inside that package serve
different roles:

| GTS surface | Box role | Meaning |
|---|---|---|
| default graph | TBox/RBox | The authored ontology vocabulary, properties, roles, and shape-visible annotations |
| `gmeow:graph/statements` | CBox | RDF 1.2 reifiers and statement annotations: provenance, confidence, time, standpoint, evidence, and related assertion context |
| `gmeow:graph/alignments` | TBox/RBox | Projection and mapping metadata that explains how GMEOW terms and properties bridge outward |
| `gmeow:graph/imports` | TBox/RBox | Vendored or extracted reference vocabularies used for reasoning and validation context |
| `gmeow:graph/metadata` | CBox/TBox | Package, citation, VoID/DCAT, and documentation metadata about the bundle itself |

These roles are descriptive metadata for package readers and validation
diagnostics. They do not change the narrow-waist rule: exports still consume the
verified GTS fold, never the canonical source tree directly.

## Why

Exporters that each re-read and re-interpret the sources drift from one
another; exporters that consume one verified fold cannot. The remaining
export tiers shipped this way: N-Quads/TriG, the statements JSONL
bundle, SKOS, OBO Graphs, and ShEx are emitters inside the sealed `exports`
generator — `GTS → *` shims: no parser, no
drift surface, the same fold the published `gts`/`gmeow` packages read.
(OFN/OWX/OMN are release-tier ROBOT conversions in `gmeow build`; HDT was
refused — no maintained writer to pin.)

See [`GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md)
(in the `gmeow-gts` repo) for the format itself.

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GTS — Graph Transport Substrate — Specification

> **Status:** Draft `v0.3` (2026-06-11). **Stability:** the wire format below is
> a working draft and MAY change before `v1.0`.
> **Changes in v0.3 (#327):** multi-segment files (`cat`-append composition, §3.1);
> segment-scoped term-ids (§7.2); per-segment fold + value-union semantics (§7.5);
> cross-segment suppression (§11); profile union and per-section language-tag
> discipline (§13); composition-tool requirements (§14.1); vectors 15–21 (§19).
> **Changes in v0.3 (#342):** layout states and the streamable claim (§3.3, §5);
> streamable compaction with detached frame signatures (§10.1); the `stream`
> vocabulary (§13.3); the `compact` verb (§14.1); vectors 24–26 (§19).
>
> GTS is a single-file, language-independent transport for an **RDF 1.2** graph
> (statements *and* statement-level metadata) together with any **content-addressed
> binary** the graph references. Its lodestar is **reader simplicity**: a conformant
> baseline reader — "the rdflib of GTS" — is small enough to prototype in a weekend in any
> language with a CBOR library, and a consumer can do ~90% of what they would do with an RDF library
> *without parsing RDF text*. The remaining 10% is delegated to **transforms** that
> convert GTS to an operating substrate (`.ttl`, `.nq`, DuckDB, SQLite, …).
>
> GTS is explicitly **not** a database, a query engine, or an operating substrate. It
> is a *good-enough, durable, self-describing container* — the narrow waist through
> which graphs and their referenced data travel.

## 1. Overview and non-goals

GTS encodes a graph as an **append-only log of CBOR frames**. The logical graph is the
*fold* (replay) of the log. Growth is an append; "deletion" is **suppression**, never a
physical removal; optimisation is a separate, explicitly **lossy** compaction that rewrites
the log into a snapshot.

Four properties define the format:

1. **CBOR all the way down** (RFC 8949). One ubiquitous, IETF-standardised binary encoding
   with native byte strings (no base64 tax), deterministic encoding (clean content hashes),
   and CBOR Sequences — concatenated data items with no enclosing length, so append is cheap. A
   reader needs only a CBOR library.
2. **A durable transform catalog.** Each frame's payload carries a *stackable* chain of
   codecs drawn from an open, long-lived catalog (`identity`, `base64`, `base85`, `gzip`,
   `zstd`, `lzma2`, `cose-encrypt`, …). The catalog separates *structure durability* (CBOR +
   this spec, forever) from *density and confidentiality* (swappable codecs).
3. **Integrity by construction.** Every frame carries an independent **BLAKE3 self-hash** (a
   content-id) and names its predecessor's id — a git-style content-addressed chain.
   Verification is **parallel**, a damaged frame is **independently detectable** (and the
   survivors recoverable given an intact index, §9.1), and the head id transitively commits to
   all history. Cryptographic signatures and encryption (COSE,
   RFC 9052) are optional, layered, and algorithm-agile.
4. **Recursive composition (matryoshka).** A payload, after its transforms are reversed, is
   just bytes — and a GTS file is just bytes. So a payload MAY itself be a complete GTS,
   wrapped in any transform (compressed *or* encrypted). A whole signed graph can ride inside
   an encrypted field, with its own independent header, chain, and signatures (§12.1).

**Non-goals.** GTS does not define a query language, an index format mandatory for reading,
a reasoner, or a mutation protocol. Random-access query, deep traversal, and SPARQL are the
job of a transform target, not of GTS.

## 2. Terminology and conformance

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHOULD**, **MAY**, and
**OPTIONAL** are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174).

- **Log** — the ordered sequence of frames in a GTS file.
- **Frame** — one CBOR data item in the log (§6).
- **Fold** — the deterministic replay of the log into a graph state (§7.5).
- **Term** — an RDF term (IRI, literal, blank node, or quoted triple) with a stable integer id.
- **Reifier** — a term that denotes a quoted triple, carrying statement-level metadata (RDF 1.2).
- **Capability** — what a reader must hold to decode a payload: a *codec library* or a *key*.
- **Opaque node** — the graph representation of a frame the reader could not decode (§7.6).

### 2.1 Conformance classes

- A **Baseline Reader** MUST: parse the CBOR sequence; verify the id/prev chain (§9.1); fold `terms`,
  `quads`, `reifies`, `annot`, `blob`, `suppress`, `meta`, and `snapshot` frames; support the
  `identity`, `gzip`, and `zstd` codecs; and surface any frame it cannot decode as an opaque
  node (§7.6). It MAY ignore signatures and encryption.
- A **Streaming Reader** is a Baseline Reader that processes frames one at a time and emits to a
  sink **without materialising the whole graph**: it maintains only the term dictionary (and a
  running chain check), giving O(distinct-terms) memory rather than O(triples) (§7.7). The
  `gts → duckdb`/`sqlite` transforms (§14) are Streaming Readers and run in bounded memory.
- A **Full Reader** additionally verifies COSE signatures, decrypts COSE-encrypted frames for
  which it holds keys, MAY recurse into nested GTS blobs (§12.1), and MAY use the optional index
  frame (§6.2) for parallel verification and random access.
- A **Writer** MUST emit deterministic CBOR (§4) for any bytes that are hashed or signed, and
  MUST compute each frame's `"id"` self-hash and set `"prev"` to the previous item's `"id"`.

### 2.2 The reader contract ("the rdflib of GTS")

A Baseline Reader SHOULD expose at least:

```text
open(bytes|path)            -> Graph          # parse + verify chain + fold
Graph.quads()               -> iterator[(s,p,o,g)]   # term ids resolved to terms
Graph.term(id)              -> Term
Graph.annotations(reifier)  -> iterator[(prop, value)]
Graph.blob(digest)          -> bytes | OpaqueRef
Graph.opaque()              -> iterator[OpaqueNode]
Graph.to_nquads(out)        # §14
```

Every one of these is a few lines over the folded in-memory tables; there is no tokeniser,
no IRI resolver, and no prefix machinery.

### 2.3 Reader diagnostics

Readers surface machine-observable diagnostics with these canonical classes (an implementation
MAY map them to error returns or structured warnings):

| class | meaning |
|---|---|
| `TornAppendError` | trailing incomplete CBOR item at EOF (§3) |
| `DamagedFrame` | self-`"id"` mismatch / invalid frame hash (content corruption); opaque `reason:"damaged"` (§7.6) |
| `BrokenChain` | valid frame hash, but `"prev"` ≠ the previous item's `"id"` (insertion / reorder / splice) (§9.1) |
| `TruncatedLog` | a head commitment is present but the observed head differs (§9, §18) |
| `UnknownCodec` | a transform names a codec the reader lacks; opaque `reason:"unknown-codec"` |
| `MissingKey` | an `encrypt` codec the reader cannot decrypt; opaque `reason:"missing-key"` |
| `ConflictingReifier` | a reifier rebound to a different triple (§7.8) |
| `RecursionLimit` | nested-GTS depth or decoded-size budget exceeded (§12.1, §18) |
| `StreamableLayoutError` | a segment claims `"layout": "streamable"` but its covered region violates delivery ordering, or its index footer is missing or contradicts the frames it covers (§3.3) |

## 3. File structure

A GTS file is a **CBOR Sequence** (RFC 8742): zero framing bytes between items, each item a
well-formed CBOR data item. The file MAY begin with the CBOR self-describe tag `55799`
(`0xd9 0xd9 0xf7`) as a magic number. If present, tag `55799` MUST tag the **Header** data item;
it is not a separate log item, has no `"id"`, and does not participate in the id/prev chain.

```text
GTS-file = segment *segment
segment  = [self-describe-tag] header *frame
```

- The **first** data item of a segment MUST be a **Header** (§5).
- Every subsequent data item of a segment is a **Frame** (§6), in log order, until the next
  Header (which begins a new segment) or end of input.
- **Append** = concatenate one more frame (extending the last segment), or concatenate a whole
  further segment (§3.1). No length prefix or count is stored, so a writer never rewrites
  earlier bytes.

### 3.1 Multi-segment files (`cat`-append composition)

A GTS file is one or more **segments**, each a complete, self-contained
`header *frame` log. The defining property: **byte-concatenation of valid GTS files is a
valid GTS file** —

```sh
cat music.gts >> core.gts        # core.gts is now a valid two-segment GTS
```

- **Boundary detection (normative).** A reader that has consumed at least one frame and
  encounters a data item that is a map containing the key `"gts"` and lacking the key `"t"`
  MUST treat it as the Header of a **new segment** (the optional self-describe tag `55799`
  MAY precede it; writers SHOULD emit the tag on every segment header to make boundaries
  eyeball-visible). Any other non-frame item remains malformed input (§17).
- **Independent integrity.** Each segment has its own genesis (its header `"id"`), its own
  id/prev chain, its own signatures, and its own optional `index` (an index covers ONLY its
  segment). The file's composite identity is the **ordered list of segment head ids**. A
  third-party segment carries its own signer; concatenation rewrites nothing (a `cat` cannot
  rewrite an earlier segment's header without breaking its self-hash — by design).
- **Identity across segments.** Term-ids are **segment-scoped** (§7.2); the ONLY cross-segment
  identity is the term **value** (IRI, literal, quoted-triple structure). Blank-node labels are
  segment-local and MUST NOT be merged across segments (the §12.1 nested-GTS rule, applied at
  the top level).
- **Profiles union.** The file's effective profile/requirement set is the union of the segment
  headers' `"prof"` values (and any profile requirements carried in segment metadata). A reader
  lacking the capabilities a segment requires degrades that segment's frames to opaque nodes
  (§7.6) — "this data needs the gmeow-music profile" is a header read, not an error.
- **Relationship to nesting.** Nested GTS (§12.1) composes by *containment* (a sealed,
  independently-shippable subgraph); segments compose by *concatenation* (open, tool-free
  aggregation). Both yield a union fold; choose nesting when the part must travel or seal
  independently, segments when plain `cat` must work.

### 3.2 Streaming and progressive enhancement

The append-only log makes streaming a **property of the format**, not a feature of a tool.
Three facts compose, and conformant implementations MUST preserve all three:

- **Prefix-fold validity (normative).** Every byte prefix of a valid GTS file that ends on a
  data-item boundary is itself a valid GTS file, and a reader MUST fold it to exactly the
  state it would reach folding those same items inside the complete file. A live stream in
  flight is therefore *indistinguishable* from a file with a torn append (§3): the partial
  trailing item means "not yet arrived", and a consumer MAY keep reading as bytes land
  (`tail -f` semantics) — every intermediate fold is a real, usable graph state, never a
  half-parsed error state.
- **Monotone refinement.** Appended frames only ever *add* knowledge: quads accumulate
  (§7.8 set semantics), a reifier binding is first-wins so an established rendering never
  changes under it, and suppression is an additive display overlay (§11) — arrival of a
  `suppress` frame refines presentation without invalidating any prior fold. The chain check
  is likewise incremental: O(1) state (the expected `"prev"`) verifies each frame as it
  arrives.
- **Chunk-safe framing.** CBOR Sequence items are self-delimiting, so item boundaries are
  safe re-chunking points for relays and proxies, and resumption is content-addressed: a
  receiver that states the last frame `"id"` it verified can be resumed from the next byte
  with no negotiation beyond that hash.

**Progressive enhancement.** Producers SHOULD order content most-significant-first so an
early prefix is maximally useful: within a segment, `terms`/`quads` (the graph) before bulky
`blob` frames, and small or preview manifestations before large ones; across a file, segments
ARE the enhancement layers — a base segment (core graph + thumbnails) followed by enhancement
segments (full-resolution blobs, computed projections) gives a receiver a complete, verifiable
package at every segment boundary, §3.1's composition rules applied as a delivery schedule.
**Checkpoint `index` frames** (§6.2) emitted periodically give a streaming consumer
intermediate truncation anchors (`"head"`), random-access offsets for ranged re-fetch, and a
manifest of what has arrived; the index remains an accelerator, never a dependency (§3, §6.2).

**The manifest is the graph.** GTS needs no table-of-contents structure, because the frames
that *describe* content can precede the frames that *carry* it: a producer SHOULD emit the
quads naming each upcoming manifestation — its content digest, media type, size, role —
before the `blob` frames whose bytes they promise. The fold of an early prefix then contains
the delivery schedule as ordinary knowledge: every digest the graph names but the stream has
not yet delivered is a content-addressed IOU, so "stop here", "skip ahead" and "range-fetch
only the RAW file" are *informed* consumer decisions, taken against a verifiable catalog
rather than a guess. (A blob that never arrives in this file is simply an external blob, §12
— the reference degrades gracefully to "bytes live elsewhere".)

*Worked delivery schedule* — a photograph as a progressive stream; a consumer may stop at any
item boundary with a complete, verified package of everything above its stopping point:

```text
header                          profile, codec catalog
terms/quads                     the catalog: Work + every manifestation below,
                                each with digest, mt, size, role (the IOUs)
blob  image/webp        ~20 KB  thumbnail — first paint
blob  image/jxl         ~8 MB   full-resolution render
terms/quads                     scene description (what is IN the image)
blob  image/x-raw       ~80 MB  RAW sensor dump
meta/quads                      full camera metadata
terms/quads/annot               AI analysis as RDF, statement-level provenance
terms/quads/annot               opinions — standpoint-qualified claims
terms/quads                     processing-pipeline provenance
index                           footer: offsets, head anchor, MMR (§6.2)
```

A casual viewer stops after the thumbnail; an archivist takes everything; an editor
range-fetches the RAW by digest after reading only the catalog. Same bytes, same chain,
three consumers.

A reader streams items until end of input. Trailing partial bytes (a torn append) MUST be
detected and ignored with a diagnostic: a reader attempts to decode each successive CBOR item,
and if the decoder signals an incomplete item or unexpected EOF at end-of-file, it MUST treat
the trailing bytes as a torn append, ignore that incomplete item, and surface a
machine-observable diagnostic (e.g. a `TornAppendError` warning). In particular, if a crash
occurred while writing an `index` frame (§6.2) the trailing index is torn: a reader MUST ignore
it and fall back to an earlier intact `index` or to a plain **sequential scan**, so every
surviving frame remains recoverable. The optional index is an accelerator, never a dependency.

Every property above holds for any frame order; what a producer *chooses* as the order is a
separate, named concern: a segment is in one of two **layout states** — **accretive**
(append-ordered) or **streamable** (delivery-ordered) — defined next (§3.3).

### 3.3 Layout states: accretive and streamable

A GTS segment is always valid and always prefix-foldable (§3.2), but it lives in one of two
layout states:

- **Accretive** — append-optimized. Frames land in arrival order (live capture, agent memory
  accrual, evidence accrual). Writes are cheap forever and the stream is consumable as it
  lands, but significance is not front-loaded and the catalog may trail the bytes it
  describes. This is the default state; it is never declared.
- **Streamable** — delivery-ordered. The catalog *presages* the payload: a **leading streaming
  index** (ordinary `terms`/`quads` frames in the `stream` vocabulary, §13.3 — one
  `stream:Manifestation` per promised blob, carrying digest, media type, size, role, and
  intended order) precedes every `blob` frame, blobs follow most-significant-first, and a
  trailing offset `index` (§6.2) closes the covered region as the random-access footer.

Append-friendly and stream-optimal are different *layouts of the same content* (precedent:
mp4 `faststart`, zip central-directory rewrites, LSM compaction). A one-pass writer cannot
produce the second state, so conversion is an explicit rewrite — **streamable compaction**
(§10.1), exposed as `gts compact --streamable` (§14.1).

**The claim (normative).** A segment declares the streamable state with the optional header
key `"layout": "streamable"` (§5). The claim is per-segment (each segment has its own header,
§3.1) and tamper-evident (the header self-hash covers it). Streamability is a
**declared-vs-computed claim** in the sense of §14.1 — refuse-don't-trust:

- The **covered region** of a claimed segment is the prefix delimited by the segment's **last
  intact `index` frame**: `"count"` frames, ending at the frame whose `"id"` equals the
  index's `"head"`. The footer MUST immediately follow the frames it covers (`"count"` =
  the index's own frame position − 1) — otherwise frames could sit between the covered
  prefix and the footer, counted neither as covered nor as accretive tail. A claimed
  segment with no intact `index` frame, whose last index is not immediately adjacent to
  its covered prefix, or whose `"head"` does not equal the id of frame `"count"`, is in
  violation.
- Within the covered region, every inline `blob` frame MUST be preceded by a `quads` frame
  that describes its digest via `stream:digest` (§13.3) — catalog-before-payload. A covered
  blob delivered before its description is in violation.
- A reader encountering a violation MUST surface a **`StreamableLayoutError`** diagnostic
  (§2.3); a verifying tool treats it as an error (§14.1). The claim can never rot against
  the bytes.

**Appends after compaction are legal and foldable.** Frames after the last `index` are simply
*unpresaged*: they are the segment's **accretive tail**, carry no ordering obligation, and
trigger no diagnostic. The segment is then "streamable through frame *N*, accretive after" —
tooling SHOULD report the boundary (§14.1). Re-compact to re-streamline. Likewise, a segment
appended by `cat` makes no claim unless its own header claims.

**In-flight prefixes.** A prefix of a streamable segment cut before the trailing `index` has,
by construction, a claim and no footer yet; a streaming consumer MUST NOT treat the missing
footer as a lie while input may still be arriving — the missing-footer violation applies to a
*complete* file. The catalog-before-payload rule, by contrast, is prefix-stable: a violation
observed in any prefix is a violation of the whole file.

## 4. CBOR conventions

- Maps use **short text-string keys** (e.g. `"t"`, `"d"`) for self-description and eyeball
  debuggability; compactness is the transform layer's job, not the schema's.
- Any bytes that are **hashed or signed** MUST use **Deterministic Encoding** (RFC 8949 §4.2):
  shortest-form integers, definite-length items, and map keys sorted **bytewise on their
  encoded form** — explicitly the RFC 8949 rule, NOT RFC 7049's length-first canonical
  ordering. (For the short text keys GTS itself uses the two coincide, because a CBOR text
  string's initial byte embeds its length; the rules diverge on mixed-type keys, so
  implementations MUST NOT rely on a CBOR library's legacy "canonical" mode without checking
  which ordering it implements.)
- Unsigned integers are used for all ids. BLAKE3 digests are 32-byte (256-bit) byte strings.
- The grammar below is given in **CDDL** (RFC 8610).

```cddl
term-id      = uint            ; append-order, frozen (§7.2)
digest       = bstr .size 32   ; BLAKE3-256
content-id   = digest          ; a frame's self-hash (§9.1)
codec-id     = uint            ; index into the header codec catalog (§8)
```

## 5. Header

The Header is the first data item and the chain genesis; it is not a frame (it has no `"prev"`).

```cddl
header = {
  "gts"  : "GTS1",                    ; magic / format id
  "v"    : uint,                      ; spec major version (1)
  "prof" : tstr,                      ; profile (§13); "generic" if unspecified
  "cat"  : { * codec-id => codec },   ; the transform catalog (§8)
  ? "layout": tstr,                   ; layout-state claim (§3.3); absent = accretive
  ? "dct": { * tstr => bstr },        ; named, UNCOMPRESSED dictionaries for dict-codecs
  ? "meta": any,                      ; free-form, non-normative metadata
  "id"   : content-id,                ; self-hash of the header content (the chain genesis)
}

codec = {
  "name" : tstr,                      ; "identity" | "gzip" | "zstd" | "lzma2" | "cose-encrypt" | ...
  "cls"  : "encode" / "compress" / "encrypt",
  ? "dct": tstr,                      ; references header "dct" key (dict codecs)
  ? "p"  : any,                       ; codec parameters (e.g. lzma2 level)
}
```

The catalog is **closed within a file** (a frame may only reference codec-ids the header
declares) but **open across the ecosystem** (new codecs may be registered by name). The
Header carries its own `"id"` (self-hash of its content) and no `"prev"` — it is the genesis,
and the first frame's `"prev"` is the Header's `"id"`. The Header `"id"` MUST equal the
BLAKE3-256 of the deterministic CBOR of the Header map **excluding the `"id"` key**; all other
keys (including `"meta"`) participate. The optional `"layout"` key claims a layout state
(§3.3): the only value defined by this revision is `"streamable"`, which a verifying reader
MUST check against the segment's actual layout; readers MUST ignore unknown `"layout"` values
(forward compatibility — an unknown state imposes no check). Dictionaries are stored **uncompressed
and in-band** — there is no external-dictionary dependency. A codec's `"dct"` value MUST match
a key in the header `"dct"` map, and the codec MUST use the corresponding byte string as its
compression/encoding dictionary.

## 6. Frames

All frames share one envelope:

```cddl
frame = {
  "t"   : frame-type,        ; discriminator
  ? "x" : [+ codec-id],      ; transform chain, applied in order on encode; default [identity]
  ? "pub": any,              ; CLEARTEXT public envelope (always readable; §9.4)
  ? "to": [+ recipient],     ; recipients, for encrypt-class chains
  ? "d" : bstr / any,        ; payload: bstr when "x" transforms it; structured CBOR otherwise
  "prev": content-id,        ; the PREVIOUS data item's "id" (chain link; §9.1)
  "id"  : content-id,        ; BLAKE3-256 self-hash of this frame's CONTENT (all keys but "id"/"sig")
    ? "sig": bstr,           ; COSE_Sign1 over "id" (§9.2)
}

frame-type = "terms" / "quads" / "reifies" / "annot" / "blob" / "suppress"
           / "snapshot" / "meta" / "index" / "opaque"

recipient = { "kid": tstr, ? "alg": tstr, * tstr => any }   ; key identifier; never the key
```

Each frame's `"id"` MUST equal the BLAKE3-256 of the deterministic CBOR of its content (every
key except `"id"` and `"sig"`). Each frame's `"prev"` MUST equal the previous data item's
`"id"`; the **first** frame's `"prev"` is the Header's `"id"`. Because `"prev"` is inside the
hashed content, each `"id"` transitively commits to all prior frames (§9.1).

### 6.1 Payload resolution

To obtain a frame's logical payload:

1. If `"x"` is absent, the payload is `"d"` directly (structured CBOR) — equivalent to a single
   `identity` transform; a chain resolving to only `identity` likewise leaves `"d"` unchanged.
2. If `"x"` is present, `"d"` MUST be a byte string and every codec-id MUST resolve through the
   header `"cat"`; apply the **reverse** of each codec, last to first. Each step requires a
   **capability** (§8.3). On any missing capability (unknown codec or missing key), stop and
   treat the frame as **opaque** (§7.6).
3. The fully-decoded bytes are a CBOR item; decode them to the type-specific structure (§7).

### 6.2 Index frame (optional)

A writer MAY append an `index` frame — a footer that accelerates large files without raising
the simple-reader floor (a Baseline Reader ignores it). Because the log is append-only, a fresh
`index` MAY be appended after more frames; the **last** `index` wins.

```cddl
index-payload = {
  "count"  : uint,                        ; frames covered
  "head"   : content-id,                  ; "id" of the last covered frame (truncation anchor)
  ? "off"  : [+ uint],                    ; byte offset of each frame (random access; parallel verify)
  ? "ti"   : { * frame-type => [+ uint] },; frame indices by type
  ? "dict" : [+ uint],                    ; indices of "terms" frames (dictionary locator; §7.7)
  ? "mmr"  : content-id,                  ; Merkle-Mountain-Range root over frame ids (§9.1)
}
```

Given `"off"`, a Full Reader dispatches frame-hash verification across threads and seeks to any
frame; given `"dict"`, a Streaming Reader loads only the dictionary (§7.7); given `"head"`/
`"mmr"`, it detects truncation and produces O(log n) inclusion proofs. A **checkpoint** index is
simply an `index` emitted periodically rather than only as a footer; an earlier `index` MAY still
serve as a recovery anchor even though the last intact `index` is preferred for acceleration.

## 7. Graph data model and fold

The folded graph is four tables built from the log.

### 7.1 Terms (`terms` frame)

Payload: an **ordered array** of terms. Ids are assigned by append order across the whole log.

```cddl
terms-payload = [+ term]
term = {
  "k"   : 0 / 1 / 2 / 3,   ; 0=IRI 1=literal 2=bnode 3=quoted-triple
  ? "v" : tstr,            ; IRI string | literal lexical form | bnode label
  ? "dt": term-id,         ; literal datatype IRI (a term)
  ? "l" : tstr,            ; literal language tag (BCP 47)
  ? "rf": term-id,         ; quoted-triple: the reifier (§7.3) whose triple this term denotes
}
```

**Literal datatype defaulting (normative).** For a `k:1` (literal) term: if `"l"` (language
tag) is present and `"dt"` is absent, the datatype is `rdf:langString`; if both `"l"` and
`"dt"` are absent, the datatype is `xsd:string`.

**Blank-node labels (normative).** A `k:2` (blank node) term's `"v"` label is **local to the
GTS file** and MUST NOT be treated as a globally stable identifier; transforms MAY relabel blank
nodes while preserving blank-node isomorphism.

### 7.2 Term-id assignment (normative)

Term ids are unsigned integers assigned **in append order, per segment**, starting at `0` at
each segment's header, and are **frozen within their segment**: a term minted while folding
frame *N* keeps its id for the rest of that segment. A `quads`, `annot`, or `reifies` frame at
position *N* MUST only reference term-ids introduced at positions `0..N-1` **of the same
segment** (such frames introduce no terms of their own). This makes writing pure-append,
reading single-pass, and concatenation sound: term-ids are **compression artifacts, never
identity** — cross-segment identity is the term *value* only (§3.1), exactly as a `snapshot`'s
dictionary already restarts at `0` (§10). An implementation that applied file-global ids to a
multi-segment file would misfold silently; the boundary rule (§3.1) and vector 17 (§19) exist
to make that failure loud instead.

### 7.3 Quoted triples and reifiers (`reifies` frame)

RDF 1.2 lets a triple be the subject or object of another. GTS keeps quoted triples in the
id domain: a **reifier** is an ordinary IRI/bnode term; a `reifies` frame binds it to the
triple it quotes.

```cddl
reifies-payload = { * term-id => [term-id, term-id, term-id] }  ; reifier => (s, p, o)
```

A quoted triple used as a node is a term with `"k": 3` and `"rf"` pointing at its reifier.

**RDF 1.2 mapping (normative).** A `reifies` binding `R => (S,P,O)` means the triple
`R rdf:reifies <<( S P O )>>`; a `k:3` term denotes that triple term, reached through its
reifier `R`; and each `annot` row `(R, P', V')` is the triple `R P' V'`. **Quotation does not
imply assertion:** referencing a triple term (via a reifier or a `k:3` term) does NOT assert the
base triple `(S P O)`. The base triple is asserted iff it also appears in a `quads` frame.

### 7.4 Quads and annotations

```cddl
quads-payload = [+ [term-id, term-id, term-id, ? term-id]]  ; s, p, o, (g; default graph if absent)
annot-payload = [+ [term-id, term-id, term-id]]             ; reifier, predicate, value
```

Statement-level metadata (confidence, validity interval, standpoint/vantage, modality, …) is
expressed as `annot` rows on a reifier. **Contested claims coexist**: several `annot` rows on
one reifier, or several reifiers over one (s,p,o), are all retained — none is privileged.

**Position constraints (normative).** In a `quads` row the predicate `p` MUST be an IRI (`k:0`);
the subject `s` MUST be an IRI, blank node, or quoted triple (`k:0|2|3`); the object `o` MAY be
any term; and the graph name `g`, when present, MUST be an IRI or blank node (`k:0|2`) — never a
literal or quoted triple. A `reifies` triple `(S,P,O)` obeys the same subject/predicate/object
constraints. In an `annot` row the predicate MUST be an IRI.

### 7.5 Fold algorithm (normative)

```text
result := empty union state (graph, reif, annot, blobs, meta, suppressed, opaque[])
for segment in file order:                      # §3.1; single-segment files: one iteration
  verify each frame's id (self-hash) and prev-link within the segment;
  record sig status if "sig" present
  terms := []   graph := {}   reif := {}   meta := {}   blobs := {}   suppressed := {}
  for frame in segment log order:
    P := resolve payload (§6.1); if undecodable -> add opaque node (§7.6); continue
    switch frame.t:
      "terms"    : append each term (assign next id); each "dt"/"rf" MUST name an
                   already-introduced term-id (no forward references)
      "quads"    : add each (s,p,o,g) to graph
      "reifies"  : reif[reifier] := (s,p,o)
      "annot"    : record (reifier, predicate, value)
      "blob"     : if "d" present -> blobs[BLAKE3(decoded "d")] := bytes (inline);
                   else -> register external blob by "pub".digest
      "suppress" : mark referenced subgraph/frame in `suppressed` (display contract; §11)
      "snapshot" : load a self-contained fold wholesale (§10)
      "meta"     : shallow-merge map into global meta (later keys overwrite earlier)
      "opaque"   : add explicit opaque node
  union segment fold into result BY TERM VALUE     # ids resolve locally, never cross segments;
                                                   # bnode labels stay segment-local (§3.1)
result
```

The fold is deterministic: the same log yields the same graph in every conformant reader.
Within a segment, `meta` accumulates as a shallow union over one map — a later frame's keys
replace earlier ones; values are not concatenated. **Across segments**, each segment's folded
`meta` is exposed per segment (keyed by segment head id) AND shallow-merged in file order into
a file-level view — a later segment's keys win, but the per-segment originals remain
addressable (a third-party segment's metadata is never silently absorbed).

### 7.6 Opaque nodes

When a frame's payload cannot be decoded — an unknown codec, or a `cose-encrypt` codec for
which the reader holds no key — the reader MUST NOT drop it. It MUST add an **opaque node** to
the graph carrying everything still in cleartext:

```cddl
opaque-node = {
  "id"      : content-id,      ; the frame's self-hash
  "type"    : frame-type,      ; declared "t"
  ? "pub"   : any,             ; the cleartext public envelope, if any
  ? "to"    : [+ recipient],   ; declared recipients
  "sigstat" : "none" / "valid" / "invalid" / "unverified",
  "reason"  : "unknown-codec" / "missing-key" / "damaged",
}
```

Most opaque nodes are produced by a reader at decode time; a writer MAY also emit an explicit
`opaque` frame (e.g. a redaction placeholder) whose payload is the structure above, in which
case `"sigstat"` is omitted (a reader determines it). A `damaged` frame (failed self-hash, or
absent) is isolated and folded as an opaque node too (§9.1): a reader MAY surface its cleartext
fields as **untrusted** diagnostic metadata, but MUST set `"sigstat"` to `invalid`/`unverified`
and `"reason": "damaged"` — the bytes are not trustworthy. The frame still participates in the
id/prev chain, so it cannot be silently stripped.

### 7.7 Streaming fold and bounded memory

A graph need not be materialised to be *transformed*. A **Streaming Reader** (§2.1) processes
frames in order and emits to a sink, holding only the term dictionary and the running id/prev
check:

- `gts → duckdb`/`sqlite` (§14) keep the **integer-id** model: stream `terms` deltas into a
  `terms` table and `quads`/`reifies`/`annot` deltas into id-valued tables, bulk-inserting as
  frames arrive. **No term resolution and no graph materialisation occur** — memory is O(1)
  beyond the dictionary, and the dictionary is O(distinct-terms) ≪ O(triples). The relational
  join that resolves ids is the engine's job, later.
- `gts → ttl/nq` must resolve ids to emit text. If the dictionary exceeds memory, the reader
  uses the index `"dict"` locator (§6.2) to load (or memory-map, or spill to an on-disk kv)
  only the `terms` frames first, then streams the quads.

Even O(distinct-terms) can exceed memory for pathologically irregular graphs (e.g. a crawl
dumping millions of unique UUID IRIs). A Streaming Reader therefore MAY **flush its in-memory
dictionary to a temporary on-disk key-value store** when a memory limit is reached, trading RAM
for a local spill file; correctness is unaffected because term-ids are append-order and frozen
(§7.2). The `gts → duckdb`/`sqlite` transforms get this for free — the target table *is* the
spill.

A multi-gigabyte log thus transforms to an operating substrate in bounded memory — the
resolve-and-materialise OOM failure mode is avoided by construction.

### 7.8 Duplicates and conflicts (normative)

- **Duplicate terms.** A writer SHOULD intern terms, but if two term entries are byte-identical
  they denote the **same RDF term** (each still gets its own id); resolution is unaffected.
- **Duplicate quads.** The folded graph is a **set**: identical `(s,p,o,g)` rows collapse to one.
- **Reifier bindings.** A reifier SHOULD have exactly one `reifies` binding. Repeated bindings
  for the same reifier MUST be identical; a **conflicting** binding is a data-quality error — the
  reader surfaces a diagnostic and keeps the first binding.
- **Annotations.** Multiple `annot` rows on a reifier coexist (contested claims, §7.4); they are
  not deduplicated beyond exact-row identity.

## 8. Transform catalog

### 8.1 Classes

Every catalog entry declares a **class**:

| class      | examples                         | capability needed to reverse |
|------------|----------------------------------|------------------------------|
| `encode`   | `identity`, `base64`, `base85`   | none (pure function)         |
| `compress` | `gzip`, `zstd`, `lzma2`          | a codec library              |
| `encrypt`  | `cose-encrypt0`, `cose-encrypt`  | a **key** (per recipient)    |

### 8.2 Stacking

`"x"` is applied in array order on encode and reversed on decode. Example: `[zstd,
cose-encrypt]` means *compress, then encrypt*; a reader decrypts (if keyed) then decompresses.

### 8.3 Capability model and graceful degradation

Decoding a chain requires **every** capability it names. A missing capability is uniform
whether it is a library (`unknown-codec`) or a key (`missing-key`): the frame becomes an
opaque node (§7.6). This single mechanism yields **in-file content negotiation** — a logical
object MAY appear as several frames in different codecs/formats (e.g. a high-fidelity
representation a reader can't decode *and* a widely-supported fallback it can), and the reader
uses the best frame for which it holds the capabilities.

### 8.4 Mandatory core set and durability

A Baseline Reader MUST implement `identity`, `gzip`, and `zstd` — so a conformant reader's full
dependency set is **CBOR + BLAKE3 + gzip + zstd**. Writers targeting maximum longevity SHOULD
restrict to the core set. Density-oriented writers MAY use `lzma2` with an in-band dictionary.
All core codecs are stable, widely deployed primitives.

**Rsyncable codecs.** A `compress`-class codec MAY be *rsyncable*: it periodically
synchronizes (resets) its compression state so that a local change in the
uncompressed input only affects a bounded neighborhood of the compressed
output. This improves delta-transfer tools (e.g. `rsync`) and version-control
delta compression (e.g. Git packfiles) at the cost of a small compression-ratio
overhead. The only rsyncable codec defined in this revision is `zstd-rsyncable`
(§8.5).

### 8.5 Canonical codec registry (v1)

Catalog entries are referenced by integer id within a file (§5), but each entry's `"name"` MUST
be a canonical identifier from this registry so writers interoperate:

| name            | cls        | baseline? | parameters                    |
|-----------------|------------|-----------|-------------------------------|
| `identity`      | `encode`   | yes       | none                          |
| `gzip`          | `compress` | yes       | `level`?                      |
| `zstd`          | `compress` | yes       | `level`?, `window`?, `dct`?   |
| `zstd-rsyncable`| `compress` | no        | `block_size`: uint (default 65536) |
| `lzma2`         | `compress` | no        | `level`?, `dct`?              |
| `base64url`     | `encode`   | no        | none (unpadded)               |
| `base85`        | `encode`   | no        | none                          |
| `cose-encrypt0` | `encrypt`  | no        | `COSE_Encrypt0` (1 recipient) |
| `cose-encrypt`  | `encrypt`  | no        | `COSE_Encrypt` (n recipients) |

A reader MUST match codecs by canonical `"name"`, not by catalog id (ids are file-local). Later
spec versions register new codecs by canonical name; an unknown name degrades to an opaque node
(§8.3).

## 9. Integrity and confidentiality

GTS keeps four integrity concerns distinct:

1. **Frame integrity** — the per-frame BLAKE3 self-hash `"id"` (§9.1).
2. **History integrity** — the `"prev"` content-id chain (§9.1).
3. **Origin / authorship** — optional COSE signatures (§9.2).
4. **Freshness / non-truncation** — a head commitment: a signature over the head `"id"`, or an
   index `"mmr"`/`"head"` root (§9.1, §13).

The first two are mandatory and key-free; the last two are optional and profile-driven.

### 9.1 Per-frame self-hash and content-id chain (mandatory)

Each frame's `"id"` is the BLAKE3-256 of its own content (every key except `"id"` and `"sig"`),
so a frame is **content-addressed and independently verifiable**. Each frame's `"prev"` names
the previous frame's `"id"`; because `"prev"` is part of the hashed content, the chain is a
git-style content-addressed list in which the **head id transitively commits to all history**.

- **Parallel verification.** Every `"id"` is a hash of a self-contained byte range; with the
  index `"off"` table (§6.2) all frame hashes are recomputed concurrently, followed by a trivial
  O(n) `"prev"`-equality pass. No accumulating dependency forces single-threaded reading. (The
  only inherently sequential step is discovering frame boundaries in a bare CBOR sequence — a
  cheap length-scan the index removes.)
- **Damage isolation and recovery.** A corrupt frame fails *its own* `"id"`, so damage is
  **independently detectable**. Recovery of *subsequent* frames, however, is guaranteed only
  when their byte offsets are known — from an intact `index` `"off"` table, a checkpoint frame,
  external framing, or the storage layer. In a bare CBOR Sequence (no per-frame length) arbitrary
  byte corruption can desynchronise the decoder: a reader **with** offsets skips the bad frame
  and folds the survivors (`reason: "damaged"`), while a reader **without** offsets MAY be unable
  to resynchronise past the damage. `evidence` writers SHOULD emit periodic checkpoint indexes
  (§13) so recovery is robust.
- **Tamper-evidence.** Any insertion, reordering, or mutation breaks a `"prev"` link or a self-
  hash. **Truncation** (dropping trailing frames) is detected only against a head commitment —
  a signature over the head `"id"`, the index `"head"`/`"mmr"` root (§6.2), or an out-of-band
  anchor. Opaque frames are part of the chain, so confidential frames cannot be stripped
  undetectably.

A **Merkle-Mountain-Range** (MMR) root over the frame ids (optional, carried in the index) is a
single whole-file commitment that is itself parallel to compute and supports O(log n) inclusion
proofs — proving a frame is in the log without shipping the log.

### 9.2 Signatures (optional, algorithm-agile)

A frame MAY carry `"sig"`, a `COSE_Sign1` (RFC 9052) over the frame's `"id"`. Because `"id"`
is the self-hash of the whole content — `"pub"`, `"d"` (the ciphertext, if encrypted), and
`"prev"` (the chain position) — one signature over `"id"` **binds** the public claims to the
sealed payload and to the chain position, and signing the head `"id"` thereby anchors all prior
history (§9.1). The signing algorithm is declared in the COSE header (e.g. `EdDSA`/Ed25519,
`ES256`); readers MUST honour the declared algorithm. The `evidence` and `opaque` profiles
(§13) REQUIRE signatures. Key discovery and trust anchoring (which keys are authentic, which
signers are authorised) are **profile/deployment policy**, not core GTS: `sigstat: "valid"`
means a signature is cryptographically valid under a *resolved* key, not that the key is trusted.

### 9.3 Encryption (optional)

An `encrypt`-class codec wraps the payload as `COSE_Encrypt`/`COSE_Encrypt0`. Recipients are
listed in cleartext `"to"` by **key identifier only** — never the key material. Multiple
recipients MAY share one sealed payload (each unwraps the content-encryption key with its own
key). Key escrow, rotation, and revocation are the **issuer's** responsibility and are out of
scope; a payload encrypted to a retired key MAY become permanently opaque.

### 9.4 The opacity invariant (normative)

> Opacity hides **content** — never **existence**, **provenance**, or **position**.

For every frame, `{"id", "prev", "t", "x", "to", "pub", "sig"}` MUST remain in cleartext (the
transform chain `"x"` is cleartext so a reader knows which codecs to reverse). A reader without
the relevant key therefore still learns *that* the frame exists, *what kind* it is, *who* it is
sealed for, *who* signed it, and *where* it sits in the chain. This is what makes selective
disclosure safe: a holder can carry — and a verifier can authenticate the position of — data
neither can read.

## 10. Compaction

Compaction folds a log and re-emits it as a single self-contained `snapshot` frame (re-interned
dictionary, deduplicated quads, dropped self-loops, optionally a materialised entailment
closure). A `snapshot`'s payload is a self-contained fold — the four tables plus inline blobs
and meta:

```cddl
snapshot-payload = {
  "terms"    : terms-payload,
  ? "quads"  : quads-payload,
  ? "reifies": reifies-payload,
  ? "annot"  : annot-payload,
  ? "blobs"  : { * digest => bstr },   ; inline content-addressed blobs
  ? "meta"   : any,
}
```

A reader folds a `snapshot` exactly as it would fold the equivalent sequence of `terms`/`quads`/
`reifies`/`annot`/`blob` frames; term-ids restart at `0` within the snapshot's own dictionary.
Compaction is **lossy by definition**: it discards the original per-frame signatures and the
temporal stacking of the log. A compactor:

- MUST record the provenance of the fold (source log digest, time, agent) as quads in the
  snapshot, and
- SHOULD emit a fresh signature over the snapshot.

Two artifact classes follow: an **evidentiary log** (append-only, signed, never compacted) and
a **distribution snapshot** (compacted, dense, lossy — ideal for shipping). A reader can tell
which it holds from the profile and the presence of a `snapshot` frame.

### 10.1 Streamable compaction (ordering-only)

Streamable compaction converts an accretive segment (or multi-segment file) into one
delivery-ordered segment in the streamable layout state (§3.3). Unlike snapshot compaction
above, it is **a re-authoring of the ORDERING, and only the ordering**: the folded graph,
the inline blobs, and every content-addressed fact are preserved. Three signature subjects
behave differently under the rewrite, and a compactor MUST honour all three:

- **Content signatures** (subject = a content digest: a blob's BLAKE3, a statement or claim
  hash — "this is true, signed by Bob") are ordinary quads/annotations about digests. They
  are **compaction-invariant** and survive fully intact: nothing they attest to has changed.
- **Frame signatures** (a COSE_Sign1 over a frame `"id"`, which commits to `"prev"`, §9.2)
  become **detached, not broken**: they verify against the original frame id forever. A
  compactor MUST carry every source frame signature in **compaction provenance** — one
  `stream:DetachedSignature` node per signature, recording the original frame id
  (`stream:sourceFrame`) and the original COSE bytes (`stream:cose`), plus one
  `stream:sourceHead` per source segment head (§13.3) — so each remains a *checkable claim
  about the original log*.
- **Ordering commitments** (a signed head, an index `"mmr"` root) are the only layout-bound
  attestations. They cannot survive a reordering; the compactor re-issues the ordering
  commitment (the new trailing `index` with its `"head"`, §6.2) and thereby becomes the
  **sole attester of the new ordering**. A compactor MAY additionally COSE-sign the new head.

A compactor MUST record the rewrite itself as provenance quads in the output — a
`stream:Compaction` node carrying the acting tool (`stream:agent`), the time
(`stream:timestamp`), and the source segment heads (`stream:sourceHead`) — the §10
provenance MUST, given concrete vocabulary by §13.3.

**Profiles demanding pristine third-party chain attestation.** For an `evidence` segment the
original signed chain *is* the artifact; a compactor MUST refuse it — unless it **seals the
original log verbatim** as a nested GTS blob (§12.1) inside the streamable rewrite (role
`"source"`, referenced from the provenance node via `stream:sealedSource`). The original
bytes, chain, and signatures stay byte-intact and independently verifiable inside; the outer
layout is delivery-ordered; one content digest binds them.

**Refusals (publish-class posture, §14.1).** A compactor MUST refuse: input that does not
verify cleanly (any diagnostic); and input whose fold carries a frame-addressed suppression
(`kind: "frame"`, §11) — the rewrite assigns new frame ids, so a frame-digest target would
silently dangle. Digest-addressed `blob` suppressions are carried forward verbatim
(content-addressing is layout-independent); id-addressed suppressions are carried forward
value-wise (§11).

## 11. Suppression (additive "deletion")

GTS never physically deletes. To retract or hide prior content, a writer appends a `suppress`
frame referencing the superseded subgraph or frame digest. The suppressed bytes remain present
and hash-linked; suppression is a **display/precedence contract**, interpreted by the consumer,
not an erasure. This preserves a complete, tamper-evident history.

```cddl
suppress-payload = { "targets": [+ suppress-target], ? "reason": tstr, ? "by": term-id }
suppress-target =
    { "kind": "frame",   "id": digest } /                                ; a frame, by its "id"
    { "kind": "blob",    "digest": digest } /                            ; a content-addressed blob
    { "kind": "term",    "id": term-id } /                               ; a term + quads it appears in
    { "kind": "quad",    "q": [term-id, term-id, term-id, ? term-id] } / ; one specific quad
    { "kind": "reifier", "id": term-id }                                 ; a reifier + its annotations
```

Suppression is **monotonic and additive**: a matched target is hidden from default resolution (a
`term` target also hides every quad in which the term appears); the bytes remain present and
hash-linked, and a consumer MAY surface suppressed content explicitly. There is no un-suppress in
v1 — later frames may add further suppressions, and a later identical assertion does not revive a
suppressed target.

**Cross-segment suppression (normative, §3.1).** Digest-addressed targets (`frame`, `blob`)
are file-global: a content-id names the same bytes wherever they sit, so a later segment MAY
suppress an earlier segment's frame or blob by digest. Id-addressed targets (`term`, `quad`,
`reifier`) carry term-ids, which are segment-local — they are first **resolved to term values
within the suppress frame's own segment**, and the suppression then applies **value-wise to the
whole union fold**: a `quad` target hides every matching `(s,p,o,g)` value tuple in any
segment, and a `term` target hides the term value (and the quads it appears in) file-wide.
This is what lets an appended belief-revision segment suppress a statement made by an earlier
segment without rewriting a byte of it — the earlier segment's record stays present, signed,
and hash-linked (Principle 10 at the wire level).

## 12. Binary and content-addressing

```cddl
; a `blob` frame carries raw bytes in "d" (subject to "x"); its metadata lives in cleartext "pub":
blob-pub = { ? "mt": tstr, ? "rep": tstr, ? "digest": digest }
; INLINE blob  -> "d" present; digest = BLAKE3(decoded "d").
; EXTERNAL blob -> "d" absent;  "pub".digest names bytes held elsewhere.
```

- A `blob` frame's bytes are addressed by their **BLAKE3-256 digest** — for an inline blob the
  `BLAKE3` of the decoded `"d"`, for an external blob `"pub".digest`; the graph references the
  blob by that digest. Identical bytes appearing twice are stored once by convention.
- A blob MAY be **inline** (bytes present, a self-contained package) or **external** (only the
  digest appears in the graph; bytes live elsewhere).
- A logical object MAY have **multiple representations** (`"rep"`/`"mt"` distinguishing, e.g.,
  a master and a widely-supported fallback) — see content negotiation, §8.3.
- Transforming to a text format (§14) externalises inline blobs to a sidecar directory.

### 12.1 Nested GTS (recursive composition)

A blob whose media type is `application/gts` is itself a complete GTS file. Because a payload
after transform reversal is opaque bytes, **any** frame payload MAY carry a nested GTS, wrapped
in any transform chain — `[zstd]`, `[cose-encrypt]`, or both. The normative carrier is a `blob`
whose `"pub".mt` is `application/gts`.

- **Fold semantics.** A Full Reader MAY recurse: decode the blob (subject to §6.1 capability
  rules), then fold the inner bytes as an independent GTS, exposing its result as a **subgraph**
  the parent graph references by the blob's digest. A Baseline Reader MAY treat a nested GTS as
  an ordinary blob (no recursion).
- **Independent integrity.** The inner GTS has its own header, id/prev chain, and signatures. The
  **outer** chain proves the nested blob is present and intact at its position; the **inner**
  chain proves the nested log is intact. The two guarantees compose but do not depend on each
  other.
- **Composed opacity.** If the nested GTS is reached through an `encrypt`-class transform and
  the reader lacks the key, the *entire subgraph* — including its inner header — is an opaque
  node (§7.6): the holder can carry and prove the position of a whole sealed graph it cannot
  read. This is the matryoshka case ("a whole GTS inside an encrypted field").
- **Bounded recursion.** Readers MUST enforce a maximum nesting depth and total decoded-size
  budget (§18).

This composition needs no new frame type: nesting is "a blob that happens to be a GTS."

## 13. Profiles

A profile is a named set of conventions over the one format (declared in header `"prof"`):

| profile      | shape                                                                              |
|--------------|------------------------------------------------------------------------------------|
| `generic`    | any conformant log.                                                                |
| `dist`       | a single compacted `snapshot`: vocabulary + definitions + materialised closure.    |
| `evidence`   | append-only, signatures REQUIRED, **never compacted**; the file is a custody chain.|
| `image`      | a `blob` (or several representations) + descriptive metadata + analysis frames.    |
| `ai-package` | a concept + logic + observations + opinions + refuted claims + embeddings + data.  |
| `opaque`     | `encrypt`-class frames; signatures + pseudonymous `kid`s REQUIRED; selective disclosure. |
| `bundle`       | a GTS whose `blob`s are themselves GTS files (`mt: application/gts`); §12.1.        |
| `files`        | a GTS archive of file-tree entries: each file is a blob described by path, size, mode, mtime, and media type (§13.2). |
| `music-package`| a frame-relative musical work/expression: segments, voices, tuning/time frames, tone events, degrees of freedom, and analysis claims, plus lossy projections to notation formats (§13.4). |

Profiles constrain conventions, not the wire format; a `generic` reader reads them all. The
`evidence` profile additionally REQUIRES a head commitment (§9, item 4), and writers SHOULD emit
a checkpoint `index` at least every 1024 frames or 64 MiB, whichever comes first, so a damaged
log recovers robustly (§9.1). In a multi-segment file each segment declares its own profile;
the file's effective requirement set is the union (§3.1).

### 13.1 Language-tag discipline (normative)

A producer's graph payload MAY carry **internal private-use language tags** (e.g. GMEOW's
`x-gmeow-*`): the payload of a `dist` or `ai-package` segment *is* the canonical form, and
canonical forms keep their internal tags. Every **projection section** — docs blobs, derived
views, down-projected representations, anything generated *for an external consumer* — MUST
carry **public BCP 47 tags only**; a producer that leaks private-use tags into a projection
section MUST fail at write time, not warn (vector 20). The boundary is per *role*, not per
file: one package legitimately carries a canonical payload with internal tags beside
public-tagged docs sections. (This mirrors the GMEOW generator framework's internal-tag leak
gate; the reference producer reuses its `retag` machinery at the section boundary.)

### 13.2 The `files` profile (normative)

The `files` profile is a portable, content-addressed archive of a file tree. It is the GTS
answer to tar's `c`/`x`/`d`: pack a directory into a single-segment GTS, unpack it later, and
`diff` it against a directory without byte comparison.

**Namespace.** The profile owns a small, spec-defined vocabulary at
`https://w3id.org/gts/files#` (prefix `files`). GTS independence means an unpacker MUST NOT
require GMEOW, schema.org, or any other ontology to read the archive; the vocabulary is
authored in the spec and carried as literal IRIs in the graph.

| term | IRI | shape |
|---|---|---|
| `FileEntry` | `https://w3id.org/gts/files#FileEntry` | Class. One archived file. |
| `path` | `https://w3id.org/gts/files#path` | Relative path string, `/` separators, no leading `/`, no `..` components. |
| `digest` | `https://w3id.org/gts/files#digest` | `blake3:<hex>` content digest of the file bytes. |
| `size` | `https://w3id.org/gts/files#size` | Byte size as `xsd:integer`. |
| `mode` | `https://w3id.org/gts/files#mode` | POSIX file mode/permissions as `xsd:integer` (e.g. `0o100644`). |
| `modified` | `https://w3id.org/gts/files#modified` | Modification time as `xsd:dateTime` in UTC. |
| `mediaType` | `https://w3id.org/gts/files#mediaType` | Declared IANA media type string. |

**Quad shape.** Each file in the archive is described by one blank-node `FileEntry`:

```text
_:entry a files:FileEntry ;
    files:path "relative/path.txt" ;
    files:digest "blake3:<hex>" ;
    files:size 1234 ;
    files:mode 33204 ;
    files:modified "2026-06-10T20:00:00Z"^^xsd:dateTime ;
    files:mediaType "text/plain" .
```

**Determinism.** A `files` archive MUST be byte-reproducible for the same input tree:

- Paths are sorted lexicographically by their UTF-8 byte sequence before emission.
- Modification times are normalised to UTC and serialised as `xsd:dateTime` with second
  precision (fractional seconds MAY be retained when present on the source).
- Only POSIX mode and mtime are recorded; ownership, uid/gid, xattrs, and ACLs are deliberately
  excluded — they are tar's portability tarpit.

**Inline and external blobs.** A file's bytes MAY be carried as an inline `blob` frame
(`"d"` present, digest = BLAKE3(decoded `"d")`) or as an external blob (`"d"` absent,
`pub.digest` names bytes held elsewhere, §12). By default all files are inline; a writer MAY
store files larger than a configured threshold externally by reference. Identical bytes
appearing under multiple paths are stored once by convention.

**Relationship to other vocabularies.** The profile is deliberately self-contained, but the
terms align by reference to common surface vocabularies: `files:size` ↔ schema.org
`contentSize`, `files:mediaType` ↔ schema.org `encodingFormat`, `files:modified` ↔ NFO
`fileLastModified`, `files:path` ↔ NFO `fileName`. These alignments live in GMEOW's mapping
DSL; the files profile itself does not depend on them.

### 13.3 The `stream` vocabulary (normative)

The streamable layout state (§3.3) and streamable compaction (§10.1) use a small,
spec-defined vocabulary at `https://w3id.org/gts/stream#` (prefix `stream`) — the same
independence decision as the `files` profile (§13.2): no GMEOW or external ontology is
required to stream a photo archive; the terms are authored here and carried as literal IRIs
in the graph. The vocabulary is deliberately distinct from `files#` (the two compose: a
`files` archive that is also streamable describes each file once as a `files:FileEntry` and
once as a `stream:Manifestation` — the profile check (§14.1) and the layout check (§3.3)
stay independent).

**Streaming-index terms** — one `stream:Manifestation` per promised blob, emitted in the
leading streaming index before any `blob` frame (§3.3):

| term | IRI | shape |
|---|---|---|
| `Manifestation` | `https://w3id.org/gts/stream#Manifestation` | Class. One blob this segment promises to deliver. |
| `digest` | `https://w3id.org/gts/stream#digest` | `blake3:<hex>` content digest — the IOU the blob redeems. |
| `mediaType` | `https://w3id.org/gts/stream#mediaType` | Declared IANA media type (mirrors the blob's `pub.mt`). |
| `size` | `https://w3id.org/gts/stream#size` | Byte size of the decoded blob as `xsd:integer`. |
| `role` | `https://w3id.org/gts/stream#role` | Delivery role string: `"preview"` / `"primary"` / `"source"`; open set. |
| `order` | `https://w3id.org/gts/stream#order` | Intended delivery position among the segment's blobs, `xsd:integer`, 0-based. |

**Compaction-provenance terms** — the concrete vocabulary for §10/§10.1's provenance MUST:

| term | IRI | shape |
|---|---|---|
| `Compaction` | `https://w3id.org/gts/stream#Compaction` | Class. One rewrite event (a blank node). |
| `agent` | `https://w3id.org/gts/stream#agent` | The acting tool, a string (e.g. `"gts-compact"`). |
| `timestamp` | `https://w3id.org/gts/stream#timestamp` | Rewrite time as `xsd:dateTime` in UTC. |
| `sourceHead` | `https://w3id.org/gts/stream#sourceHead` | `blake3:<hex>` head id of one source segment; repeated per segment. |
| `sealedSource` | `https://w3id.org/gts/stream#sealedSource` | `blake3:<hex>` digest of the nested-GTS blob holding the verbatim original (§10.1). |
| `DetachedSignature` | `https://w3id.org/gts/stream#DetachedSignature` | Class. One carried-over frame signature (a blank node). |
| `sourceFrame` | `https://w3id.org/gts/stream#sourceFrame` | `blake3:<hex>` original frame `"id"` the COSE signature verifies against, forever. |
| `cose` | `https://w3id.org/gts/stream#cose` | The original COSE_Sign1 bytes, base64url (unpadded) literal. |

**Quad shape** (a compacted segment's streaming index, then provenance):

```text
_:m0 a stream:Manifestation ;
    stream:digest "blake3:<hex>" ;
    stream:mediaType "image/webp" ;
    stream:size 20480 ;
    stream:role "primary" ;
    stream:order 0 .
_:c a stream:Compaction ;
    stream:agent "gts-compact" ;
    stream:timestamp "2026-01-01T00:00:00Z"^^xsd:dateTime ;
    stream:sourceHead "blake3:<hex>" .
_:s0 a stream:DetachedSignature ;
    stream:sourceFrame "blake3:<hex>" ;
    stream:cose "<base64url>" .
```

**Claim coupling (normative).** Use of `stream#` terms in a segment that does NOT claim
`"layout": "streamable"` is a **warning**, not an error (§14.1): provenance quads
legitimately survive `gts → nq → gts` round trips and re-accretion after appends. The error
class is reserved for the opposite rot — a claimed layout the bytes contradict (§3.3).

### 13.4 The `music-package` profile (normative)

The `music-package` profile is a single-segment GTS that carries frame-relative musical content:
a `MusicalWork`/`MusicalExpression`, its `Voice`s and `MusicalSegment`s, `TuningSystem` and
`MusicalTimeFrame` reference frames, atomic `ToneEvent`s, `DegreeOfFreedom` declarations, and
standpoint-indexed analysis claims. It is the canonical transport form for the GMEOW music slice
and the input to every notation projection.

**Namespace.** The profile reuses the GMEOW music vocabulary
(`https://blackcatinformatics.ca/gmeow/`). A `music-package` is not required to be a `dist`
profile: it may carry only the musical content graph plus any projection blobs, and it MAY rely on
an external `dist` snapshot for vocabulary definitions.

**Header.** A `music-package` segment declares `"prof": "music-package"`. The profile is
append-only for new claims; existing triples are never deleted, only superseded by statement-layer
provenance (§7.3).

**Core quad shape.** A minimal package contains:

```text
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .

:piece a gmeow:MusicalExpression ;
    gmeow:hasVoice :voice1 .

:voice1 a gmeow:Voice ;
    gmeow:voiceTuningFrame :tuning12EDO ;
    gmeow:voiceTimeFrame :timeGrid .

:tuning12EDO a gmeow:TuningSystem .
:timeGrid a gmeow:MusicalTimeFrame .

:event1 a gmeow:ToneEvent ;
    gmeow:segmentOf :voice1 ;
    gmeow:toneEventPitchValue :pitchC4 ;
    gmeow:segmentSpan :span1 .

:span1 a gmeow:MusicalTimeSpan ;
    gmeow:hasMusicalTimeFrame :timeGrid ;
    gmeow:timeStartNumerator 0 ;
    gmeow:timeStartDenominator 1 ;
    gmeow:timeDurationNumerator 1 ;
    gmeow:timeDurationDenominator 4 .
```

Time and pitch are **frame-relative** (Principle 11): `toneEventPitchValue` points to a
`PitchValue` interpreted under the event's voice tuning frame, and offsets/durations are rational
values interpreted under the voice time frame.

**Projections.** A `music-package` MAY contain `blob` frames whose bytes are down-projected
representations (MusicXML, MEI, ABC, LilyPond, Humdrum **kern, MIDI, Scala `.scl`, tablature,
mensural, graphic notation). Each projection MUST be accompanied by a declared-loss manifest that
lists the `NotationProjectionProfile` used, the `MusicalParameter`s it can represent, and the
`ProjectionLoss`es it incurs. The manifest is a Turtle sidecar or an embedded header/comment and
is considered part of the projection, not the canonical graph.

**Bundle profile coupling.** A `bundle` profile (§12.1) whose blobs are `music-package` segments
provides the multi-movement / multi-version transport case. Each nested segment keeps its own
profile declaration; the outer bundle does not impose additional conventions.

**Verification.** A conformant `gts verify` over a `music-package` segment checks that every
`NotationSystem` referenced by a projection blob has a corresponding `NotationProjectionProfile`,
and that the profile accounts for every `MusicalParameter` declared in the music slice (no silent
omissions).

## 14. Transforms out

Transforms convert GTS to operating substrates. Each is a thin shim over the folded tables —
no RDF text parser is involved.

- `gts → nquads` / `gts → turtle` — serialise `quads` + `reifies`/`annot` (the latter as RDF 1.2
  reification). Inline blobs are **externalised** to `./blobs/<blake3>.bin`, and the graph's
  digest references resolve to those paths. Opaque frames serialise as their opaque-node
  descriptions.
- `gts → duckdb` / `gts → sqlite` — bulk-load the four tables (`terms`, `quads`, `reifies`,
  `annot`) plus a `blobs` table; create the indexes appropriate to the engine. This is a
  near-mechanical load because the GTS tables already match the relational shape.

Each transform SHOULD be verifiable by **round-trip equivalence**: for **fully-decodable**
frames, `gts → nq → gts` MUST yield the same folded graph (modulo blank-node labelling and
deterministic CBOR re-encoding). Opaque nodes are excluded — they serialise as opaque-node
descriptions and re-import as ordinary quads, not as opaque frames.

### 14.1 Composition tooling requirements (normative for conformant tools)

Raw `cat` always works (§3.1); a conformant **validating composer** (`gts cat`) and verifier
(`gts verify`) add the refuse-don't-trust posture:

- **`gts cat` MUST refuse degenerate inputs**: an input that is not a valid GTS, a segment
  whose fold yields zero quads and zero blobs (almost always a wiring bug, never a real
  package), or an output in which a suppress-only segment would hide every prior frame.
  Publish-class tools never trust a pathological state to be intentional.
- **`gts verify` MUST check declared-vs-computed requirements**: a segment whose graph uses a
  profile's vocabulary without declaring the profile is an **error**; a declared-but-unused
  profile is a warning. Declarations a tool reads (the CLI dependency report, §13) must not be
  able to rot against the content they describe.
- **`gts verify` SHOULD report per-segment**: head id, signer set, profile, term/quad counts,
  opaque-node count with reasons — the composition ledger of the file.
- **`gts verify` MUST check the layout claim** (§3.3): a segment claiming
  `"layout": "streamable"` whose covered region violates delivery ordering, or whose index
  footer is missing or contradicts the frames it covers, is an **error**
  (`StreamableLayoutError`, §2.3); `stream#` vocabulary in an unclaimed segment is a
  **warning** (§13.3). `gts info` and `gts verify` SHOULD report the streamable boundary of
  a claimed segment — "streamable through frame *N*, accretive tail of *M* frame(s)".
- **`gts compact --streamable <in> -o <out>` is the layout rewrite** (§10.1). It MUST refuse
  input that does not verify cleanly, input carrying frame-addressed suppressions, and
  `evidence` input without the seal-the-original option (`--seal-original`, §10.1); it MUST
  emit a single claimed segment in the normative streamable shape (§3.3) with compaction
  provenance and detached signatures (§13.3), and its output MUST be byte-deterministic for
  the same input and parameters (blobs ordered by ascending decoded size, ties broken by
  ascending digest; the rewrite timestamp is a parameter, not ambient time).
- **Blob extraction is verification, never conversion** (`gts ls`, `gts extract`): blobs are
  addressed by content digest (frame indices are physical accidents that shift under `cat`);
  extraction re-hashes the bytes against the requested digest; a blob suppressed by digest
  (§11) is refused by default (suppression is a display contract and extraction is display) with
  an explicit override; a media-type flag is an **assertion** against the blob's declared
  `pub.mt` — a publish-class tool refuses a mismatch rather than transcoding.

### 14.2 Archive tooling (`files` profile)

The `files` profile adds three publish-class commands. They share the refuse-don't-trust posture
of §14.1: raw byte operations are always valid GTS, but a tool refuses pathological states
rather than trusting them to be intentional.

- **`gts pack <dir|file>... -o out.gts`**
  Produce a single-segment GTS whose header declares `"prof": "files"`. Each argument is
  archived: a file is added under its basename; a directory is added recursively. The resulting
  archive contains, in order, the `terms` and `quads` describing every `files:FileEntry`,
  followed by the inline `blob` frames for the file contents. The command MUST refuse:
  - inputs that contain `..` components or absolute paths in their stored path;
  - inputs that are not readable or that disappear during the walk.

- **`gts unpack <archive> [-C dir]`**
  Write every `files:FileEntry` in the archive to the destination directory (default current
  working directory). The command MUST:
  - refuse to write outside the destination directory (`..`, absolute paths, or symlinks that
    escape it);
  - re-hash each written file and verify it matches `files:digest`;
  - restore the file's declared modification time and permissions (subject to the host OS);
  - skip entries whose digest is suppressed (§11) by default, with an explicit
    `--include-suppressed` override.

- **`gts diff <archive> <dir>`**
  Compare the archive's `files:FileEntry` set to the current state of `<dir>` by content digest.
  Report added, removed, and modified paths. Exit `0` if the directory matches the archive
  exactly; exit `1` if any path differs or if the input is refused. No byte comparison is
  needed: content addressing makes the operation O(read) on the directory.

## 15. Worked examples

CBOR is shown in **diagnostic notation** (RFC 8949 §8). Hashes/signatures are elided as `h'…'`.

### 15.1 Minimal distribution snapshot (`dist`)

```text
55799(                                   / self-describe magic /
  { "gts": "GTS1", "v": 1, "prof": "dist",
    "cat": { 0: {"name":"identity","cls":"encode"},
             4: {"name":"zstd","cls":"compress"} },
    "id": h'…header.id…' }
)
{ "t": "terms", "prev": h'…header.id…', "id": h'…terms.id…',
  "d": [ {"k":0,"v":"https://example.org/Cat"},          / id 0 /
         {"k":0,"v":"http://www.w3.org/2000/01/rdf-schema#label"},  / id 1 /
         {"k":1,"v":"Cat","l":"en"} ] }                  / id 2 /
{ "t": "quads", "prev": h'…terms.id…', "id": h'…', "x": [4],
  "d": h'…zstd([[0,1,2]])…' }                            / Cat rdfs:label "Cat"@en /
```

Term 2 is a literal with a language tag and no `"dt"`, so its datatype is `rdf:langString`
(§7.1).

### 15.2 Evidence: image + signed accrual (`evidence`)

```text
{ "t": "blob", "prev": h'…header.id…', "id": h'…',
  "pub": {"mt":"image/jp2"}, "d": h'…image bytes…',      / digest = blake3(d) /
  "sig": h'COSE_Sign1 by did:photographer' }
{ "t": "annot", "prev": h'…blob.id…', "id": h'…',
  "d": [[10,11,12]],                                     / reifier 10: capturedAt … /
  "sig": h'COSE_Sign1 by did:photographer' }
{ "t": "annot", "prev": h'…prev.id…', "id": h'…',        / later custody transfer, separate signer /
  "pub": {"event":"custody-transfer"},
  "d": [[13,11,14]], "sig": h'COSE_Sign1 by did:evidence-clerk' }
```

Nothing is rewritten; every accrual is hash-linked and independently signed.

### 15.3 Notary: partially-opaque frame (`opaque`)

```text
{ "t": "annot", "prev": h'…prev.id…', "id": h'…',
  "pub": { "claim": "I hereby notarized this document.",
           "notary": "did:notary:jane", "ts": "2026-06-09T12:00:00Z" },
  "x": [4, 7],                                            / 7 = cose-encrypt /
  "to": [ {"kid":"anon:7f3a…","alg":"ECDH-ES+A256KW"} ],  / pseudonymous kid (opaque profile, §18) /
  "d": h'COSE_Encrypt(verified ID record + provenance)',
  "sig": h'COSE_Sign1 by did:notary:jane' }
```

Anyone verifies the public notarization and its signature; only the court key decrypts the
sealed record; the signature binds the two (§9.2). A reader without the court key folds this to
an opaque node with `reason:"missing-key"`, `pub` intact, `sigstat:"valid"`.

### 15.4 Graceful degradation (`image`, content negotiation)

```text
{ "t": "blob", "prev": h'…', "id": h'…', "pub": {"mt":"image/vnd.djvu","rep":"master"}, "x":[9], "d": h'…' }
{ "t": "blob", "prev": h'…', "id": h'…', "pub": {"mt":"image/jpeg","rep":"fallback"}, "d": h'…' }
```

A reader lacking codec `9` (djvu) folds the master to an opaque node and uses the JPEG
fallback — both are present, both are hash-linked.

### 15.5 Matryoshka: a whole signed GTS sealed inside a frame (`bundle` / `opaque`)

```text
{ "t": "blob", "prev": h'…', "id": h'…',
  "pub": { "rep": "sealed-evidence-graph", "mt": "application/gts" },  / payload is itself a GTS /
  "x": [4, 7],                                            / zstd then cose-encrypt /
  "to": [ {"kid":"did:court:registry"} ],
  "d": h'COSE_Encrypt( zstd( <a complete, independently-signed GTS file> ) )' }
```

Without the court key this folds to one opaque node — a whole subgraph the holder carries but
cannot read, yet whose presence and position the outer chain proves. With the key, a Full
Reader recurses (§12.1) and folds the inner GTS — header, chain, signatures and all — into a
verifiable subgraph.

## 16. Media type and HTTP serving contract

GTS files are published artifacts (Principle 8). A conformant deployment MUST advertise the
media type, support range requests, and set cache headers that respect the format's immutability.

### 16.1 Media type and file extension (normative)

- **Media type:** `application/vnd.blackcat.gts+cbor`.
  The `+cbor` structured-syntax suffix (RFC 9277) makes the CBOR-family membership explicit.
- **File extension:** `.gts`.
- **Magic bytes:** the CBOR self-describe tag `55799` (`0xd9 0xd9 0xf7`) at the start of the
  first segment's Header. A reader MAY use these three bytes to identify a GTS file before
  parsing the rest of the CBOR Sequence.

Servers that do not recognise `application/vnd.blackcat.gts+cbor` SHOULD fall back to
`application/octet-stream` rather than a wrong text type; clients SHOULD sniff the self-describe
tag when the media type is missing or generic.

### 16.2 HTTP serving semantics (normative)

A GTS package is served like any other immutable binary release, with three extra requirements:

1. **`Accept-Ranges: bytes`** MUST be sent for every `.gts` response. The format is designed for
   partial, streaming consumption (§3.2): a consumer can fold the header and a prefix of frames
   without downloading the whole file, and Range requests map directly onto CBOR item boundaries.
2. **No transforms at the edge.** Because the bytes are a content-addressed chain, proxies and
   servers MUST NOT apply compression, minification, or any byte-altering transform. The frames
   are already compressed by the writer's chosen codec; re-compressing at the transport layer
   breaks content hashes and signatures.
3. **CORS.** A public vocabulary/dataset package is expected to be cross-origin readable.
   Responses SHOULD include `Access-Control-Allow-Origin: *` for the served `.gts` origin.

### 16.3 Immutability-aware caching (normative)

GMEOW releases are immutable (Principle 6); a GTS package URL names one exact byte sequence.

- **Versioned URLs** (`…/gmeow/1.2.3/gmeow.gts`, `…/packages/music/2026-06-11/music.gts`, or any
  URL that contains a version/date/head identifier) MUST be served with:

  ```text
  Cache-Control: public, max-age=31536000, immutable
  ETag: "<last-segment-head>"
  ```

  The natural ETag is the hex of the file's last segment head id (§3.1), because it transitively
  commits to every byte of the file. The `immutable` directive tells caches they need not
  revalidate for the one-year lifetime.
- **`latest` / conneg aliases** (URLs that resolve to the current release and may change) MUST
  NOT be cached as a single variant:

  ```text
  Cache-Control: private, no-store
  Vary: Accept
  ```

  The `Vary: Accept` prevents conneg-cache poisoning when the same path negotiates to HTML,
  Turtle, or the GTS package. This is the same cache-poisoning class addressed for slice IRIs
  by the Apache generator.

Profile selection remains URL-shaped in v0.2: one URL per package. RFC 6906 / `Accept-Profile`
is noted as a possible future extension, not required for v0.2 conformance.

## 17. Versioning and durability guarantees

- The header `"v"` is the spec major version. A reader MUST refuse a major version it does not
  implement, but MUST still verify the id/prev chain and enumerate frame types/ids.
- **Segment semantics and older readers.** A reader implementing this revision MUST support
  segment boundaries (§3.1). A reader that does NOT (a pre-§3.1 implementation) encounters a
  second Header as a non-frame data item: such input is **malformed for that reader**, and it
  MUST surface a fatal diagnostic for the remainder of the file rather than skip the item —
  *silently misfolding (applying file-global term-ids across a boundary) is the one forbidden
  outcome* (vector 17). Because `cat` cannot rewrite the first segment's header (the self-hash
  seals it), multi-segment files cannot advertise themselves in the first header; boundary
  detection is therefore structural, and the hard-fail rule is what protects the ecosystem's
  oldest readers.
- **Structure durability:** a GTS file plus this specification is decodable forever with no
  engine and no external dictionary — CBOR is an IETF standard and dictionaries are in-band.
- **Density durability:** governed by the codec catalog; the mandatory core set
  (`identity`/`gzip`/`zstd`) guarantees a baseline that any era can decode.

## 18. Security considerations

- The id/prev chain provides integrity, **not** confidentiality; use `encrypt`-class codecs for
  confidentiality.
- **Truncation** (dropping trailing frames) is undetectable from the chain alone; an `evidence`
  artifact MUST anchor the head — a signature over the head `"id"`, or the index `"head"`/`"mmr"`
  root (§6.2) — so a verifier can detect a shortened log.
- **Recovery** of frames *after* a damaged one is guaranteed only with known offsets (an intact
  index, a checkpoint frame, or external framing); a bare CBOR Sequence can desynchronise on
  arbitrary corruption (§9.1). GTS defines no parity/erasure coding — durability against bulk
  loss is the storage layer's concern.
- `"to"`/`kid` values can leak relationship metadata (who a frame is sealed for). The `opaque`
  profile therefore REQUIRES pseudonymous `kid`s; other high-privacy profiles SHOULD use them.
  Use a per-document or pairwise identifier — e.g. `"kid": "anon:<BLAKE3(true-kid ∥ head-id)>"` —
  or key blinding, so the same recipient is unlinkable across files.
- A valid signature attests the signer over the frame's bytes; it does **not** assert the truth
  of the claims (consistent with attestation semantics — vouching ≠ correctness).
- Opaque frames are unreadable but **not** invisible; do not place secrets in `"pub"`,
  `"to"`, or `"meta"`.
- Snapshot compaction (§10) destroys original signatures; an `evidence` artifact MUST NOT be
  snapshot-compacted. Streamable compaction (§10.1) detaches frame signatures rather than
  destroying them, but the re-ordered chain is attested only by the compactor; an `evidence`
  artifact MUST NOT be streamable-compacted except by sealing the original verbatim (§10.1),
  and a consumer's trust in the *ordering* of a compacted file is trust in the compactor.
- Decompression of attacker-supplied frames MUST be bounded (zip-bomb resistance); readers
  SHOULD cap decoded sizes.
- Nested GTS (§12.1) MUST be bounded: readers MUST enforce a maximum recursion depth and a
  total decoded-size budget across all nesting levels (matryoshka-bomb resistance).
- **Segments are independently authentic, not mutually vouched.** Concatenation implies no
  endorsement: segment A's signer attests nothing about segment B. A verifier MUST report
  signer sets per segment (§14.1), and a consumer deciding trust MUST NOT treat the file-level
  union as carrying the strongest segment's authority. Value-wise cross-segment suppression
  (§11) means an untrusted appended segment can HIDE earlier content from default resolution —
  readers SHOULD surface which segment suppressed what, and high-assurance consumers MAY
  resolve suppression only from segments whose signers they trust.
- A torn append at a segment boundary looks like a torn header: the §3 torn-append rule
  applies; the prior segments fold intact.

## 19. Conformance test vectors

A conformant implementation MUST pass a shared corpus. v1 requires at least these vectors
(shipped with the reference implementation), each as the GTS bytes plus the expected folded graph
(N-Quads) and the expected diagnostics:

1. Minimal valid file (header + one `terms` + one `quads`).
2. A `zstd`-transformed `quads` frame.
3. An unknown-codec frame → opaque `reason:"unknown-codec"`.
4. A frame with a wrong self-`"id"` → `DamagedFrame` opaque.
5. A torn append at EOF → `TornAppendError`, survivors intact.
6. Header self-hash verification (positive and tampered).
7. RDF 1.2 reifier + `annot` round-trip (`gts → nq → gts`), including quotation-without-assertion.
8. A nested GTS blob (`mt: application/gts`), recursed and folded.
9. Suppression over a term-id and over a frame digest.
10. Truncation detection against a signed head / index `"mmr"` root.
11. Literal datatype defaulting (§7.1): a literal with `"l"` and no `"dt"` → `rdf:langString`;
    with neither → `xsd:string`.
12. A reifier rebound to a different triple → `ConflictingReifier`, first binding kept (§7.8).
13. A position-constraint violation, e.g. a literal in predicate position → rejected/diagnosed
    (§7.4).
14. Blank-node label locality (§7.1, §12.1): identical bnode labels in an outer and a nested GTS
    remain isolated (not merged).
15. **Two-segment union (§3.1)**: `cat` of two single-segment files folds to the value-union of
    both graphs; term-ids resolve segment-locally (a shared IRI unifies; identical ids naming
    different values do NOT collide); identical bnode labels across segments stay isolated.
    *15b*: label-less blank nodes (absent **or empty** `"v"`) are distinct terms within a
    segment and across segments, and the union's serialized labels MUST keep them distinct —
    relabeling that merges what the graph separates is the forbidden outcome.
16. **Composed round-trip (§3.1, §14)**: a `cat`-composed file survives `gts → nq → gts` with
    the same union fold.
17. **Pre-segment reader hard-fail (§17, negative)**: an implementation in pre-§3.1 mode fed a
    two-segment file MUST surface a fatal diagnostic at the second header — folding frames past
    the boundary with file-global term-ids is the forbidden outcome this vector exists to catch.
18. **Cross-segment suppression (§11)**: a second segment suppresses (a) an earlier segment's
    frame by digest and (b) a quad by value; default resolution hides both; the suppressed
    segment's bytes verify intact; the verifier reports which segment suppressed what (§18).
19. **Profile union + graceful segment opacity (§3.1)**: a two-segment file whose second
    segment requires an undeclared-to-the-reader capability folds segment one fully and
    segment two as opaque nodes with the profile named in the diagnostics.
20. **Language-tag discipline (§13.1, negative)**: a producer emitting a private-use language
    tag into a projection/docs section MUST fail at write time; the same tag in a canonical
    `dist` payload section is accepted.
21. **Degenerate composition refused (§14.1, negative)**: `gts cat` refuses an empty-fold
    segment and a suppress-everything composition; raw byte `cat` of the same inputs still
    yields a structurally valid file (the tool is stricter than the format, by design).
22. **Inline blob (§12, §14.1)**: an inline blob folds to its `blake3:<hex>` digest with
    declared metadata (`pub.mt`) retained; extraction by digest re-verifies the bytes;
    a digest-suppressed blob is refused by default.
23. **Prefix-fold streaming property (§3.2, derived)**: not a vector but a property test over
    EVERY vector in this corpus — each item-boundary prefix folds without error, and across
    growing prefixes the folded tables only ever extend (terms/quads are list-prefixes while
    the segment count is unchanged; ground (blank-node-free) N-Quads lines are monotone
    across the single→multi-segment representation switch).
24. **Streamable compaction (§3.3, §10.1, §13.3)**: an accretive source (blobs interleaved
    before their catalog, one COSE-signed frame, no claim) and its compacted rewrite — the
    rewrite claims `"layout": "streamable"`, leads with the streaming index, orders blobs
    most-significant-first, closes with the offset `index` footer, and carries compaction
    provenance including the detached source signature; both files fold to the same content
    graph; the compacted bytes are **frozen** and double as the cross-engine determinism
    oracle (same input + same timestamp parameter ⇒ byte-identical output in every engine).
25. **Streamable claim that lies (§3.3, negative)**: a segment claiming
    `"layout": "streamable"` that delivers a covered blob before the quads describing its
    digest → `StreamableLayoutError`; a verifying tool MUST refuse (exit non-zero).
26. **Appended-after-compaction boundary (§3.3)**: a compacted segment with frames appended
    after its `index` footer folds cleanly with no diagnostic, and tooling reports
    "streamable through frame *N*, accretive tail" — the unpresaged tail is legal.

## 20. References

- **RFC 8949** — Concise Binary Object Representation (CBOR).
- **RFC 8742** — CBOR Sequences.
- **RFC 8610** — Concise Data Definition Language (CDDL).
- **RFC 9052 / RFC 9053** — CBOR Object Signing and Encryption (COSE).
- **RFC 2119 / RFC 8174** — Requirement-level keywords.
- **BLAKE3** — cryptographic hash function (256-bit output).
- **RDF 1.2** — RDF concepts and the quoted-triple / reifier model (statement-level metadata).
- **BCP 47** — language tags.

---

*GTS is the transport waist of the GMEOW toolchain: one `RDF 1.2 → GTS` producer, many thin
`GTS → *` shims. Because every projection derives from one folded GTS, the projections cannot
drift from one another or from the ontology.*

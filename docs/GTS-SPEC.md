<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GTS — Graph Transport Substrate — Specification

> **Status:** Draft `v0.1` (2026-06-09). **Stability:** the wire format below is
> a working draft and MAY change before `v1.0`.
>
> GTS is a single-file, language-independent transport for an **RDF 1.2** graph
> (statements *and* statement-level metadata) together with any **content-addressed
> binary** the graph references. Its lodestar is **reader simplicity**: a conformant
> baseline reader — "the rdflib of GTS" — is a weekend of work in any language with a
> CBOR library, and a consumer can do ~90% of what they would do with an RDF library
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
   and indefinite-length sequences (cheap append). A reader needs only a CBOR library.
2. **A durable transform catalog.** Each frame's payload carries a *stackable* chain of
   codecs drawn from an open, long-lived catalog (`identity`, `base64`, `base85`, `gzip`,
   `zstd`, `lzma2`, `cose-encrypt`, …). The catalog separates *structure durability* (CBOR +
   this spec, forever) from *density and confidentiality* (swappable codecs).
3. **Integrity by construction.** Every frame carries an independent **BLAKE3 self-hash** (a
   content-id) and names its predecessor's id — a git-style content-addressed chain.
   Verification is **parallel**, a damaged frame is **isolated and recoverable**, and the head
   id transitively commits to all history. Cryptographic signatures and encryption (COSE,
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

## 3. File structure

A GTS file is a **CBOR Sequence** (RFC 8742): zero framing bytes between items, each item a
well-formed CBOR data item. The file MAY begin with the CBOR self-describe tag `55799`
(`0xd9 0xd9 0xf7`) as a magic number.

```text
GTS-file = [self-describe-tag] header *frame
```

- The **first** data item MUST be a **Header** (§5).
- Every subsequent data item is a **Frame** (§6), in log order.
- **Append** = concatenate one more frame. No length prefix or count is stored, so a writer
  never rewrites earlier bytes.

A reader streams items until end of input. Trailing partial bytes (a torn append) MUST be
detected and ignored with a diagnostic: a reader attempts to decode each successive CBOR item,
and if the decoder signals an incomplete item or unexpected EOF at end-of-file, it MUST treat
the trailing bytes as a torn append, ignore that incomplete item, and surface a
machine-observable diagnostic (e.g. a `TornAppendError` warning). In particular, if a crash
occurred while writing an `index` frame (§6.2) the trailing index is torn: a reader MUST ignore
it and fall back to an earlier intact `index` or to a plain **sequential scan**, so every
surviving frame remains recoverable. The optional index is an accelerator, never a dependency.

## 4. CBOR conventions

- Maps use **short text-string keys** (e.g. `"t"`, `"d"`) for self-description and eyeball
  debuggability; compactness is the transform layer's job, not the schema's.
- Any bytes that are **hashed or signed** MUST use **Deterministic Encoding** (RFC 8949 §4.2):
  shortest-form integers, definite-length items, sorted map keys.
- Unsigned integers are used for all ids. BLAKE3 digests are 32-byte (256-bit) byte strings.
- The grammar below is given in **CDDL** (RFC 8610).

```cddl
term-id      = uint            ; append-order, frozen (§7.2)
digest       = bstr .size 32   ; BLAKE3-256
content-id   = digest          ; a frame's self-hash (§9.1)
codec-id     = uint            ; index into the header codec catalog (§8)
```

## 5. Header frame

```cddl
header = {
  "gts"  : "GTS1",                    ; magic / format id
  "v"    : uint,                      ; spec major version (1)
  "prof" : tstr,                      ; profile (§13); "generic" if unspecified
  "cat"  : { * codec-id => codec },   ; the transform catalog (§8)
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
and the first frame's `"prev"` is the Header's `"id"`. Dictionaries are stored **uncompressed
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

1. If `"x"` is absent or `[identity]`, the payload is `"d"` directly (structured CBOR).
2. Otherwise `"d"` is a byte string; apply the **reverse** of each codec in `"x"`, last to
   first. Each step requires a **capability** (§8.3). On any missing capability, stop and treat
   the frame as **opaque** (§7.6).
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
`"mmr"`, it detects truncation and produces O(log n) inclusion proofs.

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

### 7.2 Term-id assignment (normative)

Term ids are unsigned integers assigned **in append order**, starting at `0`, and are
**frozen**: a term minted while folding frame *N* keeps its id forever. A `quads`, `annot`,
or `reifies` frame at position *N* MUST only reference term-ids introduced at positions
`0..N-1` (such frames introduce no terms of their own). This makes writing pure-append and
reading single-pass.

### 7.3 Quoted triples and reifiers (`reifies` frame)

RDF 1.2 lets a triple be the subject or object of another. GTS keeps quoted triples in the
id domain: a **reifier** is an ordinary IRI/bnode term; a `reifies` frame binds it to the
triple it quotes.

```cddl
reifies-payload = { * term-id => [term-id, term-id, term-id] }  ; reifier => (s, p, o)
```

A quoted triple used as a node is a term with `"k": 3` and `"rf"` pointing at its reifier.

### 7.4 Quads and annotations

```cddl
quads-payload = [+ [term-id, term-id, term-id, ? term-id]]  ; s, p, o, (g; default graph if absent)
annot-payload = [+ [term-id, term-id, term-id]]             ; reifier, predicate, value
```

Statement-level metadata (confidence, validity interval, standpoint/vantage, modality, …) is
expressed as `annot` rows on a reifier. **Contested claims coexist**: several `annot` rows on
one reifier, or several reifiers over one (s,p,o), are all retained — none is privileged.

### 7.5 Fold algorithm (normative)

```text
verify each frame's id (self-hash) and prev-link; record sig status if "sig" present
terms := []   graph := {}   reif := {}   meta := {}   blobs := {}   suppressed := {}
for frame in log order:
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
result := (terms, graph, reif, annot, blobs, meta, suppressed, opaque[])
```

The fold is deterministic: the same log yields the same graph in every conformant reader.
`meta` accumulates as a shallow union over one global map — a later frame's keys replace earlier
ones; values are not concatenated.

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
absent) is isolated and folded as an opaque node too (§9.1). The frame still participates in the
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

A Baseline Reader MUST implement `identity`, `gzip`, and `zstd`. Writers targeting maximum
longevity SHOULD restrict to the core set. Density-oriented writers MAY use `lzma2` with an
in-band dictionary. All core codecs are decades-stable, ubiquitously available primitives.

## 9. Integrity and confidentiality

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
- **Damage isolation and recovery.** A corrupt or obliterated frame fails *its own* `"id"` (or
  is simply absent); it does not poison its neighbours. With the index `"off"` table a reader
  **skips the bad frame and folds the survivors**, surfacing the loss as an opaque node with
  `reason: "damaged"`. Every frame is self-verifying and the log is rebuildable around gaps.
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
(§13) REQUIRE signatures.

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
closure). Compaction is **lossy by definition**: it discards the original per-frame signatures
and the temporal stacking of the log. A compactor:

- MUST record the provenance of the fold (source log digest, time, agent) as quads in the
  snapshot, and
- SHOULD emit a fresh signature over the snapshot.

Two artifact classes follow: an **evidentiary log** (append-only, signed, never compacted) and
a **distribution snapshot** (compacted, dense, lossy — ideal for shipping). A reader can tell
which it holds from the profile and the presence of a `snapshot` frame.

## 11. Suppression (additive "deletion")

GTS never physically deletes. To retract or hide prior content, a writer appends a `suppress`
frame referencing the superseded subgraph or frame digest. The suppressed bytes remain present
and hash-linked; suppression is a **display/precedence contract**, interpreted by the consumer,
not an erasure. This preserves a complete, tamper-evident history.

```cddl
suppress-payload = { "targets": [+ (digest / term-id)], ? "reason": tstr, ? "by": term-id }
```

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
  budget (§17).

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
| `opaque`     | carries `encrypt`-class frames; signatures REQUIRED; selective disclosure.         |
| `bundle`     | a GTS whose `blob`s are themselves GTS files (`mt: application/gts`); §12.1.        |

Profiles constrain conventions, not the wire format; a `generic` reader reads them all.

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
  "to": [ {"kid":"did:court:registry","alg":"ECDH-ES+A256KW"} ],
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

## 16. Versioning and durability guarantees

- The header `"v"` is the spec major version. A reader MUST refuse a major version it does not
  implement, but MUST still verify the id/prev chain and enumerate frame types/ids.
- **Structure durability:** a GTS file plus this specification is decodable forever with no
  engine and no external dictionary — CBOR is an IETF standard and dictionaries are in-band.
- **Density durability:** governed by the codec catalog; the mandatory core set
  (`identity`/`gzip`/`zstd`) guarantees a baseline that any era can decode.

## 17. Security considerations

- The id/prev chain provides integrity, **not** confidentiality; use `encrypt`-class codecs for
  confidentiality.
- **Truncation** (dropping trailing frames) is undetectable from the chain alone; an `evidence`
  artifact MUST anchor the head — a signature over the head `"id"`, or the index `"head"`/`"mmr"`
  root (§6.2) — so a verifier can detect a shortened log.
- **Recovery** is bounded by self-hashes: per-frame `"id"` plus the index `"off"` table localise
  damage (a corrupt frame is isolated and skipped, not fatal to the file), but GTS defines no
  parity/erasure coding — durability against bulk loss is the storage layer's concern.
- A valid signature attests the signer over the frame's bytes; it does **not** assert the truth
  of the claims (consistent with attestation semantics — vouching ≠ correctness).
- Opaque frames are unreadable but **not** invisible; do not place secrets in `"pub"`,
  `"to"`, or `"meta"`.
- Compaction destroys original signatures; an `evidence` artifact MUST NOT be compacted.
- Decompression of attacker-supplied frames MUST be bounded (zip-bomb resistance); readers
  SHOULD cap decoded sizes.
- Nested GTS (§12.1) MUST be bounded: readers MUST enforce a maximum recursion depth and a
  total decoded-size budget across all nesting levels (matryoshka-bomb resistance).

## 18. References

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

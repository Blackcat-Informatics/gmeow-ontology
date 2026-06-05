<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Import provenance & carrier time — the "envelope" problem

When you import a contact file (a vCard, an LDIF, a Google Takeout record), you
receive an **envelope** — the file — carrying a **bundle of claims**:

```text
bob.vcf  (mtime 2009-03-14, 1.2 KB, blake3:9f86…)
└── FN:Bob
└── EMAIL:bob@bob.dob
└── GENDER:M
```

The claims usually have **no tenure** (when did Bob hold that address?) and **no
observation time** (when did anyone check?). All you have is *file* metadata. The
question this slice answers: **what, if anything, can the envelope's timestamps
tell us — and where do they belong in the graph?**

## The four clocks

The reason a file's `mtime` feels like "not tenure, not observed-at" is that it is
a *third kind of time*. Keep these four apart — conflating them is the classic
provenance bug:

| Clock | Meaning | GMEOW term | Lives on |
|---|---|---|---|
| **Valid time** | when the fact holds in the world (tenure) | `gmeow:validFrom` / `gmeow:validUntil` | the claim (RDF-star) |
| **Assertion time** | when an agent observed/asserted it (email `Date`, vCard `REV`) | `gmeow:assertedAt` | the claim (RDF-star) |
| **Carrier time** | when the artifact bearing the claim was last written (`mtime`) | `gmeow:sourceModifiedAt` | the **`gmeow:Source`** (the file) |
| **Transaction time** | when *we* recorded it | `gmeow:ingestedAt` | the **`gmeow:ImportActivity`** |

A plain vCard gives you only **carrier time** (and you stamp transaction time at
import). It is silent on validity and observation. The cardinal rule:

> **`mtime` is carrier metadata, not claim metadata.** Put it on the `Source`,
> never in a claim's `validFrom` or `assertedAt` slot — that would fabricate
> precision you don't have.

## What `mtime` *does* give you: a terminus ante quem

The envelope's modification time bounds when its claims were **committed to a
record**: every claim in `bob.vcf` was recorded **no later than** its `mtime`.
That is the one honest derivation — an **upper bound** (`recordedNoLaterThan`),
not a point:

- It is a (weak) upper bound on the unknown assertion time.
- Combined with `ingestedAt` it gives a "known-by" window: `recorded ≤ mtime …
  ingested = now`.
- It is **derived and low-confidence** — `mtime` is reset by copy, sync, and
  `git checkout`, and a freshly-touched file can hold decade-old data. So
  **carrier-recency ≠ fact-recency**. Attach a low `gmeow:confidence`.

**Not derivable:** tenure, a point observation time, or anything trustworthy about
*fact* freshness.

## The new terms

### On the `Source` (the envelope) — `sources` module

- **`gmeow:sourceModifiedAt`** (functional, `xsd:dateTime`) — the carrier's
  last-modification time (the file `mtime`). A terminus-ante-quem on the recording
  of the claims it carries. Advisory and resettable.
- **`gmeow:contentDigest`** (`xsd:string`-ish literal) — a content hash, e.g.
  `"blake3:9f86…"`. **This is the reliable identity** of the carrier: two imports
  of the same bytes are the same `Source` regardless of `mtime` or path. (Not
  functional — a source may carry digests under several algorithms.)
- **`gmeow:sourceLocation`** — the origin path / filename / URL. Audit only; no
  identity value.

### On the import event — `provenance` module

- **`gmeow:ImportActivity`** (`⊑ gmeow:Activity ⊑ gufo:Event`) — the ingestion
  event.
- **`gmeow:ingestedAt`** (functional, `xsd:dateTime`) — the transaction time:
  when the system recorded the claims. (`closeMatch prov:endedAtTime`.)

### On the claim — `temporal` module (statement-level / RDF-star annotations)

- **`gmeow:assertedAt`** — when an agent observed/asserted the claim (populate
  from a vCard `REV` or an email `Date`; *absent* for a plain vCard).
  (`closeMatch prov:generatedAtTime`.)
- **`gmeow:recordedNoLaterThan`** — the **derived** upper bound described above.
  Populate it in the importer/query layer from the source's `sourceModifiedAt`
  when no stronger time is known; carry a low `gmeow:confidence`. **Do not** infer
  it with OWL — like trust validity and SHACL cardinality elsewhere in GMEOW, the
  vocabulary *represents* the bound; the *rule for populating it* stays in code.

## Worked example

A plain vCard (no `REV`) imported from a 2009-dated file. Coarse, per-entity
provenance:

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/import/> .

ex:bob-vcf a gmeow:Source ;
    gmeow:sourceLocation   "/imports/contacts/bob.vcf" ;
    gmeow:sourceModifiedAt "2009-03-14T08:22:00Z"^^xsd:dateTime ;  # carrier time
    gmeow:contentDigest    "blake3:9f86d081…" .                     # reliable id

ex:import-2026-06-05 a gmeow:ImportActivity ;
    gmeow:ingestedAt   "2026-06-05T12:00:00Z"^^xsd:dateTime ;       # transaction time
    gmeow:wasAttributedTo ex:vcard-importer .

ex:bob a gmeow:Person ;
    gmeow:name  "Bob" ;
    gmeow:email "bob@bob.dob" ;
    gmeow:hasSource      ex:bob-vcf ;          # evidence
    gmeow:wasDerivedFrom ex:bob-vcf ;          # provenance
    gmeow:wasGeneratedBy ex:import-2026-06-05 .
```

Per-**claim** provenance and the derived bound ride the RDF-star layer (RDF 1.2),
so each statement carries its own evidence, confidence, and clocks:

```turtle
# "Bob's email is bob@bob.dob" — derived from the vCard, recorded no later than
# the file's mtime, with low confidence in that bound.
<< ex:bob gmeow:email "bob@bob.dob" >>
    gmeow:hasSource            ex:bob-vcf ;
    gmeow:recordedNoLaterThan  "2009-03-14T08:22:00Z"^^xsd:dateTime ;
    gmeow:confidence           0.3 .
```

If the same vCard *had* a `REV:2008-11-02T00:00:00Z`, you would additionally set
`gmeow:assertedAt "2008-11-02T…"` on the statement — a stronger, observed time —
and the derived `recordedNoLaterThan` becomes redundant.

## vCard → GMEOW importer mapping

| vCard / file fact | GMEOW term | Clock / role |
|---|---|---|
| file `mtime` | `gmeow:sourceModifiedAt` (on the `Source`) | carrier time |
| file path / original name | `gmeow:sourceLocation` | provenance |
| BLAKE3 of bytes | `gmeow:contentDigest` | **identity** |
| the import run | `gmeow:ImportActivity` + `gmeow:ingestedAt` | transaction time |
| `REV` (if present) | `gmeow:assertedAt` (on the claim) | observation |
| *(no `REV`)* → derive from `mtime` | `gmeow:recordedNoLaterThan` (low confidence) | derived bound |
| `FN`, `EMAIL`, … | the claims, linked by `gmeow:hasSource` / `gmeow:wasDerivedFrom` | — |

## Using the bound well

- **Audit / dedup:** always link claims to their `Source`; identify the source by
  `contentDigest`, not `sourceLocation` or `sourceModifiedAt`.
- **Cross-import recency:** when two envelopes disagree, the carrier with the
  later `sourceModifiedAt` is a *tiebreaker* for which claim was recorded more
  recently — never ground truth. Pair it with low `gmeow:confidence`; source
  priority is a separate axis (`gmeow:importanceLevel`, max across imports).
- **Retraction:** a claim that appears in older carriers but drops out of newer
  ones (by `sourceModifiedAt`) is a weak signal to mark it historical — derivable
  only across a *series* of dated envelopes, not from one file.

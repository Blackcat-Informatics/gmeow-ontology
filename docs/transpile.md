<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Transpile — consumer RDF → pure GMEOW → MAXIMAL multi-vocab

[Projections](./projections.md) take GMEOW *down* to a single consumer
vocabulary (lossy, directional). The **transpile** runs the whole loop the other
way and back again: it ingests a foreign-vocabulary source (schema.org, FOAF,
vCard, …), lifts it *up* into pure GMEOW, then re-projects that GMEOW *down* into
**every** vocabulary at once — one fat, provenance-audited multi-vocabulary
publication.

```text
source.ttl  ──up-project──▶  <stem>.gmeow.ttl  ──MAXIMAL──▶  index.ttl / .nq / .gts / .jsonld
 (schema.org)                (the pure-GMEOW draft)          (gmeow + every vocab)
```

So a schema.org record is **ingested, understood *as* GMEOW, and re-expressed
maximally** across everything GMEOW can reach.

## The two halves

| Half | What it does | Doctrine |
|---|---|---|
| **Up-projection** (#451) | Lift the source to pure GMEOW. Mechanically-invertible terms become **facts**; non-equivalences become provenance-stamped **claims** (`gmeow:StatementMetadata` carrying `gmeow:confidence` + `gmeow:mappedFrom`). Each edge is resolved by its **position in the graph** — the same consumer predicate maps to different GMEOW terms by the subject's type (`schema:about` on a `MediaObject` → `gmeow:depicts`, on a document → `gmeow:isAbout`). Nothing is guessed: an unresolvable term is reported, never invented. | [up-projection audit](./up-projection-audit.md) |
| **Maximal down-projection** (#34) | Run `MAXIMAL(G) = G + E(G) + P(G)` over the draft — the canonical base `G`, its strong-equivalence saturation `E(G)`, and every projection profile `P(G)`. | [projections](./projections.md) |

The two outputs of the lift compose cleanly with the maximal pass: **facts**
re-project to every vocabulary, while the **claims** pass through untouched into
the GMEOW base (no consumer projection matches a `StatementMetadata` node) — so
the provenance of every inference survives into the publication.

## The pure-GMEOW draft is a first-class artifact

The transpile always writes the intermediate `<stem>.gmeow.ttl` **alongside** the
maximal family — it is not a throwaway temp file. It is:

- **The canonical interpretation.** Everything the transpile believes the source
  *means*, stated once in GMEOW's own vocabulary — the single source of truth the
  maximal file family is derived from.
- **Auditable.** Facts are bare triples; every inferred term is a
  `gmeow:StatementMetadata` claim you can inspect (what was claimed, from which
  source predicate via `gmeow:mappedFrom`, at what `gmeow:confidence`). You can
  see exactly where the transpile guessed and how sure it was.
- **Re-runnable.** Feed the draft straight back into `gmeow transform` to
  reproduce (or re-scope) the maximal output without re-doing the lift.

## Run it

```sh
gmeow transpile source.ttl                    # → dist/transpile/source/
gmeow transpile source.ttl -o out/            # choose the output directory
gmeow transpile source.ttl --profiles schema-org,foaf   # a subset of the maximal pass
gmeow transpile source.ttl --floor            # use the per-term floor, not the descent
```

The output directory receives the draft `<stem>.gmeow.ttl` plus the maximal
family: `<stem>.gts` (canonical, full RDF 1.2 provenance), `index.nq` (RDF 1.2
N-Quads), and `index.ttl` / `index.jsonld` (the asserted base triples, plain-RDF
readable). See [projections](./projections.md) for the file family in detail.

### Streaming

Every stage reads `-` from stdin, so the halves compose in a pipe:

```sh
cat source.ttl | gmeow transpile -                    # one-shot, source on stdin
cat source.ttl | gmeow up-project - --descend | gmeow transform -   # the two halves, explicit
```

`gmeow up-project` writes the pure-GMEOW draft to stdout; `gmeow transform -`
reads a GMEOW A-Box from stdin and emits the maximal family. The explicit
two-stage form lets you inspect or post-process the draft mid-pipeline.

## Scope — what the up-projection does and does not resolve

The descent resolves a term by the **subject's own type** and by the
**structural legs** of multi-atom cells (so a blank-node `schema:PropertyValue`
identifier recovers its `gmeow:identifierUrl` / `identifierValue` / `scheme`).
Terms it cannot resolve from that context fall through to the per-term floor, and
a genuinely ambiguous many-to-one term (`dc:relation`, peer-ambiguous terms on
typed subjects) is **held out and reported**, never guessed.

**Measured non-goal:** typing an *untyped* node from the `rdfs:range` of its
incoming edge ("path context") was prototyped and resolved **zero** extra edges
on the real corpus — the untyped nodes' own predicates are mostly gaps, so an
inferred type buys nothing. It is deliberately not implemented; see the
`up_projection_descend` module docstring.

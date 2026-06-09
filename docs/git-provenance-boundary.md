<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Git Provenance Boundary Strategy

> **Principle 4** (one canonical source) + **Principle 10** (suppression, never erasure) + **Principle 12** (compute outside the logic).

GMEOW's software module models the full git object graph — commits, trees, blobs,
refs, push events, merges, code reviews, and diffs — but it **does not mandate
materialising every object of a 500 000-commit repository as triples**.

The boundary is explicit, configurable, and enforced by projection rather than
deletion.

---

## What is materialised in the ontology

The canonical model **always** includes:

- **Commits** — every commit that is a merge-base, a boundary commit, a release
tag target, or explicitly referenced by an event (push, merge, review).
- **Refs** — branches and tags that mark lines of development or releases.
- **Releases** — versioned release events with their tags and artifacts.
- **Events** — push, merge, and code-review activities that carry provenance.
- **Repository metadata** — type, hosting platform, clone URL, web URL, and
`gmeow:materializationDepth`.

These are the "spine" of the provenance graph: they answer "who did what, when,
and where" without enumerating every intermediate blob.

## What is left to external resolution

Deep Merkle traversal — every blob and tree of a large history — is **left to
native git APIs or Software Heritage (SWHID) resolution**.

- A `gmeow:Blob` or `gmeow:SourceTree` that appears in the triplestore carries
its `gmeow:contentDigest` (git hash or SWHID). If the object is not materialised
as triples, the digest is still a resolvable identifier.
- Software Heritage's object graph (`swh:1:cnt:`, `swh:1:dir:`, `swh:1:rev:`)
provides authoritative, persistent resolution for any content hash.
- Native git (`git cat-file`, `git rev-parse`, `git log`) provides fast,
repository-local traversal.

This is **not a gap** — it is a deliberate solver boundary (Principle 12). The
ontology models the *logic* of the git object graph; the *traversal* of that
graph is computation, not assertion.

## Configuring the boundary: `gmeow:materializationDepth`

Each `gmeow:Repository` may declare:

```turtle
ex:repo a gmeow:Repository ;
    gmeow:materializationDepth 2 .
```

| Depth | Meaning |
|---|---|
| `0` | No tree materialisation. Only commits and refs are triples. |
| `1` | Root tree of each materialised commit is a triple. |
| `2` | Root tree plus one level of children (files and subdirectories). |
| `n` | `n` levels of Merkle tree depth, recursively. |

A consumer that needs deeper traversal resolves the `contentDigest` via SWHID or
git. The depth value is a **projection hint**, not a hard limit — it guides
exporters and loaders without restricting what can be asserted.

## Suppression, not erasure

If a repository owner decides to remove a file from the materialised view (e.g.
a leaked secret), the triple is suppressed by projection (Principle 10), not
deleted from the canonical source. The `contentDigest` may still resolve via
SWHID for archival purposes, but the projection layer withholds it from the
consumed graph.

## Alignment to Software Heritage

The mapping compiler emits SSSOM rows that bridge GMEOW terms to SH concepts:

| GMEOW | Software Heritage |
|---|---|
| `gmeow:Blob` | SH Content (`swh:1:cnt:`) |
| `gmeow:SourceFile` | SH Content (`swh:1:cnt:`) — the named view |
| `gmeow:SourceTree` | SH Directory (`swh:1:dir:`) |
| `gmeow:Commit` | SH Revision (`swh:1:rev:`) |
| `gmeow:Release` | SH Release (`swh:1:rel:`) |
| `gmeow:Repository` | SH Origin (`swh:1:ori:`) |

These are `skos:closeMatch` alignments to informal concept URIs; SH does not
yet publish a standard RDF ontology. When stable vocabularies emerge, the
mapping-dsl source will be updated and the generated SSSOM re-rendered
(Principle 4).

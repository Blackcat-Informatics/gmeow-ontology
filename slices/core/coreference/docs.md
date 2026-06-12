<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Coreference — identity by reference, never by merger

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/coreference` · **tier: core**
> The slice that lets GMEOW point at the rest of the world's records without ever marrying them.

Universal identity and coreference links: stable GMEOW entities link to external records and cross-realm counterparts by reference, never by owl:sameAs merger. Wikidata is the recommended hub, but all authority records remain external claims that may be exact, close, contested, or standpoint-indexed.

The doctrine is Principle 5 applied to identity. `owl:sameAs` is a logical sledgehammer:
it merges every property of both nodes, propagates upstream errors as entailments, and
collapses standpoints that were deliberately kept apart. GMEOW therefore never asserts
it across system boundaries. The stable entity is the GMEOW node itself; an external
record is an *authority reference* attached with `gmeow:authorityLink`, and the
*strength* of the coreference is a separate, explicit claim (`skos:exactMatch` /
`skos:closeMatch` in instance data). A contested identification is several
standpoint-indexed claims that coexist (Principle 9); a withdrawn one is suppressed
with `gmeow:displayable` false, never deleted (Principle 10).

The same by-reference stance covers identity *within* the graph: counterparts across
realms and frames stay linked but unmerged, and version/edition/supersession lineage
keeps every member of the lineage first-class.

## Authority and counterpart links

### gmeow:authorityLink

The universal pointer from a GMEOW entity to a record in an external authority,
registry, database, gazetteer, or catalogue — by IRI, range intentionally open. It is a
see-also authority pointer, **not** an OWL identity merge; assert match strength
separately with `skos:exactMatch` or `skos:closeMatch`. Wikidata is the recommended hub
because its cross-references reach most other authorities — one well-chosen QID buys
VIAF, GeoNames, and ORCID transitively (the maximal-linkage convention curl-validates
QIDs at gate time).

### gmeow:counterpartOf

Symmetric — and deliberately *not* transitive — linkage between two GMEOW entities that
are recognisable counterparts across realms, datasets, editions, or modelling contexts
without being safely mergeable: the historical person and their fictionalized portrayal,
the same place in two gazetteers' worldviews. Non-transitivity is the point: counterpart
chains must not silently weld A to C through B.

## Version, edition, and supersession lineage

### gmeow:versionOf

Relates a concrete version entity to the stable lineage entity it versions — a language
version to its language, a software release to its project, a data release to its
dataset. Functional (a version belongs to one lineage); the lineage has many versions.

### gmeow:editionOf

The creative-work specialization: a concrete edition, issue, or manifestation pointing
to the stable work it editions. Functional per edition, non-merge semantics — editions
remain first-class CreativeWorks with their own identifiers, dates, rights, and
provenance. Bridges directly to the WEMI spine in `documents`/`creative-works`.

### gmeow:supersedes

Relates a newer entity, version, record, or claim-bearing artifact to a prior one it
replaces. Non-functional: one successor may consolidate several predecessors. The
superseded entity is retained and usable (Principle 10); suppression from display is a
separate decision (`gmeow:displayable` false). The lifecycle slice supplies the inverse
(`gmeow:supersededBy`) and the event-shaped view (`gmeow:eventTypeSupersession`) when
the replacement itself needs a date, location, or participants.

## Solver layer & deferred alignment

Entity *resolution* — deciding that two records corefer, scoring the match, clustering
candidates — is solver-layer computation (Principle 12): the slice records asserted
links and their declared strengths; it never derives identity. The SKOS mapping
vocabulary is consumed by reference in instance data (Principle 5), and the
maximal-linkage doctrine expects every slice to exercise these links — coreference is
the one slice whose job is to be everyone else's out-edge.

## Dependencies

Depends on `kernel` and `documents` (the CreativeWork range of `editionOf`). Consumed
by every slice that links to Wikidata and external registries — which, under the
maximal-linkage convention, is all of them.

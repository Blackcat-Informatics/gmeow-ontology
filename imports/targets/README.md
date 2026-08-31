# Target-vocabulary axiom snapshots (validation-time only)

These are **generated**, minimal snapshots of external vocabularies. The snapshot
*shape* depends on the target's `kind` (`config.AlignmentTarget`):

- **Property-axiom shape** (`schema` / `concept_scheme` targets — bridged at the
  property level). Keeps `rdfs:domain`/`rdfs:range` (and schema.org's
  `domainIncludes`/`rangeIncludes`), `owl:inverseOf`/`schema:inverseOf`, and
  property-type triples. No labels or prose. Used by the native correspondence-
  soundness checks to verify, offline, whether a GMEOW mapping points at the right
  target term *or its inverse*.
- **Class-fact shape** (`upper` ontologies — bridged at the class level). Keeps each
  `owl:Class` in the namespace, its in-namespace `rdfs:subClassOf` parents, and its
  short `rdfs:label`. Used by the gUFO↔BFO **foundational bridge** to verify, offline,
  that every emitted upper-ontology IRI is a real class with the expected label;
  the native foundational-bridging conformance suite owns this invariant. Labels are kept only for
  IMPORT_OK upper ontologies whose license permits it (BFO is CC-BY-4.0).

## Not part of the published ontology

This directory is a **subdirectory** of `imports/`, and `graph.iter_import_files()`
globs `imports/*.ttl` non-recursively — so these snapshots are never merged into
the published CC BY 4.0 artifact. They are a validation-time concern only.

## License policy: reference, not copy

Only **IMPORT_OK** targets (per `config.policy_for_license`) are vendored here:

| file | vocabulary | license |
|------|------------|---------|
| `org.ttl` | W3C ORG | PDDL-1.0 |
| `foaf.ttl` | FOAF | CC-BY-1.0 |
| `vcard.ttl` | W3C vCard | W3C-Document |
| `prov.ttl` | PROV-O | W3C-Document |
| `time.ttl` | OWL-Time | CC-BY-4.0 |
| `geo.ttl` | GeoSPARQL | OGC |
| `bfo.ttl` | BFO 2020 (ISO/IEC 21838-2) — *class-fact shape* | CC-BY-4.0 |
| `bot.ttl` | BOT (Building Topology Ontology) | BSD-3-Clause |

**Reference-only** targets (e.g. schema.org, CC-BY-SA) are *never* vendored;
`refresh_snapshot` refuses them. Their axioms are fetched live under the
`network` test mark / `make maint-test-network`, with a tiny hand-authored
fixture (`tests/fixtures/target_axioms/`) covering the cases needed offline.

## Refreshing

```sh
make maint-refresh-target-axioms
```

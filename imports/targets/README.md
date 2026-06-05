# Target-vocabulary axiom snapshots (validation-time only)

These are **generated**, minimal snapshots of external vocabularies' *structural
axioms* — `rdfs:domain`/`rdfs:range` (and schema.org's `domainIncludes`/
`rangeIncludes`), `owl:inverseOf`/`schema:inverseOf`, and property-type triples.
No labels, definitions, or prose are kept.

They exist so the SSSOM alignment-direction linter
(`gmeow_tools.alignment_lint`) can check, offline, whether a GMEOW mapping points
at the right target term *or its inverse* (issue #25).

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

**Reference-only** targets (e.g. schema.org, CC-BY-SA) are *never* vendored;
`refresh_snapshot` refuses them. Their axioms are fetched live under the
`network` test mark / `gmeow lint-alignment --network`, with a tiny hand-authored
fixture (`tests/fixtures/target_axioms/`) covering the cases needed offline.

## Refreshing

```sh
make refresh-target-axioms          # or: gmeow refresh-target-axioms --target all
```

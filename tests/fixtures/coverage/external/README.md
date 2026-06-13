<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# External-input snapshots

The `.ttl` files here are **verbatim dumps of real-world public site graphs**
(currently `paudley` and `bii`, each a copy of that site's `dist/index.ttl`).
They exist as **parity targets** for the transpiler coverage harness: we measure
how much of a real published graph GMEOW can derive.

## These are NOT GMEOW-authored

A snapshot carries whatever the outside world emits — including `owl:sameAs`
links to external entities (geni, FamilySearch, Wikidata, DBpedia). That is
perfectly normal in real RDF. **Principle 5 is a policy on _our_ ontology, not a
rule we can impose on all RDF in the world.** So the gates that police
GMEOW-authored RDF deliberately **skip this subtree**:

- the `owl:sameAs` ban (`validate.check_sameas_ban`),
- the disjointness-coherence reasoning
  (`test_worked_fixtures_stay_coherent_under_disjointness`),
- the declared-term surface check (`test_coverage_fixtures_use_only_declared_terms`,
  which globs the parent directory non-recursively and so never descends here).

Everything **outside** `external/` — the hand-authored worked-example A-boxes —
is GMEOW-authored and stays fully policed (use `gmeow:authorityLink` /
`skos:exactMatch`, declared terms only).

## Adding a snapshot

Drop the site's `dist/index.ttl` in here as `<site>.ttl`. No allowlist edits, no
scrubbing — the directory itself is the marker, and the `EXTERNAL_FIXTURES_DIR`
exemption covers it automatically. **Do not edit the triples to satisfy a lint:**
a doctored snapshot is no longer a faithful parity target.

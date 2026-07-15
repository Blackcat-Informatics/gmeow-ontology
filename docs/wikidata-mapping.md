<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Wikidata Interoperability in GMEOW

This document is the normative policy for how GMEOW uses Wikidata and Wikibase RDF. It governs term mappings, authority links, direct properties, statement-node projection, and the boundaries between them.

> **Principle 5** (Maximal bridging by reference): GMEOW mints exactly one canonical term per concept and aligns it to every relevant surface vocabulary by reference — never rewriting anyone else's data.
>
> **Principle 4** (One canonical source): Every fact is authored once in the canonical core; all other forms are generated. Wikidata mappings are authored in `mapping-dsl/equivalences/` and compiled to `mappings/*.sssom.tsv`.

---

## Namespace Semantics

GMEOW uses four Wikidata-related namespaces. Each has a precise role; mixing them is a modelling error.

| Prefix | Namespace | Role |
|--------|-----------|------|
| `wd:` | `http://www.wikidata.org/entity/` | **Item and property concept IRIs**. Use for `skos:exactMatch`, `skos:closeMatch`, and `gmeow:authorityLink` when the target is the Wikidata *concept* (Q-item or P-item as an entity). |
| `wdt:` | `http://www.wikidata.org/prop/direct/` | **Truthy direct-claim properties**. Use when aligning a GMEOW property to the Wikidata *predicate* that appears in a simple truthy triple. |
| `p:` / `ps:` / `pq:` / `pr:` | `http://www.wikidata.org/prop/` … | **Statement-node form**. Use ONLY in projection or enrichment layers when qualifiers, references, or provenance are required. Never in the canonical ontology core. |
| `wikibase:` | `http://wikiba.se/ontology#` | **Wikibase ontology terms** (e.g. `wikibase:Item`, `wikibase:Statement`). Use for describing Wikibase data structures, not for GMEOW concept alignment. |

### Common Misuses

- Using `wd:P275` when you mean the *direct property* `wdt:P275`. `wd:P275` is the property *as an entity* (usable in `skos:exactMatch`); `wdt:P275` is the predicate used in a truthy triple.
- Using `wdt:Q42` — the direct-property namespace must only contain property IDs (`P…`), never item IDs (`Q…`).
- Using HTTPS entity URLs (`https://www.wikidata.org/entity/Q42`) in authored Turtle instead of the canonical `wd:Q42` CURIE. The HTTPS URL is what dereferences; the CURIE is what we author.

---

## Link-Type Boundaries

GMEOW uses a strict hierarchy of link types when bridging to Wikidata. Choosing the wrong type collapses standpoint-indexed claims or overclaims equivalence.

| Predicate | When to use with Wikidata | When NOT to use |
|-----------|--------------------------|-----------------|
| `skos:exactMatch` | Extensional equivalence in practice: the GMEOW term and the Wikidata item denote the same set of things in the real world. | When the match is lossy, directional, or GMEOW is more refined. |
| `skos:closeMatch` | Semantics are close but GMEOW is more precise, reified, or culturally nuanced. | When the concepts are genuinely identical. |
| `skos:relatedMatch` | Directional or lossy relationship; the Wikidata item is a neighbour, not an equivalent. | When claiming equivalence. |
| `skos:broadMatch` | The Wikidata item is a proper superclass of the GMEOW term. | When the match is close or exact. |
| `skos:narrowMatch` | The Wikidata item is a proper subclass of the GMEOW term. | Rare — GMEOW tends to be the refined model. |
| `gmeow:authorityLink` | **Instance-level coreference**: "this specific place instance is the same as Wikidata Q84". | Never for TBox (class/property) alignment. |
| `owl:sameAs` | **Strongly discouraged** for Wikidata. It forces standpoint collapse and can silently merge contested claims. | Almost always. Prefer `skos:exactMatch` or `gmeow:authorityLink`. |
| `schema:sameAs` | Acceptable for social-web profile pages (a person's Wikipedia page). | Never for ontology alignment or instance coreference in the knowledge graph. |

### Standpoint Sensitivity

Wikidata is community-curated and sometimes reflects dominant-culture categories. When mapping identity-related terms (gender, sexuality, ethnicity), `skos:closeMatch` is the default — even when the community treats the items as equivalent — because GMEOW's reified, self-asserted, standpoint-indexed model is structurally richer. See `mapping-dsl/equivalences/gender.ttl` for the precedent.

---

## Mapping Conventions

All term-level Wikidata mappings are authored in `mapping-dsl/equivalences/*.ttl` and compiled to `mappings/*.sssom.tsv` by `gmeow-dev sync --mode update --outputs generated`. Do not hand-edit generated files.

### Required fields

Every `gmeow:TermEquivalence` targeting Wikidata MUST carry:

- `gmeow:alignSubject` — the GMEOW term being aligned
- `gmeow:alignObject` — a `wd:Q…` or `wdt:P…` IRI (or `p:/ps:` in projection DSL)
- `gmeow:alignPredicate` — one of the SKOS mapping predicates or `owl:equivalentProperty`/`owl:equivalentClass`
- `gmeow:confidence` — a float in [0, 1]
- `gmeow:justification` — typically `semapv:ManualMappingCuration`
- `gmeow:objectLabel` — the English label of the Wikidata item/property at the time of curation
- `gmeow:sssomFile` — the target SSSOM file name

### Recommended fields

- `gmeow:comment` — REQUIRED when the match is lossy, culturally sensitive, or semantically narrow. Document the direction of lossiness (GMEOW → Wikidata or Wikidata → GMEOW).

### Validation

- Offline: `make wikidata` checks syntax and namespace usage.
- Live: `make maint-wikidata-live` checks existence, redirects, and stale labels (network required).
- Coverage: `make maint-wikidata-coverage` reports how much of the ontology is mapped.
- Fixtures: `make maint-wikidata-audit` scans fixtures for invalid or misused Wikidata IRIs.

---

## Wikidata as Reference, Not Import

Wikidata is CC0 structured data, but the full graph is operationally large and semantically noisy for OWL 2 DL reasoning. GMEOW's stance:

1. **Link, don't import** — Curated alignments in SSSOM and EDOAL reference Wikidata by IRI. No Wikidata axioms are copied into the GMEOW ontology.
2. **Project, don't assert** — Any Wikidata-style RDF output (truthy `wdt:` triples, statement-node form) is a generated, lossy projection from the canonical GMEOW model.
3. **Validate, don't trust** — Every QID and PID used in mappings or fixtures is syntax-checked offline. Live validation checks existence and label freshness.
4. **Cache, don't hammer** — Live validation caches API responses on disk with a TTL to respect Wikidata's infrastructure.

---

## Useful References

- [Wikidata RDF dump format](https://www.wikidata.org/wiki/Wikidata:RDF)
- [Wikidata Data Access](https://www.wikidata.org/wiki/Help:Data_access)
- [Wikidata Data Model](https://www.wikidata.org/wiki/Wikidata:Data_model)
- [SSSOM Specification](https://mapping-commons.github.io/sssom/spec/)

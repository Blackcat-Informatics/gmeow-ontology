<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Schema projections — OWL → LinkML developer schemas

> **Status.** Implemented in issue #57.  CLI: `gmeow compile-schemas` / `uv run --package gmeow-dev gmeow-dev regenerate schemas`.
> CI gate: `uv run --package gmeow-dev gmeow-dev check-generated schemas` (runs in the `ontology` job).

GMEOW is an **OWL 2 DL** ontology.  OWL is a *logic* language — it supports intersection,
cardinality, inverse properties, and open-world reasoning.  Developer schemas (JSON Schema,
Pydantic, TypeScript, GraphQL, OpenAPI) are *closed-world structural* contracts.  Translating
from the former to the latter is **intentionally lossy** by design (CONSTITUTION Principle 5:
*maximal bridging — by reference*).  This document inventories exactly what is lost and why.

---

## What is preserved

| OWL construct | LinkML representation |
|---------------|----------------------|
| `owl:Class` → `rdfs:label` / `rdfs:comment` | `classes` entry with `title` / `description` |
| `rdfs:subClassOf` (single named parent) | `is_a` |
| `owl:ObjectProperty` / `owl:DatatypeProperty` / `owl:AnnotationProperty` | `slots` entry with `slot_uri` |
| `rdfs:domain` (first named class) | `domain` + attached to class `slots` |
| `rdfs:range` (first named URIRef) | `range` (XSD → LinkML built-in; GMEOW class → class name) |
| `owl:FunctionalProperty` | `multivalued: False` |
| `owl:inverseOf` | **dropped** (LinkML has no inverse slot) |
| Named individuals of GMEOW classes | `enums` with `permissible_values` |

---

## What is lost (and why)

### 1. OWL restrictions (intersection, cardinality, value constraints)

OWL restrictions are expressed as anonymous blank nodes:

```turtle
:Adult a owl:Class ;
  owl:equivalentClass [
    owl:intersectionOf ( :Person [ owl:someValuesFrom :Age ; owl:onProperty :hasAge ] )
  ] .
```

LinkML has no intersection construct.  The compiler drops all BNode-shaped
`rdfs:subClassOf` / `owl:equivalentClass` axioms.  Cardinality constraints
(`owl:minCardinality`, `owl:maxCardinality`) are likewise dropped.

> **Rationale.** Developer schemas are structural, not logical.  A Pydantic model
cannot express "Person AND (hasAge some Age)".  The loss is documented so consumers
know they must validate against SHACL or reasoned OWL for completeness.

### 2. RDF 1.2 reification, standpoint indexing, and the four-clocks temporal model

GMEOW's canonical statement-level metadata (issue #51) uses RDF 1.2 / RDF* triple terms:

```turtle
<< :E1 :occursAt "2024-01-01"^^xsd:date >> :assertedBy :SourceA ;
                                           :confidence 0.9 .
```

LinkML and JSON Schema have no native representation for quoted triples or
standpoint-indexed claims.  The compiler drops all statement-level provenance.

> **Rationale.** These layers live in `statement-dsl/` and `statements/gmeow.rdf12.ttl`.
The developer schema is a *simplified structural view*, not the canonical model.

### 3. `owl:inverseOf`

LinkML has no inverse-slot construct.  Properties declared with `owl:inverseOf`
are emitted as forward-direction slots only.

> **Rationale.** JSON Schema / Pydantic / TypeScript model properties, not
bidirectional logical relations.  Consumers can infer inverses at the OWL layer.

### 4. Multiple `rdfs:domain` / `rdfs:range` values

If a property has more than one named domain or range, the compiler keeps the
first and warns about the rest.

> **Rationale.** LinkML slots have a single `domain` and `range`.  Multiple
values would require union types, which many target generators handle poorly.

### 5. External-class ranges

If a property's range is a class outside GMEOW (e.g., `schema:Person`, `foaf:Agent`),
the compiler degrades it to `string` with a warning.

> **Rationale.** The developer schema only defines GMEOW classes.  External
classes would require importing their full type hierarchy, which is out of scope.

### 6. Open-world assumptions

OWL is open-world: absence of a triple does not mean negation.  JSON Schema /
Pydantic are closed-world: missing required fields fail validation.

The compiler never marks slots as `required: true` (LinkML `required` is not
inferred from OWL cardinality).  All slots default to optional.

> **Rationale.** Requiredness in OWL is a *logical* constraint (`owl:minCardinality 1`),
not a *structural* one.  Mapping logical to structural requiredness would over-constrain
consumers and contradict the open-world design.

---

## Generator-specific caveats

| Generator | Known limitation |
|-----------|-----------------|
| **Pydantic** | Embeds the source LinkML path (normalized to `gmeow.linkml.yaml` for determinism).  Custom types (`duration`) inherit from `string`. |
| **TypeScript** | Unknown `type.base` warnings for `datetime`, `decimal`, `duration`, `uri` — these fall back to `string`. |
| **GraphQL** | Names containing illegal characters (e.g. `signatureSchemeBLS12-381`) are not valid in GraphQL identifiers. Normalize them before consumption (e.g. replace `-` with `_` → `signatureSchemeBLS12_381`, or remove entirely → `signatureSchemeBLS12381`). The schema generator preserves the original name so the mapping is explicit. |
| **JSON Schema** | LinkML `mergeimports=True` inlines all definitions; the schema is self-contained but large. |
| **OpenAPI** | Minimal path set (`GET /entities/{id}`) added for spec validity; not a functional API definition. |

---

## Using the schemas

```bash
# Generate all artifacts into dist/schemas/
uv run --package gmeow-dev gmeow-dev regenerate schemas

# Verify they match the current ontology (CI gate)
uv run --package gmeow-dev gmeow-dev check-generated schemas

# Regenerate after JSON-LD context changes before checking drift.
```

Output files:

* `dist/schemas/gmeow.linkml.yaml` — canonical LinkML schema
* `dist/schemas/gmeow.schema.json` — JSON Schema
* `dist/schemas/gmeow.py` — Pydantic models
* `dist/schemas/gmeow.ts` — TypeScript interfaces
* `dist/schemas/gmeow.graphql` — GraphQL type stubs
* `dist/schemas/gmeow.openapi.json` — OpenAPI 3.1

These are **build artifacts** (`dist/` is git-ignored).  Do not edit them by hand.
If a term is wrong, fix the OWL source in `ontology/modules/` and re-run
`uv run --package gmeow-dev gmeow-dev regenerate schemas`.

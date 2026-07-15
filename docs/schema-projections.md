<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Schema projections — two provenances, two world assumptions

> **Status.**
> **JSON Schema** (`gmeow.schema.json`) + **OpenAPI** (`gmeow.openapi.json`) are compiled
> **natively in Rust from the SHACL shapes** (`crates/shacl`).  They are
> *closed-world* validators.
> The **LinkML targets** (`gmeow.linkml.yaml`, `gmeow.ts`, `gmeow.graphql`) are
> projected **OWL → LinkML**.  They are *open-world* structural views.
> The **Pydantic surface** is no longer an OWL→LinkML projection: it is the
> SHACL-derived `gmeow_models/` package, co-derived from the SAME shape
> compilation as the JSON Schema (so a model's `model_json_schema()` agrees with
> `gmeow.schema.json`).
> Update: `cargo run -p gmeow-dev-cli -- sync --mode update --outputs generated`.
> CI gate: `cargo run -p gmeow-dev-cli -- sync --mode check --outputs generated` (in the `ontology` job).

GMEOW is an **OWL 2 DL** ontology with a parallel layer of **SHACL** shapes.  OWL is a *logic*
language (intersection, cardinality, inverse properties, open-world reasoning); SHACL is a
*constraint* language that validates the well-formedness of instance data.  Developer schemas
are *closed-world structural* contracts.  Which schema you can derive — and what it preserves —
depends on **which source layer it is projected from**:

* **From SHACL** → real validators with `required`, `enum`, `pattern`, cardinality, and ranges.
* **From OWL (via LinkML)** → structural *type stubs*; requiredness is never inferred.

This document inventories both, and exactly what each one loses and why.

---

## 1. JSON Schema + OpenAPI — closed-world, native from SHACL

`gmeow.schema.json` and `gmeow.openapi.json` are compiled **directly from the SHACL shapes**
by the native Rust emitter (`gmeow_shacl::json_schema::compile`, `crates/shacl/src/json_schema.rs`),
not from LinkML.  Because SHACL constraints *are* closed-world structural assertions, the
projection preserves them faithfully:

| SHACL construct | JSON Schema representation |
|-----------------|----------------------------|
| `sh:minCount ≥ 1` | `required` (the property is actually mandatory) |
| `sh:minCount` / `sh:maxCount` | `minItems` / `maxItems` (or a single value for `maxCount 1`) |
| `sh:in` | `enum` |
| `sh:pattern` | `pattern` |
| `sh:minInclusive` / `sh:maxInclusive` / `sh:minExclusive` / `sh:maxExclusive` | numeric `minimum` / `maximum` / `exclusiveMinimum` / `exclusiveMaximum` |
| `sh:closed` | `additionalProperties: false` |
| `sh:datatype` / `sh:nodeKind` | typed-literal / node-reference value schemas |

These schemas **actually validate instance data**.  They drive `gmeow validate --schema`
and IDE autocomplete for the YAML-LD-star surface.

`gmeow validate` dispatches by file type: RDF serializations
(`.nq`/`.trig`/`.ttl`/`.nt`/`.rdf`/`.jsonld`) run repo-free SHACL + OntoUML
conformance against the bundled shapes, while `.json`/`.yaml` run JSON-Schema
instance validation.  Passing `--schema` forces the JSON-Schema path for any
input — so a JSON-LD instance is checked against this schema with
`gmeow validate --schema … instance.jsonld`, whereas a bare `instance.jsonld`
is validated as an RDF graph.

### The `@type`-discriminated envelope

The schema validates a JSON-LD `@graph` of instance nodes against a single
`$defs/Node` envelope.  `Node` discriminates on `@type`: an `allOf` of conditionals reads
*if `@type` includes `gmeow:<Class>` (as a bare string or inside an array), then the node MUST
satisfy `#/$defs/<Class>`*.  So a node is validated against the class def(s) named in its own
`@type`, and **an instance missing a property required by its class is REJECTED** (closed-world
enforcement).  Nodes typed only by unmodeled classes fall through permissively.

### What is lost (and why) — JSON Schema / OpenAPI

This is a **lossy** projection (CONSTITUTION Principle 17 / the loss ledger).  The emitter
records each drop as a `LossRecord` and annotates the affected schema with a `$comment`:

* **`sh:sparql` (SHACL-AF) constraints** have no JSON Schema equivalent and are **DROPPED**.
  A JSON Schema cannot run a SPARQL query, so these node- and property-level constraints are
  omitted (and logged).  This is the analogue of the ShEx/SPARQL loss: structural well-formedness
  is preserved, but arbitrary SPARQL-expressed business rules are not.
* **External (non-gmeow) class references** degrade to a permissive node reference / string.
  A `gmeow:` class with no NodeShape likewise degrades to a node reference (no `$def` is emitted).

Because it validates structure but is **not an entailment relation**, the projection is ledgered
`logic:ValidationOnly` — see
[`slices/grounding/logic/examples/projection-loss-ledger.ttl`](../slices/grounding/logic/examples/projection-loss-ledger.ttl),
entry `ex:shaclJsonSchemaReport`.

---

## 2. LinkML targets — open-world, OWL → LinkML

The remaining developer artifacts are projected from the **OWL** source through **LinkML**:

* `gmeow.linkml.yaml` — the canonical LinkML schema
* `gmeow.ts` — TypeScript interfaces
* `gmeow.graphql` — GraphQL type stubs

The **Pydantic** developer surface is derived from the **SHACL** layer instead —
the `gmeow_models/` package (see §1), not an OWL → LinkML projection.

OWL is *open-world*: absence of a triple is not negation.  LinkML projection of OWL is therefore
**intentionally lossy** by design (CONSTITUTION Principle 5: *maximal bridging — by reference*),
and crucially it **never infers `required`** — these are type stubs, not validators.

### What is preserved — LinkML

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

### What is lost (and why) — LinkML

#### 2.1 OWL restrictions (intersection, cardinality, value constraints)

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

> **Rationale.** These targets are structural, not logical.  A Pydantic model
cannot express "Person AND (hasAge some Age)".  The loss is documented so consumers
know they must validate against the SHACL-derived JSON Schema (§1) or reasoned OWL for
completeness.

#### 2.2 RDF 1.2 reification, standpoint indexing, and the four-clocks temporal model

GMEOW's canonical statement-level metadata uses RDF 1.2 / RDF* triple terms:

```turtle
<< :E1 :occursAt "2024-01-01"^^xsd:date >> :assertedBy :SourceA ;
                                           :confidence 0.9 .
```

LinkML has no native representation for quoted triples or
standpoint-indexed claims.  The compiler drops all statement-level provenance.

> **Rationale.** These layers live in `dsl/statements/` and `statements/gmeow.rdf12.ttl`.
The LinkML schema is a *simplified structural view*, not the canonical model.

#### 2.3 `owl:inverseOf`

LinkML has no inverse-slot construct.  Properties declared with `owl:inverseOf`
are emitted as forward-direction slots only.

> **Rationale.** Pydantic / TypeScript / GraphQL model properties, not
bidirectional logical relations.  Consumers can infer inverses at the OWL layer.

#### 2.4 Multiple `rdfs:domain` / `rdfs:range` values

If a property has more than one named domain or range, the compiler keeps the
first and warns about the rest.

> **Rationale.** LinkML slots have a single `domain` and `range`.  Multiple
values would require union types, which many target generators handle poorly.

#### 2.5 External-class ranges

If a property's range is a class outside GMEOW (e.g., `schema:Person`, `foaf:Agent`),
the compiler degrades it to `string` with a warning.

> **Rationale.** The LinkML schema only defines GMEOW classes.  External
classes would require importing their full type hierarchy, which is out of scope.

#### 2.6 Open-world assumptions — requiredness is never inferred

OWL is open-world: absence of a triple does not mean negation.  Pydantic / TypeScript
are closed-world, but the LinkML projection **never marks slots as `required: true`**
(LinkML `required` is not inferred from OWL cardinality).  All slots default to optional.

> **Rationale.** Requiredness in OWL is a *logical* constraint (`owl:minCardinality 1`),
not a *structural* one.  Mapping logical to structural requiredness in the *type stubs*
would over-constrain consumers and contradict the open-world design.  When you need real
requiredness, use the **SHACL-derived JSON Schema** (§1), which carries `required` from
`sh:minCount`.

---

## Generator-specific caveats

| Generator | Provenance | Known limitation |
|-----------|-----------|------------------|
| **JSON Schema** | SHACL | `sh:sparql` constraints are dropped (no equivalent); external / NodeShape-less classes degrade to a node reference. |
| **OpenAPI** | SHACL | Minimal path set (`GET /entities/{id}`) added for spec validity; not a functional API definition.  Inherits the JSON Schema losses. |
| **Pydantic** | LinkML | Embeds the source LinkML path (normalized to `gmeow.linkml.yaml` for determinism).  Custom types (`duration`) inherit from `string`. |
| **TypeScript** | LinkML | Unknown `type.base` warnings for `datetime`, `decimal`, `duration`, `uri` — these fall back to `string`. |
| **GraphQL** | LinkML | Names with illegal characters (e.g. `signatureSchemeBLS12-381`) are not valid GraphQL identifiers. Normalize before consumption (e.g. `-` → `_` → `signatureSchemeBLS12_381`, or remove → `signatureSchemeBLS12381`). The generator preserves the original name so the mapping is explicit. |

---

## Using the schemas

```bash
# Generate all artifacts into dist/schemas/
cargo run -p gmeow-dev-cli -- sync --mode update --outputs generated

# Verify they match the current ontology + shapes (CI gate)
cargo run -p gmeow-dev-cli -- sync --mode check --outputs generated

# Validate an instance document against the SHACL-derived JSON Schema
gmeow validate --schema dist/schemas/gmeow.schema.json instance.jsonld
```

Output files:

* `dist/schemas/gmeow.schema.json` — JSON Schema (closed-world, native from SHACL)
* `dist/schemas/gmeow.openapi.json` — OpenAPI 3.1 (closed-world, native from SHACL)
* `dist/schemas/gmeow.linkml.yaml` — canonical LinkML schema (OWL → LinkML)
* `gmeow_models/` — Pydantic v2 package (closed-world, native from SHACL)
* `dist/schemas/gmeow.ts` — TypeScript interfaces (OWL → LinkML)
* `dist/schemas/gmeow.graphql` — GraphQL type stubs (OWL → LinkML)

These are **build artifacts** (`dist/` is git-ignored).  Do not edit them by hand.
If a term is wrong, fix the OWL source in `ontology/modules/` (LinkML targets) or the SHACL
shapes (JSON Schema / OpenAPI), then re-run
`cargo run -p gmeow-dev-cli -- sync --mode update --outputs generated`.

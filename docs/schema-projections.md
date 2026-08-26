<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Schema projections — one compiled schema, five closed-world surfaces

> **Status.**
> **JSON Schema** (`gmeow.schema.json`), **OpenAPI** (`gmeow.openapi.json`), **LinkML**
> (`gmeow.linkml.yaml`), **TypeScript** (`gmeow.ts`), **GraphQL** (`gmeow.graphql` +
> `gmeow.graphql.name-map.json`), and **Pydantic** (`gmeow_models/`) are ALL compiled
> **natively in Rust from the SHACL shapes**, through purrdf's shared
> `CompiledSchema`. They are ALL *closed-world* structural
> surfaces — `required` on every one of them is real, derived from `sh:minCount ≥ 1`,
> not a stub.
> There is no separate "OWL → LinkML" projection anymore: the former hand-rolled
> OWL-ABox-reading LinkML/TypeScript/GraphQL emitter was deleted, and LinkML is no
> longer an intermediate step the TypeScript/GraphQL/Pydantic surfaces are derived
> through. All six artifacts are independent renderings of the SAME compiled `$defs`.
> Update: `cargo run -p gmeow-dev-cli -- sync --mode update --outputs generated`.
> CI gate: `cargo run -p gmeow-dev-cli -- sync --mode check --outputs generated` (in the `ontology` job).

GMEOW is an **OWL 2 DL** ontology with a parallel layer of **SHACL** shapes, itself
*derived* from the EL-safe cardinality/class/datatype/node-kind/value-set axioms
authored in each slice's `module.ttl` (never hand-authored — see
[`docs/MIGRATING-SHAPES-TO-LOGIC.md`](./MIGRATING-SHAPES-TO-LOGIC.md)). SHACL is a
*constraint* language that validates the well-formedness of instance data, and
because every developer schema surface below is compiled from that SAME SHACL shape
union, every one of them is a **closed-world structural** contract: `required`,
`enum`, `pattern`, cardinality, and ranges are all real, not stubs.

This document inventories all six generated artifacts and exactly what each one's
own target-language emitter loses (and why) once it renders the shared compiled
schema.

---

## 1. The shared compiled schema

Every developer schema surface compiles through ONE builder,
[`crate::stages::schema_compile::enriched_compiled_schema`]
(`crates/pipeline/src/stages/schema_compile.rs`):

1. `purrdf::shapes::json_schema::compile` compiles
   the fresh SHACL shape union into a closed-world JSON Schema (draft 2020-12) +
   OpenAPI 3.1 pair — the SAME shape union the live SHACL validator enforces.
2. gmeow's own `crate::stages::value_vocab::enrich_value_vocab_enums` folds in the
   ontology's open value vocabularies (an `owl:Class` whose members are seeded named
   individuals) as closed `enum` `$defs`, WITHOUT mutating the live SHACL shapes
   (those vocabularies stay deliberately open — "anchor, not a fence" — in the
   validator).

The result — one enriched [`purrdf::shapes::json_schema::CompiledSchema`] — is the
SINGLE `$defs` every downstream emitter renders:

* `crate::stages::json_schema` (`stage-export-json-schema`) — emits the JSON Schema
  and OpenAPI documents directly.
* `crate::stages::schemas` (`stage-export-schemas`) — renders LinkML, TypeScript, and
  GraphQL via `purrdf::shapes::{linkml,typescript,graphql}::emit_*`.
* `crate::stages::pydantic` (`stage-export-pydantic`) — renders the Pydantic v2
  package via `purrdf::shapes::pydantic::emit_pydantic`.

Because all six artifacts share the same compiled `$defs`, they agree by
construction: a Pydantic model's `model_json_schema()` matches `gmeow.schema.json`,
and a LinkML slot's `required` matches the same property's `required` entry in the
JSON Schema.

### JSON Schema construct → representation

| SHACL construct | Representation |
|-----------------|-----------------|
| `sh:minCount ≥ 1` | `required` (the property is actually mandatory) |
| `sh:minCount` / `sh:maxCount` | `minItems` / `maxItems` (or a single value for `maxCount 1`) |
| `sh:in` | `enum` |
| `sh:pattern` | `pattern` |
| `sh:minInclusive` / `sh:maxInclusive` / `sh:minExclusive` / `sh:maxExclusive` | numeric `minimum` / `maximum` / `exclusiveMinimum` / `exclusiveMaximum` |
| `sh:closed` | `additionalProperties: false` |
| `sh:datatype` / `sh:nodeKind` | typed-literal / node-reference value schemas |

These schemas **actually validate instance data**. They drive `gmeow validate --schema`
and IDE autocomplete for the YAML-LD-star surface.

`gmeow validate` dispatches by file type: RDF serializations
(`.nq`/`.trig`/`.ttl`/`.nt`/`.rdf`/`.jsonld`) run repo-free SHACL + OntoUML
conformance against the bundled shapes, while `.json`/`.yaml` run JSON-Schema
instance validation. Passing `--schema` forces the JSON-Schema path for any
input — so a JSON-LD instance is checked against this schema with
`gmeow validate --schema … instance.jsonld`, whereas a bare `instance.jsonld`
is validated as an RDF graph.

### The `@type`-discriminated envelope

The JSON Schema validates a JSON-LD `@graph` of instance nodes against a single
`$defs/Node` envelope. `Node` discriminates on `@type`: an `allOf` of conditionals reads
*if `@type` includes `gmeow:<Class>` (as a bare string or inside an array), then the node MUST
satisfy `#/$defs/<Class>`*. So a node is validated against the class def(s) named in its own
`@type`, and **an instance missing a property required by its class is REJECTED** (closed-world
enforcement). Nodes typed only by unmodeled classes fall through permissively.

### What is lost at compile time (and why)

This is a **lossy** projection (CONSTITUTION Principle 17 / the loss ledger). Every
loss below is dropped ONCE, at this shared compile step, so it is never re-lost
independently by each of the six downstream renderers. `purrdf::shapes::json_schema::compile`
records each drop as a `LossRecord` and annotates the affected schema with a `$comment`:

* **`sh:sparql` (SHACL-AF) constraints** have no JSON Schema equivalent and are **DROPPED**.
  A JSON Schema cannot run a SPARQL query, so these node- and property-level constraints are
  omitted (and logged). This is the analogue of the ShEx/SPARQL loss: structural well-formedness
  is preserved, but arbitrary SPARQL-expressed business rules are not.
* **External (non-gmeow) class references** degrade to a permissive node reference / string.
  A `gmeow:` class with no NodeShape likewise degrades to a node reference (no `$def` is emitted).
* **OWL constructs never expressible in SHACL to begin with** — intersection/union
  restrictions, `owl:inverseOf`, RDF 1.2 reification/standpoint metadata, the four-clocks
  temporal model — are not carried by the SHACL shape union in the first place, so no
  compiled `$defs` (JSON Schema, LinkML, TypeScript, GraphQL, or Pydantic) ever sees them.
  They remain canonical only at the OWL/RDF-1.2 layer (`ontology/modules/`,
  `dsl/statements/`, `statements/gmeow.rdf12.ttl`); consult that layer, or the reasoned
  OWL, for logical completeness.

Because it validates structure but is **not an entailment relation**, the projection is ledgered
`logic:ValidationOnly` — see
[`slices/grounding/logic/examples/projection-loss-ledger.ttl`](../slices/grounding/logic/examples/projection-loss-ledger.ttl),
entry `ex:shaclJsonSchemaReport`.

---

## 2. LinkML, TypeScript, GraphQL, Pydantic — purrdf-native renderers over the same `$defs`

`gmeow.linkml.yaml`, `gmeow.ts`, `gmeow.graphql` (+ `gmeow.graphql.name-map.json`), and
the `gmeow_models/` Pydantic v2 package are ALL rendered from the shared
[`CompiledSchema`](#1-the-shared-compiled-schema) above by `purrdf::shapes::linkml`,
`purrdf::shapes::typescript`, `purrdf::shapes::graphql`, and `purrdf::shapes::pydantic`
respectively — filesystem-free, deterministic emitters that consume the compiled
`$defs` and return an in-memory artifact package. None of them reads the OWL ontology
or the SHACL shapes directly, and none of them is derived through another one of
these four (in particular, TypeScript/GraphQL/Pydantic are **not** projected "through
LinkML" — that was the former hand-rolled model, retired in this cutover). Because
they share the compiled `$defs`, all four inherit the closed-world guarantees of §1:
`required`, `enum`, cardinality, and ranges are real, not inferred or stubbed.

Losses at this layer are downstream of the §1 compile step: each emitter's own
target-language type system cannot express every JSON Schema construct, and each
emitter records what it drops on its own `LossLedger`, at the JSON Pointer location
of the affected construct. gmeow aggregates and logs each package's ledger (`tracing`
targets `schemas_loss` / `pydantic_loss`, one event per `(code, reason)` pair) — never
a silent drop.

### 2.1 LinkML (`gmeow.linkml.yaml`)

`purrdf::shapes::linkml::emit_linkml` (LinkML 1.11 metamodel) maps each compiled
`$def` to a `classes` entry and each property to a `slots` entry, with `required`
copied verbatim from the compiled schema's `required` array — a LinkML slot's
`required: true` really does mean the underlying SHACL shape has `sh:minCount ≥ 1`.

**gmeow-side pre-pass (not a purrdf loss):** LinkML slot names double as
code-generation identifiers, so `emit_linkml` requires every property to resolve
under a caller-registered CURIE prefix with a valid NCName local part, and hard-fails
the WHOLE document on the first violation. `crate::stages::schemas::sanitize_linkml_property_names`
renames the small set of properties that don't qualify — e.g. an openEHR OPT-lifted
cardinality helper property whose local part embeds `/`-separated archetype path
segments (`gmeow:openehr/bloodpressure/occurrences/at0005`), or a property riding in
under an unregistered prefix — on a PRIVATE copy of the schema used only for the
LinkML render; TypeScript, GraphQL, and Pydantic render the UNMODIFIED shared schema,
so their property names are never affected. Every rename is logged (`tracing` target
`schemas_loss`, `surface = "linkml"`, `construct = "property-name"`) — never silent.

### 2.2 TypeScript (`gmeow.ts`)

`purrdf::shapes::typescript::emit_typescript` targets TypeScript 7.0 under `strict`
and `exactOptionalPropertyTypes`, using type aliases (which preserve unions,
intersections, tuples, literal values, and recursive JSON-object compatibility
without declaration merging). JSON Schema assertions with no exact structural
type-system equivalent — e.g. `pattern`, numeric bounds, `format` — are recorded on
the package's loss ledger at their JSON Pointer location rather than silently
enforced; no output path degrades to `any`.

### 2.3 GraphQL (`gmeow.graphql` + `gmeow.graphql.name-map.json`)

`purrdf::shapes::graphql::emit_graphql` targets the GraphQL September 2025
type-system (a type-system fragment, not an executable service — operation roots,
resolvers, authorization, and pagination stay caller-owned). Every structural object
is emitted as paired output/input types. GraphQL variable coercion is not JSON Schema
validation, so differences such as singleton-list coercion, fixed input-field sets,
custom-scalar behavior, and required/null presence are recorded on the package's loss
ledger.

`gmeow.graphql.name-map.json` is a **new artifact** in this cutover: the
source-field/enum-value → GraphQL-identifier codec `emit_graphql` produces alongside
the SDL. GraphQL's identifier grammar rejects characters a compacted CURIE local part
may legally carry (e.g. `signatureSchemeBLS12-381`), so the emitter renames such
fields/enum values for the SDL and ships the canonical mapping rather than requiring
a consumer to guess the normalization rule. It is shipped, not dropped —
no-optionality forbids silently discarding a produced artifact.

### 2.4 Pydantic (`gmeow_models/`)

`purrdf::shapes::pydantic::emit_pydantic` renders one Pydantic v2 model per compiled
`$def`, with field aliases matching the JSON property names and a class-owned
`model_json_schema()` hook onto the SAME compiled `$defs` — so `model_json_schema(by_alias=True)`
reconstructs the originating JSON Schema definition. JSON Schema assertions with no
exact Pydantic runtime-annotation equivalent are recorded on the package's loss
ledger the same way. See [§4](#4-generator-specific-caveats) for the package-layout
change (a flat single-module package) this cutover also made.

---

## 3. What changed from the former OWL → LinkML model

Before this cutover, JSON Schema/OpenAPI were already SHACL-native, but LinkML,
TypeScript, and GraphQL were a **separate** hand-rolled projection that read the
composed OWL/RDF carrier dataset directly (`rdfs:subClassOf`, `owl:ObjectProperty`,
`rdfs:domain`/`rdfs:range`, …) and Pydantic was rendered *from that LinkML schema*.
That path is open-world by construction — OWL's absence-of-a-triple is not
negation — so it could never mark a slot `required`, dropped every OWL restriction
(intersection/cardinality), `owl:inverseOf`, and the RDF 1.2 reification/standpoint
layer, and collapsed multiple `rdfs:domain`/`rdfs:range` values to "keep the first,
warn about the rest."

None of that applies anymore. All four target-language surfaces (§2) compile from
the closed-world SHACL-derived `CompiledSchema` (§1), directly, in parallel — never
through each other, and never by re-reading the OWL ontology. `required` is real
everywhere; the losses in §2 are purely target-language expressiveness gaps (a
TypeScript type alias can't carry `pattern`; a GraphQL identifier can't carry `-`),
not open-world-vs-closed-world gaps.

---

## 4. Generator-specific caveats

| Generator | Package layout | Known limitation |
|-----------|-----------------|-------------------|
| **JSON Schema** | Single file | `sh:sparql` constraints are dropped (no equivalent); external / NodeShape-less classes degrade to a node reference. |
| **OpenAPI** | Single file | Minimal path set (`GET /entities/{id}`) added for spec validity; not a functional API definition. Inherits the JSON Schema losses. |
| **LinkML** | Single file | A compacted-CURIE property whose local part is not a valid NCName is renamed for this surface only (§2.1); the rename is logged. |
| **TypeScript** | Single `.d.ts` | Assertions with no structural type-system equivalent (`pattern`, numeric bounds, `format`) are recorded on the loss ledger, not enforced by the type. |
| **GraphQL** | SDL + `name-map.json` | Identifier-illegal source names (e.g. `signatureSchemeBLS12-381`) are renamed for the SDL; the shipped `name-map.json` carries the canonical original ↔ GraphQL-identifier mapping. |
| **Pydantic** | `gmeow_models/` — a FLAT single-module package (`_base.py`, `models.py`, `__init__.py`, `py.typed`, `__about__.py`) | The current GMEOW call selects PurRDF's flat compatibility configuration even though PurRDF now supports caller-owned routed topology and class metadata. The resulting absence of per-slice modules, per-class docs-digest linkage, and a generated `README.md` is live, durably tracked developer-surface work, not an authorized capability regression or a `.deficiencies` descope. `gmeow_models/__about__.py` (the wheel version, from the ontology's `owl:versionInfo`) remains gmeow-orchestrated, stamped alongside PurRDF's artifacts. |

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
* `dist/schemas/gmeow.linkml.yaml` — LinkML 1.11 schema (closed-world, native from SHACL)
* `dist/schemas/gmeow.ts` — TypeScript 7.0 declarations (closed-world, native from SHACL)
* `dist/schemas/gmeow.graphql` + `dist/schemas/gmeow.graphql.name-map.json` — GraphQL SDL + canonical name map (closed-world, native from SHACL)
* `gmeow_models/` — Pydantic v2 package (closed-world, native from SHACL)

These are **build artifacts** (`dist/` is git-ignored). Do not edit them by hand.
If a term is wrong, fix the OWL/RDFS axioms in the slice's `module.ttl` — the
pipeline derives the SHACL shapes those axioms compile into, and every schema
surface above compiles from that SAME shape union — then re-run
`cargo run -p gmeow-dev-cli -- sync --mode update --outputs generated`.

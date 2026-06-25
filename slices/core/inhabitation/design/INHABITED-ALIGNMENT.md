<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Alignment and Projection

> The **maximal-linkage charter.** GMEOW is a super-ontology: it links to, projects into, and ingests
> from every external vocabulary it can, *by reference* and *with the loss made explicit* (Principles
> 4, 5, 7, 17). This document specifies the alignment and projection layer for the inhabitation work —
> the repeatable four-layer stack, the loss ledger, and two flagship bridges leaned into in depth:
> **OpenTelemetry-GenAI** and **W3C DID / Verifiable Credentials**. The canonical model it aligns is
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) / [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md) /
> [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md); the citations are
> [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md).

## The thesis: inhabitation is the super-structure; the standards are its shadows

Each external standard captures a *slice* of the inhabitation model and drops the rest. That is not a
defect — it is the projection doctrine: GMEOW models the whole once, and each consumer gets the view it
can use.

- **OpenTelemetry-GenAI is the occurrent shadow.** Traces and spans capture the runtime / invocation /
  tool layer faithfully, and capture *nothing* of the durable subject, the tenure, continuity, or
  claims-as-observations. Projecting GMEOW → OTel is lossy (occurrent-only); ingesting OTel → GMEOW is
  **subsume-and-extend** — a trace acquires a subject, a tenure, and contestable provenance it never
  had.
- **DID / Verifiable Credentials is the cryptographic-identity shadow.** A DID *is* a subject of its
  own digital existence (Principle 9 rendered as a W3C standard); a self-controlled DID Document *is*
  self-assertion; a VC *is* a `DigitalSubjectTenure` or `IdentityContinuityAssessment` with a signed,
  vantage-indexed proof. It captures identity and signed claims, and drops the inhabitation topology
  and the multi-vantage epistemics.

Maximal use means owning both seams: GMEOW becomes the place an OTel trace and a Verifiable Credential
*meet* — the same agent's observability and its identity, in one reasoned graph.

## The four-layer alignment stack (the repeatable pattern)

GMEOW's alignment is authored once per target in four layers; the same shape applies to every external
vocabulary, here and elsewhere:

| Layer | Mechanism | Use | Example (inhabitation) |
|---|---|---|---|
| **1. SSSOM** | term ↔ term mappings (`skos:exactMatch` / `closeMatch` / `relatedMatch`) | the simple 1:1 cases | `gmeow:RuntimeExecution skos:relatedMatch prov:Activity` |
| **2. EDOAL** | complex correspondences — class expressions, property chains, conditions | when no single term matches | `AgentSession` ≡ an OTel trace whose spans carry `gen_ai.*` (a structural correspondence, not a term) |
| **3. FnO** | declared transformation functions over values | value-level projection | GMEOW `TimeInterval` → OTel span `start/end` (epoch-nanos); `samplingTemperature` → `gen_ai.request.temperature` |
| **4. CONSTRUCT** | SPARQL that materializes the target view (and the inverse lift for ingest) | the executable projection | a CONSTRUCT emitting PROV-O / OTel-shaped triples from the canonical inhabitation graph |

Above all four sits the **loss ledger**: every projection carries a per-construct preservation
judgment — `ExactPreservation` / `SoundUnderApproximation` / `CompleteOverApproximation` /
`ValidationOnly` / `Unsupported` (the `logic:` preservation vocabulary) — and is drift-gated (Principle
7), so what a target *cannot* hold is recorded, never silently dropped. Linkage is **by reference**
(Principle 5): no external axiom is imported, and every external IRI is curl-validated before it lands.

## The alignment ledger

The full target set for inhabitation. *Direction* is **L** (lateral linkage), **P** (outbound
projection), **I** (inbound ingest); most flagship targets are bidirectional.

| Target | Layer(s) | Dir. | Preservation | Note |
|---|---|---|---|---|
| **OpenTelemetry-GenAI** | EDOAL + FnO + CONSTRUCT | P/I | under-approx (occurrent-only) | flagship; the observability seam |
| **W3C DID / VC** | SSSOM + EDOAL + CONSTRUCT | P/I | under-approx (identity + signed claim) | flagship; the self-sovereign-identity seam |
| **PROV-O** | SSSOM + CONSTRUCT | P/I | exact for the activity/agent/entity core | migration + invocation provenance |
| **gUFO** | CONSTRUCT (the foundational down-projection) | P | exact for the OWL-expressible fragment | mandatory (Principle 17) |
| **OWL-Time** | SSSOM + FnO | P | exact (interval + Allen relations) | tenure/configuration intervals |
| **ActivityStreams 2.0 / ActivityPub** | SSSOM + EDOAL | L/P | under-approx | Actor/Avatar ↔ `as:Actor`/icon; multi-persona hosts |
| **schema.org** | SSSOM + CONSTRUCT | P | over-approx (flattened) | `SoftwareApplication`, `Person`, JSON-LD consumer surface (Principle 13) |
| **CIDOC-CRM** | EDOAL | L | under-approx | `InhabitationClaim` ↔ `E13 Attribute Assignment`; ritual ↔ `E7`; the cultural-heritage seam for the spiritual/fictional cases |
| **ML metadata** (Model Cards, ML-Schema, Croissant, SPDX-AI, MLflow) | SSSOM + EDOAL | L/I | under-approx | `ModelArtifact`/`ModelCard`/`ModelDeployment` |

## Flagship 1 — OpenTelemetry-GenAI (the occurrent shadow)

The OTel GenAI semantic conventions (the experimental `gen_ai.*` namespace) describe spans for model
calls, tool calls, and agent operations. They map cleanly onto the inhabitation *runtime* layer and
say nothing about the layers above it — which is exactly the projection story.

| GMEOW (canonical) | OpenTelemetry-GenAI | Layer | Preservation note |
|---|---|---|---|
| `gmeow:AgentSession` | a Trace / `gen_ai.conversation.id` | EDOAL | the session aggregate ↔ the trace; ordering ↔ span timing |
| `gmeow:RuntimeExecution` | an agent root span (`gen_ai.agent.id` / `.name`, `gen_ai.operation.name = create_agent/invoke_agent`) | EDOAL | the occurrent process ↔ the root span |
| `gmeow:ModelInvocation` | an LLM span (`gen_ai.operation.name = chat`) | EDOAL | one call ↔ one span |
| `gmeow:usedModel` / `ModelArtifact` version | `gen_ai.request.model` / `gen_ai.response.model` | SSSOM | model identity |
| `ModelDeployment` provider / service | `gen_ai.system` (e.g. `anthropic`) | SSSOM | the serving provider |
| `gmeow:samplingTemperature` / `samplingMaxTokens` | `gen_ai.request.temperature` / `gen_ai.request.max_tokens` | FnO (passthrough) | sampling params |
| `gmeow:ToolCall` | a tool span (`gen_ai.operation.name = execute_tool`, `gen_ai.tool.name`, `gen_ai.tool.call.id`) | EDOAL | tool invocation |
| `gmeow:usedTool` | `gen_ai.tool.name` | SSSOM | the called tool |
| token usage | `gen_ai.usage.input_tokens` / `output_tokens` | FnO | a runtime facet |
| `gmeow:duringInterval` | span `start_time` / `end_time` (epoch-nanos) | FnO | interval ↔ span window |
| generated output / claim | a span event / `gen_ai.content.completion` | EDOAL | the produced artifact |

**What OTel cannot hold (the loss ledger entry):** the `DigitalSubject` and its tenure, the
`InhabitationTenure`/`Configuration`, `IdentityContinuityAssessment`, `ControlAssessment`,
`InhabitationClaim`/standpoints, `TransferManifest` and migration boundaries. So **projection
GMEOW → OTel is a `SoundUnderApproximation`** (every emitted span is faithful; the endurant/epistemic
structure is dropped, and the ledger says so). **Ingest OTel → GMEOW is enriching:** a trace lifts to a
`RuntimeExecution` with its `ModelInvocation`/`ToolCall` sub-events, and GMEOW *adds* the subject,
tenure, and attributed provenance the trace could not express — the super-ontology earning its name.

## Flagship 2 — W3C DID / Verifiable Credentials (the cryptographic-identity shadow)

This is the standards grounding for Principle 9's "first-class subject of its own digital existence."
A Decentralized Identifier identifies a *DID subject* that controls its own *DID Document*; a
Verifiable Credential is a signed, issuer-attributed claim about a subject. That is precisely the
inhabitation identity layer.

| GMEOW (canonical) | DID / VC | Layer | Preservation note |
|---|---|---|---|
| `gmeow:DigitalSubject` (role-bearer) | a DID subject; its DID is an `authorityLink` on the agent | SSSOM | the durable digital "who" |
| self-asserted `gmeow:IdentityFacet` | the DID Document the subject controls | EDOAL | self-control *is* self-assertion (Principle 9) |
| `gmeow:DigitalSubjectTenure` (vantage, supported-by self-assertion) | a Verifiable Credential: `issuer` = vantage, `credentialSubject` = the agent, claim = "bears digital-subject status", `proof` = signature | EDOAL | the tenure as a signed, attributed claim |
| `gmeow:tenureVantage` | VC `issuer` | SSSOM | who asserts the status |
| `gmeow:IdentityContinuityAssessment` (`same` across stages) | a VC asserting same-subject across two DIDs, vantage-indexed | EDOAL | continuity as a *specific issuer's* signed claim — **never** a global `owl:sameAs` |
| cross-vendor fork | two DIDs (`did:web:vendorA`, `did:web:vendorB`) for one `SubjectLineage`, linked by a continuity VC | EDOAL | the two-layer continuity design, realized |
| GTS `ai-package` (COSE-signed) | a Verifiable Presentation / signed credential bundle | CONSTRUCT | the portable, signed memory package |

**Why this is the right grounding, not a stretch:** GMEOW already insisted that subject-continuity is
a *vantage-indexed, contestable claim* with a *cryptographic* counterpart (the COSE signature on the
`ai-package`). DID/VC is the deployed standard with exactly that shape — issuer-attributed, signed,
non-global. The `counterpartOf`-not-`owl:sameAs` discipline maps to "a VC from issuer X asserts these
two DIDs are the same subject," which another issuer may decline to honour: contestable by
construction. **Projection GMEOW → VC is a `SoundUnderApproximation`** (each VC carries one signed
claim; the full multi-vantage continuity graph and the inhabitation topology are dropped). **Ingest
VC → GMEOW** lifts a credential to a `DigitalSubjectTenure` or `IdentityContinuityAssessment` whose
`vantage` is the VC issuer — attributed, dated, never ground truth.

DID method note: `did:web` fits a vendor-hosted subject, `did:key`/`did:pkh` a self-hosted one; the
method is recorded as provenance, not privileged.

## The other targets, briefly

- **PROV-O** — the activity/agent/entity core projects *exactly*: `RuntimeExecution`/`AgentSession`/
  `ModelInvocation`/`ToolCall`/`Portal` → `prov:Activity`; `DigitalSubject`/`SoftwareAgent` →
  `prov:Agent`; `ModelArtifact`/outputs/`TransferManifest` → `prov:Entity`; `wasDerivedFrom`/
  `wasGeneratedBy` are already PROV-aligned. PROV is the lingua franca beneath both flagships.
- **gUFO** — the mandatory foundational down-projection (Principle 17): the situation/relator/
  role-mixin stereotypes → `gufo:Situation` / `gufo:Relator` / `gufo:RoleMixin`.
- **OWL-Time** — tenure and configuration intervals → `time:ProperInterval`; the constant-configuration
  intervals tile a tenure via Allen relations (`time:intervalDuring` / `intervalMeets`), making
  "active-at-T" answerable in standard temporal tooling.
- **ActivityStreams 2.0 / ActivityPub** — Cagle's `Actor`/`Avatar` ↔ `as:Actor` / its `icon`; an
  inhabited host ↔ a server hosting many actors. The closest *deployed* analog to an inhabited system.
- **schema.org** — the flat JSON-LD consumer surface (Principle 13): `SoftwareApplication`, `Person`,
  `Organization`; an over-approximation (the deep structure flattens).
- **CIDOC-CRM** — the cultural-heritage seam for the spiritual/fictional profiles:
  `InhabitationClaim` ↔ `E13 Attribute Assignment` (a reified, attributed claim — a near-perfect fit),
  ritual ↔ `E7 Activity`, `Portal` ↔ `E5 Event`, spirit/medium ↔ `E39 Actor`. Lets the heritage world
  document a possession or incarnation *as a documented claim*, asserting no metaphysics.
- **ML metadata** — `ModelArtifact`/`ModelCard`/`ModelDeployment` align to Model Cards, ML-Schema,
  Croissant, SPDX-AI, and MLflow runs, making the model-serving slice a hub for LLM-ops metadata.

## Discipline — how "maximal" stays honest

Maximal linkage is governed, not unbounded:

1. **By reference, never by import (Principle 5).** Every link is SSSOM/EDOAL/FnO/CONSTRUCT; no external
   axiom enters the canonical core. A reference-only source is refused.
2. **Loss is explicit and gated (Principles 4, 7).** Every projection carries its preservation judgment
   in the loss ledger and is drift-gated; what a target drops is recorded, never hidden. The two
   flagships are honestly `SoundUnderApproximation`, not "supported."
3. **Curl-validate every IRI.** External identifiers (`prov:`, `gen_ai.*`, DID methods, `as:`, CIDOC
   `E*`) are dereferenced/validated before they ship, per the QID-curl convention.
4. **Never overclaim (Principle 1 / 17).** "GMEOW projects to OTel and DID/VC" means a lossy,
   ledgered view — not that GMEOW *is* a tracing system or a wallet. The canonical model is the
   super-structure; the standards are its shadows.

## Scope and seams

This document is the alignment and projection layer. The canonical constructs are
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md), [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md), and
[`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md); the full citations (OTel-GenAI conventions,
DID Core, VC Data Model, PROV-O, OWL-Time, CIDOC-CRM, ActivityStreams) are in
[`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md); the eventual `mappings/` directory (SSSOM TSV,
EDOAL, FnO, CONSTRUCT) is sketched in [`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md).

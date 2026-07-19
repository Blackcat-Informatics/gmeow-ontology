# Reasoning in GMEOW — OWL infers, SHACL validates

GMEOW is *reasoning-centric* (CONSTITUTION **Principle 8**) and *verified by construction*
(**Principle 7**): the invariants we care about should be **entailed or checked by the
toolchain**, not maintained by hand-written code that can drift from the ontology. Doing that
well means using the *right logic for each job*. OWL 2 and SHACL are not competitors; they
answer different questions, and GMEOW uses both deliberately.

> **The split, in one line.** *OWL is open-world and classifies; SHACL is closed-world and
> validates.* (Knublauch, ["SHACL and OWL Compared"](https://spinrdf.org/shacl-and-owl.html);
> [SHACL-AF](https://www.w3.org/TR/shacl-af/).)

GMEOW also annotates graph roles with `gmeow:graphBoxRole` (see
[`docs/four-boxes.md`](./four-boxes.md)). In this document, the ABox is the
asserted data graph being checked, the TBox is the vocabulary/shape/logic layer,
the RBox is the property and role behavior that reasoners and path constraints
traverse, and the CBox is statement context: RDF 1.2 reifiers plus provenance,
evidence, confidence, time, standpoint, determinacy, and disclosure metadata.
Those labels help reports and docs explain which layer failed; they do not
change which sources are canonical or which projection is generated.

## Why not "just use OWL cardinality"?

Under the **open-world assumption**, OWL never reports a *violation* — it draws *inferences*.
Assert that a person has one gender-identity facet and an OWL `maxCardinality 1` does not flag
a second facet as illegal; it *infers the two facets are `owl:sameAs`*. OWL cannot say "this
data is missing a required value" or "this relator has too many relata", because absence is not
contradiction in an open world. Worse, OWL 2 cardinality pushes the core out of the **EL**
profile, costing us the fast EL pre-check (see [the EL boundary](https://www.w3.org/TR/owl2-profiles/)).

So GMEOW keeps two halves of every relator invariant:

| Logic | Axiom (phase 2) | Constraint (phase 3) |
|---|---|---|
| **OWL 2 EL** (open-world) | `gmeow:GenderIdentity ⊑ ∃ gmeow:genderValue . gmeow:Gender` — *points at some Gender*; the reasoner uses it to **classify**. | — |
| **SHACL** (closed-world) | — | `sh:minCount 1 ; sh:maxCount 1` on `gmeow:genderValue` — *points at **exactly** one*; `gmeow_shacl` uses it to **validate**. |

The OWL existential lives in `ontology/modules/*.ttl`; the matching SHACL cardinality lives in
`shapes/gmeow-shapes.ttl`. `ontology/modules/gender.ttl` even says so inline: *"'exactly one'
lives in SHACL"*.

## The three lanes

GMEOW runs three complementary verification lanes. Each owns a distinct class of invariant.

| Lane | Tool | World | Owns | Where |
|---|---|---|---|---|
| **Native EL/DL gate** | `gmeow_logic` | open | Docker-free profile, consistency, and entailment authority | `make reason` |
| **Entailment tests** | native OWL 2 RL chase (`gmeow_logic::reason::rl_closure`) | open | positive derivations — property chains, transitivity, sub-property closure | `crates/logic/tests/ontology_entailments.rs` |
| **Closed-world validation** | SHACL (`gmeow_shacl`) + native `verify` | closed | cardinality, required shapes, display contract, orthogonality data-checks | `make validate`, `make verify`, `tests/test_shapes.py` |

Reasoning order is **reason first to enrich, then validate the enriched graph**. The native
`gmeow_logic` lane is the **single** reasoning authority — there is no live second reasoner
on-gate. Engine-independent coverage of the retired `purrdf::entail` comparison is retained
without running a second engine, via the committed, frozen `dl_oracle_gold` corpus (an offline
conformance test asserting native ⊇ frozen oracle) and the native gap-zero DL⊇EL crosscheck
ledger (`dl-el-crosscheck-report.ttl`).

### Lane 1 — Native reasoner

`make reason` runs the native Docker-free EL/DL authority. `make reason-verify` computes one
complete native result and verifies that value in the same closure, so the aggregate gate never
chases the bundle twice. A contradiction — e.g. an
individual placed in two disjoint identity axes, or two disjoint Kinds (Person ⊓ Organization) —
makes the native reasoner report an inconsistency. These are the *open-world* gates:
unsatisfiability and inconsistency, nothing else.

### Lane 2 — native OWL 2 RL entailment tests (Docker-free)

For *positive* entailments we run the native OWL 2 RL chase (`gmeow_logic::reason::rl_closure`, in
`crates/logic/tests/ontology_entailments.rs`) so the tests run on every Rust test invocation without
Docker. They load a **real authored module** plus a tiny A-Box and assert a
triple the reasoner *derives*:

- `hasParent ∘ hasParent ⊑ hasAncestor` — derived ancestry (a property chain);
- `locatedAt ∘ containedInPlace ⊑ locatedAt` — location through containment;
- `subOrganizationOf` transitivity.

The entailment-dependent competency questions test what GMEOW **entails**, not merely what is
asserted (entailment is monotonic, so every asserted answer survives). The ancestry-by-reasoning
check in `crates/logic/tests/ontology_entailments.rs` makes the gain explicit: the
`gmeow:hasAncestor` answer triple is **absent** from the asserted graph and **present** after
materialization — it is entailed by the property chain, authored nowhere in the A-Box. Competency
questions whose expected answer is entailed rather than asserted opt into the native RDFS-closed
lane (`gmeow:cqReasoning gmeow:reasoningRdfs`, see `crates/slicetest/src/stores.rs`).

### Lane 3 — closed-world validation (SHACL + native `verify`)

Two sub-lanes, both closed-world, for the constraints OWL deliberately cannot enforce:

- **SHACL (`gmeow_shacl`), always-on, asserted graph.** `make validate` runs the shapes in
  `shapes/gmeow-shapes.ttl`. Because it validates the **term graph** (the TBox has no
  instances), the instance-data shapes stay dormant there and cannot regress the gate; they bite
  when a *data* graph is checked (`tests/test_shapes.py`, against the
  `tests/fixtures/shapes/relator-{wellformed,malformed}.ttl` fixtures). SHACL owns:
  - **relator well-formedness** — exactly-one cardinality on each facet's value property and on
    `gmeow:NameUsage`'s two relata (the closed-world dual of the phase-2 existentials);
  - **the suppression / display contract (Principle 10)** — a superseded identity facet (one
    carrying `gmeow:validUntil`) must set `gmeow:displayable false`: *suppressed from display,
    never deleted*. A **warning**, like the deadname shape — a source may legitimately lag the
    flag;
  - **orthogonality (Principle 9)** — no individual may fill two of the seven disjoint identity
    axes (the closed-world counterpart of the OWL `AllDisjointClasses`, caught without a
    reasoner).

- **Native `verify`, reasoned graph, Java/Docker-free.** `make verify` runs the Rust
  `verify --mode native`: the native EL/DL reasoner (`crates/logic`) materializes the asserted
  graph **unioned with** the derived subsumption/type closure into an oxigraph store, then runs
  the SPARQL **SELECT** "bad-example" queries in `queries/verify/*.rq` (+ per-slice verify
  queries) over it — the OBO query-check (QC) pattern: any returned row is a
  violation, surfaced as an `error` diagnostics finding. Unlike the `gmeow_shacl` lane (asserted
  only), these see the reasoned closure, so they catch problems that appear *after* inference.
  They currently assert:
  - every GMEOW class is punned with a gUFO meta-class (meta-grounding completeness);
  - each of the seven identity axes is a member of the disjointness matrix;
  - no class is a subclass — asserted **or inferred** — of two disjoint axes.

  This lane is now on the required path (the `ontology` CI job) and in `make check`, with no
  Docker. The native reasoner is the sole authority for the reasoned closure; there is no live
  second reasoner confirming it on-gate. Engine-independent coverage of that former comparison
  is retained via the committed, frozen `dl_oracle_gold` corpus and the native gap-zero DL⊇EL
  crosscheck ledger.

## The gUFO grounding reaches outward (foundational bridging)

The meta-grounding above makes gUFO GMEOW's foundational spine. That spine is **bridged by
reference** to the broader top-level world — gUFO's nature categories align (`skos:closeMatch`,
never imported) to **BFO 2020** (ISO/IEC 21838-2), so GMEOW interoperates with the OBO-Foundry /
ISO lineage without adding a single axiom to the reasoned core. This is the final phase of the
reasoning-depth epic. Full rationale, the mapping table, the recorded gaps, and the maintenance /
extension guide (incl. how to add DOLCE/SUMO): [`docs/foundational-bridging.md`](./foundational-bridging.md).

## Suppression, never erasure (Principle 10)

The display contract is **validated, not merely conventional**. There is exactly one display
control in GMEOW — `gmeow:displayable` — and no `preferred`/`primary` marker anywhere (display
selection is locale-relative and symmetric; only *suppression* is modelled). A superseded name
(deadname) or superseded identity facet is **kept** with `gmeow:displayable false`, never
deleted. SHACL `gmeow:DeadnameSuppressionShape` and `gmeow:SupersededFacetSuppressionShape`
enforce that contract on data; consumers MUST honour `false` and never surface the string.

## Running it

```bash
make validate   # Rust SHACL + syntax + term-annotation lint (always-on)
make reason     # native Docker-free EL/DL reasoning authority
make reason-verify  # native reasoning + reasoned-graph verify, one closure (Docker-free)
make verify     # reasoned-graph SPARQL QC — native EL/DL closure (Java/Docker-free)
make rust-test  # native OWL 2 RL entailment tests (crates/logic) + conformance tests
uv run pytest   # SHACL data-shape tests + native verify tests
```

## References

- Holger Knublauch, *SHACL and OWL Compared* — <https://spinrdf.org/shacl-and-owl.html>
- W3C, *SHACL Advanced Features (SHACL-AF)* — <https://www.w3.org/TR/shacl-af/>
- W3C, *OWL 2 Web Ontology Language Profiles* (the EL boundary) — <https://www.w3.org/TR/owl2-profiles/>
- Baader, Brandt, Lutz, *Pushing the EL Envelope* (IJCAI 2005)
- Grüninger & Fox, *competency questions* (IJCAI-95); Bezerra et al., *Verifying DL Ontologies
  based on Competency Questions and Unit Testing* (CEUR Vol-1908)
- CONSTITUTION Principles **7** (verified by construction), **8** (reasoning-centric & FAIR),
  **9** (orthogonality / anti-overtyping), **10** (suppression, never erasure)

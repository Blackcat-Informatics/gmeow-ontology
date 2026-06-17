# Reasoning in GMEOW — OWL infers, SHACL validates

GMEOW is *reasoning-centric* (CONSTITUTION **Principle 8**) and *verified by construction*
(**Principle 7**): the invariants we care about should be **entailed or checked by the
toolchain**, not maintained by hand-written code that can drift from the ontology. Doing that
well means using the *right logic for each job*. OWL 2 and SHACL are not competitors; they
answer different questions, and GMEOW uses both deliberately.

> **The split, in one line.** *OWL is open-world and classifies; SHACL is closed-world and
> validates.* (Knublauch, ["SHACL and OWL Compared"](https://spinrdf.org/shacl-and-owl.html);
> [SHACL-AF](https://www.w3.org/TR/shacl-af/).)

## Why not "just use OWL cardinality"?

Under the **open-world assumption**, OWL never reports a *violation* — it draws *inferences*.
Assert that a person has one gender-identity facet and an OWL `maxCardinality 1` does not flag
a second facet as illegal; it *infers the two facets are `owl:sameAs`*. OWL cannot say "this
data is missing a required value" or "this relator has too many relata", because absence is not
contradiction in an open world. Worse, OWL 2 cardinality pushes the core out of the **EL**
profile, costing us the fast ELK pre-check (see [the EL boundary](https://www.w3.org/TR/owl2-profiles/)).

So GMEOW keeps two halves of every relator invariant:

| Logic | Axiom (phase 2, #38) | Constraint (phase 3, #39) |
|---|---|---|
| **OWL 2 EL** (open-world) | `gmeow:GenderIdentity ⊑ ∃ gmeow:genderValue . gmeow:Gender` — *points at some Gender*; the reasoner uses it to **classify**. | — |
| **SHACL** (closed-world) | — | `sh:minCount 1 ; sh:maxCount 1` on `gmeow:genderValue` — *points at **exactly** one*; `gmeow_shacl` uses it to **validate**. |

The OWL existential lives in `ontology/modules/*.ttl`; the matching SHACL cardinality lives in
`shapes/gmeow-shapes.ttl`. `ontology/modules/gender.ttl` even says so inline: *"'exactly one'
lives in SHACL (#39)"*.

## The four lanes

GMEOW runs four complementary verification lanes. Each owns a distinct class of invariant.

| Lane | Tool | World | Owns | Where |
|---|---|---|---|---|
| **EL pre-check** | ELK (ROBOT, Docker) | open | fast incoherence / unsatisfiability in the EL fragment | `make reason` |
| **DL gate** | HermiT (ROBOT, Docker) | open | sound + complete consistency, disjointness contradictions | `make reason-hermit`; `tests/test_reasoning_entailments.py` |
| **Entailment tests** | `owlrl` (pure-Python OWL 2 RL) | open | positive derivations — property chains, transitivity, sub-property closure | `tests/test_reasoning_entailments.py`, `tests/test_competency.py` |
| **Closed-world validation** | SHACL (`gmeow_shacl`) + ROBOT `verify` | closed | cardinality, required shapes, display contract, orthogonality data-checks | `make validate`, `make verify`, `tests/test_shapes.py` |

Reasoning order is **reason first to enrich, then validate the enriched graph**. ELK is an
*incomplete* pre-check (GMEOW already uses `owl:inverseOf`, `SymmetricProperty`, functional
properties — strictly outside EL), so it catches incoherence early and cheaply; HermiT is the
complete authority at release time.

### Lane 1–2 — OWL reasoners (ELK, HermiT)

`make reason` merges the import closure, checks the OWL 2 **DL** profile, and runs **ELK** for
fast incoherence. `make reason-hermit` runs **HermiT** for sound-and-complete consistency. A
contradiction — e.g. an individual placed in two disjoint identity axes, or two disjoint Kinds
(Person ⊓ Organization) — makes ROBOT exit non-zero. These are the *open-world* gate:
unsatisfiability and inconsistency, nothing else.

### Lane 3 — `owlrl` entailment tests (pure-Python, Docker-free)

For *positive* entailments we use `owlrl` (OWL 2 RL) so the tests run on every `pytest`
invocation without Docker. They load a **real authored module** plus a tiny A-Box and assert a
triple the reasoner *derives*:

- `hasParent ∘ hasParent ⊑ hasAncestor` — derived ancestry (a property chain);
- `locatedAt ∘ containedInPlace ⊑ locatedAt` — location through containment;
- `subOrganizationOf` transitivity.

`tests/test_competency.py` runs the competency questions over an `owlrl`-materialized graph, so
they test what GMEOW **entails**, not merely what is asserted (entailment is monotonic, so every
asserted answer survives). `test_competency_ancestry_is_answered_only_by_reasoning` makes the
gain explicit: it shows the `gmeow:hasAncestor` answer triple is **absent** from the asserted
graph and **present** after materialization — it is entailed by the property chain, authored
nowhere in the A-Box.

### Lane 4 — closed-world validation (SHACL + ROBOT `verify`)

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

- **ROBOT `verify`, release-grade, reasoned graph.** `make verify` reuses the ELK-reasoned
  graph already produced by `make reason` (`dist/gmeow-reasoned-elk.ttl`), avoiding a duplicate
  reasoning pass. It runs the SPARQL **SELECT** "bad-example" queries in `queries/verify/*.rq`
  over the **materialized** graph — the [OBO QC pattern](http://robot.obolibrary.org/): any
  returned row is a violation. The underlying reason step uses `--exclude-tautologies structural`,
  so trivial entailments like `X ⊑ owl:Thing` never trip a query. Unlike the `gmeow_shacl` lane
  (asserted only), these see the reasoned closure, so they catch problems that appear *after*
  inference. They currently assert:
  - every GMEOW class is punned with a gUFO meta-class (meta-grounding completeness);
  - each of the seven identity axes is a member of the disjointness matrix;
  - no class is a subclass — asserted **or inferred** — of two disjoint axes.

  This lane is skipped when the pinned ROBOT image is absent (like the HermiT tests), but never
  silently passed — CI's reasoning job runs it for real.

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
make validate   # SHACL + syntax + term-annotation lint (pure Python, always-on)
make reason     # merge → OWL 2 DL profile → ELK incoherence (Docker)
make reason-hermit  # sound + complete consistency (Docker)
make verify     # reasoned-graph SPARQL QC — ROBOT verify (Docker)
uv run pytest   # owlrl entailment tests + SHACL data-shape tests + (Docker) HermiT/verify
```

## References

- Holger Knublauch, *SHACL and OWL Compared* — <https://spinrdf.org/shacl-and-owl.html>
- W3C, *SHACL Advanced Features (SHACL-AF)* — <https://www.w3.org/TR/shacl-af/>
- W3C, *OWL 2 Web Ontology Language Profiles* (the EL boundary) — <https://www.w3.org/TR/owl2-profiles/>
- ELK reasoner — <http://liveontologies.github.io/elk-reasoner/>; Baader, Brandt, Lutz,
  *Pushing the EL Envelope* (IJCAI 2005)
- ROBOT `reason` / `verify` — <http://robot.obolibrary.org/>
- Grüninger & Fox, *competency questions* (IJCAI-95); Bezerra et al., *Verifying DL Ontologies
  based on Competency Questions and Unit Testing* (CEUR Vol-1908)
- CONSTITUTION Principles **7** (verified by construction), **8** (reasoning-centric & FAIR),
  **9** (orthogonality / anti-overtyping), **10** (suppression, never erasure)

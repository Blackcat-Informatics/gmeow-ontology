# Engine cross-check: trusting pyoxigraph for speed (#242)

The test suite and the projection executor run SPARQL on **pyoxigraph** (Rust,
in-process) instead of rdflib's pure-Python engine, because it is dramatically
faster on the hot paths:

| operation                              | rdflib | pyoxigraph |
| -------------------------------------- | -----: | ---------: |
| merged-ontology load (per call)        |  ~72 ms (deep copy) | ~16 ms (fresh store) / ~0 ms (cached) |
| `schema-org` CONSTRUCT projection      | ~180 ms | ~11 ms |

(Measure locally with `uv run python scripts/bench_engines.py`.)

## The trust model

pyoxigraph is a **non-authoritative acceleration path**. The authoritative
full-ontology validation is unchanged:

- **Jena** emits the canonical RDF 1.2 lead artifact and runs OWL reasoning
  (ELK / HermiT via ROBOT). pyoxigraph does **not** reason.
- **`gmeow_shacl`** (Rust + oxigraph) is the canonical SHACL engine. pyoxigraph
  does **not** validate SHACL.

What licenses the suite to trust the fast engine is the **engine-equivalence
gate** (`gmeow crosscheck-queries`, `make crosscheck`, and the CI `ontology`
job): every committed query under `queries/` is executed on the same merged graph
under **both** rdflib and pyoxigraph, and the answers are compared **by value**.
If the two engines ever disagree, the gate fails. This extends the RDF 1.2
round-trip cross-check of #177 from the statement compiler to the whole query
surface.

### Value-based comparison

The engines canonicalize some literals differently while meaning the same thing;
the cross-check folds these so only *semantic* divergence fails the gate:

- numeric literals are compared as `Decimal` (`"645.0"^^xsd:decimal` ≡ `"645"`);
- a plain literal (no datatype) and an `xsd:string` literal are the same;
- decimal tokens inside string literals (e.g. a `STR(?decimal)`-built
  `POINT(-113.924350 …)` WKT string) are normalized, since `STR()` of a decimal
  keeps trailing zeros in rdflib but not in pyoxigraph.

A file holding several demonstration queries (not one executable query) is
**skipped** — and reported as skipped — because *both* engines reject it; a
one-sided rejection is a real divergence.

## Store loading caveat

Stores are seeded by loading the canonical Turtle sources **directly** into
pyoxigraph (`gmeow_tools.sparql._load_base`). An rdflib→N-Triples→pyoxigraph
hand-off silently breaks blank-node RDF collections (`owl:members` disjointness
lists), so it is avoided for the base graph; `store_from_graph` / `extra_triples`
use the N-Triples hand-off only for small, list-free ad-hoc additions.

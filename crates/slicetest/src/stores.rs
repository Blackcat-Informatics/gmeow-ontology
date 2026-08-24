// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The graphs competency questions run over.
//!
//! Competency questions are ontology-wide ("what kinds of agent does GMEOW
//! model?", "what contribution roles?"), so they run over the full merged
//! ontology — `ontology/gmeow.ttl` plus every `slices/*/*/module.ttl` (imports
//! excluded), the full merged-ontology source-set.
//!
//! ## Two lanes (the D+C design — see `docs/TESTING.md`)
//!
//! * [`merged_store`] — the **asserted** merged graph. The default lane. SPARQL
//!   property paths (`rdfs:subClassOf*`, `rdfs:subPropertyOf*`) supply transitive
//!   closure at query time, so the great majority of competency questions are
//!   answered correctly with no materialization, in a sub-second graph build.
//! * [`rdfs_closed_store`] — the merged graph closed under **RDFS** (rdfs2/3/5/
//!   7/9/11: domain/range typing, `rdf:type` propagation up the class hierarchy,
//!   and subclass/subproperty transitivity), computed via native `CONSTRUCT` rules
//!   iterated to fixpoint (each round's derived graph is unioned back into the base
//!   and re-frozen). A competency question opts into this with
//!   `gmeow:cqReasoning gmeow:reasoningRdfs` when its expected answer is entailed,
//!   not asserted (e.g. a type inferred from a property's domain).
//!
//! Why not full OWL 2 RL: the native RL chase (`gmeow_logic::reason::rl_closure`)
//! is ~4 minutes over the merged
//! ontology — unacceptable for a test lane. RDFS covers the entailments
//! competency questions actually need (subsumption + type inference) for a tiny
//! fraction of the cost, and reasoning is monotonic so the asserted default can
//! only ever under-answer (a loud test failure), never silently mislead.

use std::path::PathBuf;
use std::sync::Arc;

use gmeow_errors::{Diag, Result};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlResult};

use crate::error::{LogicReasoning, MergedGraph, RdfsClosure, UnexpectedResultForm};
use crate::native_query::{self, merge_preserving_blanks};
use crate::paths;

/// Build the **asserted** merged ontology dataset (no materialized entailment).
///
/// # Errors
///
/// Hard-fails if a source file fails to read or parse.
pub fn merged_store() -> Result<Arc<RdfDataset>> {
    let raw = native_query::dataset_from_files(&source_files()?).map_err(|e| {
        Diag::of_kind(MergedGraph {
            detail: format!("merging ontology sources: {e}"),
        })
    })?;
    // Competency questions are authored against the OWL/RDFS surface (they filter on
    // `owl:Class` / `owl:ObjectProperty` and walk `rdfs:subClassOf*`), so materialize
    // the complete `owl:`/`rdfs:` projection of the canonical `logic:` merged graph.
    // The projection is dual-write (the canonical `logic:` edges are kept), so a
    // question that queries either surface sees its answer. Doing it here also feeds
    // the projected `rdfs:subClassOf` / `rdfs:domain` edges to the RDFS-closure lane
    // ([`rdfs_closed_store`]), so rdfs2/3/9/11 fire on re-authored subsumption/typing.
    Ok(native_query::with_owl_rdfs_projection(&raw))
}

/// Build the merged ontology and return it closed under **RDFS**.
///
/// # Errors
///
/// Hard-fails if the merged dataset cannot be built or the closure fails to reach
/// a fixpoint within the safety bound.
pub fn rdfs_closed_store() -> Result<Arc<RdfDataset>> {
    rdfs_close(merged_store()?)
}

/// Build the native `logic:`-reasoned closure the `gmeow:reasoningLogic` lane queries.
///
/// The source graph is [`native_reasoning_source_files`]: the authored algebra-law example
/// files — each carries one of the four `math:` laws (associativity, the determinant
/// homomorphism, the E8 group action, and the homomorphic-encryption law) as a real
/// first-order `logic:Formula` AST plus the minimal pre-reified operation-table witness EDB
/// it fires over — plus the lang slice's `module.ttl`, which carries the GMN security-ring
/// lattice's `gmeow:ruleGmnRingWithinDerive` / `gmeow:ruleGmnRingCompartmentGap` (Horn
/// `logic:Rule`s, the `logic:ruleProjectIsAwareOf` idiom) and their EDB witness (the ring
/// individuals' authored `gmeow:gmnRingLevel` / `gmeow:gmnRingCompartment` coordinates).
/// That graph is compiled to a canonical [`gmeow_logic_compile::ir::LogicProgram`]
/// ([`parse_logic_dataset`](gmeow_logic_compile::frontend::parse_logic_dataset)) and
/// evaluated over the same graph as its EDB through
/// [`reason_program_closure_dataset`](gmeow_logic::reason::reason_program_closure_dataset);
/// the returned dataset is the full entailment closure (asserted + derived), so a competency
/// question sees the consequents the laws and rules DERIVE — the reified n-ary tuples and the
/// computed `gmeow:gmnRingWithin` edges included — not just the asserted data.
///
/// The lane is deliberately scoped to these law/rule-carrying sources rather than the whole
/// merged ontology: nothing else fires, and reasoning the entire ontology would balloon the
/// closure (a ~37k-quad DL closure of unrelated vocabulary) past the per-test time budget for
/// no added entailment. A future `reasoningLogic` question extends the source set in
/// [`native_reasoning_source_files`], exactly as a new module extends [`merged_store`]. This is
/// the ONE lane that pays the full native chase; the default (`reasoningNone`) and
/// `reasoningRdfs` lanes stay lightweight.
///
/// # Errors
///
/// Hard-fails if the source files cannot be built, if the program cannot be compiled
/// (a parse error, or any `Severity::Error` diagnostic — never papered over), or if
/// the native reasoner fails.
pub fn native_closed_store() -> Result<Arc<RdfDataset>> {
    let files = native_reasoning_source_files();
    let store = native_query::dataset_from_files(&files).map_err(|e| {
        Diag::of_kind(LogicReasoning {
            detail: format!("loading the native-reasoning source files: {e}"),
        })
    })?;
    // The program is extracted from the DEFAULT-graph source (parse_logic_dataset reads the
    // default graph), but the native reasoner's EDB is world-scoped: it only reasons over
    // NAMED-graph quads (WorldStore::worlds skips the default graph by design). Slice Turtle
    // is a single default graph, so re-scope every quad into the reasoner's default world
    // before reasoning; the closure projection maps that world back to the RDF default graph.
    let (program, diagnostics) =
        gmeow_logic_compile::frontend::parse_logic_dataset(store.as_ref(), None).map_err(|e| {
            Diag::of_kind(LogicReasoning {
                detail: format!(
                    "compiling the logic program from the native-reasoning sources: {e}"
                ),
            })
        })?;
    // The front-end is fail-soft (a malformed cell becomes a WARNING and is skipped), but a
    // Severity::Error diagnostic is a hard, non-recoverable fault — refuse to reason over a
    // program the compiler flagged as erroneous rather than silently under-reasoning.
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    if !errors.is_empty() {
        return Err(Diag::of_kind(LogicReasoning {
            detail: format!(
                "the native-reasoning sources did not compile to a clean logic program ({} error diagnostic(s)): {}",
                errors.len(),
                errors.join("; ")
            ),
        }));
    }
    let edb = world_scoped(&store)?;
    let closure = gmeow_logic::reason::reason_program_closure_dataset(&program, edb.as_ref())
        .map_err(|e| {
            Diag::of_kind(LogicReasoning {
                detail: format!("native logic reasoning over the native-reasoning sources: {e}"),
            })
        })?;
    // Reasoning runs over the CANONICAL `logic:` EDB (the compiler and chase read the
    // authored surface); the competency queries that consume the closure are authored
    // against the OWL/RDFS surface. Project the closure AFTER reasoning so the reasoner
    // is untouched and the query still sees the `owl:`/`rdfs:` view (dual-write keeps
    // the canonical edges).
    Ok(native_query::with_owl_rdfs_projection(&closure))
}

/// Re-scope every quad of `dataset` into the native reasoner's default world
/// ([`gmeow_logic::reason::rl::DEFAULT_WORLD`]) so it is visible to the world-scoped chase.
///
/// The reasoner reads its EDB from NAMED graphs only; slice Turtle is a single default
/// graph, so without this every fact would be invisible and the closure empty. The closure
/// projection maps this world back to the RDF default graph, so the round-trip is transparent
/// to a competency query.
fn world_scoped(dataset: &Arc<RdfDataset>) -> Result<Arc<RdfDataset>> {
    let world = RdfTerm::iri(gmeow_logic::reason::rl::DEFAULT_WORLD);
    let mut builder = RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        let scoped =
            RdfQuad::new(quad.subject, quad.predicate, quad.object).in_graph(world.clone());
        builder.push_owned_quad(&scoped);
    }
    builder.freeze().map_err(|e| {
        Diag::of_kind(LogicReasoning {
            detail: format!("re-scoping the merged graph into the reasoner world: {e}"),
        })
    })
}

/// The four authored algebra-law example files — the `math:` component of
/// [`native_reasoning_source_files`]. Each carries one of the four `math:` laws as a real
/// logic:Formula AST plus its pre-reified operation-table witness EDB.
///
/// `algebra-axioms.ttl` (associativity), `algebra-homomorphisms.ttl` (the determinant
/// homomorphism), `e8-symmetry.ttl` (the E8 group action), and `homomorphic-encryption.ttl`
/// (the homomorphic-encryption law).
fn algebra_law_files() -> Vec<PathBuf> {
    let examples = paths::slices_root()
        .join("grounding")
        .join("math")
        .join("examples");
    [
        "algebra-axioms.ttl",
        "algebra-homomorphisms.ttl",
        "e8-symmetry.ttl",
        "homomorphic-encryption.ttl",
    ]
    .iter()
    .map(|f| examples.join(f))
    .collect()
}

/// The `gmeow:reasoningLogic` source-set, extended (per the module-doc "a future
/// question extends the source set here" note) with the GMN security-ring lattice's
/// `logic:Rule` derivation: `gmeow:ruleGmnRingWithinDerive` /
/// `gmeow:ruleGmnRingCompartmentGap`, authored directly in the lang slice's module —
/// the same canonical-TBox location `logic:ruleProjectIsAwareOf` uses (logic/module.ttl)
/// — plus their EDB witness, the default-preset and NATO ring individuals' authored
/// `gmeow:gmnRingLevel` / `gmeow:gmnRingCompartment` coordinates. The whole lang module
/// is ~3k triples (small relative to the ~37k-quad full-ontology closure this lane
/// avoids), so it stays within the per-test time budget.
fn native_reasoning_source_files() -> Vec<PathBuf> {
    let mut files = algebra_law_files();
    files.push(
        paths::slices_root()
            .join("grounding")
            .join("lang")
            .join("module.ttl"),
    );
    // The chain/cochain square-zero laws (math:boundarySquareZeroLaw /
    // math:coboundarySquareZeroLaw) are canonical `logic:Formula`s in the math module,
    // referenced by `math:definingLaw`; their live-entailment witness EDB (the pre-authored
    // triangle diamond relations) lives in `examples/chain-complex.ttl`. Both are added so
    // the boundary/coboundary-square-zero competency questions fire the real law and read
    // back the derived math:cancellingPairRel / math:coCancellingPairRel — exactly as the
    // algebra laws fire from their example files.
    let math = paths::slices_root().join("grounding").join("math");
    files.push(math.join("module.ttl"));
    files.push(math.join("examples").join("chain-complex.ttl"));
    files
}

/// The reasoned source-set: `ontology/gmeow.ttl` followed by every slice module,
/// imports excluded — identical to `iter_source_files(include_imports=False)`.
fn source_files() -> Result<Vec<PathBuf>> {
    // The ontology root is REQUIRED — silently filtering it out would build a
    // partial merged graph against which competency questions could falsely pass.
    let root = paths::repo_root().join("ontology/gmeow.ttl");
    if !root.is_file() {
        return Err(Diag::of_kind(MergedGraph {
            detail: format!(
                "missing required ontology root {} — refusing to build a partial merged graph",
                root.display()
            ),
        }));
    }
    let mut files = vec![root];
    files.extend(module_files()?);
    Ok(files)
}

/// Every `slices/<group>/<name>/module.ttl`, in sorted (deterministic) order.
fn module_files() -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for group in sorted_subdirs(&paths::slices_root())? {
        for slice in sorted_subdirs(&group)? {
            let module = slice.join("module.ttl");
            if module.is_file() {
                out.push(module);
            }
        }
    }
    Ok(out)
}

fn sorted_subdirs(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    // Propagate per-entry read errors rather than `filter_map(Result::ok)`: an
    // unreadable entry must surface, not silently shrink the discovered slice set.
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| {
        Diag::of_kind(MergedGraph {
            detail: format!("read_dir {}: {e}", dir.display()),
        })
    })? {
        let entry = entry.map_err(|e| {
            Diag::of_kind(MergedGraph {
                detail: format!("read_dir entry under {}: {e}", dir.display()),
            })
        })?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

// ── RDFS closure ────────────────────────────────────────────────────────────────

/// A generous bound on closure rounds; RDFS converges in a handful (the depth of
/// the subclass/subproperty hierarchy). Hitting it signals a bug, not slow data.
const MAX_ROUNDS: usize = 64;

/// The RDFS entailment rules as SPARQL `CONSTRUCT` queries (the type/subsumption
/// subset that matters to competency questions). Iterated to fixpoint.
const RDFS_RULES: &[&str] = &[
    // rdfs9: type propagation up the class hierarchy.
    "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
     CONSTRUCT { ?s rdf:type ?d } WHERE { ?s rdf:type ?c . ?c rdfs:subClassOf ?d }",
    // rdfs11: subclass transitivity.
    "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
     CONSTRUCT { ?c rdfs:subClassOf ?e } WHERE { ?c rdfs:subClassOf ?d . ?d rdfs:subClassOf ?e }",
    // rdfs5: subproperty transitivity.
    "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
     CONSTRUCT { ?p rdfs:subPropertyOf ?r } WHERE { ?p rdfs:subPropertyOf ?q . ?q rdfs:subPropertyOf ?r }",
    // rdfs7: property propagation along subPropertyOf.
    "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
     CONSTRUCT { ?s ?q ?o } WHERE { ?s ?p ?o . ?p rdfs:subPropertyOf ?q }",
    // rdfs2: domain typing.
    "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
     CONSTRUCT { ?s rdf:type ?c } WHERE { ?p rdfs:domain ?c . ?s ?p ?o }",
    // rdfs3: range typing.
    "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
     CONSTRUCT { ?o rdf:type ?c } WHERE { ?p rdfs:range ?c . ?s ?p ?o }",
];

/// Close `dataset` under [`RDFS_RULES`] to fixpoint, returning the closed dataset.
///
/// Each round materializes every rule's CONSTRUCT over the current dataset and merges
/// the derived graphs back into it via [`merge_preserving_blanks`] — which, unlike
/// `RdfDataset::union`, does NOT re-scope blank nodes, so a re-derived quad over a
/// blank-bearing subject dedups against its prior copy rather than minting a fresh
/// blank every round (the property the count-stable fixpoint depends on). Fixpoint is
/// reached when a round adds no new quads.
fn rdfs_close(mut dataset: Arc<RdfDataset>) -> Result<Arc<RdfDataset>> {
    for _ in 0..MAX_ROUNDS {
        let before = dataset.quad_count();
        // Materialize every rule's CONSTRUCT graph, then merge them all (plus the
        // base) into one re-frozen dataset, preserving blank identity so identical
        // derived quads collapse and the count is stable at fixpoint.
        let mut round: Vec<Arc<RdfDataset>> = Vec::with_capacity(RDFS_RULES.len() + 1);
        round.push(Arc::clone(&dataset));
        for rule in RDFS_RULES {
            round.push(construct(&dataset, rule)?);
        }
        dataset = merge_preserving_blanks(&round);
        if dataset.quad_count() == before {
            return Ok(dataset);
        }
    }
    Err(Diag::of_kind(RdfsClosure {
        detail: format!("RDFS closure did not reach a fixpoint within {MAX_ROUNDS} rounds"),
    }))
}

/// Run a CONSTRUCT query and return its derived graph as a frozen dataset.
fn construct(dataset: &Arc<RdfDataset>, query: &str) -> Result<Arc<RdfDataset>> {
    match native_query::query(dataset, query).map_err(|e| {
        Diag::of_kind(RdfsClosure {
            detail: format!("RDFS rule error: {e}"),
        })
    })? {
        SparqlResult::Graph(graph) => Ok(graph),
        SparqlResult::Solutions { .. } | SparqlResult::Boolean(_) => {
            Err(Diag::of_kind(UnexpectedResultForm {
                detail: "RDFS rule must be a CONSTRUCT".to_owned(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_store_materializes() {
        let store = merged_store().expect("merged store must build");
        assert!(
            store.quad_count() > 1000,
            "merged ontology should have many triples"
        );
    }

    #[test]
    fn rdfs_close_infers_type_and_subclass() {
        // A synthetic graph exercising the entailments competency questions rely
        // on: a domain typing (rdfs2) and type propagation up a subclass chain
        // (rdfs9 + rdfs11) — neither present in the asserted data.
        let nt = concat!(
            "<https://example.org/hasPet> <http://www.w3.org/2000/01/rdf-schema#domain> <https://example.org/Owner> .\n",
            "<https://example.org/Owner> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Person> .\n",
            "<https://example.org/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Agent> .\n",
            "<https://example.org/ada> <https://example.org/hasPet> <https://example.org/cat> .\n",
        );
        let base = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("synthetic NT must load");
        let closed = rdfs_close(base).expect("rdfs closure must converge");

        // rdfs2: ada hasPet => ada a Owner. rdfs9/11: => Person, => Agent.
        for cls in ["Owner", "Person", "Agent"] {
            assert!(
                ask_type(
                    &closed,
                    "https://example.org/ada",
                    &format!("https://example.org/{cls}")
                ),
                "expected ex:ada to be inferred a ex:{cls}"
            );
        }
    }

    fn ask_type(store: &Arc<RdfDataset>, s: &str, c: &str) -> bool {
        let q = format!("ASK {{ <{s}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{c}> }}");
        matches!(
            native_query::query(store, &q).expect("ask"),
            SparqlResult::Boolean(true)
        )
    }
}

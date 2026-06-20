// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The graphs competency questions run over.
//!
//! Competency questions are ontology-wide ("what kinds of agent does GMEOW
//! model?", "what contribution roles?"), so they run over the full merged
//! ontology — `ontology/gmeow.ttl` plus every `slices/*/*/module.ttl` (imports
//! excluded), the same source-set `tests/test_competency.py` merges.
//!
//! ## Two lanes (the D+C design — see `docs/TESTING.md`)
//!
//! * [`merged_store`] — the **asserted** merged graph. The default lane. SPARQL
//!   property paths (`rdfs:subClassOf*`, `rdfs:subPropertyOf*`) supply transitive
//!   closure at query time, so the great majority of competency questions are
//!   answered correctly with no materialization, in a sub-second graph build.
//! * [`rdfs_closed_store`] — the merged graph closed under **RDFS** (rdfs2/3/5/
//!   7/9/11: domain/range typing, `rdf:type` propagation up the class hierarchy,
//!   and subclass/subproperty transitivity), computed natively in oxigraph via
//!   `CONSTRUCT` rules iterated to fixpoint. A competency question opts into this
//!   with `gmeow:cqReasoning gmeow:reasoningRdfs` when its expected answer is
//!   entailed, not asserted (e.g. a type inferred from a property's domain).
//!
//! Why not full OWL 2 RL: the native RL chase (`gmeow_logic::reason::rl_closure`,
//! the same one `tests/test_competency.py` pays) is ~4 minutes over the merged
//! ontology — unacceptable for a test lane. RDFS covers the entailments
//! competency questions actually need (subsumption + type inference) for a tiny
//! fraction of the cost, and reasoning is monotonic so the asserted default can
//! only ever under-answer (a loud test failure), never silently mislead.

use std::collections::HashSet;
use std::path::PathBuf;

use oxigraph::model::{GraphName, Quad, Triple};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use gmeow_validate::store::build_store;

use crate::paths;

/// Build the **asserted** merged ontology store (no materialized entailment).
///
/// # Errors
///
/// Returns `Err(String)` if a source file fails to read or parse.
pub fn merged_store() -> Result<Store, String> {
    build_store(&source_files()?).map_err(|e| format!("merging ontology sources: {e}"))
}

/// Build the merged ontology and close it under **RDFS** in place.
///
/// # Errors
///
/// Returns `Err(String)` if the merged store cannot be built or the closure
/// fails to reach a fixpoint within the safety bound.
pub fn rdfs_closed_store() -> Result<Store, String> {
    let store = merged_store()?;
    rdfs_close(&store)?;
    Ok(store)
}

/// The reasoned source-set: `ontology/gmeow.ttl` followed by every slice module,
/// imports excluded — identical to `iter_source_files(include_imports=False)`.
fn source_files() -> Result<Vec<PathBuf>, String> {
    // The ontology root is REQUIRED — silently filtering it out would build a
    // partial merged graph against which competency questions could falsely pass.
    let root = paths::repo_root().join("ontology/gmeow.ttl");
    if !root.is_file() {
        return Err(format!(
            "missing required ontology root {} — refusing to build a partial merged graph",
            root.display()
        ));
    }
    let mut files = vec![root];
    files.extend(module_files()?);
    Ok(files)
}

/// Every `slices/<group>/<name>/module.ttl`, in sorted (deterministic) order.
fn module_files() -> Result<Vec<PathBuf>, String> {
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

fn sorted_subdirs(dir: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    // Propagate per-entry read errors rather than `filter_map(Result::ok)`: an
    // unreadable entry must surface, not silently shrink the discovered slice set.
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("read_dir entry under {}: {e}", dir.display()))?;
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

/// Close `store` under [`RDFS_RULES`] to fixpoint (in place).
///
/// Each round materializes every rule's CONSTRUCT and inserts the results;
/// fixpoint is reached when a round adds no new triples (the store size is
/// unchanged — oxigraph inserts are idempotent on duplicate quads).
fn rdfs_close(store: &Store) -> Result<(), String> {
    let store_len = |s: &Store| s.len().map_err(|e| format!("store len: {e}"));
    for _ in 0..MAX_ROUNDS {
        let before = store_len(store)?;
        // Dedupe inferred triples across all rules in the round before inserting;
        // oxigraph inserts are idempotent, but a HashSet avoids redundant inserts.
        let mut derived: HashSet<Triple> = HashSet::new();
        for rule in RDFS_RULES {
            derived.extend(construct(store, rule)?);
        }
        for triple in derived {
            let quad = Quad::new(
                triple.subject,
                triple.predicate,
                triple.object,
                GraphName::DefaultGraph,
            );
            store
                .insert(&quad)
                .map_err(|e| format!("RDFS closure insert failed: {e}"))?;
        }
        if store_len(store)? == before {
            return Ok(());
        }
    }
    Err(format!(
        "RDFS closure did not reach a fixpoint within {MAX_ROUNDS} rounds"
    ))
}

/// Run a CONSTRUCT query and collect its triples.
fn construct(store: &Store, query: &str) -> Result<Vec<Triple>, String> {
    let results = SparqlEvaluator::new()
        .parse_query(query)
        .map_err(|e| format!("RDFS rule parse error: {e}"))?
        .on_store(store)
        .execute()
        .map_err(|e| format!("RDFS rule evaluation error: {e}"))?;
    match results {
        QueryResults::Graph(triples) => triples
            .map(|t| t.map_err(|e| format!("RDFS rule triple error: {e}")))
            .collect(),
        QueryResults::Solutions(_) | QueryResults::Boolean(_) => {
            Err("RDFS rule must be a CONSTRUCT".to_owned())
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
            store.len().expect("len") > 1000,
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
        let store = gmeow_validate::store::build_store_from_nt(nt).expect("synthetic NT must load");
        rdfs_close(&store).expect("rdfs closure must converge");

        // rdfs2: ada hasPet => ada a Owner. rdfs9/11: => Person, => Agent.
        for cls in ["Owner", "Person", "Agent"] {
            assert!(
                ask_type(
                    &store,
                    "https://example.org/ada",
                    &format!("https://example.org/{cls}")
                ),
                "expected ex:ada to be inferred a ex:{cls}"
            );
        }
    }

    fn ask_type(store: &Store, s: &str, c: &str) -> bool {
        let q = format!("ASK {{ <{s}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{c}> }}");
        matches!(
            SparqlEvaluator::new()
                .parse_query(&q)
                .unwrap()
                .on_store(store)
                .execute()
                .unwrap(),
            QueryResults::Boolean(true)
        )
    }
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dependency-free named-graph boundary of the object-level reasoning EDB.
//!
//! The pipeline assembles these named worlds plus the authored default world; bundle
//! readers and lower-layer coherence tests consume the same set. Keeping the identifiers in
//! the reasoning crate prevents the producer and its gate-teeth proof from silently drifting.

/// The RDF 1.2 statement layer admitted to object-level reasoning.
pub const GRAPH_STATEMENTS: &str = "https://blackcatinformatics.ca/gmeow/graph/statements";
/// The vendored import closure admitted to object-level reasoning.
pub const GRAPH_IMPORTS: &str = "https://blackcatinformatics.ca/gmeow/graph/imports";
/// The canonical compiled `logic:` program admitted to object-level reasoning.
pub const GRAPH_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/graph/logic";
/// The compiled relational-core lowering admitted to object-level reasoning.
pub const GRAPH_RELATIONAL_CORE: &str =
    "https://blackcatinformatics.ca/gmeow/graph/relational-core";

/// Demonstrator world witnessing the **jointly-acyclic** chase-termination class in the
/// shipped bundle (its per-world certificate is `chase.certificate.jointly-acyclic`).
pub const GRAPH_DEMO_JOINTLY_ACYCLIC: &str =
    "https://blackcatinformatics.ca/gmeow/graph/demo/jointly-acyclic";
/// Demonstrator world witnessing the **super-weakly-acyclic** chase-termination class.
pub const GRAPH_DEMO_SUPER_WEAKLY_ACYCLIC: &str =
    "https://blackcatinformatics.ca/gmeow/graph/demo/super-weakly-acyclic";
/// Demonstrator world witnessing the self-hosted **model-summarizing-acyclic** class.
pub const GRAPH_DEMO_MODEL_SUMMARIZING: &str =
    "https://blackcatinformatics.ca/gmeow/graph/demo/model-summarizing-acyclic";

/// Every named graph admitted to the object-level reasoning EDB. The default graph is also
/// admitted, but has no IRI and therefore is not represented in this list.
pub const OBJECT_LEVEL_NAMED_GRAPHS: [&str; 7] = [
    GRAPH_STATEMENTS,
    GRAPH_IMPORTS,
    GRAPH_LOGIC,
    GRAPH_RELATIONAL_CORE,
    GRAPH_DEMO_JOINTLY_ACYCLIC,
    GRAPH_DEMO_SUPER_WEAKLY_ACYCLIC,
    GRAPH_DEMO_MODEL_SUMMARIZING,
];

/// Whether a named graph belongs to the object-level reasoning EDB.
pub fn is_object_level_named_graph(iri: &str) -> bool {
    OBJECT_LEVEL_NAMED_GRAPHS.contains(&iri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_is_unique_and_excludes_meta_graphs() {
        let unique = OBJECT_LEVEL_NAMED_GRAPHS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), OBJECT_LEVEL_NAMED_GRAPHS.len());
        assert!(!is_object_level_named_graph(
            "https://blackcatinformatics.ca/gmeow/graph/correspondence"
        ));
        assert!(!is_object_level_named_graph(
            "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws"
        ));
        // The grounding seam registry asserts governance/policy data (which
        // cross-grounding reference channels are sanctioned), not object-level
        // axioms — excluded exactly like the correspondence-laws graph.
        assert!(!is_object_level_named_graph(
            "https://blackcatinformatics.ca/gmeow/graph/grounding-seams"
        ));
    }
}

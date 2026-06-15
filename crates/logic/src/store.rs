// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! World-indexed named-graph store.
//!
//! World-indexed semantics only: no dataset-union queries are provided.
//! Each world is isolated in its own named graph. The `WorldStore` wraps an
//! in-memory [`oxigraph::store::Store`] and routes every insert and query
//! through the named-graph IRI that identifies the world.

use oxigraph::model::{GraphName, GraphNameRef, NamedNode, NamedOrBlankNodeRef, Quad};
use oxigraph::store::Store;

/// A world-indexed RDF store.
///
/// Each world is an oxigraph named graph identified by an IRI string. Only
/// world-indexed (named-graph–scoped) operations are exposed; no cross-graph
/// union queries exist by design. This is the core isolation guarantee: a
/// triple inserted into world A is never visible through a query on world B.
pub struct WorldStore {
    inner: Store,
}

impl WorldStore {
    /// Create a new, empty in-memory `WorldStore`.
    pub fn new() -> Self {
        Self {
            inner: Store::new().expect("in-memory oxigraph Store::new() is infallible"),
        }
    }

    /// Insert the triple `(s, p, o)` — all IRI strings — into the named graph
    /// whose IRI is `world`.
    ///
    /// # Panics
    ///
    /// Panics if any of the IRI strings is not a valid IRI.
    pub fn insert_quad(&self, world: &str, s: &str, p: &str, o: &str) {
        let subject = NamedNode::new(s).unwrap_or_else(|e| panic!("invalid subject IRI {s:?}: {e}"));
        let predicate =
            NamedNode::new(p).unwrap_or_else(|e| panic!("invalid predicate IRI {p:?}: {e}"));
        let object = NamedNode::new(o).unwrap_or_else(|e| panic!("invalid object IRI {o:?}: {e}"));
        let graph = NamedNode::new(world)
            .unwrap_or_else(|e| panic!("invalid world IRI {world:?}: {e}"));
        let quad = Quad::new(subject, predicate, object, graph);
        self.inner
            .insert(&quad)
            .expect("in-memory store insert is infallible");
    }

    /// Return all quads in the named graph `world`, in unspecified order.
    ///
    /// Returns `Vec<[String; 4]>` where each element is
    /// `[subject_iri, predicate_iri, object_iri, world_iri]`.
    /// Only the quads stored under that exact named graph are returned;
    /// no cross-world union is performed.
    pub fn quads_in_world(&self, world: &str) -> Vec<[String; 4]> {
        let graph_node = match NamedNode::new(world) {
            Ok(n) => n,
            Err(_) => return vec![],
        };
        let graph_ref: GraphNameRef<'_> = GraphNameRef::NamedNode(graph_node.as_ref());
        self.inner
            .quads_for_pattern(
                None::<NamedOrBlankNodeRef<'_>>,
                None,
                None,
                Some(graph_ref),
            )
            .filter_map(|r| {
                r.ok().map(|q| {
                    [
                        q.subject.to_string(),
                        q.predicate.to_string(),
                        q.object.to_string(),
                        match &q.graph_name {
                            GraphName::NamedNode(n) => n.as_str().to_owned(),
                            GraphName::BlankNode(b) => b.as_str().to_owned(),
                            GraphName::DefaultGraph => String::new(),
                        },
                    ]
                })
            })
            .collect()
    }

    /// Return the distinct world IRIs (named graph IRIs) present in the store.
    pub fn worlds(&self) -> Vec<String> {
        self.inner
            .named_graphs()
            .filter_map(|r| {
                r.ok().and_then(|g| match g {
                    oxigraph::model::NamedOrBlankNode::NamedNode(n) => {
                        Some(n.as_str().to_owned())
                    }
                    oxigraph::model::NamedOrBlankNode::BlankNode(_) => None,
                })
            })
            .collect()
    }
}

impl Default for WorldStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD_A: &str = "http://world/A";
    const WORLD_B: &str = "http://world/B";

    const S_A: &str = "http://example.org/s/a";
    const P_A: &str = "http://example.org/p/a";
    const O_A: &str = "http://example.org/o/a";

    const S_B: &str = "http://example.org/s/b";
    const P_B: &str = "http://example.org/p/b";
    const O_B: &str = "http://example.org/o/b";

    fn populated_store() -> WorldStore {
        let store = WorldStore::new();
        store.insert_quad(WORLD_A, S_A, P_A, O_A);
        store.insert_quad(WORLD_B, S_B, P_B, O_B);
        store
    }

    #[test]
    fn world_a_contains_its_own_quad() {
        let store = populated_store();
        let quads = store.quads_in_world(WORLD_A);
        assert_eq!(quads.len(), 1, "world A should have exactly 1 quad");
        let q = &quads[0];
        assert!(
            q[0].contains("s/a"),
            "subject should be A's subject, got {q:?}"
        );
    }

    #[test]
    fn world_b_contains_its_own_quad() {
        let store = populated_store();
        let quads = store.quads_in_world(WORLD_B);
        assert_eq!(quads.len(), 1, "world B should have exactly 1 quad");
        let q = &quads[0];
        assert!(
            q[0].contains("s/b"),
            "subject should be B's subject, got {q:?}"
        );
    }

    #[test]
    fn no_cross_world_leakage_a_to_b() {
        let store = populated_store();
        let a_quads = store.quads_in_world(WORLD_A);
        // none of world A's quads should appear in world B
        for q in &a_quads {
            assert!(
                !q[0].contains("s/b"),
                "world A contains B's subject — cross-world leak: {q:?}"
            );
        }
        // world B should not see A's triple
        let b_quads = store.quads_in_world(WORLD_B);
        for q in &b_quads {
            assert!(
                !q[0].contains("s/a"),
                "world B contains A's subject — cross-world leak: {q:?}"
            );
        }
    }

    #[test]
    fn worlds_lists_both_world_iris() {
        let store = populated_store();
        let mut worlds = store.worlds();
        worlds.sort();
        assert_eq!(worlds, vec![WORLD_A, WORLD_B]);
    }

    #[test]
    fn empty_store_has_no_worlds() {
        let store = WorldStore::new();
        assert!(store.worlds().is_empty());
    }

    #[test]
    fn quads_in_nonexistent_world_returns_empty() {
        let store = populated_store();
        let quads = store.quads_in_world("http://world/doesNotExist");
        assert!(quads.is_empty());
    }

    #[test]
    fn quad_world_column_matches_world_iri() {
        let store = populated_store();
        for q in store.quads_in_world(WORLD_A) {
            assert_eq!(q[3], WORLD_A, "fourth column must be the world IRI");
        }
        for q in store.quads_in_world(WORLD_B) {
            assert_eq!(q[3], WORLD_B, "fourth column must be the world IRI");
        }
    }
}

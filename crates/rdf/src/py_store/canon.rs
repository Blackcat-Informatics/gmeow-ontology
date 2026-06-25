// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF canonicalization for the `gmeow_rdf` Python extension: the
//! `CanonicalizationAlgorithm` pyclass and the `canonicalize_quads` wrapper.
//!
//! All canonicalization now runs the **native full W3C RDFC-1.0** engine
//! (`gmeow_rdf_core::ir::canon`, via [`crate::canon`]); `oxrdf`'s
//! `Dataset::canonicalize` is gone (#910 / EPIC #906 oxigraph eviction). The
//! `CanonicalizationAlgorithm` pyclass is retained for Python API compatibility, but
//! both variants now resolve to the one native canonicalizer (greenfield: there is a
//! single canonicalization algorithm).

use oxigraph::model::Quad;
use pyo3::prelude::*;

/// The graph canonicalization algorithms. Mirrors the oxigraph Python
/// `CanonicalizationAlgorithm` so the Python surface is unchanged.
#[pyclass(name = "CanonicalizationAlgorithm", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum PyCanonicalizationAlgorithm {
    /// The standard RDF Canonicalization 1.0 algorithm (SHA-256).
    RDFC_1_0,
    /// Retained for API compatibility; now an alias of the native RDFC-1.0 engine
    /// (the former oxrdf "unstable" fast path no longer exists).
    UNSTABLE,
}

/// Canonicalize a quad set's blank-node labels under native RDFC-1.0, returning the
/// canonicalized quads sorted by their N-Quads string. The `algorithm` selector is
/// retained for API compatibility; both variants map to the one native engine.
pub fn canonicalize_quads(quads: Vec<Quad>, _algorithm: PyCanonicalizationAlgorithm) -> Vec<Quad> {
    crate::canon::canonicalize_quads(quads)
        .expect("native RDFC-1.0 canonicalization of valid oxigraph quads")
}

#[cfg(test)]
mod tests {
    use oxigraph::io::RdfFormat;

    use super::*;
    use crate::py_store::io::parse_quads;

    #[test]
    fn canonicalize_quads_is_deterministic_rdfc10() {
        // Two isomorphic graphs with different blank-node labels must canonicalize
        // to byte-identical quad strings under RDFC-1.0.
        let g1 = "_:a <https://example.org/p> _:b .\n_:b <https://example.org/q> _:a .";
        let g2 = "_:x <https://example.org/p> _:y .\n_:y <https://example.org/q> _:x .";
        let c1 = canonicalize_quads(
            parse_quads(g1.as_bytes(), RdfFormat::NTriples).unwrap(),
            PyCanonicalizationAlgorithm::RDFC_1_0,
        );
        let c2 = canonicalize_quads(
            parse_quads(g2.as_bytes(), RdfFormat::NTriples).unwrap(),
            PyCanonicalizationAlgorithm::RDFC_1_0,
        );
        let s1: Vec<String> = c1.iter().map(Quad::to_string).collect();
        let s2: Vec<String> = c2.iter().map(Quad::to_string).collect();
        assert_eq!(s1, s2, "isomorphic graphs must canonicalize identically");
    }

    #[test]
    fn canonicalize_quads_unstable_is_self_consistent() {
        let g = "_:a <https://example.org/p> _:b .";
        let c1 = canonicalize_quads(
            parse_quads(g.as_bytes(), RdfFormat::NTriples).unwrap(),
            PyCanonicalizationAlgorithm::UNSTABLE,
        );
        let c2 = canonicalize_quads(
            parse_quads(g.as_bytes(), RdfFormat::NTriples).unwrap(),
            PyCanonicalizationAlgorithm::UNSTABLE,
        );
        let s1: Vec<String> = c1.iter().map(Quad::to_string).collect();
        let s2: Vec<String> = c2.iter().map(Quad::to_string).collect();
        assert_eq!(s1, s2);
    }
}

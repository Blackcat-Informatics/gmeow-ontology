// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF canonicalization for the `gmeow_rdf` Python extension: the
//! `CanonicalizationAlgorithm` pyclass and the pure-Rust `canonicalize_quads`
//! core (RDFC-1.0 / oxigraph's unstable algorithm).

use oxigraph::model::dataset::{CanonicalizationAlgorithm, CanonicalizationHashAlgorithm};
use oxigraph::model::{Dataset, Quad};
use pyo3::prelude::*;

/// The graph canonicalization algorithms. Mirrors
/// the oxigraph Python `CanonicalizationAlgorithm`.
#[pyclass(name = "CanonicalizationAlgorithm", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum PyCanonicalizationAlgorithm {
    /// The standard RDF Canonicalization 1.0 algorithm (SHA-256).
    RDFC_1_0,
    /// OxRDF's faster non-stable algorithm (canonical *within* a build/version).
    UNSTABLE,
}

impl PyCanonicalizationAlgorithm {
    pub(crate) fn to_ox(self) -> CanonicalizationAlgorithm {
        match self {
            PyCanonicalizationAlgorithm::RDFC_1_0 => CanonicalizationAlgorithm::Rdfc10 {
                hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
            },
            PyCanonicalizationAlgorithm::UNSTABLE => CanonicalizationAlgorithm::Unstable,
        }
    }
}

/// Canonicalize a quad set's blank-node labels under `algorithm`, returning the
/// canonicalized quads (sorted by their N-Quads string for a stable order).
pub fn canonicalize_quads(quads: Vec<Quad>, algorithm: CanonicalizationAlgorithm) -> Vec<Quad> {
    let mut dataset: Dataset = quads.into_iter().collect();
    dataset.canonicalize(algorithm);
    let mut out: Vec<Quad> = dataset.iter().map(|q| q.into_owned()).collect();
    out.sort_by_key(Quad::to_string);
    out
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
        let alg = CanonicalizationAlgorithm::Rdfc10 {
            hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
        };
        let c1 = canonicalize_quads(
            parse_quads(g1.as_bytes(), RdfFormat::NTriples).unwrap(),
            alg,
        );
        let c2 = canonicalize_quads(
            parse_quads(g2.as_bytes(), RdfFormat::NTriples).unwrap(),
            alg,
        );
        let s1: Vec<String> = c1.iter().map(Quad::to_string).collect();
        let s2: Vec<String> = c2.iter().map(Quad::to_string).collect();
        assert_eq!(s1, s2, "isomorphic graphs must canonicalize identically");
    }

    #[test]
    fn canonicalize_quads_unstable_is_self_consistent() {
        let g = "_:a <https://example.org/p> _:b .";
        let alg = CanonicalizationAlgorithm::Unstable;
        let c1 = canonicalize_quads(parse_quads(g.as_bytes(), RdfFormat::NTriples).unwrap(), alg);
        let c2 = canonicalize_quads(parse_quads(g.as_bytes(), RdfFormat::NTriples).unwrap(), alg);
        let s1: Vec<String> = c1.iter().map(Quad::to_string).collect();
        let s2: Vec<String> = c2.iter().map(Quad::to_string).collect();
        assert_eq!(s1, s2);
    }
}

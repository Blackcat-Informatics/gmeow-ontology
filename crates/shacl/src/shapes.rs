// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL shapes graph parser.
//!
//! Real parser arrives in Task 2.

use oxigraph::model::Term;
use oxigraph::store::Store;

/// A minimal SHACL shape stub — expended in Task 2.
#[derive(Debug, Clone)]
pub struct Shape {
    /// The shape's identity term (IRI or blank node).
    pub id: Term,
}

/// The parsed shapes graph — a collection of [`Shape`]s.
#[derive(Debug, Default, Clone)]
pub struct Shapes {
    /// Node shapes extracted from the shapes graph.
    pub node_shapes: Vec<Shape>,
}

/// Parse shapes from an oxigraph store.
///
/// Task 1 stub: always returns an empty [`Shapes`] — the real parser
/// arrives in Task 2.
pub fn from_store(_store: &Store) -> Result<Shapes, String> {
    Ok(Shapes::default())
}

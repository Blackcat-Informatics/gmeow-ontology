// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The second live consumer of the reusable exact-rational geometry layer.
//!
//! `gmeow-validate` computes a `math:Norm` and a `math:Distance` THROUGH
//! `gmeow_math::InnerProductSpace` over the committed pure-`math:` linear-algebra
//! example, and cross-checks the results against their exact hand-computed values.
//!
//! This proves the `gmeow-math` engine is genuinely reusable across crates: the
//! affect-intensity CLI is consumer #1, and this native conformance gate is
//! consumer #2, both loading exact rational Gram-matrix / coordinate-vector cells
//! out of a graph and computing THROUGH the one shared inner-product space.

use std::path::{Path, PathBuf};

use gmeow_math::{InnerProductSpace, Rational, index_turtle, load_gram, load_vector};

/// The committed pure-`math:` worked example, resolved by walking up from this
/// crate to the repo root.
fn linear_algebra_example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
        .join("slices/grounding/math/examples/linear-algebra-and-learning.ttl")
}

const INTENSITY_GRAM: &str = "http://example.org/math/intensityGram";
const AFFECT_VECTOR: &str = "http://example.org/math/affectVector";

fn rat(num: i128, den: i128) -> Rational {
    Rational::new(num, den).expect("rational")
}

/// Build the dense inner-product space from the loaded `(row, col, value)` Gram
/// cells (the example authors both off-diagonal cells explicitly, so this is a
/// direct fill; the space's `new` still hard-fails a non-square matrix).
fn space_from_cells(cells: &[(usize, usize, Rational)]) -> InnerProductSpace {
    let dim = cells
        .iter()
        .flat_map(|(r, c, _)| [*r, *c])
        .max()
        .map(|m| m + 1)
        .expect("Gram matrix has entries");
    let mut gram = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in cells {
        gram[*row][*col] = *value;
    }
    InnerProductSpace::new(gram).expect("square Gram matrix")
}

// Consumer #2 computes a norm and a distance THROUGH gmeow_math::InnerProductSpace
// over the shipped pure-math: example, and the values match the exact hand
// computation over G = [[1, 1/4], [1/4, 1]] and x = (1, 1/4).
#[gmeow_test_batch_macros::batch_test]
fn shipped_math_norm_and_distance_computed_through_shared_layer() {
    let turtle = std::fs::read(linear_algebra_example()).expect("read linear-algebra example");
    let index = index_turtle(&turtle).expect("index the math: example graph");

    // Load the Gram matrix and the coordinate vector out of the graph — the same
    // pure-math: loaders consumer #1 (affect) uses.
    let cells = load_gram(&index, INTENSITY_GRAM).expect("load ex:intensityGram");
    let x = load_vector(&index, AFFECT_VECTOR).expect("load ex:affectVector");
    assert_eq!(x, vec![rat(1, 1), rat(1, 4)], "x = (1, 1/4) from the graph");

    let space = space_from_cells(&cells);
    assert_eq!(space.dim(), 2);

    // The authored form is positive-definite: the LDLᵀ witness must certify it with
    // pivots [1, 15/16] (Sylvester's criterion — every pivot strictly positive).
    let pivots = space.ldlt_pivots().expect("authored PD certified by LDLᵀ");
    assert_eq!(pivots, vec![rat(1, 1), rat(15, 16)]);

    // The quadratic form is exact: xᵀGx = 1·1 + 2·(1/4)·(1·1/4) + (1/4)² = 19/16.
    let q = space.quadratic_form(&x).expect("quadratic form");
    assert_eq!(q, rat(19, 16));
    assert_eq!(q.ratio_string(), "19/16");

    // ‖x‖_G = √(19/16). Pinned to the engine's fixed-precision decimal output.
    let norm = space.norm(&x).expect("norm through the shared layer");
    assert_eq!(norm, "1.089725");

    // A metric distance through the same space: d(x, y) with y = (1/4, 1).
    // x − y = (3/4, −3/4); (x−y)ᵀG(x−y) = 27/32; d = √(27/32).
    let y = [rat(1, 4), rat(1, 1)];
    let diff_form = space
        .quadratic_form(&[rat(3, 4), rat(-3, 4)])
        .expect("difference quadratic form");
    assert_eq!(diff_form, rat(27, 32));
    let distance = space
        .distance(&x, &y)
        .expect("distance through the shared layer");
    assert_eq!(distance, "0.918559");

    // Determinism: the same inputs yield byte-identical strings on re-run.
    assert_eq!(space.norm(&x).unwrap(), norm);
    assert_eq!(space.distance(&x, &y).unwrap(), distance);
}

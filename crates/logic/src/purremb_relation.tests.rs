// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::annotation::TupleAnnotationAlgebra;

#[test]
fn purremb_cosine_zero_vector_is_rejected() {
    let zero = [0.0_f32, 0.0, 0.0];
    let unit = [1.0_f32, 0.0, 0.0];
    assert_eq!(
        score(&zero, &unit, &DistanceMetric::Cosine),
        Err(ScoreError::ZeroMagnitude)
    );
}

#[test]
fn purremb_cosine_orthogonal_is_unit_distance() {
    let left = [1.0_f32, 0.0];
    let right = [0.0_f32, 1.0];
    let distance = score(&left, &right, &DistanceMetric::Cosine).expect("finite");
    assert!((distance - 1.0).abs() < 1e-9);
}

#[test]
fn purremb_cosine_identical_is_zero_distance() {
    let vector = [0.3_f32, -0.7, 0.1];
    let distance = score(&vector, &vector, &DistanceMetric::Cosine).expect("finite");
    assert!(distance.abs() < 1e-9);
}

#[test]
fn purremb_squared_euclidean_matches_manual() {
    let left = [1.0_f32, 2.0, 3.0];
    let right = [0.0_f32, 0.0, 0.0];
    let distance = score(&left, &right, &DistanceMetric::SquaredEuclidean).expect("finite");
    assert!((distance - 14.0).abs() < 1e-9);
}

#[test]
fn purremb_dimension_mismatch_is_rejected() {
    assert_eq!(
        score(&[1.0_f32], &[1.0_f32, 2.0], &DistanceMetric::NegativeDot),
        Err(ScoreError::DimensionMismatch)
    );
}

#[test]
fn purremb_extension_metric_is_rejected() {
    let metric = DistanceMetric::Extension {
        identifier: "example".to_owned(),
        parameter_encoding: "raw".to_owned(),
        parameters: vec![],
    };
    assert_eq!(
        score(&[1.0_f32], &[1.0_f32], &metric),
        Err(ScoreError::UnsupportedMetric)
    );
}

#[test]
fn purremb_order_key_is_monotonic_including_negatives() {
    let ordered = [
        f64::NEG_INFINITY,
        -1000.0,
        -1.5,
        -0.0,
        0.0,
        0.25,
        1.5,
        1000.0,
        f64::INFINITY,
    ];
    for window in ordered.windows(2) {
        let left = total_order_bits(window[0]);
        let right = total_order_bits(window[1]);
        assert!(
            left <= right,
            "order-key monotonicity broke at {:?} -> {:?}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn purremb_order_key_round_trips() {
    for value in [-1234.5_f64, -1.0, -0.0, 0.0, 2.5, 1e12] {
        let restored = from_total_order_bits(total_order_bits(value));
        assert_eq!(restored.to_bits(), value.to_bits());
    }
}

#[test]
fn purremb_bounded_topk_keeps_the_best_prefix() {
    // Ascending: smaller distance is better. Insert out of order; expect the three
    // smallest, best-first.
    let mut heap = BoundedTopK::new(3, RelationOrderDirection::Ascending);
    for (row, distance) in [
        (0, 5.0_f64),
        (1, 1.0),
        (2, 9.0),
        (3, 2.0),
        (4, 0.5),
        (5, 7.0),
    ] {
        heap.offer(ScannedCandidate {
            row,
            distance,
            bits: total_order_bits(distance),
            target: [row as u8; 32],
        });
    }
    let ranked = heap.into_ranked();
    let rows: Vec<usize> = ranked.iter().map(|candidate| candidate.row).collect();
    assert_eq!(rows, vec![4, 1, 3]);
}

#[test]
fn purremb_bounded_topk_descending_keeps_the_largest() {
    let mut heap = BoundedTopK::new(2, RelationOrderDirection::Descending);
    for (row, distance) in [(0, 1.0_f64), (1, 8.0), (2, 3.0), (3, 9.0)] {
        heap.offer(ScannedCandidate {
            row,
            distance,
            bits: total_order_bits(distance),
            target: [row as u8; 32],
        });
    }
    let rows: Vec<usize> = heap
        .into_ranked()
        .iter()
        .map(|candidate| candidate.row)
        .collect();
    assert_eq!(rows, vec![3, 1]);
}

#[test]
fn purremb_bounded_topk_breaks_ties_by_target() {
    // Two rows tie on distance; the smaller target sorts first (ascending tie-break).
    let mut heap = BoundedTopK::new(2, RelationOrderDirection::Ascending);
    heap.offer(ScannedCandidate {
        row: 0,
        distance: 1.0,
        bits: total_order_bits(1.0),
        target: [9; 32],
    });
    heap.offer(ScannedCandidate {
        row: 1,
        distance: 1.0,
        bits: total_order_bits(1.0),
        target: [2; 32],
    });
    let rows: Vec<usize> = heap
        .into_ranked()
        .iter()
        .map(|candidate| candidate.row)
        .collect();
    assert_eq!(rows, vec![1, 0]);
}

fn space_id(byte: u8) -> VectorSpaceId {
    VectorSpaceId::from_raw([byte; 32])
}

#[test]
fn purremb_multiply_same_space_combines() {
    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let left = SpaceTaggedScore::single(1.0, space_id(1), 1);
    let right = SpaceTaggedScore::single(2.0, space_id(1), 1);
    let product = algebra
        .multiply(&left, &right)
        .expect("same space combines");
    assert!((product.score.distance() - 3.0).abs() < 1e-9);
    assert_eq!(product.spaces.len(), 1);
}

#[test]
fn purremb_multiply_cross_space_refused_without_licensing() {
    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let left = SpaceTaggedScore::single(1.0, space_id(1), 1);
    let right = SpaceTaggedScore::single(2.0, space_id(2), 1);
    assert!(algebra.multiply(&left, &right).is_err());
}

#[test]
fn purremb_multiply_cross_space_licensed_combines() {
    let mut licensing = BTreeSet::new();
    licensing.insert(([1_u8; 32], [2_u8; 32]));
    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(licensing);
    let left = SpaceTaggedScore::single(1.0, space_id(1), 1);
    let right = SpaceTaggedScore::single(2.0, space_id(2), 1);
    let product = algebra
        .multiply(&left, &right)
        .expect("licensed cross-space combines");
    assert!((product.score.distance() - 3.0).abs() < 1e-9);
    assert_eq!(product.spaces.len(), 2);
}

#[test]
fn purremb_multiply_identity_is_neutral() {
    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let element = SpaceTaggedScore::single(4.0, space_id(3), 1);
    let one = algebra.one();
    assert_eq!(algebra.multiply(&one, &element).expect("neutral"), element);
    assert_eq!(algebra.multiply(&element, &one).expect("neutral"), element);
}

#[test]
fn purremb_add_chooses_the_better_alternative() {
    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let near = SpaceTaggedScore::single(0.5, space_id(1), 1);
    let far = SpaceTaggedScore::single(5.0, space_id(2), 1);
    let sum = algebra.add(&near, &far).expect("alternatives combine");
    assert!((sum.score.distance() - 0.5).abs() < 1e-9);
    assert_eq!(sum.spaces.len(), 2);
}

#[test]
fn purremb_algebra_identity_folds_declared_deviations() {
    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    assert!(algebra.identity().contains("multiply-associative"));
    let plain = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    assert_ne!(algebra.identity(), plain.identity());
}

#[test]
fn purremb_generation_iri_distinguishes_selection() {
    let exact = purremb_generation_iri(
        "https://example.org/gen",
        "abcd",
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Certified,
    );
    let matryoshka = purremb_generation_iri(
        "https://example.org/gen",
        "abcd",
        RetrievalPolicy::MatryoshkaPrefixThenRerank,
        SourceVerificationMode::Certified,
    );
    assert_ne!(exact, matryoshka);
    assert!(exact.contains("abcd"));
}

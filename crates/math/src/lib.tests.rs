// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::cmp::Ordering;

/// The `math` GTS bundle is authored GMEOW GTS output: every payload frame it
/// carries uses the one mandated transform (`zstd-rsyncable` @ L12), with no
/// size-threshold fallback to plain `zstd`. `index_turtle` is an on-demand
/// path that materializes no file, so this crate-local audit is the on-gate
/// coverage for it.
#[test]
fn math_bundle_uses_the_mandated_frame_profile() {
    let bytes = {
        let emission = turtle_to_gts(
            concat!(
                "@prefix math: <https://blackcatinformatics.ca/math/> .\n",
                "<urn:gmeow:math:space> a math:InnerProductSpace ; math:dimension 2 .\n",
            )
            .as_bytes(),
        )
        .expect("emit the math bundle");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };
    gmeow_gts_profile::validate_mandated_frames(&bytes)
        .expect("math bundle uses the mandated zstd-rsyncable-L12 frame profile");
}

fn r(num: i128, den: i128) -> Rational {
    Rational::new(num, den).expect("rational")
}

/// The canonical correlated metric G = [[1, 1/4], [1/4, 1]].
fn correlated_gram() -> InnerProductSpace {
    InnerProductSpace::new(vec![vec![r(1, 1), r(1, 4)], vec![r(1, 4), r(1, 1)]]).expect("space")
}

// Hash is consistent with Eq: equal-valued rationals (normalized to the same
// canonical pair) hash equal, so Rational is a sound HashMap/HashSet key.
#[test]
fn equal_rationals_hash_equal() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_of(value: Rational) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    // 1/2 and 2/4 normalize to the same (1, 2); Eq and Hash must agree.
    let half = r(1, 2);
    let also_half = r(2, 4);
    assert_eq!(half, also_half);
    assert_eq!(hash_of(half), hash_of(also_half));

    // A negative denominator normalizes to a positive one; still hashes equal.
    let neg = r(1, -3);
    let pos = r(-1, 3);
    assert_eq!(neg, pos);
    assert_eq!(hash_of(neg), hash_of(pos));

    // Distinct values (overwhelmingly) hash apart — a weak inequality sanity check.
    assert_ne!(hash_of(r(1, 2)), hash_of(r(1, 3)));
}

#[test]
fn parse_decimal_is_exact() {
    assert_eq!(Rational::parse_decimal("0.7").unwrap(), r(7, 10));
    assert_eq!(Rational::parse_decimal("-1.0").unwrap(), r(-1, 1));
    assert_eq!(Rational::parse_decimal("0.5").unwrap(), r(1, 2));
    assert_eq!(Rational::parse_decimal("2").unwrap(), r(2, 1));
}

// √(xᵀGx) over the NON-orthogonal G differs from raw L².
#[test]
fn metric_norm_distinct_from_raw_l2() {
    let space = correlated_gram();
    let x = [r(7, 10), r(2, 5)]; // valence 0.7, arousal 0.4
    // Q = xᵀGx = 0.7·(0.7 + 0.25·0.4) + 0.4·(0.25·0.7 + 0.4) = 79/100.
    let q = space.quadratic_form(&x).unwrap();
    assert_eq!(q, r(79, 100));
    assert_eq!(q.ratio_string(), "79/100");
    let intensity = space.norm(&x).unwrap();
    assert_eq!(intensity, "0.888819");
    // Raw L² over the SAME vector is √(0.49 + 0.16) = √0.65 ≈ 0.806226 — distinct.
    let raw_l2 = sqrt_rational_decimal(r(65, 100)).unwrap();
    assert_eq!(raw_l2, "0.806226");
    assert_ne!(intensity, raw_l2);
}

// LDLᵀ certifies PD (pivots 1, 15/16); an indefinite G names its pivot.
#[test]
fn ldlt_positive_definite_certificate() {
    let pivots = correlated_gram().ldlt_pivots().unwrap();
    assert_eq!(pivots, vec![r(1, 1), r(15, 16)]);

    let indefinite =
        InnerProductSpace::new(vec![vec![r(1, 1), r(2, 1)], vec![r(2, 1), r(1, 1)]]).unwrap();
    let err = indefinite.ldlt_pivots().unwrap_err();
    // Pivot 0 = 1 (> 0), pivot 1 = 1 − 4 = −3 (not > 0).
    assert!(err.message().contains("pivot 1"), "{err}");
    assert!(err.message().contains("-3"), "{err}");
}

// Metric-aware dominant axis differs from the raw-max component.
#[test]
fn dominant_axis_is_metric_aware_not_raw_max() {
    let space =
        InnerProductSpace::new(vec![vec![r(2, 1), r(0, 1)], vec![r(0, 1), r(1, 1)]]).unwrap();
    let x = [r(1, 2), r(3, 5)]; // valence 0.5, arousal 0.6
    // G-weighted: axis0 = 0.5·(2·0.5) = 0.5 > axis1 = 0.6·(1·0.6) = 0.36.
    assert_eq!(space.dominant_axis(&x).unwrap(), 0);
    // Raw-max component is arousal (axis 1): 0.6 > 0.5. Explicitly different.
    let raw_max = if x[1] > x[0] { 1 } else { 0 };
    assert_eq!(raw_max, 1);
    assert_ne!(space.dominant_axis(&x).unwrap(), raw_max);
}

// Distance and cosine match hand-computed exact values.
#[test]
fn distance_and_cosine_hand_checked() {
    // Identity metric so ⟨·,·⟩ is ordinary dot product.
    let space =
        InnerProductSpace::new(vec![vec![r(1, 1), r(0, 1)], vec![r(0, 1), r(1, 1)]]).unwrap();
    let x = [r(3, 1), r(4, 1)];
    let y = [r(4, 1), r(3, 1)];
    // x − y = (−1, 1); ‖·‖ = √2 = 1.414214 (rounded at 7th digit).
    assert_eq!(space.distance(&x, &y).unwrap(), "1.414214");
    // ⟨x,y⟩ = 24; ‖x‖‖y‖ = 25; cos = 24/25 = 0.96.
    assert_eq!(space.cosine(&x, &y).unwrap(), "0.960000");
    // Orthogonality and projection sanity.
    assert!(!space.is_orthogonal(&x, &y).unwrap());
    assert!(
        space
            .is_orthogonal(&[r(1, 1), r(0, 1)], &[r(0, 1), r(1, 1)])
            .unwrap()
    );
    // Project (3,4) onto (1,0) → (3,0).
    assert_eq!(
        space.project(&x, &[r(1, 1), r(0, 1)]).unwrap(),
        vec![r(3, 1), r(0, 1)]
    );
}

// Determinism, overflow hard-fail, and undefined-input hard fails.
#[test]
fn determinism_and_hard_fails() {
    let space = correlated_gram();
    let x = [r(7, 10), r(2, 5)];
    let first = space.norm(&x).unwrap();
    let second = space.norm(&x).unwrap();
    assert_eq!(first, second); // byte-identical, run twice

    // Overflow: (i128::MAX/2) · 4 must hard-fail, never wrap.
    let big = r(i128::MAX / 2, 1);
    assert!(
        big.checked_mul(r(4, 1))
            .unwrap_err()
            .message()
            .contains("overflow")
    );

    // Zero-vector cosine is undefined → Err.
    let zero = [r(0, 1), r(0, 1)];
    assert!(
        space
            .cosine(&x, &zero)
            .unwrap_err()
            .message()
            .contains("zero vector")
    );
}

// The Ord cross-multiply stays exact for the correlated-metric dominant-axis
// case (valence 0.7 / arousal 0.4 over G = [[1,1/4],[1/4,1]]).
#[test]
fn dominant_axis_ord_correct_for_correlated_metric() {
    let space = correlated_gram();
    let x = [r(7, 10), r(2, 5)];
    assert_eq!(space.dominant_axis(&x).unwrap(), 0);
    // Direct Ord check of the two exact G-weighted contributions.
    assert!(r(56, 100) > r(23, 100));
    assert_eq!(r(56, 100).cmp(&r(23, 100)), Ordering::Greater);
    assert_eq!(r(23, 100).cmp(&r(56, 100)), Ordering::Less);
    assert_eq!(r(56, 100).cmp(&r(56, 100)), Ordering::Equal);
}

// Overflow in the Ord cross-multiply is a loud, deterministic hard fail
// (checked_mul + expect), never a silent i128 wrap.
#[test]
#[should_panic(expected = "cross-multiplication overflow")]
fn cmp_overflow_hard_fails() {
    let a = r(i128::MAX / 2, 1);
    let b = r(1, i128::MAX / 2 + 2);
    let _ = a.cmp(&b);
}

#[test]
fn normalize_to_unit_matches_pad_scale() {
    let min = r(-1, 1);
    let max = r(1, 1);
    assert_eq!(normalize_to_unit(&r(7, 10), &min, &max).unwrap(), "0.85");
    assert_eq!(normalize_to_unit(&r(2, 5), &min, &max).unwrap(), "0.7");
}

/// Integer-part-first long division does not prematurely scale the numerator,
/// so a small-VALUED rational carried by an enormous numerator/denominator
/// (`num * 10^k` would blow past `u128::MAX`) still formats exactly.
#[test]
fn format_decimal_no_premature_overflow_on_small_valued_giant_ratios() {
    let big = 10i128.pow(33);
    // 10^33 / 10^33 = 1: old `num * 10^6 = 10^39` overflowed u128.
    let one = Rational {
        numerator: big,
        denominator: big,
    };
    assert_eq!(format_decimal(one).unwrap(), "1");
    // 10^33 / (2·10^33) = 0.5: same overflow, representable value.
    let half = Rational {
        numerator: big,
        denominator: 2 * big,
    };
    assert_eq!(format_decimal(half).unwrap(), "0.5");
    // Negative sign is preserved through the long-division path.
    let neg_half = Rational {
        numerator: -big,
        denominator: 2 * big,
    };
    assert_eq!(format_decimal(neg_half).unwrap(), "-0.5");
    // Existing exact values still format byte-identically.
    assert_eq!(format_decimal(r(17, 20)).unwrap(), "0.85");
    assert_eq!(format_decimal(r(2, 5)).unwrap(), "0.4");
    assert_eq!(format_decimal(r(12, 5)).unwrap(), "2.4");
}

// ── TripleIndex: a typed literal's datatype/language survives, and distinguishes
// three otherwise-lexically-identical literals, through BOTH `index_graph` (via
// the GTS-normalized `index_turtle`) and `index_dataset` ────────────────────────

const TYPED_LITERAL_TTL: &str = "@prefix ex: <https://example.org/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         ex:s ex:intVal \"42\"^^xsd:integer .\n\
         ex:s ex:strVal \"42\"^^xsd:string .\n\
         ex:s ex:langVal \"42\"@en .\n";

fn assert_typed_literal_round_trip(index: &TripleIndex) {
    let (lex_i, dt_i, lang_i) =
        first_literal_typed(index, "https://example.org/s", "https://example.org/intVal")
            .expect("xsd:integer literal present");
    assert_eq!(lex_i, "42");
    assert_eq!(dt_i, "http://www.w3.org/2001/XMLSchema#integer");
    assert_eq!(lang_i, None);

    let (lex_s, dt_s, lang_s) =
        first_literal_typed(index, "https://example.org/s", "https://example.org/strVal")
            .expect("xsd:string literal present");
    assert_eq!(lex_s, "42");
    assert_eq!(dt_s, "http://www.w3.org/2001/XMLSchema#string");
    assert_eq!(lang_s, None);

    let (lex_l, dt_l, lang_l) = first_literal_typed(
        index,
        "https://example.org/s",
        "https://example.org/langVal",
    )
    .expect("language-tagged literal present");
    assert_eq!(lex_l, "42");
    assert_eq!(
        dt_l,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
    );
    assert_eq!(lang_l, Some("en"));

    // Same lexical form, three DISTINCT literals: datatype/language is what
    // distinguishes them, never dropped.
    assert_ne!(dt_i, dt_s, "xsd:integer and xsd:string are distinct");
    assert_ne!(dt_s, dt_l, "xsd:string and rdf:langString are distinct");
    assert_ne!(lang_i, lang_l, "the language tag distinguishes langVal");

    // The lossy `first_literal` still returns just the lexical form.
    assert_eq!(
        first_literal(index, "https://example.org/s", "https://example.org/intVal").as_deref(),
        Some("42")
    );
}

#[test]
fn typed_literal_round_trips_through_index_graph() {
    let index = index_turtle(TYPED_LITERAL_TTL.as_bytes()).expect("index_turtle");
    assert_typed_literal_round_trip(&index);
}

#[test]
fn typed_literal_round_trips_through_index_dataset() {
    let dataset = purrdf::parse_dataset(TYPED_LITERAL_TTL.as_bytes(), "text/turtle", None)
        .expect("parse dataset");
    let index = index_dataset(&dataset);
    assert_typed_literal_round_trip(&index);
}

// ── TripleIndex: a blank node is `_:`-prefixed identically through both
// `index_graph` and `index_dataset`, so a blank-node object round-trips back
// into a followable subject key on either path ──────────────────────────────

const BLANK_NODE_TTL: &str = "@prefix ex: <https://example.org/> .\n\
         ex:s ex:p _:b1 .\n\
         _:b1 ex:q ex:o .\n";

fn assert_blank_node_round_trip(index: &TripleIndex) {
    let bnode_key = first_iri(index, "https://example.org/s", "https://example.org/p")
        .expect("blank-node object present");
    assert!(
        bnode_key.starts_with("_:"),
        "blank-node object key is `_:`-prefixed: {bnode_key}"
    );
    // The SAME key, used as a subject, resolves the blank node's own triple —
    // i.e. subject-position and object-position blank-node keys agree.
    let followed = first_iri(index, &bnode_key, "https://example.org/q")
        .expect("blank node is followable as a subject under its `_:`-prefixed key");
    assert_eq!(followed, "https://example.org/o");
}

#[test]
fn blank_node_prefixed_through_index_graph() {
    let index = index_turtle(BLANK_NODE_TTL.as_bytes()).expect("index_turtle");
    assert_blank_node_round_trip(&index);
}

#[test]
fn blank_node_prefixed_through_index_dataset() {
    let dataset = purrdf::parse_dataset(BLANK_NODE_TTL.as_bytes(), "text/turtle", None)
        .expect("parse dataset");
    let index = index_dataset(&dataset);
    assert_blank_node_round_trip(&index);
}

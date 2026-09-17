// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const MATH: &str = "https://blackcatinformatics.ca/math/";

/// `plus(1, 2)` as a `math:ApplicationExpression`, and the same expression authored a
/// second time with different blank-node labels and reversed slot order.
/// Parse `turtle` and intern the expression rooted at `root` — the two steps the deleted
/// Turtle-bytes wrapper used to bundle, spelled out at the one place that needs them.
fn intern_turtle(
    arena: &mut TermArena,
    turtle: &[u8],
    root: &str,
) -> gmeow_errors::Result<(StructNode, ContentKey)> {
    let graph = crate::physical::lower::MathGraph::from_turtle(turtle)?;
    Ok(intern_math_root(arena, &graph, root)?)
}

fn application_turtle(labels: (&str, &str, &str), reversed: bool) -> Vec<u8> {
    let (root, s0, s1) = labels;
    let slots = if reversed {
        format!("_:{s1}, _:{s0}")
    } else {
        format!("_:{s0}, _:{s1}")
    };
    format!(
        r#"@prefix math: <{MATH}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

_:{root} a math:ApplicationExpression ;
    math:operator <https://example.org/plus> ;
    math:argumentSlot {slots} .

_:{s0} a math:ArgumentSlot ;
    math:slotIndex 0 ;
    math:slotExpression _:{s0}v .
_:{s0}v a math:NumberLiteral ; math:literalValue "1"^^xsd:integer .

_:{s1} a math:ArgumentSlot ;
    math:slotIndex 1 ;
    math:slotExpression _:{s1}v .
_:{s1}v a math:NumberLiteral ; math:literalValue "2"^^xsd:integer .
"#
    )
    .into_bytes()
}

/// The wrapper interns through the shared arena: the same expression authored twice —
/// different blank-node labels, different serialization order — collapses to ONE node
/// and ONE content key, and the second lift mints nothing.
#[test]
fn intern_math_root_is_content_addressed_and_hash_consed() {
    let mut arena = TermArena::new();

    let first_mark = arena.snapshot();
    let (first, first_key) = intern_turtle(
        &mut arena,
        &application_turtle(("e", "a", "b"), false),
        "_:e",
    )
    .expect("well-formed math application");
    let first_delta = first_mark.delta_to(&arena);
    assert!(
        first_delta.distinct_nodes > 0,
        "the first lift must mint nodes"
    );

    let second_mark = arena.snapshot();
    let (second, second_key) = intern_turtle(
        &mut arena,
        &application_turtle(("z", "y", "x"), true),
        "_:z",
    )
    .expect("well-formed math application");
    let second_delta = second_mark.delta_to(&arena);

    assert_eq!(first, second, "the same expression is ONE node");
    assert_eq!(first_key, second_key, "…with ONE content key");
    assert_eq!(
        second_delta.distinct_nodes, 0,
        "re-lifting an already-interned expression mints NOTHING"
    );
    assert_eq!(
        second_delta.intern_calls, first_delta.intern_calls,
        "…while doing the same interning work"
    );
}

/// An unliftable expression graph is a typed hard failure, never a partial lift.
#[test]
fn intern_math_root_hard_fails_on_a_malformed_slot_sequence() {
    // slotIndex 0 and 2 — non-contiguous, so the argument order is undecidable.
    let turtle = format!(
        r#"@prefix math: <{MATH}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

_:e a math:ApplicationExpression ;
    math:operator <https://example.org/plus> ;
    math:argumentSlot _:a, _:b .
_:a a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression _:av .
_:av a math:NumberLiteral ; math:literalValue "1"^^xsd:integer .
_:b a math:ArgumentSlot ; math:slotIndex 2 ; math:slotExpression _:bv .
_:bv a math:NumberLiteral ; math:literalValue "2"^^xsd:integer .
"#
    )
    .into_bytes();

    let mut arena = TermArena::new();
    intern_turtle(&mut arena, &turtle, "_:e")
        .expect_err("a non-contiguous slot sequence must hard-fail");
}

/// Unparsable bytes are a typed hard failure too — no empty-graph fallback.
#[test]
fn intern_math_root_hard_fails_on_unparsable_turtle() {
    let mut arena = TermArena::new();
    intern_turtle(&mut arena, b"this is not turtle {{{", "_:e")
        .expect_err("unparsable input must hard-fail");
}

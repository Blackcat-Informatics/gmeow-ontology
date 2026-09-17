// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// ---- compare_rdf tests ----

#[test]
fn rdf_identical_graphs_match() {
    let nt = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/B> .\n";
    assert_eq!(compare_rdf(nt, nt, NTRIPLES), Vec::<String>::new());
}

#[test]
fn rdf_isomorphic_blank_node_graphs_match() {
    let g1 = "<https://example.org/A> <https://example.org/rel> _:x .\n_:x <https://example.org/label> <https://example.org/val> .\n";
    let g2 = "<https://example.org/A> <https://example.org/rel> _:y .\n_:y <https://example.org/label> <https://example.org/val> .\n";
    assert_eq!(compare_rdf(g1, g2, NTRIPLES), Vec::<String>::new());
}

#[test]
fn rdf_differing_graphs_fail() {
    let g1 = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/B> .\n";
    let g2 = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/C> .\n";
    assert!(!compare_rdf(g1, g2, NTRIPLES).is_empty());
}

#[test]
fn rdf_empty_vs_nonempty_fails() {
    let g2 = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/B> .\n";
    assert!(!compare_rdf("", g2, NTRIPLES).is_empty());
}

#[test]
fn rdf_empty_vs_empty_passes() {
    assert_eq!(compare_rdf("", "", NTRIPLES), Vec::<String>::new());
}

#[test]
fn rdf12_triple_terms_compare() {
    let ttl = "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n<https://example.org/r> rdf:reifies <<( <https://example.org/s> <https://example.org/p> <https://example.org/o> )>> .\n";
    assert_eq!(compare_rdf(ttl, ttl, TURTLE), Vec::<String>::new());
}

// ---- compare_canonical_json (ports TestCompareCanonicalJson) ----

#[test]
fn json_identical_match() {
    let d = serde_json::json!({"a": 1, "b": "hello"});
    assert_eq!(compare_canonical_json(&d, &d), Vec::<String>::new());
}

#[test]
fn json_key_order_independent() {
    let d1 = serde_json::json!({"z": 3, "a": 1, "m": "foo"});
    let d2 = serde_json::json!({"a": 1, "m": "foo", "z": 3});
    assert_eq!(compare_canonical_json(&d1, &d2), Vec::<String>::new());
}

#[test]
fn json_nested_key_order_independent() {
    let d1 = serde_json::json!({"x": {"b": 2, "a": 1}});
    let d2 = serde_json::json!({"x": {"a": 1, "b": 2}});
    assert_eq!(compare_canonical_json(&d1, &d2), Vec::<String>::new());
}

#[test]
fn json_value_difference_fails() {
    let d1 = serde_json::json!({"a": 1});
    let d2 = serde_json::json!({"a": 2});
    assert!(!compare_canonical_json(&d1, &d2).is_empty());
}

#[test]
fn json_missing_key_fails() {
    let d1 = serde_json::json!({"a": 1, "b": 2});
    let d2 = serde_json::json!({"a": 1});
    assert!(!compare_canonical_json(&d1, &d2).is_empty());
}

#[test]
fn json_string_normalization_unchanged() {
    let d1 = serde_json::json!({"k": "SoundUnderApproximation"});
    let d2 = serde_json::json!({"k": "SoundUnderApproximation"});
    assert_eq!(compare_canonical_json(&d1, &d2), Vec::<String>::new());
}

#[test]
fn json_list_order_matters() {
    let d1 = serde_json::json!({"arr": [1, 2, 3]});
    let d2 = serde_json::json!({"arr": [3, 2, 1]});
    assert!(!compare_canonical_json(&d1, &d2).is_empty());
}

// ---- compare_explanation_skeleton (ports TestCompareExplanationSkeleton) ----

const IRI_A: &str = "https://example.org/rule/A";
const IRI_B: &str = "https://example.org/term/B";
const IRI_C: &str = "https://example.org/reifier/abc";

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn skeleton_identical_match() {
    let iris = set(&[IRI_A, IRI_B]);
    assert_eq!(
        compare_explanation_skeleton(&iris, &iris),
        Vec::<String>::new()
    );
}

#[test]
fn skeleton_different_fail() {
    let actual = set(&[IRI_A, IRI_B]);
    let expected = set(&[IRI_A, IRI_C]);
    assert!(!compare_explanation_skeleton(&actual, &expected).is_empty());
}

#[test]
fn skeleton_extra_iri_flagged() {
    let actual = set(&[IRI_A, IRI_B, IRI_C]);
    let expected = set(&[IRI_A, IRI_B]);
    let result = compare_explanation_skeleton(&actual, &expected);
    assert!(!result.is_empty());
    let combined = result.join("\n");
    assert!(combined.to_lowercase().contains("extra") || combined.contains(IRI_C));
}

#[test]
fn skeleton_missing_iri_flagged() {
    let actual = set(&[IRI_A]);
    let expected = set(&[IRI_A, IRI_B]);
    let result = compare_explanation_skeleton(&actual, &expected);
    assert!(!result.is_empty());
    let combined = result.join("\n");
    assert!(combined.to_lowercase().contains("missing") || combined.contains(IRI_B));
}

// ---- skeleton/reifier parsing (ports the diff_case explanation-test parsing) ----

#[test]
fn parse_skeleton_block_collects_iris() {
    let md = "# Explanation for `<https://example.org/reifier/x>`\n\n\
                  <!-- cited-iri-skeleton\n  https://example.org/rule/A\n  https://example.org/term/B\n-->\n\n\
                  <!-- step-skeleton\n  step ...\n-->\nProse here is ignored.\n";
    let iris = parse_cited_iri_skeleton(md);
    assert_eq!(
        iris,
        set(&["https://example.org/rule/A", "https://example.org/term/B"])
    );
}

#[test]
fn parse_skeleton_stops_at_close_marker() {
    // IRIs after the `-->` (e.g. inside the step-skeleton block) must NOT leak in.
    let md =
        "<!-- cited-iri-skeleton\n  https://example.org/only\n-->\n  https://example.org/leaked\n";
    assert_eq!(
        parse_cited_iri_skeleton(md),
        set(&["https://example.org/only"])
    );
}

#[test]
fn parse_reifier_from_header() {
    let md = "# Explanation for `<https://example.org/reifier/abc>`\nbody\n";
    assert_eq!(
        parse_explanation_reifier(md),
        "https://example.org/reifier/abc"
    );
}

#[test]
fn parse_reifier_absent_returns_empty() {
    assert_eq!(parse_explanation_reifier("no header here\n"), "");
}

// ---- leading_comment_block (banner helper) ----

#[test]
fn banner_identical_headers_produce_no_drift() {
    // Two documents with the SAME two-line banner but different (isomorphic) graphs:
    // the banner check must pass.
    let banner = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
                      # OWL 2 DL projection of the canonical logic: program.";
    let body_a = "<https://example.org/A> <https://example.org/p> <https://example.org/B> .\n";
    let body_b = "<https://example.org/A> <https://example.org/p> <https://example.org/B> .\n";
    let doc_a = format!("{banner}\n{body_a}");
    let doc_b = format!("{banner}\n{body_b}");
    let produced_hdr = leading_comment_block(&doc_a);
    let golden_hdr = leading_comment_block(&doc_b);
    assert_eq!(produced_hdr, golden_hdr, "same banner must not flag drift");
}

#[test]
fn banner_stale_python_module_suffix_detected() {
    // The headline defect: old golden had `(logic_projections.py)` in the GENERATED
    // line; produced banner no longer has it. The helper must detect the mismatch.
    let produced_banner = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
                               # OWL 2 DL projection of the canonical logic: program.";
    let stale_golden_banner = "# GENERATED by `gmeow logic compile` (logic_projections.py) — DO NOT EDIT.\n\
             # OWL 2 DL projection of the canonical logic: program.";
    let body = "<https://example.org/A> <https://example.org/p> <https://example.org/B> .\n";
    let produced_doc = format!("{produced_banner}\n{body}");
    let stale_doc = format!("{stale_golden_banner}\n{body}");
    let produced_hdr = leading_comment_block(&produced_doc);
    let golden_hdr = leading_comment_block(&stale_doc);
    assert_ne!(
        produced_hdr, golden_hdr,
        "stale (logic_projections.py) banner must be detected as drift"
    );
}

#[test]
fn banner_helper_stops_at_first_non_comment_non_blank_line() {
    let doc = "# line one\n# line two\n<https://example.org/s> <https://example.org/p> <https://example.org/o> .\n# not in banner\n";
    assert_eq!(leading_comment_block(doc), "# line one\n# line two");
}

#[test]
fn banner_helper_no_comments_returns_empty() {
    let doc = "<https://example.org/s> <https://example.org/p> <https://example.org/o> .\n";
    assert_eq!(leading_comment_block(doc), "");
}

#[test]
fn banner_helper_only_comments_returns_all() {
    let doc = "# line one\n# line two\n";
    assert_eq!(leading_comment_block(doc), "# line one\n# line two");
}

// ---- nquads_by_named_graph ----

#[test]
fn nquads_buckets_by_named_graph_and_drops_default() {
    let nq = "<https://example.org/s> <https://example.org/p> <https://example.org/o> <https://example.org/w1> .\n\
                  <https://example.org/s2> <https://example.org/p> <https://example.org/o> <https://example.org/w2> .\n\
                  <https://example.org/sd> <https://example.org/p> <https://example.org/o> .\n";
    let by_graph = nquads_by_named_graph(nq).expect("parse");
    assert_eq!(by_graph.len(), 2);
    assert!(by_graph.contains_key("https://example.org/w1"));
    assert!(by_graph.contains_key("https://example.org/w2"));
    // Each bucket re-parses as valid N-Triples and round-trips through compare_rdf.
    let w1 = &by_graph["https://example.org/w1"];
    assert_eq!(compare_rdf(w1, w1, NTRIPLES), Vec::<String>::new());
}

#[test]
fn nquads_empty_input_is_empty_map() {
    assert!(nquads_by_named_graph("   \n").expect("parse").is_empty());
}

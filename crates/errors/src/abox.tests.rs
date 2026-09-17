// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/example/subject";
const LABEL: &str = "Example subject";
const DEFINITION: &str = "A subject minted for a unit test.";
const GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/example";

/// Exactly four triples, in the fixed (label, definition, isDefinedBy,
/// graphBoxRole) order — the order both adapters commit to.
#[test]
fn annotate_nquads_emits_four_triples_in_fixed_order() {
    let mut out = Vec::new();
    annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut out);
    assert_eq!(out.len(), 4, "must emit exactly four annotation triples");
    assert!(
        out[0].contains(RDFS_LABEL),
        "line 0 must be rdfs:label: {out:?}"
    );
    assert!(
        out[1].contains(SKOS_DEFINITION),
        "line 1 must be skos:definition: {out:?}"
    );
    assert!(
        out[2].contains(RDFS_IS_DEFINED_BY),
        "line 2 must be rdfs:isDefinedBy: {out:?}"
    );
    assert!(
        out[3].contains(GRAPH_BOX_ROLE),
        "line 3 must be gmeow:graphBoxRole: {out:?}"
    );
}

/// `isDefinedBy`'s object is exactly the passed graph IRI, and
/// `graphBoxRole`'s object is exactly `gmeow:boxABox`.
#[test]
fn annotate_nquads_isdefinedby_and_role_objects_are_correct() {
    let mut out = Vec::new();
    annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut out);
    assert!(
        out[2].contains(&format!("<{GRAPH}>")),
        "isDefinedBy must point at the containing graph IRI: {}",
        out[2]
    );
    assert!(
        out[3].contains(&format!("<{BOX_ABOX}>")),
        "graphBoxRole must point at gmeow:boxABox: {}",
        out[3]
    );
}

/// Both label and definition literals carry the `x-gmeow-english` carrier
/// language tag, never bare `en`.
#[test]
fn annotate_nquads_literals_carry_the_carrier_language_tag() {
    let mut out = Vec::new();
    annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut out);
    for line in &out[0..2] {
        assert!(
            line.ends_with(&format!("\"@{X_GMEOW_ENGLISH} <{GRAPH}> .")),
            "literal line must carry the x-gmeow-english carrier tag: {line}"
        );
        assert!(
            !line.contains("\"@en "),
            "literal line must never carry a bare @en tag: {line}"
        );
    }
}

/// Determinism: identical inputs produce byte-identical output, every call.
#[test]
fn annotate_nquads_is_deterministic() {
    let mut a = Vec::new();
    let mut b = Vec::new();
    annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut a);
    annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut b);
    assert_eq!(a, b);
}

/// A subject or literal carrying characters `nq_escape` must escape (a quote,
/// a backslash) round-trips through the string adapter without corrupting the
/// N-Quads line shape (still four triples, still ending in the graph token).
#[test]
fn annotate_nquads_escapes_literal_content() {
    let mut out = Vec::new();
    annotate_nquads(SUBJECT, "has \"quotes\"", "has\\backslash", GRAPH, &mut out);
    assert!(out[0].contains("has \\\"quotes\\\""), "{}", out[0]);
    assert!(out[1].contains("has\\\\backslash"), "{}", out[1]);
}

/// Cross-substrate parity (the key LSP test): the quad SET the string
/// adapter emits equals the quad set the builder adapter emits, for the
/// same inputs — parsed back into logical `(s, p, o, g)` tuples so the
/// comparison is substrate-independent (a builder-frozen dataset needn't
/// preserve N-Quads' exact literal spelling, only the same RDF term
/// identity).
#[test]
fn cross_substrate_parity_string_and_builder_emit_the_same_quad_set() {
    use std::collections::BTreeSet;

    let mut lines = Vec::new();
    annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut lines);

    let mut builder = purrdf_core::RdfDatasetBuilder::new();
    annotate_builder(&mut builder, SUBJECT, LABEL, DEFINITION, GRAPH);
    let dataset = builder.freeze().expect("valid dataset");

    // Render the builder's quads through the SAME term Display the
    // production N-Triples/N-Quads codec uses, so both sides compare in
    // the identical textual term grammar (`<iri>` / `"lex"@tag`).
    let mut from_builder: BTreeSet<String> = BTreeSet::new();
    for quad in dataset.owned_quads() {
        let g = quad
            .graph_name
            .as_ref()
            .map(|g| g.to_string())
            .unwrap_or_default();
        from_builder.insert(format!(
            "{} <{}> {} {}",
            quad.subject, quad.predicate, quad.object, g
        ));
    }

    // Parse the string adapter's N-Quads lines into the same
    // "s p o g" shape (predicate is already bare-angle-bracketed like the
    // subject/graph, so strip the trailing " ." and re-join on a single
    // space, dropping the doubled predicate brackets vs. Display's IRI
    // rendering — both sides normalize to `<iri>`/`"lex"@tag` tokens).
    let mut from_nquads: BTreeSet<String> = BTreeSet::new();
    for line in &lines {
        let body = line
            .strip_suffix(" .")
            .expect("every emitted line ends with ' .'");
        from_nquads.insert(body.to_owned());
    }

    assert_eq!(
        from_builder, from_nquads,
        "string and builder adapters must emit the identical logical quad set"
    );
}

/// The Turtle adapter renders the four clauses as prefixed-Turtle lines, at the
/// requested indent, in the fixed order — the exact spelling the shape emitters
/// hand-rolled before routing through this module. `isDefinedBy`'s graph IRI (a
/// `gmeow:graph/…` path) stays angle-bracketed; `graphBoxRole`'s role IRI
/// prefixes to `gmeow:boxABox`; the predicates prefix to `rdfs:`/`skos:`/`gmeow:`.
#[test]
fn turtle_lines_render_the_prefixed_four_clause_skeleton() {
    let lines = abox_annotation_turtle_lines(SUBJECT, LABEL, DEFINITION, GRAPH, BOX_ABOX, "    ");
    assert_eq!(
        lines,
        [
            format!("    rdfs:label \"{LABEL}\"@{X_GMEOW_ENGLISH} ;"),
            format!("    skos:definition \"{DEFINITION}\"@{X_GMEOW_ENGLISH} ;"),
            format!("    rdfs:isDefinedBy <{GRAPH}> ;"),
            "    gmeow:graphBoxRole gmeow:boxABox ;".to_string(),
        ]
    );
}

/// The box role is parameterized: a T-Box `owl:Ontology` header carries
/// `gmeow:boxTBox`, reusing the same core the A-Box default rides.
#[test]
fn turtle_lines_honor_the_box_role_parameter() {
    let lines = abox_annotation_turtle_lines(SUBJECT, LABEL, DEFINITION, GRAPH, BOX_TBOX, "    ");
    assert_eq!(lines[3], "    gmeow:graphBoxRole gmeow:boxTBox ;");
}

/// The Turtle adapter escapes literal content the same way the N-Quads adapter
/// does, so a definition with a quote can never break the emitted statement.
#[test]
fn turtle_lines_escape_literal_content() {
    let lines = abox_annotation_turtle_lines(
        SUBJECT,
        "has \"quotes\"",
        "has\\backslash",
        GRAPH,
        BOX_ABOX,
        "    ",
    );
    assert!(lines[0].contains("has \\\"quotes\\\""), "{}", lines[0]);
    assert!(lines[1].contains("has\\\\backslash"), "{}", lines[1]);
}

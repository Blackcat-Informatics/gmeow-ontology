// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// `okf_dir_to_graph` reads a bundle directory (nested subdirectories
/// included), lifts it through purrdf's native in-process codec (no
/// subprocess, no temp `.gts` file), and returns the flattened asserted
/// `okf:` base triples — the `okf:links` Markdown-link edge survives, but the
/// RDF-star reifier / linkText / linkOccurrence sidecar rows the reader
/// emits are filtered out (matching the prior `gts_base_graph` contract).
#[test]
fn okf_dir_to_graph_lifts_a_bundle_directory_in_process() {
    let tmp = tempfile::Builder::new()
        .prefix(".gmeow-test-okfin-")
        .tempdir()
        .expect("tempdir");
    std::fs::create_dir_all(tmp.path().join("classes")).expect("mkdir");
    std::fs::write(
        tmp.path().join("classes/schema.md"),
        "---\ntype: Class\ntitle: Schema\n---\nColumns: id.\n",
    )
    .expect("write schema.md");
    std::fs::write(
            tmp.path().join("classes/table.md"),
            "---\ntype: Class\ntitle: Table\nresource: https://example.org/data/Table\n---\nSee [schema](schema.md).\n",
        )
        .expect("write table.md");

    let quads = okf_dir_to_graph(tmp.path()).expect("lift bundle directory");
    assert!(!quads.is_empty(), "expected lifted quads");

    let type_predicate = format!("{OKF_NS}type");
    let links_predicate = format!("{OKF_NS}links");
    assert!(
        quads.iter().any(|q| q.predicate == type_predicate),
        "expected an okf:type triple"
    );
    assert!(
        quads.iter().any(|q| q.predicate == links_predicate),
        "expected the schema->table okf:links edge"
    );
    // No RDF-1.2 reifier / quoted-triple row survives the filter.
    assert!(
        quads.iter().all(|q| q.predicate != RDF_REIFIES
            && !matches!(q.subject, RdfTerm::Triple(_))
            && !matches!(q.object, RdfTerm::Triple(_))),
        "reifier/quoted-triple rows must be filtered out"
    );
}

/// An unrecognized frontmatter key is a HARD FAIL under the closed
/// [`OKF_RECOGNIZED_KEYS`] profile — never a silently-accepted ad-hoc
/// predicate.
#[test]
fn okf_dir_to_graph_hard_fails_on_unrecognized_frontmatter_key() {
    let tmp = tempfile::Builder::new()
        .prefix(".gmeow-test-okfin-bad-")
        .tempdir()
        .expect("tempdir");
    std::fs::write(
        tmp.path().join("concept.md"),
        "---\ntype: Class\nunrecognized_key: value\n---\nBody.\n",
    )
    .expect("write concept.md");

    let err = okf_dir_to_graph(tmp.path()).expect_err("unrecognized key must hard-fail");
    assert!(
        err.to_string().contains("unrecognized"),
        "expected an unrecognized-key error, got: {err}"
    );
}

#[test]
fn lift_maps_the_recognized_okf_subset() {
    let subject = RdfTerm::Iri("https://example.org/Dog".to_string());
    let source = vec![
        RdfQuad::new(
            subject.clone(),
            format!("{OKF_NS}type"),
            RdfTerm::Literal(RdfLiteral::simple("Class")),
        ),
        RdfQuad::new(
            subject.clone(),
            format!("{OKF_NS}title"),
            RdfTerm::Literal(RdfLiteral::simple("Dog")),
        ),
        RdfQuad::new(
            subject.clone(),
            format!("{OKF_NS}examples"),
            RdfTerm::Literal(RdfLiteral::simple("[\"Rex\", \"Fido\"]")),
        ),
        // An unmapped okf:* triple is retained verbatim.
        RdfQuad::new(
            subject.clone(),
            format!("{OKF_NS}path"),
            RdfTerm::Literal(RdfLiteral::simple("classes/Dog.md")),
        ),
        // The redundant okf:resource self-reference is dropped.
        RdfQuad::new(
            subject.clone(),
            format!("{OKF_NS}resource"),
            RdfTerm::Iri("https://example.org/Dog".to_string()),
        ),
    ];
    let (out, report) = lift_okf_graph(&source);

    // type→owl:Class, title→rdfs:label, two example items → skos:example.
    assert_eq!(report.lifted, 4, "type + title + 2 examples lifted");
    assert_eq!(report.retained, 1, "okf:path retained");
    assert_eq!(report.subjects, 1);

    let has = |p: &str, matcher: &dyn Fn(&RdfTerm) -> bool| {
        out.iter().any(|q| q.predicate == p && matcher(&q.object))
    };
    assert!(has(
        RDF_TYPE,
        &|o| matches!(o, RdfTerm::Iri(i) if i == OWL_CLASS)
    ));
    assert!(has(
        RDFS_LABEL,
        &|o| matches!(o, RdfTerm::Literal(l) if l.lexical_form == "Dog")
    ));
    let examples: Vec<&str> = out
        .iter()
        .filter(|q| q.predicate == SKOS_EXAMPLE)
        .filter_map(|q| match &q.object {
            RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(examples, vec!["Rex", "Fido"]);
    // The okf:resource identity triple never survives.
    assert!(
        !out.iter()
            .any(|q| q.predicate == format!("{OKF_NS}resource"))
    );
    // The okf:path annotation is retained verbatim.
    assert!(out.iter().any(|q| q.predicate == format!("{OKF_NS}path")));
}

#[test]
fn non_okf_triples_pass_through_unchanged() {
    let subject = RdfTerm::Iri("https://example.org/Dog".to_string());
    let source = vec![RdfQuad::new(
        subject,
        RDFS_LABEL,
        RdfTerm::Literal(RdfLiteral::simple("Dog")),
    )];
    let (out, report) = lift_okf_graph(&source);
    assert_eq!(report.lifted, 0);
    assert_eq!(report.retained, 0);
    assert_eq!(out.len(), 1, "the non-okf triple passes through");
}

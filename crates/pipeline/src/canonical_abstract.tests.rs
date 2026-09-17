// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const ABSTRACT: &str = "One canonical abstract with a \\\"quote\\\".";

#[test]
/// Turtle projection is byte-stable and leaves every unmanaged field unchanged.
fn turtle_projection_is_deterministic_and_field_local() {
    let input = "<x>\n    a owl:Ontology ;\n    dcterms:description \"old\"@en ;\n    dcterms:title \"kept\" .\n\n<y> a <Z> .\n";
    let once = replace_turtle_field(
        input,
        "ontology/gmeow.ttl",
        "<x>\n    a owl:Ontology ;\n",
        "dcterms:description",
        ABSTRACT,
    )
    .expect("projection succeeds");
    let twice = replace_turtle_field(
        &once,
        "ontology/gmeow.ttl",
        "<x>\n    a owl:Ontology ;\n",
        "dcterms:description",
        ABSTRACT,
    )
    .expect("second projection succeeds");
    assert_eq!(once, twice);
    assert!(once.contains("dcterms:title \"kept\""));
    assert!(once.contains("@x-gmeow-english"));
    assert_ne!(input.as_bytes(), once.as_bytes(), "drift is byte-visible");
}

#[test]
/// Subject-and-type anchoring prevents replacement in another matching resource.
fn turtle_projection_anchors_subject_and_type_together() {
    let input = "<target>\n    a gmeow:Work ;\n    skos:definition \"old\"@x-gmeow-english ;\n    rdfs:isDefinedBy <target> .\n\n<other>\n    a gmeow:Work ;\n    skos:definition \"kept\"@x-gmeow-english .\n";
    let projected = replace_turtle_field(
        input,
        "metadata/gmeow-self.ttl",
        "<target>\n    a gmeow:Work ;\n",
        "skos:definition",
        ABSTRACT,
    )
    .expect("targeted projection succeeds");

    assert!(projected.contains(&format!(
        "<target>\n    a gmeow:Work ;\n    skos:definition {} ;",
        turtle_literal(ABSTRACT)
    )));
    assert!(
        projected.contains(
            "<other>\n    a gmeow:Work ;\n    skos:definition \"kept\"@x-gmeow-english ."
        )
    );
}

#[test]
/// CFF projection removes the complete prior scalar and reaches a fixed point.
fn citation_projection_replaces_the_whole_folded_scalar_only() {
    let input = "title: kept\nabstract: >-\n  stale text\n  on two lines\ntype: dataset\n";
    let rendered = replace_citation_abstract(input, ABSTRACT).expect("projection succeeds");
    assert!(rendered.starts_with("title: kept\nabstract: \""));
    assert!(rendered.ends_with("\ntype: dataset\n"));
    assert!(!rendered.contains("stale text"));
    assert_eq!(
        rendered,
        replace_citation_abstract(&rendered, ABSTRACT).expect("fixed point")
    );
}

#[test]
/// Field-like prose inside the scalar cannot become a second YAML field match.
fn citation_projection_ignores_field_text_inside_the_abstract_value() {
    let value = "Canonical prose may literally discuss an abstract: field.";
    let input = "title: kept\nabstract: stale\ntype: dataset\n";
    let rendered = replace_citation_abstract(input, value).expect("projection succeeds");
    let reparsed: serde_yaml::Value =
        serde_yaml::from_str(&rendered).expect("projected CFF remains YAML");

    assert_eq!(reparsed["abstract"].as_str(), Some(value));
    assert_eq!(
        rendered,
        replace_citation_abstract(&rendered, value).expect("embedded text remains fixed point")
    );
}

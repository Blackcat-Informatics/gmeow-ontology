// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::parse_dataset;

const PREFIXES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

fn surface(ttl: &str) -> std::sync::Arc<RdfDataset> {
    parse_dataset(format!("{PREFIXES}{ttl}").as_bytes(), "text/turtle", None)
        .expect("fixture surface parses")
}

/// A `sh:sparql` procedural record (the production shape of both constraint
/// surfaces): the judgment re-derives to `logic:ValidationOnly` and the entry
/// carries the record subject, the formalized IRI, and the failure class.
#[test]
fn sparql_record_derives_validation_only() {
    let ds = surface(
        "gmeow:DemoConstraintShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:DemoLegacyShape ;\n\
                 gmeow:enforcesFailureClass gmeow:MissingRequiredProperty ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"demo\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/p> $this . }\"\"\" ] .\n",
    );
    let certs = derive_grounding_certificates(&[ds]).expect("derivable");
    assert_eq!(certs.len(), 1);
    let c = &certs[0];
    assert_eq!(
        c.record,
        "https://blackcatinformatics.ca/gmeow/DemoConstraintShape"
    );
    assert_eq!(
        c.formalizes,
        vec!["https://blackcatinformatics.ca/gmeow/DemoLegacyShape".to_owned()]
    );
    assert_eq!(
        c.failure_classes,
        vec!["https://blackcatinformatics.ca/gmeow/MissingRequiredProperty".to_owned()]
    );
    assert_eq!(c.preservation, PreservationKind::ValidationOnly);
    assert!(
        c.basis.contains("shacl#sparql"),
        "the basis names the closed-world construct: {}",
        c.basis
    );
}

/// A fully-declarative record (cardinality + `sh:class`): the lift/certify
/// round-trip re-derives its whole enforcement, so the judgment is
/// `logic:SoundUnderApproximation`.
#[test]
fn declarative_record_certifies_sound_under() {
    let ds = surface(
        "gmeow:DemoDeclShape a sh:NodeShape ;\n\
                 logic:formalizes logic:demoAxiom ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:property [ sh:path gmeow:p ; sh:minCount 1 ; sh:class gmeow:Other ] .\n",
    );
    let certs = derive_grounding_certificates(&[ds]).expect("derivable");
    assert_eq!(certs.len(), 1);
    assert_eq!(certs[0].preservation, PreservationKind::SoundUnder);
    assert!(
        certs[0].basis.contains("lift/certify"),
        "the basis records the certify round-trip: {}",
        certs[0].basis
    );
}

/// A record whose subject is NOT a readable `sh:NodeShape` is a hard error —
/// an underivable judgment is never a skipped entry.
#[test]
fn unreadable_record_is_an_error_not_a_skip() {
    let ds = surface("gmeow:NotAShape logic:formalizes gmeow:DemoLegacyShape .\n");
    let err = derive_grounding_certificates(&[ds]).expect_err("underivable");
    assert!(
        err.to_string().contains("NotAShape"),
        "the error names the underivable record: {err}"
    );
}

/// The same record subject on two surfaces is ambiguous — a hard error.
#[test]
fn duplicate_record_across_surfaces_is_an_error() {
    let block = "gmeow:DupShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:DemoLegacyShape ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"demo\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/p> $this . }\"\"\" ] .\n";
    let err = derive_grounding_certificates(&[surface(block), surface(block)])
        .expect_err("ambiguous record");
    assert!(
        err.to_string()
            .contains("more than one projected constraint surface"),
        "unexpected error: {err}"
    );
}

/// The renderer is a deterministic pure fold: entries ride in sorted record order,
/// one `logic:formalizes` line per formalized IRI, and re-rendering is byte-equal.
#[test]
fn renderer_is_deterministic_and_line_countable() {
    let ds = surface(
        "gmeow:BShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:LegacyB ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"b\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/p> $this . }\"\"\" ] .\n\
             gmeow:AShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:LegacyA ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"a\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/q> $this . }\"\"\" ] .\n",
    );
    let certs = derive_grounding_certificates(&[ds]).expect("derivable");
    assert_eq!(certs.len(), 2);
    assert!(
        certs[0].record < certs[1].record,
        "certificates are sorted by record IRI"
    );
    let a = render_grounding_ledger(&certs);
    let b = render_grounding_ledger(&certs);
    assert_eq!(a, b, "re-rendering is byte-identical");
    assert_eq!(
        a.matches("    logic:formalizes <").count(),
        2,
        "one logic:formalizes line per formalized IRI (count-consistent with the surfaces)"
    );
    assert_eq!(
        a.matches("logic:preservationKind").count(),
        2,
        "one judgment per record"
    );
    // The rendered document parses as Turtle.
    parse_dataset(a.as_bytes(), "text/turtle", None).expect("ledger parses");
}

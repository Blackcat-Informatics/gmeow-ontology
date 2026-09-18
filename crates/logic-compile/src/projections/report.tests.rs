// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn header(correspondence_count: usize, lawful_uplift_count: usize) -> ReportHeader {
    ReportHeader {
        axiom_count: 0,
        rule_count: 0,
        profile_count: 0,
        formula_count: 0,
        correspondence_count,
        lawful_uplift_count,
        claimed_uplift_count: 0,
    }
}

#[test]
fn projection_target_segment_encodes_embedded_fragment_delimiters() {
    let key =
        "sssom:http://rdfs.org/sioc/ns#UserAccount|http://www.w3.org/2000/01/rdf-schema#subClassOf";
    let encoded = iri_safe_segment(key);
    assert!(
        !encoded.contains('#'),
        "fragment delimiter leaked: {encoded}"
    );
    assert_eq!(encoded.matches("%23").count(), 2, "{encoded}");
}

/// Borrowed joins preserve source-attributed residue and the exact legacy fold,
/// including overlapping witnesses that the native ledger union deduplicates.
#[test]
fn compact_report_rows_preserve_complete_loss_union_and_counts() {
    use crate::ir::PreservationKind;
    let projection = ProjectionResult {
        target: "owl-dl".into(),
        content: "@prefix ex: <https://example.org/> . ex:s ex:p ex:o .".into(),
        is_rdf: true,
        preservation: PreservationKind::SoundUnder,
        complexity: "2NEXPTIME".into(),
    };
    let mapping = ProjectionResult {
        target: "sssom:cell".into(),
        content: "subject_id\tpredicate_id\tobject_id\nex:s\tex:p\tex:o\n".into(),
        is_rdf: false,
        preservation: PreservationKind::SoundUnder,
        complexity: "P".into(),
    };
    let mut compiler_loss = LossLedger::new();
    compiler_loss.record_projection_drops_attributed(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["standpoint projection".into()],
        &[(
            "authored context".into(),
            Some("https://example.org/source-a".into()),
        )],
    );
    let mut mapping_loss = compiler_loss.clone();
    mapping_loss.record_projection_drops_attributed(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["standpoint projection".into()],
        &[(
            "authored context".into(),
            Some("https://example.org/source-b".into()),
        )],
    );
    mapping_loss.record_projection_drops(
        "sssom:cell",
        PreservationKind::SoundUnder,
        &["leg body".into()],
        &["authored correspondence law is omitted".into()],
    );
    let mut merged = compiler_loss.clone();
    merged.union(&mapping_loss);
    let mut counts = header(7, 4).with_claimed_uplift(2);
    counts.formula_count = 3;
    let expected =
        build_projection_report_from(counts, &[projection.clone(), mapping.clone()], &merged)
            .unwrap();
    let compact = ProjectionReportRow::from(&projection);
    let actual = build_projection_report_rows(
        counts,
        [
            ProjectionReportRowRef::from(&compact),
            ProjectionReportRowRef::from(&mapping),
        ],
        &[&compiler_loss, &mapping_loss],
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert!(actual.contains("https://example.org/source-a"));
    assert!(actual.contains("https://example.org/source-b"));
    assert!(actual.contains("claimedUpliftCount"));
}

/// Compact rows retain both overclaim refusals without relying on output bodies.
#[test]
fn compact_report_rows_preserve_overclaim_and_residue_refusals() {
    use crate::ir::PreservationKind;
    let mut loss = LossLedger::new();
    loss.record_projection_drops(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["standpoint projection".into()],
        &[],
    );
    let row = ProjectionReportRow {
        target: "owl-dl".into(),
        is_rdf: true,
        preservation: PreservationKind::Exact,
        complexity: "2NEXPTIME".into(),
    };
    assert!(
        build_projection_report_rows(header(0, 0), [ProjectionReportRowRef::from(&row)], &[&loss],)
            .is_err()
    );
    let unsupported = ProjectionReportRow {
        preservation: PreservationKind::Unsupported,
        ..row
    };
    assert!(
        build_projection_report_rows(
            header(0, 0),
            [ProjectionReportRowRef::from(&unsupported)],
            &[&LossLedger::new()],
        )
        .is_err()
    );
}

#[test]
fn liftability_statistic_emitted_only_when_correspondences_present() {
    // The derived liftability statistic (3 of 4 lawful) appears in the ledger.
    let ttl = build_projection_report_from(header(4, 3), &[], &LossLedger::new()).expect("report");
    assert!(
        ttl.contains("correspondenceCount"),
        "expected correspondenceCount in:\n{ttl}"
    );
    assert!(
        ttl.contains("lawfulUpliftCount"),
        "expected lawfulUpliftCount in:\n{ttl}"
    );

    // A correspondence-free report is byte-unchanged (no statistic emitted).
    let empty =
        build_projection_report_from(header(0, 0), &[], &LossLedger::new()).expect("report");
    assert!(
        !empty.contains("correspondenceCount"),
        "a correspondence-free report must not emit the statistic:\n{empty}"
    );
    assert!(!empty.contains("lawfulUpliftCount"), "{empty}");
}

/// Shift-left for the A-Box annotation contract (`gmeow-errors::abox`): every
/// `logic:ProjectionTarget` and `logic:TermProjectionLoss` individual this report
/// mints carries all four mandatory annotations (`rdfs:label`, `skos:definition`,
/// `rdfs:isDefinedBy`, `gmeow:graphBoxRole`); the label/definition literals carry
/// the `x-gmeow-english` carrier tag (never bare `en`); and the label is the
/// human-readable complete correspondence identity, retained independently of loss.
///
/// `gmeow-logic-compile` has zero dependency on `gmeow-validate` (the reverse
/// dependency would cycle: `gmeow-validate` depends on this crate), so this parses
/// the emitted dataset directly and asserts on it, rather than driving
/// `gmeow_validate::lint::structural_lint_dataset` as the pipeline-level provenance/
/// evals tests do.
#[test]
fn projection_targets_and_term_losses_carry_the_full_abox_annotation_contract() {
    use crate::graphutil::{Node, Subject, nn, objects};
    use crate::ir::PreservationKind;
    use gmeow_errors::abox::{
        BOX_ABOX, GRAPH_BOX_ROLE, RDFS_IS_DEFINED_BY, RDFS_LABEL, SKOS_DEFINITION, X_GMEOW_ENGLISH,
    };

    // A source-owned target remains readable without inventing a loss note.
    let key = "fno:https://example.org/KnowsAboutMapping|get:full-semantic-digest";
    let target_name = key.to_owned();
    let source_term = "https://blackcatinformatics.ca/gmeow/knowsAbout".to_owned();
    let mut ledger = LossLedger::new();
    ledger.record_projection_drops_attributed(
        &target_name,
        PreservationKind::SoundUnder,
        &[],
        &[(
            "fno:hasParameter arity dropped".to_owned(),
            Some(source_term.clone()),
        )],
    );
    let proj = ProjectionResult {
        target: target_name.clone(),
        content: String::new(),
        is_rdf: false,
        preservation: PreservationKind::SoundUnder,
        complexity: "P".to_owned(),
    };

    let ttl = build_projection_report_from(header(0, 0), &[proj], &ledger).expect("report");
    let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("emitted report Turtle must parse");
    let ds = dataset.as_ref();

    let target_iri = format!("{LOGIC_NS}target/{}", iri_safe_segment(&target_name));
    let target_subject = Subject::Iri(target_iri.clone());

    // The label is the human-readable correspondence key, never the opaque hash,
    // and carries the x-gmeow-english carrier tag.
    let labels = objects(ds, &target_subject, &nn(RDFS_LABEL));
    assert_eq!(labels.len(), 1, "exactly one rdfs:label: {labels:?}");
    match &labels[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(lexical, key, "label must be the correspondence key");
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("rdfs:label must be a literal: {other:?}"),
    }
    assert_ne!(
        labels[0],
        Node::iri(target_name.clone()),
        "label must never be the opaque hash target name"
    );

    // skos:definition is present, carrier-tagged, and derived from the key +
    // preservation + complexity (never fabricated).
    let definitions = objects(ds, &target_subject, &nn(SKOS_DEFINITION));
    assert_eq!(
        definitions.len(),
        1,
        "exactly one skos:definition: {definitions:?}"
    );
    match &definitions[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(
                lexical,
                "Projection to fno:https://example.org/KnowsAboutMapping|get:full-semantic-digest: preservation \
                     SoundUnderApproximation, complexity P."
            );
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("skos:definition must be a literal: {other:?}"),
    }

    // rdfs:isDefinedBy points at the projection-ledger named graph this report is
    // folded into downstream.
    assert_eq!(
        objects(ds, &target_subject, &nn(RDFS_IS_DEFINED_BY)),
        vec![Node::iri(GRAPH_PROJECTION_LEDGER)],
        "rdfs:isDefinedBy must point at the projection-ledger graph"
    );

    // gmeow:graphBoxRole is the assertional-tier role every generated individual
    // carries.
    assert_eq!(
        objects(ds, &target_subject, &nn(GRAPH_BOX_ROLE)),
        vec![Node::iri(BOX_ABOX)],
        "graphBoxRole must be gmeow:boxABox"
    );

    // The reified TermProjectionLoss node carries the same four-annotation
    // contract, with a label/definition derived from the key + the DOCUMENTED
    // source term (never the opaque hash).
    let term_loss_iri = format!("{target_iri}/termloss/{}", iri_safe_segment(&source_term));
    let term_loss_subject = Subject::Iri(term_loss_iri);
    let term_labels = objects(ds, &term_loss_subject, &nn(RDFS_LABEL));
    assert_eq!(
        term_labels.len(),
        1,
        "exactly one term-loss rdfs:label: {term_labels:?}"
    );
    match &term_labels[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(lexical, &format!("{key}: loss of {source_term}"));
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("term-loss rdfs:label must be a literal: {other:?}"),
    }
    let term_definitions = objects(ds, &term_loss_subject, &nn(SKOS_DEFINITION));
    assert_eq!(term_definitions.len(), 1, "{term_definitions:?}");
    match &term_definitions[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(
                lexical,
                &format!("Term {source_term} is not preserved by the projection to {key}.")
            );
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("term-loss skos:definition must be a literal: {other:?}"),
    }
    assert_eq!(
        objects(ds, &term_loss_subject, &nn(RDFS_IS_DEFINED_BY)),
        vec![Node::iri(GRAPH_PROJECTION_LEDGER)]
    );
    assert_eq!(
        objects(ds, &term_loss_subject, &nn(GRAPH_BOX_ROLE)),
        vec![Node::iri(BOX_ABOX)]
    );

    // A whole-program logic target carries its own readable identity
    // falls back to its own already-human-readable `proj.target` as the label —
    // still carrier-tagged, still carrying all four annotations.
    let owl_dl_proj = ProjectionResult {
        target: "owl-dl".to_owned(),
        content: String::new(),
        is_rdf: false,
        preservation: PreservationKind::SoundUnder,
        complexity: "EL".to_owned(),
    };
    let ttl2 = build_projection_report_from(header(0, 0), &[owl_dl_proj], &LossLedger::new())
        .expect("report");
    let dataset2 = purrdf::parse_dataset(ttl2.as_bytes(), "text/turtle", None)
        .expect("emitted report Turtle must parse");
    let ds2 = dataset2.as_ref();
    let owl_dl_subject = Subject::Iri(format!("{LOGIC_NS}target/owl-dl"));
    let owl_dl_labels = objects(ds2, &owl_dl_subject, &nn(RDFS_LABEL));
    assert_eq!(owl_dl_labels.len(), 1, "{owl_dl_labels:?}");
    match &owl_dl_labels[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(lexical, "owl-dl");
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("fallback rdfs:label must be a literal: {other:?}"),
    }
    assert_eq!(
        objects(ds2, &owl_dl_subject, &nn(GRAPH_BOX_ROLE)),
        vec![Node::iri(BOX_ABOX)]
    );
}
#[test]
fn correspondence_identity_is_not_loss_and_cannot_discharge_preservation_gates() {
    use crate::ir::PreservationKind;
    let source = "https://example.org/exact-source";
    let mut loss = LossLedger::new();
    let row = crate::projections::correspondence_result(
        &mut loss,
        "canonical-rdf12",
        source,
        Vec::new(),
        Some(source.into()),
    );
    assert!(row.target.contains(source));
    assert_eq!(row.target.rsplit(':').next().unwrap().len(), 64);
    assert!(loss.projection_drops_for(&row.target).is_empty());
    assert!(loss.term_source_drops(&row.target).is_empty());
    let exact = build_projection_report_from(header(0, 0), &[row.clone()], &loss).unwrap();
    assert!(
        !exact.contains("lossyDrop"),
        "an identity is not an actual loss"
    );
    let mut unsupported = row;
    unsupported.preservation = PreservationKind::Unsupported;
    assert!(
        build_projection_report_from(header(0, 0), &[unsupported], &loss).is_err(),
        "a target label must not satisfy Unsupported residue admission"
    );
    let changed = crate::projections::correspondence_result(
        &mut loss,
        "canonical-rdf12",
        source,
        vec!["actual source value omitted".into()],
        Some(source.into()),
    );
    assert_eq!(
        loss.term_source_drops(&changed.target),
        vec![("actual source value omitted".into(), source.into())]
    );
    assert!(
        build_projection_report_from(header(0, 0), &[changed], &loss).is_err(),
        "Exact with actual loss must still fail"
    );
}

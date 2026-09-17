// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::model::{Finding, FindingCategory, Report, Severity};
use gmeow_errors::render::to_gmeow_rdf;

#[test]
fn native_meta_projection_appends_one_symmetric_witness_and_keeps_source_rows() {
    let root = "https://example.org/root".to_owned();
    let effect = "https://example.org/effect".to_owned();
    let opposed = "https://example.org/opposed".to_owned();
    let derivation = MetaDerivation {
        root_cause: BTreeSet::from([(effect.clone(), root.clone())]),
        cluster: BTreeSet::from([(effect.clone(), root.clone())]),
        cluster_root: BTreeSet::from([root.clone()]),
        cluster_typed: BTreeSet::from([root.clone()]),
        root_finding_typed: BTreeSet::from([root.clone()]),
        glut: BTreeSet::from([
            (effect.clone(), opposed.clone()),
            (opposed.clone(), effect.clone()),
        ]),
        ..MetaDerivation::default()
    };
    let graph = "https://example.org/diagnostics";
    let witness = glut_witness_iri(&effect, &opposed);
    let mut builder = RdfDatasetBuilder::new();
    let source = RdfQuad::new(
        RdfTerm::iri(&effect),
        "https://example.org/source",
        RdfTerm::iri(&root),
    );
    builder.push_owned_quad(&source);
    derivation.append_to(&mut builder, graph);
    let dataset = builder.freeze().unwrap();
    let rows: Vec<_> = dataset.owned_quads().collect();
    assert_eq!(
        rows.len(),
        19,
        "one source row, five public edges and thirteen annotated witness rows"
    );
    assert!(rows.contains(&source));
    assert!(
        rows.iter().any(|row| {
            row.subject == RdfTerm::iri(&witness)
                && row.predicate == gmeow_errors::abox::SKOS_DEFINITION
                && matches!(&row.object, RdfTerm::Literal(literal)
                    if literal.language.as_deref() == Some(gmeow_errors::abox::X_GMEOW_ENGLISH)
                        && literal.lexical_form.contains(&effect)
                        && literal.lexical_form.contains(&opposed))
        }),
        "the minted A-Box witness has a carrier-language definition"
    );
    for (subject, predicate, object) in [
        (&effect, FINDING_ROOT_CAUSE, root.as_str()),
        (&effect, FINDING_CLUSTER, root.as_str()),
        (&root, CLUSTER_ROOT, root.as_str()),
        (&root, RDF_TYPE, FINDING_CLUSTER_CLASS),
        (&root, RDF_TYPE, ROOT_FINDING_CLASS),
        (&witness, GLUT_WITNESS_OF, effect.as_str()),
        (&witness, GLUT_WITNESS_OF, opposed.as_str()),
        (&witness, FINDING_CATEGORY, FINDING_PERMITTED_CONFLICT),
        (&witness, FINDING_STANDPOINT, STANDPOINT_ADVISORY),
    ] {
        assert!(
            rows.contains(
                &RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
                    .in_graph(RdfTerm::iri(graph))
            ),
            "missing {subject} {predicate} {object}"
        );
    }
}

fn tiny_theory(extra: &str) -> CompiledTheory {
    let text = format!(
        r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            @prefix ex: <https://example.org/> .
            ex:selected a logic:Rule, gmeow:DiagnosticMetaRule ;
                logic:provenance ex:document ;
                logic:head [ rdf:subject ex:a ; rdf:predicate ex:selectedHead ; rdf:object ex:b ] .
            ex:other a logic:Rule ; logic:provenance ex:selected ;
                logic:head [ rdf:subject ex:a ; rdf:predicate ex:otherHead ; rdf:object ex:b ] .
            {extra}
        "#
    );
    let dataset = dataset_from_bytes(text.as_bytes(), NativeRdfFormat::Turtle).unwrap();
    PreparedLogicSource::new(&dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap()
}

#[test]
fn native_meta_derivation_retains_annotated_facts_and_trace_evidence() {
    let theory = tiny_theory(
        r#"
            ex:trace a logic:Rule, gmeow:DiagnosticMetaRule ;
                logic:head [ rdf:subject "?x" ; rdf:predicate gmeow:findingTraces ; rdf:object "?y" ] ;
                logic:body [ rdf:subject "?x" ; rdf:predicate ex:seed ; rdf:object "?y" ] .
        "#,
    );
    let meta = MetaProgram::from_compiled_theory(&theory).unwrap().unwrap();
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri("https://example.org/a");
    let predicate = builder.intern_iri("https://example.org/seed");
    let object = builder.intern_iri("https://example.org/b");
    let world = builder.intern_iri("https://example.org/world");
    builder.push_annotation_in_graph(subject, predicate, object, Some(world));
    let dataset = builder.freeze().unwrap();
    let derived = meta.derive_dataset(&dataset).unwrap();
    assert_eq!(
        derived.traces,
        BTreeSet::from([(
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned()
        )])
    );
    // Internal reachability evidence is retained without adding a public projection.
    assert!(derived.to_nquads(WORLD).is_empty());
    let encoded = serde_json::to_vec(&derived).unwrap();
    assert_eq!(
        serde_json::from_slice::<MetaDerivation>(&encoded).unwrap(),
        derived
    );
}

#[test]
fn compiled_meta_can_borrow_separate_wiring_without_changing_source_selection() {
    let theory = tiny_theory("");
    let wiring = dataset_from_bytes(
            b"<https://example.org/c> <https://blackcatinformatics.ca/gmeow/categoryPolarity> <https://example.org/p> .",
            NativeRdfFormat::Turtle,
        ).unwrap();
    let meta = MetaProgram::from_compiled_theory_with_wiring(&theory, &wiring)
        .unwrap()
        .unwrap();
    assert_eq!(meta.program.rules.len(), 1);
    assert_eq!(
        meta.category_polarity,
        vec![(
            "https://example.org/c".to_owned(),
            "https://example.org/p".to_owned()
        )]
    );
}

#[test]
fn compiled_meta_selection_uses_source_owners_instead_of_provenance() {
    let theory = tiny_theory("");
    let meta = MetaProgram::from_compiled_theory(&theory).unwrap().unwrap();
    assert_eq!(meta.program.rules.len(), 1);
    assert_eq!(
        meta.program.rules[0].head.predicate,
        "https://example.org/selectedHead"
    );
    assert_eq!(
        meta.program.rules[0].scope.provenance.as_deref(),
        Some("https://example.org/document")
    );
}

#[test]
fn compiled_meta_selection_rejects_a_partly_missing_fold() {
    for missing in ["ex:missing", "_:missing"] {
        let theory = tiny_theory(&format!("{missing} a gmeow:DiagnosticMetaRule ."));
        let error = MetaProgram::from_compiled_theory(&theory).err().unwrap();
        assert!(error.to_string().contains("1 of 2 selected"), "{error}");
    }
}

#[test]
fn compiled_meta_selection_sees_native_type_annotations() {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri("https://example.org/missing");
    let predicate = builder.intern_iri(RDF_TYPE);
    let class = builder.intern_iri(DIAGNOSTIC_META_RULE);
    builder.push_annotation(subject, predicate, class);
    let source = PreparedLogicSource::new(&builder.freeze().unwrap()).unwrap();
    let theory = source.into_compiled(None).unwrap();
    let error = MetaProgram::from_compiled_theory(&theory).err().unwrap();
    assert!(error.to_string().contains("1 of 1 selected"), "{error}");
}

/// A finding that already carries the ledger-witness identity fields the meta
/// rules join on (`finding_iri`, and optionally an antecedent edge / anchor).
fn witness_finding(
    iri: &str,
    code: &str,
    category: FindingCategory,
    antecedents: Vec<String>,
) -> Finding {
    let mut finding = Finding::new(Severity::Error, code, "boom")
        .with_tool("shacl")
        .with_category(category);
    finding.finding_iri = Some(iri.to_owned());
    finding.antecedents = antecedents;
    finding
}

fn finding_iri(local: &str) -> String {
    format!("{GMEOW_NS}diagnostics/finding/{local}")
}

#[test]
fn shared_antecedent_report_gains_root_cause_in_nq_and_on_findings() {
    let (root, effect_a, effect_b) = (
        finding_iri("rootF1"),
        finding_iri("effectF2"),
        finding_iri("effectF3"),
    );
    let mut report = Report::new("shacl");
    // Two effects that each derive from the ONE childless root.
    report.add_finding(witness_finding(
        &root,
        "discipline/relator-mediation",
        FindingCategory::ModelingDisciplineViolation,
        Vec::new(),
    ));
    report.add_finding(witness_finding(
        &effect_a,
        "shacl.MinCountConstraintComponent",
        FindingCategory::DataShapeViolation,
        vec![root.clone()],
    ));
    report.add_finding(witness_finding(
        &effect_b,
        "shacl.NodeKindConstraintComponent",
        FindingCategory::DataShapeViolation,
        vec![root.clone()],
    ));

    let projected = to_gmeow_rdf(&report);
    let observation = crate::stages::conformance::diagnostic_observations().reports["root"]
        .as_ref()
        .expect("producer-derived report meta-findings");
    assert_eq!(projected, observation.nquads);
    let derivation = &observation.derivation;

    // Both effects derive the shared childless root; the root itself derives none.
    assert!(
        derivation
            .root_cause
            .contains(&(effect_a.clone(), root.clone())),
        "effect A must derive gmeow:findingRootCause → root; got {:?}",
        derivation.root_cause
    );
    assert!(
        derivation
            .root_cause
            .contains(&(effect_b.clone(), root.clone())),
        "effect B must derive gmeow:findingRootCause → root"
    );

    // The derived nq carries the root-cause edge and the cluster grouping.
    let nq = derivation.to_nquads(GMEOW_NS);
    assert!(
        nq.contains(&format!("<{effect_a}> <{FINDING_ROOT_CAUSE}> <{root}>")),
        "derived nq must carry the findingRootCause edge:\n{nq}"
    );
    assert!(
        nq.contains(&format!("<{root}> <{RDF_TYPE}> <{FINDING_CLUSTER_CLASS}>")),
        "derived nq must type the shared root as a FindingCluster:\n{nq}"
    );

    // Enrichment lands the root cause + cluster on the effect findings.
    enrich_report(&mut report, derivation);
    let ea = report
        .findings
        .iter()
        .find(|f| f.finding_iri.as_deref() == Some(effect_a.as_str()))
        .expect("effect A present");
    assert_eq!(ea.root_cause.as_deref(), Some(root.as_str()));
    assert_eq!(ea.cluster.as_deref(), Some(root.as_str()));
    // The childless root has no root cause of its own.
    let r = report
        .findings
        .iter()
        .find(|f| f.finding_iri.as_deref() == Some(root.as_str()))
        .expect("root present");
    assert_eq!(r.root_cause, None);
}

#[test]
fn opposing_polarity_pair_mints_a_content_addressed_glut_witness() {
    let (supported, opposed) = (finding_iri("glutSupported"), finding_iri("glutOpposed"));
    let anchor = format!("{GMEOW_NS}diagnostics/anchor/shared0");

    // Two DIFFERENT-code findings at ONE non-trivial anchor whose category
    // polarities oppose (DataShapeViolation = Supported, PermittedEpistemicConflict
    // = Opposed).
    let mut supported_finding = Finding::new(
        Severity::Error,
        "shacl.MinCountConstraintComponent",
        "supported",
    )
    .with_tool("shacl")
    .with_category(FindingCategory::DataShapeViolation);
    supported_finding.finding_iri = Some(supported.clone());
    supported_finding.anchor_iri = Some(anchor.clone());
    supported_finding.anchor_non_trivial = true;

    let mut opposed_finding = Finding::new(
        Severity::Warning,
        "validate.deep.permitted-conflict",
        "opposed",
    )
    .with_tool("shacl")
    .with_category(FindingCategory::PermittedEpistemicConflict);
    opposed_finding.finding_iri = Some(opposed.clone());
    opposed_finding.anchor_iri = Some(anchor.clone());
    opposed_finding.anchor_non_trivial = true;

    let mut report = Report::new("shacl");
    report.add_finding(supported_finding);
    report.add_finding(opposed_finding);

    let projected = to_gmeow_rdf(&report);
    let observation = crate::stages::conformance::diagnostic_observations().reports["glut"]
        .as_ref()
        .expect("producer-derived report meta-findings");
    assert_eq!(projected, observation.nquads);
    let derivation = &observation.derivation;
    assert!(
        derivation
            .glut
            .contains(&(supported.clone(), opposed.clone())),
        "the opposing-polarity pair must derive gmeow:crossNodeGlutWith; got {:?}",
        derivation.glut
    );

    // The witness is materialized with a content-addressed IRI over the SORTED
    // pair + head predicate, and carries exactly two glutWitnessOf edges.
    let (lo, hi) = if supported <= opposed {
        (&supported, &opposed)
    } else {
        (&opposed, &supported)
    };
    let witness = glut_witness_iri(lo, hi);
    let nq = derivation.to_nquads(GMEOW_NS);
    assert!(
        nq.contains(&format!(
            "<{witness}> <{RDF_TYPE}> <{CROSS_NODE_GLUT_WITNESS_CLASS}>"
        )),
        "derived nq must mint the CrossNodeGlutWitness node:\n{nq}"
    );
    assert!(
        nq.contains(&format!("<{witness}> <{GLUT_WITNESS_OF}> <{supported}>"))
            && nq.contains(&format!("<{witness}> <{GLUT_WITNESS_OF}> <{opposed}>")),
        "the witness must link BOTH conflicting findings via glutWitnessOf:\n{nq}"
    );
    // The witness IRI is stable/deterministic and swap-invariant (content address).
    assert_eq!(witness, glut_witness_iri(hi, lo));
    // The witness carries its own well-formed grade (FindingShape).
    assert!(nq.contains(&format!(
        "<{witness}> <{FINDING_SEVERITY}> <{SEVERITY_NOTE}>"
    )));

    // Enrichment surfaces the symmetric glut edge on BOTH findings.
    enrich_report(&mut report, derivation);
    let s = report
        .findings
        .iter()
        .find(|f| f.finding_iri.as_deref() == Some(supported.as_str()))
        .unwrap();
    assert_eq!(s.cross_node_glut_with, vec![opposed.clone()]);
    let o = report
        .findings
        .iter()
        .find(|f| f.finding_iri.as_deref() == Some(opposed.as_str()))
        .unwrap();
    assert_eq!(o.cross_node_glut_with, vec![supported.clone()]);
}

#[test]
fn absent_meta_rules_yield_none() {
    // A source graph with polarity wiring but NO gmeow:DiagnosticMetaRule → None.
    let ttl = format!(
        "@prefix gmeow: <{GMEOW_NS}> .\n@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             logic:FindingDataShapeViolation gmeow:categoryPolarity logic:InfoSupported .\n"
    );
    let dataset =
        dataset_from_bytes(ttl.as_bytes(), NativeRdfFormat::Turtle).expect("parse minimal");
    assert!(
        MetaProgram::from_source_dataset(&dataset)
            .expect("a well-formed source parses")
            .is_none(),
        "a source without any gmeow:DiagnosticMetaRule must yield None"
    );
}

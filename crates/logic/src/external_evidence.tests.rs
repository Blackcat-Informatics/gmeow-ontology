// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn sentence(body: &str) -> FofSentence {
    FofSentence {
        name: "gmeow_problem_axiom".to_owned(),
        role: "axiom".to_owned(),
        body: body.to_owned(),
        source_units: BTreeSet::from(["source-unit".to_owned()]),
    }
}

fn artifact(kind: &str, text: &str) -> SzsArtifact {
    SzsArtifact {
        kind: kind.to_owned(),
        problem: Some("problem.p".to_owned()),
        channel: ArtifactChannel::Stdout,
        start_line: 2,
        end_line: 4,
        digest: blake3::hash(text.as_bytes()).to_hex().to_string(),
        bytes: text.len(),
        text: text.to_owned(),
    }
}

#[test]
fn framed_artifact_capture_is_exact_and_problem_bound() {
    let transcript = "% SZS status Satisfiable for problem.p\n\
                      % SZS output start FiniteModel for problem.p\n\
                      fof(domain,fi_domain,! [X] : X = d0).\n\
                      % SZS output end FiniteModel for problem.p\n";
    let artifacts =
        extract_szs_artifacts(transcript, "", &BTreeSet::from(["problem.p".to_owned()]))
            .expect("framed artifact");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "FiniteModel");
    assert_eq!(artifacts[0].text, "fof(domain,fi_domain,! [X] : X = d0).\n");
    assert_eq!(artifacts[0].start_line, 2);
    assert_eq!(artifacts[0].end_line, 4);
}

#[test]
fn artifact_problem_binding_accepts_the_exact_submitted_path_alias() {
    let transcript = "% SZS output start Proof for '/tmp/run/problem.p'\n\
                      fof(input,axiom,p(a)).\n\
                      % SZS output end Proof for '/tmp/run/problem.p'\n";
    let artifacts =
        extract_szs_artifacts(transcript, "", &BTreeSet::from(["problem.p".to_owned()]))
            .expect("the exact basename aliases the submitted path");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(
        artifacts[0].problem.as_deref(),
        Some("'/tmp/run/problem.p'")
    );
}

#[test]
fn foreign_and_unbalanced_artifact_markers_are_protocol_failures() {
    let foreign = extract_szs_artifacts(
        "% SZS output start Proof for other.p\n% SZS output end Proof for other.p\n",
        "",
        &BTreeSet::from(["problem.p".to_owned()]),
    )
    .expect_err("foreign problem");
    assert_eq!(
        gmeow_errors::code::code_str(foreign.code()),
        "FOREIGN_SZS_ARTIFACT_PROBLEM"
    );

    let unterminated = extract_szs_artifacts(
        "% SZS output start Proof for problem.p\nfof(a,axiom,p(a)).\n",
        "",
        &BTreeSet::from(["problem.p".to_owned()]),
    )
    .expect_err("unterminated artifact");
    assert_eq!(
        gmeow_errors::code::code_str(unterminated.code()),
        "UNTERMINATED_SZS_ARTIFACT"
    );
}

#[test]
fn structural_refutation_never_becomes_a_certificate_without_rule_replay() {
    let proof = artifact(
        "Proof",
        "fof(input, axiom, p(a), file('problem.p', gmeow_problem_axiom)).\n\
         cnf(done, plain, $false, inference(unchecked_rule, [], [input])).\n",
    );
    let check = check_external_artifact(&proof, &[sentence("p(a)")], "problem-digest");
    assert_eq!(check.disposition, ArtifactCheckDisposition::Structural);
    assert_eq!(check.code, "TSTP_STRUCTURE_CHECKED_INFERENCES_UNREPLAYED");
}

#[test]
fn foreign_refutation_leaf_is_invalid_even_with_a_false_terminal() {
    let proof = artifact(
        "Proof",
        "fof(input, axiom, q(a), file('problem.p', foreign)).\n\
         cnf(done, plain, $false, inference(unchecked_rule, [], [input])).\n",
    );
    let check = check_external_artifact(&proof, &[sentence("p(a)")], "problem-digest");
    assert_eq!(check.disposition, ArtifactCheckDisposition::Invalid);
    assert_eq!(check.code, "FOREIGN_TSTP_LEAF_FORMULA");
    let diagnostic = check.diagnostic.expect("typed refutation refusal");
    assert_eq!(
        diagnostic.evidence.operation,
        EvidenceOperation::RefutationParsing
    );
    assert_eq!(diagnostic.evidence.locus.as_deref(), Some("input"));
    assert_eq!(
        diagnostic.evidence.source_text.as_deref(),
        Some(proof.text.as_str())
    );
}

#[test]
fn vampire_single_sorted_model_is_independently_checked() {
    let model = artifact(
        "FiniteModel",
        "tff('declare_$i1',type,d0:$i).\n\
         tff('finite_domain_$i',axiom,! [X:$i] : (X = d0)).\n\
         tff(declare_a,type,a:$i).\n\
         tff(a_definition,axiom,a = d0).\n\
         tff(declare_p,type,p: $i > $o).\n\
         tff(predicate_p,axiom,p(d0)).\n",
    );
    let check = check_external_artifact(&model, &[sentence("p(a)")], "problem-digest");
    assert_eq!(check.disposition, ArtifactCheckDisposition::Certificate);
    assert_eq!(check.code, "FINITE_MODEL_CHECKED");
    assert!(check.evaluations > 0);
    assert_eq!(check.budget, Some(MODEL_EVALUATION_BUDGET));
}

#[test]
fn equal_property_objects_cannot_receive_a_split_predicate_countermodel() {
    use gmeow_logic_compile::frontend::{PreparedLogicSource, SourceAdmission, SourceSelection};
    use gmeow_logic_compile::tptp::{FofProjectionStatus, project_tptp_fof};

    let input = purrdf::parse_dataset(
        br#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <https://example.org/> .
        ex:p logic:sameAs ex:q . ex:subject ex:p ex:object .
        ex:negative a logic:Formula ; logic:not ex:atom .
        ex:atom a logic:Formula ; logic:relation ex:q ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:subject ],
                           [ logic:termIndex 1 ; logic:termIri ex:object ] .
    "#,
        "text/turtle",
        None,
    )
    .unwrap();
    let theory = PreparedLogicSource::new(&input)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let mut projection = project_tptp_fof(&theory, &admission);
    assert_eq!(projection.status, FofProjectionStatus::Complete);
    let negative = projection
        .sentences
        .iter()
        .find(|sentence| !sentence.source_units.is_empty() && sentence.body.starts_with("(~ "))
        .unwrap()
        .name
        .clone();
    // Keep every emitted premise. Evaluate source claims before profile laws so
    // rejection identifies the exact required negative claim, not an unrelated
    // defect of this deliberately adversarial model.
    projection
        .sentences
        .sort_by_key(|sentence| sentence.source_units.is_empty());
    let signature =
        Signature::from_problem(&parsed_problem(&projection.sentences).unwrap()).unwrap();
    let quote = |symbol: &str| format!("'{}'", symbol.replace('\\', "\\\\").replace('\'', "\\'"));
    let mut model = String::from(
        "fof(finite_domain,axiom,! [X] : (X = d0 | X = d1)).\n\
         fof(distinct_domain,axiom,d0 != d1).\n",
    );
    for (index, (symbol, arity)) in signature.functions.iter().enumerate() {
        assert_eq!(*arity, 0, "this source has no applied function terms");
        let value = if symbol.ends_with("/Nothing")
            || symbol.ends_with("#Nothing")
            || symbol == "I|https://example.org/subject"
            || symbol == "I|https://example.org/object"
        {
            "d1"
        } else {
            "d0"
        };
        model.push_str(&format!(
            "fof(c{index},axiom,{} = {value}).\n",
            quote(symbol)
        ));
    }
    for (index, (symbol, arity)) in signature.predicates.iter().enumerate() {
        for bits in 0..(1usize << arity) {
            let arguments: Vec<_> = (0..*arity)
                .map(|position| {
                    if bits & (1 << position) == 0 {
                        "d0"
                    } else {
                        "d1"
                    }
                })
                .collect();
            // The old per-name encoding could assign p and q different tables
            // while their object names denoted the same domain element.
            let truth = match symbol.as_str() {
                "P|2|https://example.org/q" => false,
                "P|2|https://blackcatinformatics.ca/logic/instanceOf" => arguments[1] == "d0",
                _ => true,
            };
            let application = if arguments.is_empty() {
                quote(symbol)
            } else {
                format!("{}({})", quote(symbol), arguments.join(","))
            };
            model.push_str(&format!(
                "fof(p{index}_{bits},axiom,{}{application}).\n",
                if truth { "" } else { "~ " }
            ));
        }
    }
    let artifact = artifact("FiniteModel", &model);
    let check = check_external_artifact(
        &artifact,
        &projection.sentences,
        projection.problem_digest.as_deref().unwrap(),
    );
    assert_eq!(
        check.disposition,
        ArtifactCheckDisposition::Invalid,
        "{check:?}"
    );
    assert_eq!(check.code, "FINITE_MODEL_FALSIFIES_PROBLEM", "{check:?}");
    assert_eq!(
        check.diagnostic.unwrap().evidence.locus.as_deref(),
        Some(negative.as_str())
    );
}

#[test]
fn model_that_falsifies_an_emitted_sentence_is_invalid() {
    let model = artifact(
        "FiniteModel",
        "tff('declare_$i1',type,d0:$i).\n\
         tff('finite_domain_$i',axiom,! [X:$i] : (X = d0)).\n\
         tff(declare_a,type,a:$i).\n\
         tff(a_definition,axiom,a = d0).\n\
         tff(declare_p,type,p: $i > $o).\n\
         tff(predicate_p,axiom,~p(d0)).\n",
    );
    let check = check_external_artifact(&model, &[sentence("p(a)")], "problem-digest");
    assert_eq!(check.disposition, ArtifactCheckDisposition::Invalid);
    assert_eq!(check.code, "FINITE_MODEL_FALSIFIES_PROBLEM");
    let diagnostic = check.diagnostic.expect("typed false-model refusal");
    assert_eq!(diagnostic.evidence.operation, EvidenceOperation::Evaluation);
    assert_eq!(
        diagnostic.evidence.locus.as_deref(),
        Some("gmeow_problem_axiom")
    );
    assert_eq!(
        diagnostic.evidence.source_text.as_deref(),
        Some(model.text.as_str())
    );
}

#[test]
fn partial_model_table_is_invalid_before_formula_evaluation() {
    let model = artifact(
        "FiniteModel",
        "tff('declare_$i1',type,d0:$i).\n\
         tff('declare_$i2',type,d1:$i).\n\
         tff('finite_domain_$i',axiom,! [X:$i] : (X = d0 | X = d1)).\n\
         tff('distinct_domain_$i',axiom,d0 != d1).\n\
         tff(declare_a,type,a:$i).\n\
         tff(a_definition,axiom,a = d0).\n\
         tff(declare_p,type,p: $i > $o).\n\
         tff(predicate_p,axiom,p(d0)).\n",
    );
    let check = check_external_artifact(&model, &[sentence("p(a)")], "problem-digest");
    assert_eq!(check.disposition, ArtifactCheckDisposition::Invalid);
    assert_eq!(check.code, "INVALID_FINITE_MODEL_TABLE");
    assert!(
        check.detail.contains("total interpretation"),
        "{}",
        check.detail
    );
}

#[test]
fn protocol_diagnostic_retains_channel_marker_and_expected_identity() {
    let marker = "% SZS output start Proof for foreign.p\n";
    let expected = BTreeSet::from(["problem.p".to_owned()]);
    let diagnostic = extract_szs_artifacts("", marker, &expected).expect_err("foreign problem");
    let payload = diagnostic
        .downcast_ref::<ArtifactProtocolError>()
        .expect("typed protocol cause");
    assert_eq!(payload.channel, Some(ArtifactChannel::Stderr));
    assert_eq!(payload.line, Some(1));
    assert_eq!(payload.source_text.as_deref(), Some(marker));
    assert_eq!(payload.expected_problems, expected);
    assert_eq!(
        diagnostic.grade(),
        Grade::new(
            gmeow_errors::Severity::Error,
            gmeow_errors::FindingCategory::ModelingDisciplineViolation,
            gmeow_errors::Standpoint::Binding,
        )
    );
    assert_eq!(diagnostic.inner().context[0].label, "Stderr line 1");
}

#[test]
fn malformed_model_retains_the_typed_parser_cause_and_serialized_evidence() {
    let source = "fof(domain,fi_domain, ! [X] : ).\n";
    let diagnostic = parse_model_document(source).expect_err("missing quantified body");
    assert_eq!(
        gmeow_errors::code::code_str(diagnostic.code()),
        "logic.external-evidence.model-parse"
    );
    let payload = diagnostic
        .downcast_ref::<ExternalEvidenceFailure>()
        .expect("typed checker cause");
    let cause = payload.cause.as_ref().expect("retained parser diagnostic");
    assert!(cause.is::<gmeow_math_lift::error::TstpParse>());
    assert_eq!(payload.evidence.source_text.as_deref(), Some(source));
    assert_eq!(
        payload.evidence.classification,
        EvidenceFailureClass::Invalid
    );
    let checked = check_external_artifact(
        &artifact("FiniteModel", source),
        &[sentence("p(a)")],
        "identity",
    );
    assert_eq!(checked.disposition, ArtifactCheckDisposition::Invalid);
    assert_eq!(checked.code, "MALFORMED_FINITE_MODEL");
    let evidence = checked.diagnostic.as_ref().expect("serialized diagnostic");
    assert_eq!(
        evidence.source_code.as_deref(),
        Some("math.lift.proof.parse")
    );
    assert_eq!(evidence.grade, diagnostic.grade());
    assert_eq!(
        evidence.source_failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/math/MalformedTSTPDerivation")
    );
    assert_eq!(evidence.failure_class, diagnostic.inner().failure_class);
    assert!(!evidence.causes.is_empty());
    assert_eq!(
        serde_json::to_value(&checked).unwrap()["diagnostic"]["source_failure_class"],
        "https://blackcatinformatics.ca/math/MalformedTSTPDerivation"
    );
    let first = serde_json::to_string(&checked).expect("report JSON");
    let repeated = check_external_artifact(
        &artifact("FiniteModel", source),
        &[sentence("p(a)")],
        "identity",
    );
    assert_eq!(
        first,
        serde_json::to_string(&repeated).expect("repeat JSON")
    );
}

#[test]
fn normalized_model_parse_failure_retains_original_and_exact_parsed_text() {
    let source = "% retained source line\n\
                  tff(declare_a,type,a:$i).\n\
                  tff(bad,axiom,! [X:$i] : ).\n";
    let diagnostic = parse_model_document(source).expect_err("missing quantified body");
    let payload = diagnostic
        .downcast_ref::<ExternalEvidenceFailure>()
        .expect("typed model parser refusal");
    assert_eq!(payload.evidence.source_text.as_deref(), Some(source));
    let parsed = payload
        .evidence
        .parsed_text
        .as_deref()
        .expect("exact normalized parser input");
    assert_ne!(parsed, source);
    assert!(!parsed.contains("declare_a"));
    assert!(parsed.contains("fof(bad"));
}

#[test]
fn model_table_representation_overflow_is_typed_resource_evidence() {
    let diagnostic = verify_total_table("wide", usize::MAX, 2, std::iter::empty::<&TableKey>())
        .expect_err("unrepresentable table cardinality");
    assert_eq!(
        gmeow_errors::code::code_str(diagnostic.code()),
        "logic.external-evidence.resource"
    );
    let payload = diagnostic
        .downcast_ref::<ExternalEvidenceFailure>()
        .expect("typed table resource refusal");
    assert_eq!(
        payload.evidence.classification,
        EvidenceFailureClass::Resource
    );
    assert_eq!(payload.evidence.operation, EvidenceOperation::ModelTable);
    assert_eq!(payload.evidence.locus.as_deref(), Some("wide"));
    assert!(payload.evidence.observed.is_some());
    assert!(payload.evidence.limit.is_some());
}

#[test]
fn unsupported_model_sort_retains_a_typed_refusal_instead_of_invalidity() {
    let source = "tff(finite_domain,axiom, ! [X:$int] : X = d0).\n";
    let checked = check_external_artifact(
        &artifact("FiniteModel", source),
        &[sentence("p(a)")],
        "identity",
    );
    assert_eq!(checked.disposition, ArtifactCheckDisposition::Unsupported);
    assert_eq!(checked.code, "UNSUPPORTED_FINITE_MODEL");
    let diagnostic = checked.diagnostic.expect("typed unsupported evidence");
    assert_eq!(diagnostic.code, "logic.external-evidence.unsupported");
    assert_eq!(
        diagnostic.evidence.classification,
        EvidenceFailureClass::Unsupported
    );
    assert_eq!(
        diagnostic.evidence.operation,
        EvidenceOperation::ModelParsing
    );
    assert_eq!(diagnostic.evidence.source_text.as_deref(), Some(source));
}

#[test]
fn model_budget_refusal_preserves_resource_class_usage_and_sentence_locus() {
    let source = "fof(domain,fi_domain, ! [X] : X = d0).\nfof(a_value,axiom,a = d0).\nfof(p_value,axiom,p(d0)).\n";
    let checked = check_finite_model_with_budget(
        &artifact("FiniteModel", source),
        &[sentence("p(a)")],
        "identity",
        1,
    );
    assert_eq!(checked.disposition, ArtifactCheckDisposition::Unsupported);
    assert_eq!(checked.code, "FINITE_MODEL_CHECK_BUDGET_EXHAUSTED");
    assert_eq!(checked.evaluations, 1);
    assert_eq!(checked.budget, Some(1));
    let diagnostic = checked.diagnostic.expect("resource diagnostic");
    assert_eq!(diagnostic.code, "logic.external-evidence.resource");
    assert_eq!(
        diagnostic.evidence.classification,
        EvidenceFailureClass::Resource
    );
    assert_eq!(diagnostic.evidence.operation, EvidenceOperation::Evaluation);
    assert_eq!(diagnostic.evidence.observed, Some(1));
    assert_eq!(diagnostic.evidence.limit, Some(1));
    assert_eq!(
        diagnostic.evidence.locus.as_deref(),
        Some("gmeow_problem_axiom")
    );
    assert_eq!(diagnostic.evidence.source_text.as_deref(), Some(source));
}

#[test]
fn domain_size_refusal_is_resource_evidence_not_a_false_model_claim() {
    let alternatives = (0..=MAX_MODEL_DOMAIN)
        .map(|index| format!("X = d{index}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let source = format!("fof(domain,fi_domain,! [X] : ({alternatives})).\n");
    let checked = check_external_artifact(
        &artifact("FiniteModel", &source),
        &[sentence("p(a)")],
        "identity",
    );
    assert_eq!(checked.disposition, ArtifactCheckDisposition::Unsupported);
    assert_eq!(checked.code, "FINITE_MODEL_DOMAIN_LIMIT");
    let diagnostic = checked.diagnostic.expect("resource evidence");
    assert_eq!(diagnostic.evidence.operation, EvidenceOperation::Domain);
    assert_eq!(
        diagnostic.evidence.classification,
        EvidenceFailureClass::Resource
    );
    assert_eq!(
        diagnostic.evidence.observed,
        Some(u64::try_from(MAX_MODEL_DOMAIN + 1).expect("small domain"))
    );
    assert_eq!(
        diagnostic.evidence.limit,
        Some(u64::try_from(MAX_MODEL_DOMAIN).expect("small limit"))
    );
}

#[test]
fn incomplete_table_diagnostic_retains_the_symbol_and_source() {
    let source = "fof(domain,fi_domain,! [X] : X = d0).\nfof(a_value,axiom,a = d0).\n";
    let checked = check_external_artifact(
        &artifact("FiniteModel", source),
        &[sentence("p(a)")],
        "identity",
    );
    assert_eq!(checked.code, "INVALID_FINITE_MODEL_TABLE");
    let diagnostic = checked.diagnostic.expect("typed table failure");
    assert_eq!(diagnostic.code, "logic.external-evidence.model-table");
    assert_eq!(
        diagnostic.evidence.classification,
        EvidenceFailureClass::Invalid
    );
    assert_eq!(diagnostic.evidence.locus.as_deref(), Some("p"));
    assert_eq!(diagnostic.evidence.source_text.as_deref(), Some(source));
    assert!(diagnostic.evidence.detail.contains("total interpretation"));
}

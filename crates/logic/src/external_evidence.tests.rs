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
    assert_eq!(foreign.code, "FOREIGN_SZS_ARTIFACT_PROBLEM");

    let unterminated = extract_szs_artifacts(
        "% SZS output start Proof for problem.p\nfof(a,axiom,p(a)).\n",
        "",
        &BTreeSet::from(["problem.p".to_owned()]),
    )
    .expect_err("unterminated artifact");
    assert_eq!(unterminated.code, "UNTERMINATED_SZS_ARTIFACT");
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

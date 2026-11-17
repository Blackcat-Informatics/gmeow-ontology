// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One verified native computation shared by the conjecture attestation test and producer.

use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};
use gmeow_logic::conjecture_eval::{ConjectureVerdictProjection, evaluate_conjecture_ttl};

gmeow_errors::define_diag_kind! {
    /// The selected witness does not meet its declared semantic or determinism contract.
    struct WitnessContract { detail: String }
    code = "wasm.reason.witness-contract";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "native reason witness contract: {}", detail;
}

/// The reified standpoint the demo verdicts are scoped to (Principle 9: never global).
pub const STANDPOINT: &str =
    "https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint";

/// Demo 1: a ground atom already asserted by the KB, so the proof leg fires.
pub const PROOF_FORMULA: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:phi a logic:Formula ;\n\
         logic:relation rdf:type ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";

pub const PROOF_KB: &str = "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:a rdf:type ex:B .\n";

/// Demo 2: a Horn candidate whose head forces a disjointness clash.
pub const REFUTE_FORMULA: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:cand a logic:Formula ;\n\
         logic:forall ex:body ;\n\
         logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
     ex:body a logic:Formula ;\n\
         logic:antecedent ex:ant ;\n\
         logic:consequent ex:con .\n\
     ex:ant a logic:Formula ;\n\
         logic:relation ex:trigger ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
     ex:con a logic:Formula ;\n\
         logic:relation rdf:type ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";

pub const REFUTE_KB: &str = "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     ex:a ex:trigger ex:mark .\n\
     ex:a rdf:type ex:A .\n\
     ex:A owl:disjointWith ex:B .\n";

/// Both verified legs and their complete artifact response attestation.
pub struct ConjectureWitness {
    pub proof: ConjectureVerdictProjection,
    pub refutation: ConjectureVerdictProjection,
    pub attestation: String,
}

#[must_use]
pub(super) fn contract_failure(message: impl Into<String>) -> Diag {
    Diag::of_kind(WitnessContract {
        detail: message.into(),
    })
}

fn evaluate(
    formula: &str,
    kb: &str,
    label: &str,
) -> gmeow_errors::Result<ConjectureVerdictProjection> {
    evaluate_conjecture_ttl(formula, kb, STANDPOINT)
        .map_err(|error| error.with_context(format!("{label} conjecture witness evaluation")))
}

/// Evaluate both legs, verify their intended semantics and determinism, and render the
/// exact attestation bytes. The producer calls this before writing, so it cannot bless an
/// error or a semantically empty verdict.
pub fn verified_witness() -> gmeow_errors::Result<ConjectureWitness> {
    let proof = evaluate(PROOF_FORMULA, PROOF_KB, "proof")?;
    if !proof.has_proof
        || proof.has_counterproof
        || proof.witness.is_some()
        || proof.lifecycle != "corroborated"
    {
        return Err(contract_failure(format!(
            "proof leg did not produce the corroborated contract: {proof:?}"
        )));
    }

    let refutation = evaluate(REFUTE_FORMULA, REFUTE_KB, "refutation")?;
    if !refutation.has_counterproof
        || refutation.witness.is_none()
        || refutation.lifecycle != "refuted-in-standpoint"
    {
        return Err(contract_failure(format!(
            "counterproof leg did not produce the refutation contract: {refutation:?}"
        )));
    }

    let repeated_proof = evaluate(PROOF_FORMULA, PROOF_KB, "repeated proof")?;
    if proof != repeated_proof {
        return Err(contract_failure(
            "conjecture proof witness is not deterministic",
        ));
    }
    let repeated_refutation = evaluate(REFUTE_FORMULA, REFUTE_KB, "repeated refutation")?;
    if refutation != repeated_refutation {
        return Err(contract_failure(
            "conjecture refutation witness is not deterministic",
        ));
    }

    let proof_artifact = gmeow_logic::result_rdf::admit_reasoning_response(&proof.artifact)?;
    let refutation_artifact =
        gmeow_logic::result_rdf::admit_reasoning_response(&refutation.artifact)?;
    let attestation =
        serde_json::json!({"proof": proof_artifact, "refutation": refutation_artifact}).to_string();
    Ok(ConjectureWitness {
        proof,
        refutation,
        attestation,
    })
}

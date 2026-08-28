// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One verified native computation shared by the conjecture attestation test and producer.

use gmeow_logic::conjecture_eval::{ConjectureVerdictProjection, evaluate_conjecture_ttl};

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

/// The exact byte delimiter shared with the browser parity lane.
pub const DELIMITER: &str =
    "# ── conjecture witness · counterproof leg ──────────────────────────────\n";

/// Both verified legs and the byte-exact joined attestation.
pub struct ConjectureWitness {
    pub proof: ConjectureVerdictProjection,
    pub refutation: ConjectureVerdictProjection,
    pub attestation: String,
}

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    std::io::Error::other(message.into()).into()
}

fn evaluate(
    formula: &str,
    kb: &str,
    label: &str,
) -> Result<ConjectureVerdictProjection, Box<dyn std::error::Error>> {
    evaluate_conjecture_ttl(formula, kb, STANDPOINT).map_err(|error| {
        fail(format!(
            "{label} conjecture evaluation failed: {}",
            error.message()
        ))
    })
}

/// Evaluate both legs, verify their intended semantics and determinism, and render the
/// exact attestation bytes. The producer calls this before writing, so it cannot bless an
/// error or a semantically empty verdict.
pub fn verified_witness() -> Result<ConjectureWitness, Box<dyn std::error::Error>> {
    let proof = evaluate(PROOF_FORMULA, PROOF_KB, "proof")?;
    if !proof.has_proof
        || proof.has_counterproof
        || proof.witness.is_some()
        || proof.lifecycle != "corroborated"
    {
        return Err(fail(format!(
            "proof leg did not produce the corroborated contract: {proof:?}"
        )));
    }

    let refutation = evaluate(REFUTE_FORMULA, REFUTE_KB, "refutation")?;
    if !refutation.has_counterproof
        || refutation.witness.is_none()
        || refutation.lifecycle != "refuted-in-standpoint"
    {
        return Err(fail(format!(
            "counterproof leg did not produce the refutation contract: {refutation:?}"
        )));
    }

    let repeated_proof = evaluate(PROOF_FORMULA, PROOF_KB, "repeated proof")?;
    if proof != repeated_proof {
        return Err(fail("conjecture proof witness is not deterministic"));
    }
    let repeated_refutation = evaluate(REFUTE_FORMULA, REFUTE_KB, "repeated refutation")?;
    if refutation != repeated_refutation {
        return Err(fail("conjecture refutation witness is not deterministic"));
    }

    let attestation = format!("{}{DELIMITER}{}", proof.verdict_nt, refutation.verdict_nt);
    if attestation
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(fail(
            "conjecture attestation contains a raw control scalar instead of an RDF UCHAR escape",
        ));
    }
    purrdf::parse_dataset(attestation.as_bytes(), "application/n-triples", None).map_err(
        |error| {
            fail(format!(
                "conjecture attestation is not valid N-Triples: {error}"
            ))
        },
    )?;
    Ok(ConjectureWitness {
        proof,
        refutation,
        attestation,
    })
}

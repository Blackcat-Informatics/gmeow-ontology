// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal reporting only: no report can be hydrated into a native witness.

use super::*;
use purrdf_lex::json::{Format, Object, Value, record::ToJson};

impl SourceMerges {
    /// Report the complete finite result, sharing original formula/evidence
    /// definitions by source identity. Native datasets are referenced through the
    /// exact compiler publication's document receipts and the producer action's
    /// authenticated source dependency, never copied into intermediate JSON.
    /// This artifact is a description, not a persisted optimization certificate.
    ///
    /// # Errors
    /// Retains native storage failures while rendering the complete pretty report.
    pub fn report(&self) -> gmeow_errors::Result<Vec<u8>> {
        let body_names: BTreeMap<_, _> = self
            .bodies
            .iter()
            .map(|(name, body)| (Arc::as_ptr(body) as usize, name))
            .collect();
        let evidence_names: BTreeMap<_, _> = self
            .evidence
            .iter()
            .map(|(name, evidence)| (Arc::as_ptr(evidence) as usize, name))
            .collect();
        let bodies = Object::from(
            self.bodies
                .iter()
                .map(|(name, body)| (name.clone(), body.formula().as_ref().to_json()))
                .collect::<Vec<_>>(),
        );
        let evidence = Object::from(
            self.evidence
                .iter()
                .map(|(name, evidence)| {
                    (
                        name.clone(),
                        Object::new()
                            .with("origin", crate::term_serde::to_json(evidence.origin()))
                            .with("loss", evidence.loss().to_json())
                            .with("provenance", "source-publication")
                            .with("complement", "source-publication")
                            .into(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let merges = Object::from(
            self.merges
                .iter()
                .map(|(name, merge)| {
                    let output = merge.output();
                    let symbols = Value::Array(
                        output
                            .symbols()
                            .iter()
                            .map(|symbol| {
                                Object::new()
                                    .with("context", symbol.context.to_json())
                                    .with(
                                        "names",
                                        Value::Array(
                                            symbol
                                                .names
                                                .iter()
                                                .map(crate::term_serde::to_json)
                                                .collect(),
                                        ),
                                    )
                                    .with("roles", symbol.roles.to_json())
                                    .into()
                            })
                            .collect(),
                    );
                    let sentences = Value::Array(
                        output
                            .sentences()
                            .iter()
                            .map(|sentence| {
                                Object::new()
                                    .with(
                                        "body",
                                        body_names[&(Arc::as_ptr(&sentence.body) as usize)]
                                            .to_json(),
                                    )
                                    .with("bindings", sentence.bindings.to_json())
                                    .with("context", sentence.context.to_json())
                                    .with("kind", sentence.kind.to_json())
                                    .with("sign", sentence.sign.to_json())
                                    .with(
                                        "evidence",
                                        evidence_names[&(Arc::as_ptr(&sentence.evidence) as usize)]
                                            .to_json(),
                                    )
                                    .into()
                            })
                            .collect(),
                    );
                    let source_name = |presentation: &Arc<FinitePresentation>| {
                        &self.presentations[&(Arc::as_ptr(presentation) as usize)]
                    };
                    (
                        name.clone(),
                        Object::new()
                            .with("engine", merge.engine_descriptor())
                            .with(
                                "apex",
                                source_name(merge.left_embedding().source()).to_json(),
                            )
                            .with(
                                "left",
                                source_name(merge.left_injection().source()).to_json(),
                            )
                            .with(
                                "right",
                                source_name(merge.right_injection().source()).to_json(),
                            )
                            .with("left_embedding", merge.left_embedding().images().to_json())
                            .with(
                                "right_embedding",
                                merge.right_embedding().images().to_json(),
                            )
                            .with("left_injection", merge.left_injection().images().to_json())
                            .with(
                                "right_injection",
                                merge.right_injection().images().to_json(),
                            )
                            .with("contexts", output.contexts().to_json())
                            .with("contract", output.contract().as_ref().to_json())
                            .with("symbols", symbols)
                            .with("sentences", sentences)
                            .with("square", "checked")
                            .with("factorization", "unique-for-every-admitted-cocone")
                            .into(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let documents: Vec<_> = self
            .source
            .source()
            .origins()
            .iter()
            .map(|origin| &origin.document)
            .collect();
        let mut report = Value::from(
            Object::new()
                .with("schema", "native-presentation-merges-v1")
                .with("witness_scope", "invocation-local")
                .with("source_documents", documents.to_json())
                .with("source_iri", self.source.program().source_iri.to_json())
                .with("bodies", Value::Object(bodies))
                .with("evidence", Value::Object(evidence))
                .with("merges", Value::Object(merges)),
        );
        report.sort_keys();
        let mut resident = purrdf_lex::allocation::Resident;
        let mut memory = purrdf_lex::allocation::Memory::new(&mut resident);
        purrdf_lex::json::write_with_memory(&report, Format::PRETTY, &mut memory)
            .map(String::into_bytes)
            .map_err(|error| {
                gmeow_errors::Diag::from(error).with_context("serialize presentation merge report")
            })
    }
}

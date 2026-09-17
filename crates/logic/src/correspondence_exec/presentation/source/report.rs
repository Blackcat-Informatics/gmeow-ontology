// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal reporting only: no report can be hydrated into a native witness.

use super::*;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
struct NativeTerm<'a>(#[serde(with = "crate::term_serde")] &'a TermValue);

impl SourceMerges {
    /// Report the complete finite result, sharing original formula/evidence
    /// definitions by source identity. Native datasets are referenced through the
    /// exact compiler publication's document receipts and the producer action's
    /// authenticated source dependency, never copied into intermediate JSON.
    /// This artifact is a description, not a persisted optimization certificate.
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
        let bodies: BTreeMap<_, _> = self
            .bodies
            .iter()
            .map(|(name, body)| (name, body.formula().as_ref()))
            .collect();
        let evidence: BTreeMap<_, _> = self
            .evidence
            .iter()
            .map(|(name, evidence)| {
                (
                    name,
                    json!({
                        "origin": NativeTerm(evidence.origin()), "loss": evidence.loss(),
                        "provenance": "source-publication", "complement": "source-publication",
                    }),
                )
            })
            .collect();
        let merges: BTreeMap<_, _> = self.merges.iter().map(|(name, merge)| {
            let output = merge.output();
            let symbols: Vec<_> = output.symbols().iter().map(|symbol| json!({
                "context": symbol.context, "names": symbol.names.iter().map(NativeTerm).collect::<Vec<_>>(), "roles": symbol.roles,
            })).collect();
            let sentences: Vec<_> = output.sentences().iter().map(|sentence| json!({
                "body": body_names[&(Arc::as_ptr(&sentence.body) as usize)],
                "bindings": sentence.bindings, "context": sentence.context,
                "kind": sentence.kind, "sign": sentence.sign,
                "evidence": evidence_names[&(Arc::as_ptr(&sentence.evidence) as usize)],
            })).collect();
            let source_name = |presentation: &Arc<FinitePresentation>| &self.presentations[&(Arc::as_ptr(presentation) as usize)];
            (name, json!({
                "engine": merge.engine_descriptor(),
                "apex": source_name(merge.left_embedding().source()),
                "left": source_name(merge.left_injection().source()),
                "right": source_name(merge.right_injection().source()),
                "left_embedding": merge.left_embedding().images(),
                "right_embedding": merge.right_embedding().images(),
                "left_injection": merge.left_injection().images(),
                "right_injection": merge.right_injection().images(),
                "contexts": output.contexts(), "contract": output.contract().as_ref(),
                "symbols": symbols, "sentences": sentences,
                "square": "checked", "factorization": "unique-for-every-admitted-cocone",
            }))
        }).collect();
        let documents: Vec<_> = self
            .source
            .source()
            .origins()
            .iter()
            .map(|origin| &origin.document)
            .collect();
        serde_json::to_vec_pretty(&json!({
            "schema": "native-presentation-merges-v1", "witness_scope": "invocation-local",
            "source_documents": documents, "source_iri": self.source.program().source_iri,
            "bodies": bodies, "evidence": evidence, "merges": merges,
        }))
        .map_err(|error| super::error(format!("serialize presentation merge report: {error}")))
    }
}

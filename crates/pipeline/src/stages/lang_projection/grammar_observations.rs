// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native grammar observations captured from the same prepared sources and target emissions.
//! Corpus equality is evidence about these inputs, never general rewrite authorization.

use std::collections::BTreeMap;

use gmeow_lang_bridge::registry::{GrammarSource, LangEmission};
use gmeow_lang_bridge::{Formalism, Grammar, IngestDiagnostic, parse_grammar, serialize_grammar};
use gmeow_logic_compile::ir::{Correspondence, LegPath, PreservationKind};
use serde::{Deserialize, Serialize};

pub(super) const CHANNEL: &str = "pipeline/lang-grammar-observations.json";

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Observations {
    pub grammars: BTreeMap<String, GrammarObservation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GrammarObservation {
    pub source_iri: String,
    pub source_rule_count: usize,
    pub canonical: Grammar,
    pub raw_reparsed: Result<Grammar, IngestDiagnostic>,
    pub emissions: BTreeMap<String, Vec<Emission>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Emission {
    pub correspondence: Correspondence,
    pub leg_pair: Option<(LegPath, LegPath)>,
    pub round_trip_holds: bool,
    pub lossy_kind: PreservationKind,
    pub unsupported: Vec<String>,
    pub paths: Vec<String>,
    pub ebnf_reparsed: Vec<Result<Grammar, IngestDiagnostic>>,
}

impl Observations {
    pub(super) fn new(sources: &[GrammarSource]) -> Self {
        Self {
            grammars: sources
                .iter()
                .map(|source| {
                    let serialized = serialize_grammar(source.parsed());
                    let raw_reparsed = parse_grammar(serialized.as_bytes(), Formalism::Ebnf)
                        .map(|grammar| grammar.canonicalize());
                    (
                        source.name.clone(),
                        GrammarObservation {
                            source_iri: source.source_iri().to_owned(),
                            source_rule_count: source.parsed().rules.len(),
                            canonical: source.canonical().clone(),
                            raw_reparsed,
                            emissions: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
        }
    }

    pub(super) fn record(
        &mut self,
        target: &str,
        emission: &LangEmission,
    ) -> gmeow_errors::Result<()> {
        if !matches!(target, "ebnf" | "abnf" | "gbnf" | "lark") {
            return Ok(());
        }
        let mut ebnf_reparsed = Vec::new();
        for artifact in &emission.artifacts {
            let reparse = emission.grammar_reparse.as_ref().ok_or_else(|| {
                super::stage_err("selected grammar emission has no native reparse")
            })?;
            let digest = blake3::hash(&artifact.bytes).to_hex().to_string();
            if reparse.emitted_digest != digest {
                return Err(super::stage_err(format!(
                    "native grammar reparse does not match artifact {}",
                    artifact.path_suffix
                )));
            }
            if artifact.path_suffix.starts_with("ebnf/") {
                ebnf_reparsed.push(reparse.grammar.clone());
            }
        }
        for observed in self
            .grammars
            .values_mut()
            .filter(|grammar| grammar.source_iri == emission.source_iri)
        {
            observed
                .emissions
                .entry(target.to_owned())
                .or_default()
                .push(Emission {
                    correspondence: emission.correspondence.clone(),
                    leg_pair: emission.leg_pair.clone(),
                    round_trip_holds: emission.round_trip_holds,
                    lossy_kind: emission.lossy_kind,
                    unsupported: emission.unsupported.clone(),
                    paths: emission
                        .artifacts
                        .iter()
                        .map(|artifact| artifact.path_suffix.clone())
                        .collect(),
                    ebnf_reparsed: ebnf_reparsed.clone(),
                });
        }
        Ok(())
    }
}

#[path = "grammar_observations.tests.rs"]
#[cfg(test)]
mod tests;

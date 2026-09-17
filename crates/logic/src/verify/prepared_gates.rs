// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Immutable native gate laws, with exact authored identities and lowering evidence.
//! Source preparation is explicit producer work. Consumers hydrate this value without
//! parsing RDF, recompiling formulas, or strengthening any preservation claim.

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::shared;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use gmeow_logic_compile::frontend::{CompiledTheory, Diagnostic};
use purrdf::RdfDataset;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::reason::{enactment, math_gate};
use crate::relational_core::ViolationLowering;
use crate::rule_ir::EvalRule;

/// Compact authenticated source artifact containing the selected native law preparation.
pub const PREPARED_GATES_CHANNEL: &str = "pipeline/prepared-verify-gates.json";
/// Canonical source documents and the source IRIs used by their standalone compilers.
pub const GATE_SOURCES: [(&str, &str); 2] = [
    (
        "slices/grounding/math/module.ttl",
        math_gate::MATH_MODULE_SOURCE_IRI,
    ),
    (
        "slices/grounding/logic/module.ttl",
        enactment::LOGIC_MODULE_SOURCE_IRI,
    ),
];

/// Native compiled rules and all lowering evidence needed by reasoned-graph gates.
/// The producer-selected action authenticates this entire value; its native fields
/// remain private so a consumer cannot substitute an unrelated law inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedReasonedGates {
    schema_version: u32,
    source_digests: BTreeMap<String, String>,
    pub(crate) math_rules: Vec<EvalRule>,
    pub(crate) enactment: ViolationLowering,
    pub(crate) math_compilation: MathCompilation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MathCompilation {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) constraints: Vec<MathConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MathConstraint {
    pub(crate) iri: String,
    pub(crate) target: String,
    pub(crate) failure_class: Option<String>,
    pub(crate) projected_sparql: String,
}

fn fail(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Verify {
        detail: detail.into(),
    })
}

impl PreparedReasonedGates {
    /// Prepare native gates from the caller's shared, standalone module compilations.
    /// No source loading, parsing or second frontend lowering occurs here.
    ///
    /// # Errors
    /// Rejects a source identity disagreement, wrong source IRI or unenforced law.
    pub fn from_compiled_sources(
        math_source: &RdfDataset,
        math: &CompiledTheory,
        math_digest: &str,
        logic: &CompiledTheory,
        logic_digest: &str,
    ) -> gmeow_errors::Result<Self> {
        for (compiled, (_, source_iri)) in [math, logic].into_iter().zip(GATE_SOURCES) {
            if compiled.program().source_iri.as_deref() != Some(source_iri) {
                return Err(fail(format!(
                    "prepared gate source IRI must be {source_iri}"
                )));
            }
        }
        let prepared = Self {
            schema_version: 1,
            source_digests: BTreeMap::from([
                (GATE_SOURCES[0].0.to_owned(), math_digest.to_owned()),
                (GATE_SOURCES[1].0.to_owned(), logic_digest.to_owned()),
            ]),
            math_rules: math_gate::prepare_rules(math_source, math.program())?,
            enactment: enactment::prepare_rules(logic.program())?,
            math_compilation: MathCompilation {
                diagnostics: math.diagnostics().to_vec(),
                constraints: math
                    .program()
                    .constraints
                    .iter()
                    .map(|constraint| MathConstraint {
                        iri: constraint.iri.clone(),
                        target: format!("{:?}", constraint.target),
                        failure_class: constraint.failure_class.clone(),
                        projected_sparql:
                            gmeow_logic_compile::projections::shapes::project_procedural_constraint(
                                constraint,
                            ),
                    })
                    .collect(),
            },
        };
        prepared.validate_source_identity()?;
        Ok(prepared)
    }

    /// Exact identities of the two selected authored modules.
    #[must_use]
    pub fn source_digests(&self) -> &BTreeMap<String, String> {
        &self.source_digests
    }

    /// Check hydration against this executable's embedded canonical source identity.
    /// This only hashes bytes; it never parses or compiles a source document.
    ///
    /// # Errors
    /// Rejects unknown record schemas or preparation from different source bytes.
    pub fn validate_source_identity(&self) -> gmeow_errors::Result<()> {
        if self.schema_version != 1 || self.source_digests != embedded_source_digests() {
            return Err(fail(
                "prepared verify gates differ from this executable's authored source identities",
            ));
        }
        Ok(())
    }
}

fn embedded_source_digests() -> BTreeMap<String, String> {
    [math_gate::MATH_MODULE_TTL, enactment::LOGIC_MODULE_TTL]
        .into_iter()
        .zip(GATE_SOURCES)
        .map(|(text, (path, _))| {
            (
                path.to_owned(),
                format!("{:x}", Sha256::digest(text.as_bytes())),
            )
        })
        .collect()
}

/// The repository-free consumer wrapper has one immutable embedded source identity.
/// Producer callers use their explicitly retained value instead; test builds hydrate
/// the runner-selected source artifact and may never fall back to source compilation.
#[cfg(not(test))]
pub(crate) fn shared() -> &'static PreparedReasonedGates {
    static PREPARED: OnceLock<PreparedReasonedGates> = OnceLock::new();
    PREPARED.get_or_init(|| prepare_embedded().expect("embedded native verify laws must prepare"))
}

#[cfg(not(test))]
fn prepare_embedded() -> gmeow_errors::Result<PreparedReasonedGates> {
    use gmeow_logic_compile::frontend::PreparedLogicSource;
    let compile = |text: &str, source_iri: &str| {
        let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None)
            .map_err(|error| fail(error.to_string()))?;
        let compiled = PreparedLogicSource::new(&dataset)
            .map_err(|error| fail(error.to_string()))?
            .into_compiled(Some(source_iri.to_owned()))
            .map_err(|error| fail(error.to_string()))?;
        Ok::<_, gmeow_errors::Diag>((dataset, compiled))
    };
    let (math_source, math) = compile(math_gate::MATH_MODULE_TTL, GATE_SOURCES[0].1)?;
    let (_, logic) = compile(enactment::LOGIC_MODULE_TTL, GATE_SOURCES[1].1)?;
    let digests = embedded_source_digests();
    PreparedReasonedGates::from_compiled_sources(
        &math_source,
        &math,
        &digests[GATE_SOURCES[0].0],
        &logic,
        &digests[GATE_SOURCES[1].0],
    )
}

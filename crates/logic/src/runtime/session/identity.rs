// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The content-addressed seven-axis [`SessionIdentity`].
//!
//! One `descriptor_hash` folds every identity a maintained session depends on, so a
//! checkpoint minted under one world/engine/contract/algebra refuses restore under a
//! drifted one (mirrors [`crate::runtime::EngineContract`], strictly finer than it).

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, LogicProgram, ReasoningContract};
use purrdf::RdfDataset;

use crate::annotation::AnnotationContract;
use crate::runtime::EngineContract;
use crate::runtime::frame;
use crate::seam::WorldSourceIdentity;

use super::SESSION_SOURCE_CONTRACT;

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Frame `fields` under `domain` with the shared domain-tagged, length-prefixed
/// BLAKE3 discipline and return the 64-hex address. Each field is length-prefixed, so
/// no field boundary can collide with another.
pub(crate) fn digest(domain: &[u8], fields: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"domain", domain);
    for field in fields {
        frame(&mut hasher, b"field", field.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Mint the authorized-EDB data-generation identity by content-hashing the EDB facts.
///
/// The facts are built through the SAME production typed-EDB bridge
/// ([`crate::reason::build_edb_facts`]) the incremental session prepares from, then
/// rendered as sorted `(predicate, term-N3…)` rows and framed. Because the rendering is
/// a deterministic function of the dataset's fact set, `open` and `restore` mint the
/// identical generation from the identical authorized EDB — the invariant `restore`
/// relies on to gate the data-generation axis.
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built or a term cannot be rendered to N3.
pub(crate) fn mint_edb_generation(edb: &RdfDataset) -> gmeow_errors::Result<WorldSourceIdentity> {
    let hex = dataset_content_digest(b"gmeow-logic-session-edb-generation-v1", edb)?;
    Ok(WorldSourceIdentity::new(
        format!("urn:blake3:{hex}"),
        SESSION_SOURCE_CONTRACT,
    ))
}

/// The deterministic, order-independent sorted `(predicate, term-N3…)` rows of a
/// dataset, built through the same production typed-EDB bridge the maintainer uses.
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built or a term cannot be rendered to N3.
pub(crate) fn dataset_rows(edb: &RdfDataset) -> gmeow_errors::Result<Vec<Vec<String>>> {
    let typed = crate::reason::build_edb_facts(edb)?;
    let interner = typed.interner();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for fact in typed.facts() {
        let mut row = Vec::with_capacity(fact.args.len() + 1);
        row.push(fact.predicate.clone());
        for &arg in &fact.args {
            row.push(crate::provenance::term_n3(interner.resolve(arg))?);
        }
        rows.push(row);
    }
    rows.sort();
    Ok(rows)
}

/// Frame a dataset's canonical sorted rows under `domain`, returning the 64-hex
/// content address (order-independent; deterministic for a given fact set).
///
/// # Errors
///
/// Returns `Err` if the dataset cannot be built or rendered.
pub(crate) fn dataset_content_digest(
    domain: &[u8],
    edb: &RdfDataset,
) -> gmeow_errors::Result<String> {
    let rows = dataset_rows(edb)?;
    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"domain", domain);
    for row in rows {
        for field in row {
            frame(&mut hasher, b"cell", field.as_bytes());
        }
        frame(&mut hasher, b"row-end", b"");
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// The content-addressed binding of the seven identities a maintained session depends
/// on. Construct only via [`SessionIdentity::bind`]; `#[non_exhaustive]` so adding an
/// axis is an additive change.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct SessionIdentity {
    /// (1) The authorized published data-generation of the EDB.
    pub data_generation: WorldSourceIdentity,
    /// (2) Framed digest of the canonical [`LogicProgram`] (rule/program surface).
    pub program_hash: String,
    /// (2) Framed digest of the program's slice/rule provenance (see [`Self::bind`]).
    pub slice_hash: String,
    /// (3 + resource policy) The [`ReasoningContract::content_digest`].
    pub contract_hash: String,
    /// (4) The whole-engine descriptor ([`EngineContract::current`]).
    pub engine_descriptor_hash: String,
    /// (5) The tuple-annotation admission contract identity
    /// ([`AnnotationContract::canonical_key`]).
    pub annotation_identity: String,
    /// (6) The incrementally-certified fragment name.
    pub fragment: String,
    /// Framed-BLAKE3 content address over every axis above — the value a checkpoint
    /// pins and `restore` gates on.
    pub descriptor_hash: String,
}

impl SessionIdentity {
    /// Bind the seven identities into one content address.
    ///
    /// `slice_hash` derivation: when the program carries a `source_iri` provenance, the
    /// slice digest frames that IRI; otherwise it frames the program's canonical
    /// rendering under a DISTINCT domain tag from `program_hash`, so the two axes are
    /// always distinct values (a slice-provenance drift is still a detectable identity
    /// change) while remaining deterministic.
    #[must_use]
    pub fn bind(
        data_generation: WorldSourceIdentity,
        program: &LogicProgram,
        contract: &ReasoningContract,
        annotation: &AnnotationContract,
        fragment: &str,
    ) -> Self {
        let canonical = program.canonical_key();
        let program_hash = digest(b"gmeow-logic-session-program-hash-v1", &[&canonical]);
        let slice_source = program.source_iri.as_deref().unwrap_or(&canonical);
        let slice_hash = digest(b"gmeow-logic-session-slice-hash-v1", &[slice_source]);
        let contract_hash = contract.content_digest();
        let engine_descriptor_hash = EngineContract::current().descriptor_hash;
        let annotation_identity = annotation.canonical_key();

        let descriptor_hash = digest(
            b"gmeow-logic-session-identity-v1",
            &[
                &data_generation.generation,
                &data_generation.source_contract,
                &program_hash,
                &slice_hash,
                &contract_hash,
                &engine_descriptor_hash,
                &annotation_identity,
                fragment,
            ],
        );

        Self {
            data_generation,
            program_hash,
            slice_hash,
            contract_hash,
            engine_descriptor_hash,
            annotation_identity,
            fragment: fragment.to_owned(),
            descriptor_hash,
        }
    }

    /// Hard-fail (typed `Err`) when `pinned` differs from this identity's
    /// [`descriptor_hash`](Self::descriptor_hash) — the supported way to refuse a
    /// checkpoint minted under a drifted identity. Cloned from
    /// [`EngineContract::assert_matches`].
    ///
    /// # Errors
    ///
    /// Returns `Err` naming both hashes when the pin does not match.
    pub fn assert_matches(&self, pinned: &str) -> gmeow_errors::Result<()> {
        if self.descriptor_hash == pinned {
            Ok(())
        } else {
            Err(gmeow_errors::Diag::of_kind(crate::error::ContractDrift {
                detail: format!(
                    "reasoning-session identity drift: pinned to descriptor {pinned} but this \
                     session is {current}; a checkpoint minted under the pinned identity must \
                     not be restored against a different world/engine/contract/algebra",
                    current = self.descriptor_hash,
                ),
            }))
        }
    }

    /// Project the identity into N-Quads in `graph_iri`, so a consumer can fold the
    /// session identity into its own (signed) ledger AS DATA — the same lossy-projection
    /// discipline as [`EngineContract::to_nquads`]. Deterministic: the subject is
    /// content-addressed on [`descriptor_hash`](Self::descriptor_hash) and every
    /// property is fixed-order.
    #[must_use]
    pub fn to_nquads(&self, graph_iri: &str) -> String {
        let graph = format!("<{graph_iri}>");
        let subject = format!(
            "<{GMEOW_NS}logic/session-identity/{}>",
            self.descriptor_hash
        );
        let mut lines: Vec<String> = Vec::new();
        let mut triple = |s: &str, p: &str, o: &str| lines.push(format!("{s} <{p}> {o} {graph} ."));

        triple(
            &subject,
            RDF_TYPE,
            &format!("<{LOGIC_NAMESPACE}ReasoningSessionIdentity>"),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}sessionIdentityDescriptorHash"),
            &lit(&self.descriptor_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}dataGeneration"),
            &lit(&self.data_generation.generation),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}dataSourceContract"),
            &lit(&self.data_generation.source_contract),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}programHash"),
            &lit(&self.program_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}sliceHash"),
            &lit(&self.slice_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}contractHash"),
            &lit(&self.contract_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}engineDescriptorHash"),
            &lit(&self.engine_descriptor_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}annotationIdentity"),
            &lit(&self.annotation_identity),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}incrementalFragment"),
            &lit(&self.fragment),
        );

        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// Render `value` as an escaped N-Triples/N-Quads string literal.
fn lit(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-selected native medium declarations, shared by independent artifact audits.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use purrdf::TermRef;
use serde::{Deserialize, Serialize};

use super::registry::MediumRegistry;

/// Original source whose declarations govern these artifact audits.
pub const SOURCE: &str = "slices/core/gts/module.ttl";
/// Compact stage-conformance artifact exported through the source-artifact selector.
pub const CHANNEL: &str = "pipeline/source-medium-registry.json";

/// Validated typed source registry and each producer's independently authored medium.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMediumRegistry {
    /// Exact original document path; never a generated carrier projection.
    pub source_path: String,
    /// Digest of that document's producer-selected original bytes.
    pub source_digest: String,
    /// The native registry prepared once from the original source document.
    pub registry: MediumRegistry,
    /// Exactly-one medium selected by each authored producerMedium declaration.
    pub producer_media: BTreeMap<String, String>,
}

impl SourceMediumRegistry {
    /// Resolve one selected producer without a default or missing-declaration fallback.
    ///
    /// # Errors
    /// Rejects a producer absent from the source's explicit declaration map.
    pub fn declared_medium(&self, producer: &str) -> gmeow_errors::Result<&str> {
        self.producer_media
            .get(producer)
            .map(String::as_str)
            .ok_or_else(|| {
                super::invalid_declaration(format!(
                    "selected source has no producerMedium declaration for <{producer}>"
                ))
            })
    }
}

/// Read the exact produced source registry without parsing RDF or rebuilding its tables.
///
/// # Errors
/// Missing, stale or corrupt producer selections and malformed typed observations fail.
pub fn authenticated(root: &Path) -> gmeow_errors::Result<SourceMediumRegistry> {
    let bytes =
        gmeow_action_cache::selection::source_artifacts::load(root, "stage-conformance", CHANNEL)
            .map_err(|error| super::invalid_declaration(error.to_string()))?;
    let observed: SourceMediumRegistry = serde_json::from_slice(&bytes)
        .map_err(|error| super::invalid_declaration(error.to_string()))?;
    if observed.source_path != SOURCE {
        return Err(super::invalid_declaration(
            "source medium observation names a different original document",
        ));
    }
    Ok(observed)
}

/// Observe the original source through its shared native registry and borrowed quads.
pub(crate) fn record(
    sources: &crate::stages::parse_sources::SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let source = sources.document(SOURCE)?;
    let mut declarations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for quad in source.quad_refs() {
        if let (
            TermRef::Iri(producer),
            TermRef::Iri("https://blackcatinformatics.ca/gmeow/producerMedium"),
        ) = (quad.s, quad.p)
        {
            let media = declarations.entry(producer.to_owned()).or_default();
            if let TermRef::Iri(medium) = quad.o {
                media.insert(medium.to_owned());
            }
        }
    }
    // The same exactly-one IRI contract as declared_medium_of, resolved together
    // so one producer observation never rescans/materializes the dataset per row.
    let producer_media = declarations
        .into_iter()
        .map(|(producer, media)| {
            if media.len() != 1 {
                return Err(super::invalid_declaration(format!(
                    "<{producer}> declares {} producerMedium IRI values",
                    media.len()
                )));
            }
            Ok((
                producer,
                media
                    .into_iter()
                    .next()
                    .expect("exactly one declared medium"),
            ))
        })
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    let observed = SourceMediumRegistry {
        source_path: SOURCE.to_owned(),
        source_digest: sources.document_digest(SOURCE)?.to_owned(),
        registry: sources.medium_registry()?.clone(),
        producer_media,
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed)
            .map_err(|error| super::invalid_declaration(error.to_string()))?,
    );
    Ok(())
}

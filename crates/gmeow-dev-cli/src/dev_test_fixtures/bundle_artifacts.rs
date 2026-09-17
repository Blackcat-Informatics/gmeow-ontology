// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lazy derivation of the exact selected bundle's consumer artifacts.

use std::sync::OnceLock;

use gmeow_gts_profile::archive;
use gmeow_logic::coherence_observations as coherence;
use gmeow_logic_compile::action_policy;
use gmeow_mcp::corpus_observations as mcp_native;
use gmeow_pipeline::bundle_blobs::{REP_EXAMPLES, REP_MAPPINGS, REP_QUERIES, REP_SHAPES};
use gmeow_pipeline::stages::conformance::production_shapes::PreparedProductionShapes;
use gmeow_pipeline::stages::conformance::{advice_wing, contextual_results, flagship_unwired};
use gmeow_pipeline::stages::validate::norm_claims;
use gmeow_validate::data_validate::{ShapeCorpusVariants, shape_corpus_variants_from_archive};
use purrdf::{GtsBlobLimits, GtsBlobSelector, GtsImportWithBlobs, RdfDataset};

use super::{FixtureResult, fail};

mod conformance_ontology;

pub(super) const NAMES: &[&str] = &[
    gmeow_docs::gmn1_primer::teachability::ARTIFACT,
    "prepared-verify-gates.json",
    "gmn-codebook.cbor",
    "mcp-action-policy-statements.json",
    action_policy::CORPUS_ARTIFACT,
    contextual_results::ARTIFACT,
    flagship_unwired::ARTIFACT,
    advice_wing::ARTIFACT,
    norm_claims::ARTIFACT,
    coherence::DISJOINT_ARTIFACT,
    coherence::RELCOMP_ARTIFACT,
    coherence::CHARACTERISTIC_ARTIFACT,
    mcp_native::BAD_CREDENCE,
    mcp_native::FORGED_CITATION,
    mcp_native::ISOLATION,
    mcp_native::BUDGET_CUT,
    mcp_native::OMITTED_BUDGET,
    mcp_native::NORMAL_OVERLAY,
    mcp_native::ASSERTED_EXPLANATION,
    mcp_native::DERIVED_EXPLANATION,
    mcp_native::ABSENT_EXPLANATION,
    mcp_native::MEMORY_HOT_MEDIUM,
    gmeow_pipeline::docs_distribution::consumer_verification_fixture::ARTIFACT,
    conformance_ontology::PACK_ARTIFACT,
    conformance_ontology::TEXT_ARTIFACT,
    conformance_ontology::AUTHORED_PACK_ARTIFACT,
    "validate-conformance-shapes.ttl",
    "validate-domain-conformance-shapes.ttl",
    "validate-production-shapes.ttl",
    "validate-queries.ustar",
    "validate-mappings.ustar",
    "validate-constraint-shapes.ttl",
    "validate-linkml.yaml",
    "validate-statements-owl.ttl",
];
/// Exact archive profile required by every artifact in `NAMES`. Independent
/// artifact hits do not initialize this profile; any miss shares its one import.
pub(super) const BLOB_SELECTORS: &[GtsBlobSelector<'static>] = &[
    GtsBlobSelector::Representation(REP_EXAMPLES),
    GtsBlobSelector::Representation(archive::REASONING_REP),
    GtsBlobSelector::Representation("lang-projections-archive"),
    GtsBlobSelector::Representation(REP_SHAPES),
    GtsBlobSelector::Representation(REP_QUERIES),
    GtsBlobSelector::Representation(REP_MAPPINGS),
    GtsBlobSelector::Representation("generated-opaque-archive"),
    GtsBlobSelector::Representation("statements-archive"),
];

pub(super) const fn blob_limits() -> GtsBlobLimits {
    GtsBlobLimits::new(
        archive::MAX_SELECTED_ARCHIVE_BYTES,
        archive::MAX_SELECTED_ARCHIVE_BYTES,
    )
}

/// Only a missing artifact initializes its required inputs. The whole state is
/// discarded when this one bundle's artifact admission finishes.
pub(super) struct Sources<'a> {
    snapshot: &'a [u8],
    imported: OnceLock<GtsImportWithBlobs>,
    shapes: OnceLock<ShapeCorpusVariants>,
    prepared_shapes: OnceLock<PreparedProductionShapes>,
    conformance_ontology: OnceLock<conformance_ontology::ConformanceOntology>,
    mcp_native: OnceLock<mcp_native::Producer>,
    gmn_codebook: OnceLock<gmeow_lang_bridge::gmn1_codec::native::NativeCodebook>,
}

impl<'a> Sources<'a> {
    pub(super) fn new(snapshot: &'a [u8], cold_import: Option<GtsImportWithBlobs>) -> Self {
        let imported = OnceLock::new();
        if let Some(cold_import) = cold_import {
            let _ = imported.set(cold_import);
        }
        Self {
            snapshot,
            imported,
            shapes: OnceLock::new(),
            prepared_shapes: OnceLock::new(),
            conformance_ontology: OnceLock::new(),
            mcp_native: OnceLock::new(),
            gmn_codebook: OnceLock::new(),
        }
    }

    fn imported(&self) -> FixtureResult<&GtsImportWithBlobs> {
        self.imported.get_or_try_init(|| {
            purrdf::import_gts_events_with_blobs(self.snapshot, BLOB_SELECTORS, blob_limits())
                .map_err(gmeow_errors::Diag::from)
        })
    }

    fn dataset(&self) -> FixtureResult<&RdfDataset> {
        Ok(self.imported()?.bundle.dataset.as_ref())
    }

    fn gmn_codebook(
        &self,
    ) -> FixtureResult<&gmeow_lang_bridge::gmn1_codec::native::NativeCodebook> {
        use gmeow_lang_bridge::gmn1_codec::native;
        self.gmn_codebook.get_or_try_init(|| {
            let bytes = self.member(
                "lang-projections-archive",
                native::GENERATED_PATH,
                native::MAX_NATIVE_CODEBOOK_BYTES,
            )?;
            native::decode(&bytes, native::SOURCE_BLAKE3)
        })
    }

    fn blob(&self, representation: &str) -> FixtureResult<&[u8]> {
        archive::required_imported_blob(self.imported()?, representation)
            .map(|blob| blob.bytes.as_ref())
    }

    fn member(
        &self,
        representation: &'static str,
        path: &str,
        limit: usize,
    ) -> FixtureResult<Vec<u8>> {
        let blob = self.blob(representation)?;
        archive::archive_member(blob, path, limit).map(<[u8]>::to_vec)
    }

    fn shapes(&self) -> FixtureResult<&ShapeCorpusVariants> {
        self.shapes.get_or_try_init(|| {
            let blob = self.blob(REP_SHAPES)?;
            shape_corpus_variants_from_archive(blob)
        })
    }

    /// Both tiny controls use the exact same archive assembly and prefix profile.
    /// A hit does not prepare shapes; misses share one invocation-local preparation.
    fn prepared_shapes(&self) -> FixtureResult<&PreparedProductionShapes> {
        self.prepared_shapes
            .get_or_try_init(|| PreparedProductionShapes::new(&self.shapes()?.production))
    }

    fn conformance_ontology(&self) -> FixtureResult<&conformance_ontology::ConformanceOntology> {
        self.conformance_ontology
            .get_or_try_init(|| conformance_ontology::ConformanceOntology::new(self.dataset()?))
    }

    fn mcp_native(&self) -> FixtureResult<&mcp_native::Producer> {
        self.mcp_native.get_or_try_init(|| {
            mcp_native::Producer::new(
                std::sync::Arc::clone(&self.imported()?.bundle.dataset),
                std::sync::Arc::from(self.snapshot),
            )
        })
    }

    pub(super) fn produce(&self, name: &str) -> FixtureResult<Vec<u8>> {
        let member_limit = archive::MAX_NATIVE_MEMBER_BYTES;
        match name {
            gmeow_docs::gmn1_primer::teachability::ARTIFACT => {
                let bytes = self.member(
                    REP_EXAMPLES,
                    gmeow_docs::gmn1_primer::teachability::SOURCE,
                    member_limit,
                )?;
                let heldout = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(fail)?;
                let observation = gmeow_docs::gmn1_primer::teachability::observe(
                    self.dataset()?,
                    &heldout,
                    self.gmn_codebook()?.dictionary(),
                )
                .map_err(fail)?;
                serde_json::to_vec(&observation).map_err(fail)
            }
            "prepared-verify-gates.json" => self.member(
                archive::REASONING_REP,
                archive::REASONED_GATES_MEMBER,
                member_limit,
            ),
            "gmn-codebook.cbor" => self.member(
                "lang-projections-archive",
                gmeow_lang_bridge::gmn1_codec::native::GENERATED_PATH,
                gmeow_lang_bridge::gmn1_codec::native::MAX_NATIVE_CODEBOOK_BYTES,
            ),
            "mcp-action-policy-statements.json" => {
                serde_json::to_vec(&action_policy::policy_statements(self.dataset()?)).map_err(fail)
            }
            action_policy::CORPUS_ARTIFACT => self.member(
                archive::REASONING_REP,
                action_policy::BUNDLE_MEMBER,
                action_policy::MAX_BYTES,
            ),
            contextual_results::ARTIFACT => {
                serde_json::to_vec(&contextual_results::observe(self.dataset()?)).map_err(fail)
            }
            flagship_unwired::ARTIFACT => {
                let observation = flagship_unwired::observe(self.prepared_shapes()?)?;
                serde_json::to_vec(&observation).map_err(fail)
            }
            advice_wing::ARTIFACT => {
                let observation = advice_wing::observe(self.prepared_shapes()?, self.dataset()?)?;
                serde_json::to_vec(&observation).map_err(fail)
            }
            norm_claims::ARTIFACT => {
                let observation = norm_claims::observe_shipped(self.dataset()?)?;
                serde_json::to_vec(&observation).map_err(fail)
            }
            coherence::DISJOINT_ARTIFACT => {
                let observation =
                    coherence::observe_disjoint_clash(self.dataset()?).map_err(|error| {
                        gmeow_errors::DiagLedger::new()
                            .record(error, gmeow_errors::StageId::new("coherence.disjoint"))
                    });
                serde_json::to_vec(&observation).map_err(fail)
            }
            coherence::RELCOMP_ARTIFACT => {
                let observation = coherence::observe_relcomp(self.dataset()?).map_err(|error| {
                    gmeow_errors::DiagLedger::new()
                        .record(error, gmeow_errors::StageId::new("coherence.relcomp"))
                });
                serde_json::to_vec(&observation).map_err(fail)
            }
            coherence::CHARACTERISTIC_ARTIFACT => {
                let observation =
                    coherence::observe_characteristics(self.dataset()?).map_err(|error| {
                        gmeow_errors::DiagLedger::new().record(
                            error,
                            gmeow_errors::StageId::new("coherence.characteristic"),
                        )
                    });
                serde_json::to_vec(&observation).map_err(fail)
            }
            conformance_ontology::PACK_ARTIFACT => self.conformance_ontology()?.pack(),
            conformance_ontology::TEXT_ARTIFACT => self.conformance_ontology()?.text(),
            conformance_ontology::AUTHORED_PACK_ARTIFACT => {
                self.conformance_ontology()?.authored_pack()
            }
            "validate-conformance-shapes.ttl" => Ok(self.shapes()?.conformance.as_bytes().to_vec()),
            "validate-domain-conformance-shapes.ttl" => {
                Ok(self.shapes()?.domain_conformance.as_bytes().to_vec())
            }
            "validate-production-shapes.ttl" => Ok(self.shapes()?.production.as_bytes().to_vec()),
            "validate-queries.ustar" => Ok(self.blob(REP_QUERIES)?.to_vec()),
            "validate-mappings.ustar" => Ok(self.blob(REP_MAPPINGS)?.to_vec()),
            "validate-constraint-shapes.ttl" => self.member(
                REP_SHAPES,
                "generated/shapes/constraint-shapes.ttl",
                member_limit,
            ),
            "validate-linkml.yaml" => self.member(
                "generated-opaque-archive",
                "generated/schemas/gmeow.linkml.yaml",
                member_limit,
            ),
            "validate-statements-owl.ttl" => self.member(
                "statements-archive",
                "generated/statements/gmeow-statements.owl.ttl",
                member_limit,
            ),
            mcp_native::MEMORY_HOT_MEDIUM => {
                Ok(gmeow_mcp::store_medium(self.snapshot, gmeow_mcp::MEMORY_HOT_DICTIONARY)?.bytes)
            }
            gmeow_pipeline::docs_distribution::consumer_verification_fixture::ARTIFACT => {
                gmeow_pipeline::docs_distribution::consumer_verification_fixture::produce(
                    self.snapshot,
                )
            }
            name if mcp_native::ARTIFACTS.contains(&name) => self.mcp_native()?.produce(name),
            _ => Err(fail(format!("unregistered bundle corpus artifact {name}"))),
        }
    }
}

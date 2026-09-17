// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native gate observations over the actual language products and scoped source inputs.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use gmeow_lang_bridge::registry::LangEmissionBatch;
use gmeow_logic_compile::ir::{Correspondence, LegPath};
use purrdf::slice::SliceCatalog;
use purrdf::{ContentDigest, RdfDataset, parse_dataset};
use serde::{Deserialize, Serialize};

use crate::stages::gmn1_gate::{
    Gmn1CodebookDigestReport, Gmn1PackRootReport, check_gmn1_codebook_dataset,
    compare_gmn1_pack_root,
};
use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/gmn-gate-observations.json";
pub(crate) const NEGATIVE_SOURCE: &str =
    "slices/grounding/lang/tests/gmn1-vectors/negative-graph/envelope-digest-mismatch.ttl";

#[derive(Serialize, Deserialize)]
pub(crate) struct Observations {
    pub codebook: Gmn1CodebookDigestReport,
    pub codebook_sources: Vec<String>,
    pub negative: Gmn1CodebookDigestReport,
    pub negative_source_digest: String,
    pub pack: Gmn1PackRootReport,
    pub pack_artifact_digest: String,
    pub shipped: BTreeMap<String, ShippedWitness>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ShippedWitness {
    pub artifact_digest: String,
    pub source_iri: String,
    pub correspondence: Correspondence,
    pub leg_pair: Option<(LegPath, LegPath)>,
    pub round_trip_holds: bool,
}

#[derive(Default)]
pub(super) struct Recorder {
    pack: Option<(Arc<RdfDataset>, String)>,
    shipped: BTreeMap<String, ShippedWitness>,
}

impl Recorder {
    pub(super) fn record(
        &mut self,
        target: &str,
        batch: &LangEmissionBatch,
    ) -> gmeow_errors::Result<()> {
        if target != "gmn1" {
            return Ok(());
        }
        for emission in &batch.emissions {
            for artifact in &emission.artifacts {
                if artifact.path_suffix.ends_with("/conformance-pack.ttl") {
                    if self.pack.is_some() {
                        return Err(super::stage_err(
                            "native GMN gate received duplicate pack emissions",
                        ));
                    }
                    let dataset =
                        parse_dataset(&artifact.bytes, "text/turtle", None).map_err(|error| {
                            super::stage_err(format!("read actual GMN pack: {error}"))
                        })?;
                    self.pack = Some((dataset, ContentDigest::of(&artifact.bytes).to_hex()));
                } else if artifact.path_suffix.ends_with(".gmn") {
                    // This measurement belongs to the exact document Gmn1Target already
                    // wrote and read with its out-of-band reference table. Reconstructing
                    // another document here would repeat the codec's full source work.
                    let path = format!("{}/{}", super::LANG_PROJECTION_DIR, artifact.path_suffix);
                    let witness = ShippedWitness {
                        artifact_digest: ContentDigest::of(&artifact.bytes).to_hex(),
                        source_iri: emission.source_iri.clone(),
                        correspondence: emission.correspondence.clone(),
                        leg_pair: emission.leg_pair.clone(),
                        round_trip_holds: emission.round_trip_holds,
                    };
                    if self.shipped.insert(path.clone(), witness).is_some() {
                        return Err(super::stage_err(format!(
                            "duplicate native GMN output {path}"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        self,
        root: &Path,
        catalog: Option<&SliceCatalog>,
        sources: &SourceCatalog,
        pack: &super::gmn_pack::Observations,
    ) -> gmeow_errors::Result<Observations> {
        let catalog = catalog.ok_or_else(|| {
            super::stage_err("native GMN gate lacks its selected slice inventory")
        })?;
        let (native_pack, pack_artifact_digest) = self
            .pack
            .ok_or_else(|| super::stage_err("native GMN gate lacks its actual pack emission"))?;
        let digest = &pack.expected_codebook_digest;
        let mut codebook = Gmn1CodebookDigestReport::default();
        let mut codebook_sources = Vec::new();
        for record in catalog.records() {
            let Some(slice) = ["lang", "math", "logic"]
                .into_iter()
                .find(|slice| record.slice_dir.ends_with(format!("grounding/{slice}")))
            else {
                continue;
            };
            for artifact in &record.artifacts {
                let module = artifact.logical_path == "module.ttl";
                let example = artifact
                    .logical_path
                    .strip_prefix("examples/")
                    .is_some_and(|path| path.ends_with(".ttl") && !path.contains('/'));
                if !module && !example {
                    continue;
                }
                let path = format!("slices/grounding/{slice}/{}", artifact.logical_path);
                let report = if module {
                    check_gmn1_codebook_dataset(digest, &path, sources.document(&path)?)
                } else {
                    // Examples remain auxiliary source-local inputs. They never enter
                    // the globally asserted SourceCatalog merely to run this audit.
                    let dataset =
                        parse_dataset(&artifact.content, "text/turtle", None).map_err(|error| {
                            super::stage_err(format!("native envelope source {path}: {error}"))
                        })?;
                    check_gmn1_codebook_dataset(digest, &path, &dataset)
                };
                codebook.checked += report.checked;
                codebook.mismatches.extend(report.mismatches);
                codebook_sources.push(path);
            }
        }
        for slice in ["lang", "math", "logic"] {
            if !codebook_sources.contains(&format!("slices/grounding/{slice}/module.ttl")) {
                return Err(super::stage_err(format!(
                    "native envelope scope lacks grounding/{slice}/module.ttl"
                )));
            }
        }
        let pack_report = compare_gmn1_pack_root(
            pack.expected_pack_root.clone(),
            &pack.major,
            Some(&native_pack),
        );
        let report = check_gmn1_codebook_dataset(digest, &pack_report.pack_rel, &native_pack);
        codebook.checked += report.checked;
        codebook.mismatches.extend(report.mismatches);
        codebook_sources.push(pack_report.pack_rel.clone());
        codebook_sources.sort();
        codebook.mismatches.sort_by(|left, right| {
            (&left.source, &left.envelope).cmp(&(&right.source, &right.envelope))
        });

        // This explicit negative is a mappings action input, never part of the
        // authored/asserted base. It is read only by this optimized producer.
        let negative_bytes = std::fs::read(root.join(NEGATIVE_SOURCE))?;
        let negative_source_digest = ContentDigest::of(&negative_bytes).to_hex();
        let negative_dataset =
            parse_dataset(&negative_bytes, "text/turtle", None).map_err(|error| {
                super::stage_err(format!(
                    "native envelope negative {NEGATIVE_SOURCE}: {error}"
                ))
            })?;
        let negative = check_gmn1_codebook_dataset(digest, NEGATIVE_SOURCE, &negative_dataset);
        Ok(Observations {
            codebook,
            codebook_sources,
            negative,
            negative_source_digest,
            pack: pack_report,
            pack_artifact_digest,
            shipped: self.shipped,
        })
    }
}

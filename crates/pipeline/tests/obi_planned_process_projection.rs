// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! The OBI planned-process bridge reaches the projected surface.
//!
//! GMEOW never hand-authors an `obi:` cell on a consuming slice: OBI's catalog is owned
//! by the `logic:` grounding slice (`gmeow:projVocab-obi`'s `gmeow:vocabularyOwner`), the
//! two correspondences are authored ONCE there as
//! `logic:GroundingCorrespondence` cells carrying an explicit `logic:preservationKind`,
//! and every downstream `obi:` surface is a GENERATED projection of them (Principle 17).
//!
//! This test runs that projection over the REAL corpus and asserts both alignments
//! survive it — the prescription bridge (`logic:Plan` → OBI protocol) and the enactment
//! bridge (`logic:Enactment` → OBI planned process). It drives two producer legs:
//!
//! 1. the typed correspondence frontend (`transpile_correspondences_indexed`), which
//!    materializes the `logic:Correspondence` IR nodes the shipped meta-level
//!    correspondence-law graph is folded from — checked for source/target endpoints,
//!    morphism class/kind, and the preservation judgment;
//! 2. the SSSOM lowering (`lower_sssom`), the alignment dialect a consumer actually
//!    reads — checked for a rendered row per bridge in the `logic-obi` mapping set.
//!
//! It reads only authored sources, never a regenerated artifact, so it fails on a
//! dropped alignment rather than on a stale `generated/` tree.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::ir::{MorphismClass, MorphismKind, PreservationKind};
use gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed;
use gmeow_logic_compile::projections::sssom::lower_sssom;
use purrdf::slice::{ArtifactRole, SliceCatalog};
use purrdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, parse_dataset};

/// OBI protocol — "a plan of action a study or an assay follows".
const OBI_PROTOCOL: &str = "http://purl.obolibrary.org/obo/OBI_0000272";
/// OBI planned process — "a process that realizes a plan specification".
const OBI_PLANNED_PROCESS: &str = "http://purl.obolibrary.org/obo/OBI_0000011";
const LOGIC_PLAN: &str = "https://blackcatinformatics.ca/logic/Plan";
const LOGIC_ENACTMENT: &str = "https://blackcatinformatics.ca/logic/Enactment";
/// The mapping set the two bridges publish through.
const OBI_SSSOM_FILE: &str = "gmeow-logic-obi.sssom.tsv";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

fn collect_ttl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect_ttl_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
}

fn parse_turtle(bytes: &[u8]) -> Arc<RdfDataset> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None).expect("parse turtle source")
}

/// The authored alignment corpus: `dsl/mappings/**/*.ttl` then every slice's
/// `mappings/*.ttl`, merged in the order the mapping producers load them.
fn merge_alignment_sources(root: &Path) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();

    let mut dsl_files: Vec<PathBuf> = Vec::new();
    collect_ttl_files(&root.join("dsl").join("mappings"), &mut dsl_files);
    dsl_files.sort();
    for path in &dsl_files {
        builder.push_dataset(&parse_turtle(
            &std::fs::read(path).expect("read dsl source"),
        ));
    }

    let slices_dir = root.join("slices");
    let catalog = SliceCatalog::discover(&slices_dir, gmeow_ns::gmeow_slice_vocab())
        .expect("discover slices");
    let mut slice_mappings: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == ArtifactRole::Mapping {
                slice_mappings.push((
                    record.slice_dir.join(&artifact.logical_path),
                    artifact.content.clone(),
                ));
            }
        }
    }
    slice_mappings.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, bytes) in &slice_mappings {
        builder.push_dataset(&parse_turtle(bytes));
    }

    builder.freeze().expect("freeze merged dataset")
}

#[test]
fn the_obi_planned_process_bridges_reach_the_projected_surface() {
    let root = repo_root();
    let merged = merge_alignment_sources(&root);
    let view = DslView::new(&merged);
    let empty = parse_turtle(b"");

    // Producer leg 1: the typed correspondence frontend.
    let (program, lookup) = transpile_correspondences_indexed(&view, &DslView::new(&empty))
        .expect("transpile the correspondence program");

    for (source, target, what) in [
        (LOGIC_PLAN, OBI_PROTOCOL, "prescription → OBI protocol"),
        (
            LOGIC_ENACTMENT,
            OBI_PLANNED_PROCESS,
            "enactment → OBI planned process",
        ),
    ] {
        let corr = program
            .correspondences
            .iter()
            .find(|c| {
                c.source_endpoint.as_deref() == Some(source)
                    && c.target_endpoint.as_deref() == Some(target)
            })
            .unwrap_or_else(|| {
                panic!(
                    "the {what} bridge must materialize as a typed correspondence \
                     ({source} → {target}); the projected obi: surface is derived from it, \
                     so a dropped cell silently drops the alignment"
                )
            });
        assert!(
            corr.grounding,
            "the {what} bridge must be a grounding correspondence so it ships in the \
             meta-level correspondence-law graph"
        );
        assert_eq!(
            corr.morphism_class,
            MorphismClass::BridgeView,
            "the {what} bridge is commitment-shifting, never an equivalence"
        );
        assert_eq!(
            corr.morphism_kind,
            MorphismKind::CommitmentShiftingBridge,
            "the {what} bridge's morphism kind must agree with its BridgeView class"
        );
        assert_eq!(
            corr.preservation,
            Some(PreservationKind::ValidationOnly),
            "the {what} bridge must carry an EXPLICIT preservation judgment"
        );
    }

    // Producer leg 2: the SSSOM lowering — the alignment dialect a consumer reads.
    let sets = lower_sssom(&view, "test-version", "2026-01-01", &lookup)
        .expect("lower the SSSOM projection")
        .sets;
    let tsv = sets.get(OBI_SSSOM_FILE).unwrap_or_else(|| {
        panic!(
            "the SSSOM producer must emit {OBI_SSSOM_FILE}; emitted sets: {:?}",
            sets.keys().collect::<Vec<_>>()
        )
    });
    for (subject, object, what) in [
        ("logic:Plan", "obi:0000272", "prescription → OBI protocol"),
        (
            "logic:Enactment",
            "obi:0000011",
            "enactment → OBI planned process",
        ),
    ] {
        assert!(
            tsv.lines()
                .any(|l| l.starts_with(&format!("{subject}\t")) && l.contains(object)),
            "the {what} bridge must appear as a row in the emitted {OBI_SSSOM_FILE}:\n{tsv}"
        );
    }
}

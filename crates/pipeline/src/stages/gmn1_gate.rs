// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-1 round-trip gate: the executed byte witness behind
//! `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` declaration, mirroring
//! [`crate::stages::superset`]'s byte-reconstruction discipline over the SAME
//! authority — [`purrdf::canonicalize`] — rather than a bespoke comparator.
//!
//! # Scope
//!
//! Per the F1 user decision (`gmeow:gmnCorrNormalToGmn`'s carrier declaration), the
//! codec + this gate are TOTAL over the **grounding slices' GMN-0 NOW**
//! (`slices/grounding/{logic,lang,math}`, authored `module.ttl` PLUS `examples/*.ttl` —
//! the SAME domain the `axisGmn1Coverage` slice-quality axis's own definition scopes
//! coverage to). Coverage of every other slice is a separate, floor-gated quality axis
//! (Task 7), not this gate's job — this gate never reads a non-grounding slice, so it
//! can never red on a non-grounding gap.
//!
//! # What the gate proves
//!
//! For every grounding source file: parse it to a [`purrdf::RdfDataset`], build a
//! [`gmeow_lang_bridge::Gmn0Model`], run `gmn1_read(gmn1_write(model))`, and assert
//! canonical equality via [`gmeow_lang_bridge::gmn0_canonically_equal`] (which itself
//! calls `purrdf::canonicalize` — the same canonical-comparison primitive the
//! GTS/N-Quads byte-teeth gates use). A write-side uncovered construct, a read-side
//! parse defect, or a canonical mismatch is a hard failure — no skips, no optional
//! coverage, a single non-round-tripping fixture reds the gate.

use std::path::Path;

use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Error, GmnDictionary, gmn0_canonically_equal, gmn1_read, gmn1_write,
};
use purrdf::parse_dataset;

/// Which failure surface a grounding source hit — the distinction the L3 ledger-identity
/// requirement bites on: an uncovered GMN-0 construct is `lang:GmnUncoveredTerm` (interned
/// through [`gmeow_lang_bridge::error::attach_gmn_uncovered`], the dedicated DiagLedger
/// identity), never folded into the generic round-trip-mismatch code the way a canonical
/// mismatch or a malformed-text parse defect is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gmn1FailureKind {
    /// A GMN-0 construct this codec cannot losslessly encode/decode (`Gmn1Error::Uncovered`) —
    /// the `lang:GmnUncoveredTerm` hard fail.
    Uncovered,
    /// A parse defect, canonical mismatch, or Turtle-ingest failure that is not an
    /// uncovered-term hard fail.
    RoundTripDefect,
}

/// The grounding slice directories this gate is total over (mirrors the
/// `axisGmn1Coverage` axis's own `slices/grounding/` scope, minus `kernel`: the kernel
/// module carries no independent GMN-0 content beyond what `logic`/`lang`/`math`
/// already exercise structurally, and is folded into the `lang`/`logic` round-trips via
/// their cross-references — this gate's own corpus is the three content-bearing
/// grounding modules named in the carrier declaration and Task 6's own text).
const GROUNDING_SLICES: [&str; 3] = ["logic", "lang", "math"];

/// One grounding-slice source file's round-trip outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gmn1RoundTripFailure {
    /// The repo-relative source path (`slices/grounding/lang/module.ttl`, an
    /// `examples/*.ttl` fixture, …).
    pub path: String,
    /// The failure detail (an uncovered construct or a canonical mismatch), from
    /// [`gmeow_lang_bridge::Gmn1Error`]'s `Display`.
    pub detail: String,
    /// Which failure surface this is — see [`Gmn1FailureKind`].
    pub kind: Gmn1FailureKind,
}

/// The gate's outcome: every grounding source that failed to round-trip losslessly.
/// Empty ⇒ the gate is clean — `gmeow:gmnCorrNormalToGmn`'s `mnemomorphic true` claim is
/// discharged for this run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gmn1RoundTripReport {
    /// Every failing source, in a stable (sorted-path) order.
    pub failures: Vec<Gmn1RoundTripFailure>,
}

impl Gmn1RoundTripReport {
    /// The gate passes when no grounding source failed to round-trip.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Run the GMN-1 round-trip gate over every grounding slice's `module.ttl` and
/// `examples/*.ttl` under `<root>/slices/grounding/`.
pub fn check_gmn1_roundtrip(root: &Path) -> Result<Gmn1RoundTripReport, gmeow_errors::Diag> {
    // The dictionary is authored in the lang slice's `module.ttl`; every grounding
    // source is decoded/encoded against the SAME loaded dictionary (one shipped
    // `gmeow:gmnDictV1` version, per the carrier's own version-pinning discipline).
    let lang_module_path = root.join("slices/grounding/lang/module.ttl");
    let lang_bytes = std::fs::read(&lang_module_path)
        .map_err(|e| stage_err(&format!("read {}: {e}", lang_module_path.display())))?;
    let lang_ds = parse_dataset(&lang_bytes, "text/turtle", None)
        .map_err(|e| stage_err(&format!("parse {}: {e}", lang_module_path.display())))?;
    let dict = GmnDictionary::from_dataset(&lang_ds)
        .map_err(|e| stage_err(&format!("gmeow:gmnDictV1 failed to load: {}", e.0)))?;

    let mut sources: Vec<String> = Vec::new();
    for slice in GROUNDING_SLICES {
        let slice_dir = root.join("slices/grounding").join(slice);
        let module_path = slice_dir.join("module.ttl");
        if module_path.is_file() {
            sources.push(
                module_path
                    .strip_prefix(root)
                    .unwrap_or(&module_path)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let examples_dir = slice_dir.join("examples");
        if examples_dir.is_dir() {
            let entries = std::fs::read_dir(&examples_dir)
                .map_err(|e| stage_err(&format!("read dir {}: {e}", examples_dir.display())))?;
            for entry in entries {
                let entry =
                    entry.map_err(|e| stage_err(&format!("dir entry in {slice}/examples: {e}")))?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
                    sources.push(
                        path.strip_prefix(root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
    }
    sources.sort();

    let mut failures = Vec::new();
    for source in &sources {
        let bytes = std::fs::read(root.join(source))
            .map_err(|e| stage_err(&format!("read {source}: {e}")))?;
        let ds = match parse_dataset(&bytes, "text/turtle", None) {
            Ok(ds) => ds,
            Err(e) => {
                failures.push(Gmn1RoundTripFailure {
                    path: source.clone(),
                    detail: format!("failed to parse as Turtle: {e}"),
                    kind: Gmn1FailureKind::RoundTripDefect,
                });
                continue;
            }
        };
        let model = Gmn0Model::from_dataset(&ds);
        match gmn1_write(&model, &dict) {
            Ok(doc) => match gmn1_read(&doc, &dict) {
                Ok(reconstructed) => {
                    if !gmn0_canonically_equal(&model, &reconstructed) {
                        failures.push(Gmn1RoundTripFailure {
                            path: source.clone(),
                            detail: "round-trip canonical mismatch (gmn1_read(gmn1_write(x)) != x)"
                                .to_owned(),
                            kind: Gmn1FailureKind::RoundTripDefect,
                        });
                    }
                }
                Err(e) => failures.push(Gmn1RoundTripFailure {
                    path: source.clone(),
                    kind: failure_kind(&e),
                    detail: e.to_string(),
                }),
            },
            Err(e) => failures.push(Gmn1RoundTripFailure {
                path: source.clone(),
                kind: failure_kind(&e),
                detail: e.to_string(),
            }),
        }
    }

    failures.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Gmn1RoundTripReport { failures })
}

/// Classify a codec-level [`Gmn1Error`] into the gate's own [`Gmn1FailureKind`] — the
/// only place this distinction is made, so `run.rs`'s ledger-interning call and this
/// gate's own tests agree by construction.
fn failure_kind(e: &Gmn1Error) -> Gmn1FailureKind {
    match e {
        Gmn1Error::Uncovered(_) => Gmn1FailureKind::Uncovered,
        Gmn1Error::Malformed(_) => Gmn1FailureKind::RoundTripDefect,
    }
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-gmn1-gate".to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn gate_is_clean_over_the_real_grounding_slices() {
        let root = repo_root();
        let report = check_gmn1_roundtrip(&root).expect("gate runs without a hard I/O error");
        assert!(
            report.is_clean(),
            "GMN-1 round-trip gate is not clean over the grounding slices: {:#?}",
            report.failures
        );
    }

    /// The gate's own negative teeth, proven through the SAME `check_gmn1_roundtrip`
    /// entry point `run.rs` wires into `make check` — not merely the codec's own unit
    /// tests. Builds a throwaway `<tmp>/slices/grounding/{logic,lang,math}` tree with a
    /// deliberately uncovered construct (an IRI under no registered namespace) in the
    /// `math` module and asserts the gate reds on it, naming the offending path — proof
    /// the gate has teeth at the file-I/O entry point a permanent fixture can safely
    /// exercise (unlike temporarily corrupting a real committed slice file).
    #[test]
    fn gate_reds_on_a_deliberately_uncovered_construct() {
        let dir =
            std::env::temp_dir().join(format!("gmeow-gmn1-gate-negative-{}", std::process::id()));
        let lang_dir = dir.join("slices/grounding/lang");
        let logic_dir = dir.join("slices/grounding/logic");
        let math_dir = dir.join("slices/grounding/math");
        std::fs::create_dir_all(&lang_dir).unwrap();
        std::fs::create_dir_all(&logic_dir).unwrap();
        std::fs::create_dir_all(&math_dir).unwrap();

        // A minimal but real lang module.ttl so the dictionary loads (an empty
        // dictionary is a legal — if degenerate — GmnDictionary).
        std::fs::write(
            lang_dir.join("module.ttl"),
            b"@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
        )
        .unwrap();
        std::fs::write(
            logic_dir.join("module.ttl"),
            b"@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
        )
        .unwrap();
        // The deliberately uncovered construct: an IRI under a namespace this codec's
        // prefix table does not register.
        std::fs::write(
            math_dir.join("module.ttl"),
            b"@prefix unreg: <https://not-a-registered-namespace.example/> .\n\
              unreg:subject unreg:predicate unreg:object .\n",
        )
        .unwrap();

        let report = check_gmn1_roundtrip(&dir).expect("gate runs without a hard I/O error");
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            !report.is_clean(),
            "the gate must red on a deliberately uncovered construct, not pass vacuously"
        );
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.path == "slices/grounding/math/module.ttl"
                    && f.kind == Gmn1FailureKind::Uncovered),
            "the failure must name the offending source path AND classify as Uncovered \
             (so run.rs routes it through lang:GmnUncoveredTerm, not the generic \
             round-trip-mismatch code): {:#?}",
            report.failures
        );
    }

    #[test]
    fn report_is_clean_iff_no_failures() {
        let clean = Gmn1RoundTripReport::default();
        assert!(clean.is_clean());
        let dirty = Gmn1RoundTripReport {
            failures: vec![Gmn1RoundTripFailure {
                path: "x".to_owned(),
                detail: "y".to_owned(),
                kind: Gmn1FailureKind::RoundTripDefect,
            }],
        };
        assert!(!dirty.is_clean());
    }
}

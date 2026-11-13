// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Signing rebuilds the bundle it signs, so it is refused before any build when the
/// profile is not the release profile.
#[test]
fn signing_a_bundle_on_the_ordinary_profile_is_refused() {
    let key = Path::new("/does/not/exist");
    let code = compile_gts(
        None,
        Some(key),
        Some(key),
        gmeow_pipeline::BuildProfile::Ordinary,
    );
    assert_ne!(code, 0);
}

/// A synthetic source with a valid ingestion receipt but no dictionary evidence records,
/// written next to its companion the way the producer writes one.
fn record_less_source(dir: &Path) -> PathBuf {
    let dataset = purrdf::parse_dataset(
        b"<https://e/s> <https://e/p> <https://e/o> .\n",
        purrdf::NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .expect("parse the synthetic source");
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder.add_dataset(&dataset).expect("add the dataset");
    // gmeow-test-input: synthetic-only
    let emission = gmeow_gts_profile::emit_gmeow_gts(
        builder,
        Vec::new(),
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit the synthetic source");
    let path = dir.join("source.gts");
    emission
        .write_to(&path)
        .expect("write the source and its receipt");
    path
}

/// `release-bundle` admits its source through `admit_release_source`: a source whose
/// receipt verifies but which records no dictionary evidence is refused for that reason,
/// before any key is read.
#[test]
fn a_release_source_without_dictionary_evidence_records_is_refused_after_its_receipt_verifies() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = record_less_source(dir.path());
    let bytes = std::fs::read(&path).expect("read the source");
    let Err(message) = admit_release_source(&path, &bytes) else {
        panic!("a source with no dictionary evidence must not be admitted");
    };
    assert!(
        message.contains("cannot be released")
            && !message.contains("invalid release input receipt"),
        "the refusal comes from the evidence check, not the receipt: {message}"
    );
}

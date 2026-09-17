// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Named read-only GMN gate contracts executed by the shared language runner.

use gmeow_lang_bridge::{exact_round_trip_holds, is_exact_correspondence};

use crate::stages::gmn1_gate::CLASS_GMN_CODEBOOK_DIGEST_MISMATCH;

use super::{contract_fixtures, gmn_gate};

pub(crate) fn codebook_digest_gate_is_clean_over_the_real_tree() {
    let observed = contract_fixtures::gates();
    assert!(
        observed.codebook.is_clean(),
        "the on-gate codebook-digest gate must be clean over the real tree: {:#?}",
        observed.codebook.mismatches
    );
    for slice in ["lang", "logic", "math"] {
        assert!(
            observed
                .codebook_sources
                .contains(&format!("slices/grounding/{slice}/module.ttl"))
        );
    }
    assert!(
        !observed
            .codebook_sources
            .iter()
            .any(|path| path == gmn_gate::NEGATIVE_SOURCE),
        "negative examples never enter the positive source domain"
    );
}

pub(crate) fn codebook_digest_gate_reds_on_the_mismatch_fixture() {
    let observed = contract_fixtures::gates();
    let report = &observed.negative;
    assert!(
        !report.is_clean(),
        "the digest-mismatch fixture must red the gate, not pass vacuously"
    );
    assert_eq!(report.checked, 1, "exactly one envelope digest is checked");
    assert!(
        report
            .mismatches
            .iter()
            .all(|mismatch| mismatch.failure_class() == CLASS_GMN_CODEBOOK_DIGEST_MISMATCH),
        "every mismatch classifies as lang:GmnCodebookDigestMismatch: {:#?}",
        report.mismatches
    );
    assert_eq!(report.mismatches.len(), 1);
    let mismatch = &report.mismatches[0];
    assert_eq!(mismatch.source, gmn_gate::NEGATIVE_SOURCE);
    assert_eq!(
        mismatch.envelope,
        "http://example.org/lang/envelopeWrongDigest"
    );
    assert_ne!(mismatch.declared, mismatch.recomputed);
    assert_eq!(
        mismatch.recomputed,
        contract_fixtures::pack().expected_codebook_digest
    );
    assert_eq!(
        observed.negative_source_digest.len(),
        64,
        "the authored negative's exact bytes are recorded"
    );
}

pub(crate) fn pack_root_check_is_clean_over_the_real_tree() {
    let observed = contract_fixtures::gates();
    let report = &observed.pack;
    assert!(
        report.is_clean(),
        "the pack-root check must be clean over the real tree: declared={:?} recomputed={} present={}",
        report.declared_root,
        report.recomputed_root,
        report.pack_present
    );
    assert!(
        report.pack_present,
        "the selected producer emitted the actual pack"
    );
    assert_eq!(
        report.recomputed_root,
        contract_fixtures::pack().expected_pack_root
    );
    assert_eq!(
        purrdf::ContentDigest::of(contract_fixtures::product("conformance-pack.ttl").as_bytes())
            .to_hex(),
        observed.pack_artifact_digest,
        "the audited native pack is the exact authenticated emitted artifact",
    );
}

pub(crate) fn shipped_gmn1_projections_all_read_clean() {
    let observed = contract_fixtures::gates();
    let actual_artifacts = contract_fixtures::shipped_artifacts();
    assert_eq!(
        observed.shipped.keys().collect::<Vec<_>>(),
        actual_artifacts.keys().collect::<Vec<_>>(),
        "every actual shipped projection has its native readback witness"
    );
    assert!(
        !observed.shipped.is_empty(),
        "the lint must actually have exercised shipped projections, not vacuously pass on an empty set"
    );
    for (path, witness) in &observed.shipped {
        assert_eq!(
            &witness.artifact_digest, &actual_artifacts[path],
            "native readback measured the exact shipped bytes for {path}"
        );
        assert!(
            witness.round_trip_holds,
            "production codec readback failed for {path}, source {}",
            witness.source_iri
        );
        assert!(
            is_exact_correspondence(&witness.correspondence),
            "shipped correspondence is exact for {path}"
        );
        let (get, put) = witness
            .leg_pair
            .as_ref()
            .expect("shipped exact product carries logical legs");
        assert!(
            exact_round_trip_holds(get, put),
            "native readback's structural legs invert for {path}"
        );
    }
}

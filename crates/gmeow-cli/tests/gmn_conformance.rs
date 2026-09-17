// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Public GMN command dispatch over independent tiny user inputs. The complete
//! authored corpus and ring demonstrator are graded from authenticated producer
//! observations in the pipeline suite. These tests never parse or copy them.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

const CODEBOOK: &str = "blake3:26a68453ecfe47867551038b8b247e9f4b07b3815bd87979c401afb0f7edf5ce";
const INPUT: &str = "<https://blackcatinformatics.ca/gmeow/cliSubject> <https://blackcatinformatics.ca/gmeow/cliPredicate> <https://blackcatinformatics.ca/gmeow/cliObject> .\n";
const FROZEN: &str = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: gmeow__cliSubject, p: gmeow__cliPredicate, o: gmeow__cliObject}\n";

/// One independent positive and negative user vector, plus small ring controls.
struct Inputs(tempfile::TempDir);
impl Inputs {
    fn new() -> Self {
        let inputs = Self(tempfile::tempdir().unwrap());
        fs::write(inputs.path("claim-basic.in.ttl"), INPUT).unwrap();
        fs::write(inputs.path("claim-basic.gmn"), FROZEN).unwrap();
        fs::write(
            inputs.path("vector-manifest.ttl"),
            format!(
                r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
[] rdfs:label "claim-basic.in.ttl -> claim-basic.gmn" ; gmeow:gmnCodebookDigest "{CODEBOOK}" .
"#
            ),
        )
        .unwrap();
        fs::create_dir(inputs.path("negative-codec")).unwrap();
        fs::write(inputs.path("negative-codec/unknown.gmn"), "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: not_a_known_prefix__s, p: gmeow__cliPredicate, o: gmeow__cliObject}\n").unwrap();
        fs::write(
            inputs.path("negative-codec/expected.ttl"),
            r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
[] rdfs:label "unknown.gmn" ; gmeow:enforcesFailureClass lang:GmnUncoveredTerm .
"#,
        )
        .unwrap();
        fs::write(
            inputs.path("language.ttl"),
            r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
  gmeow:references gmeow:cliDictionary, lang:cliScript ;
  gmeow:gmnDictionaryVersion "3" ; gmeow:gmnGlyphTableVersion "2" .
gmeow:cliDictionary a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
lang:cliScript a lang:Script ; lang:hasGrapheme lang:cliGrapheme .
gmeow:gmnDialectVersions a gmeow:VersionSet ; gmeow:gmnAcceptWindow 1 .
gmeow:cliLatest logic:versionInfo "1" .
gmeow:cliMembership a gmeow:VersionMembership ;
  gmeow:versionMember gmeow:cliLatest ; gmeow:versionSet gmeow:gmnDialectVersions ;
  gmeow:versionRole gmeow:roleLatest .
gmeow:gmnRingCore gmeow:gmnRingLevel gmeow:cliCore .
gmeow:gmnRingTrusted gmeow:gmnRingLevel gmeow:cliTrusted .
gmeow:gmnRingNato gmeow:gmnRingLevel gmeow:cliTrusted ; gmeow:gmnRingCompartment gmeow:cliNato .
gmeow:gmnRingRestricted gmeow:gmnRingLevel gmeow:cliRestricted .
gmeow:cliCore gmeow:gmnRingLevelDominates gmeow:cliCore, gmeow:cliTrusted, gmeow:cliRestricted .
gmeow:cliTrusted gmeow:gmnRingLevelDominates gmeow:cliTrusted, gmeow:cliRestricted .
gmeow:cliRestricted gmeow:gmnRingLevelDominates gmeow:cliRestricted .
"#,
        )
        .unwrap();
        fs::write(
            inputs.path("rings.ttl"),
            r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
g:cliCoreClaim g:gmnContentRing g:gmnRingCore ; g:cliField g:cliCoreDatum .
g:cliTrustedClaim g:gmnContentRing g:gmnRingTrusted ; g:cliField g:cliTrustedDatum .
g:cliNatoClaim g:gmnContentRing g:gmnRingNato ; g:cliField g:cliNatoDatum .
g:cliRestrictedClaim g:gmnContentRing g:gmnRingRestricted ; g:cliField g:cliRestrictedDatum .
"#,
        )
        .unwrap();
        inputs
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }
}

// ── verify: PASS over independent inputs ───────────────────────────────────────────

/// `gmeow gmn verify` exits 0 over an independent tiny user corpus and prints the
/// pass summary (positives byte-frozen + round-tripped, negatives classified).
#[test]
fn gmn_verify_accepts_an_independent_user_corpus() {
    let inputs = Inputs::new();
    gmeow()
        .args([
            "gmn",
            "verify",
            "--vectors",
            inputs.path("").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("positives 1/1"))
        .stdout(predicate::str::contains("gmn conformance PASS"));
}

// ── digest / encode / decode: stable output on a small fixture ───────────────────

/// `gmeow gmn digest` prints the frozen codebook Merkle root and the fixture's
/// content digest, both `blake3:…` and stable run-to-run.
#[test]
fn gmn_digest_is_stable() {
    let inputs = Inputs::new();
    // The installed native codebook retains the product digest pinned by its producer.
    gmeow()
        .args([
            "gmn",
            "digest",
            inputs.path("claim-basic.in.ttl").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "codebook_digest blake3:26a68453ecfe47867551038b8b247e9f4b07b3815bd87979c401afb0f7edf5ce",
        ))
        .stdout(predicate::str::contains("content_digest blake3:"));
}

/// `gmeow gmn encode` reproduces the frozen `claim-basic.gmn` byte-for-byte.
#[test]
fn gmn_encode_matches_the_independent_user_vector() {
    let inputs = Inputs::new();
    let frozen = fs::read_to_string(inputs.path("claim-basic.gmn")).expect("read frozen .gmn");
    gmeow()
        .args([
            "gmn",
            "encode",
            inputs.path("claim-basic.in.ttl").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::eq(frozen));
}

/// `gmeow gmn decode` reconstructs the source triple as canonical N-Quads.
#[test]
fn gmn_decode_reconstructs_the_source() {
    let inputs = Inputs::new();
    gmeow()
        .args([
            "gmn",
            "decode",
            inputs.path("claim-basic.gmn").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "<https://blackcatinformatics.ca/gmeow/cliSubject> \
             <https://blackcatinformatics.ca/gmeow/cliPredicate> \
             <https://blackcatinformatics.ca/gmeow/cliObject> .",
        ));
}

// ── project: the consume-path security-ring filter on the real CLI surface ───────

/// `gmeow gmn project --ring gmnRingTrusted` over the demonstrator admits core + trusted +
/// nato content and EXCLUDES the out-of-ring restricted claim — the filter runs end-to-end on
/// the shipped binary, not just the library. stdout carries the ring-filtered GMN-1 payload.
#[test]
fn gmn_project_excludes_out_of_ring_content_on_the_cli() {
    let inputs = Inputs::new();
    gmeow()
        .args([
            "gmn",
            "project",
            inputs.path("rings.ttl").to_str().unwrap(),
            "--ring",
            "gmnRingTrusted",
            "--lang-module",
            inputs.path("language.ttl").to_str().unwrap(),
        ])
        .assert()
        .success()
        // admitted content is present in the projected GMN-1 …
        .stdout(predicate::str::contains("cliCoreDatum"))
        .stdout(predicate::str::contains("cliTrustedDatum"))
        .stdout(predicate::str::contains("cliNatoDatum"))
        // … and the out-of-ring restricted claim is EXCLUDED (absent from stdout).
        .stdout(predicate::str::contains("cliRestrictedDatum").not())
        .stderr(predicate::str::contains("admitted 3/4 claims, excluded 1"));
}

/// `gmeow gmn project --ring gmnRingNato` exercises the compartment axis: only nato-compartmented
/// content is admitted; plain same-level trusted content is EXCLUDED.
#[test]
fn gmn_project_compartment_axis_excludes_plain_content_on_the_cli() {
    let inputs = Inputs::new();
    gmeow()
        .args([
            "gmn",
            "project",
            inputs.path("rings.ttl").to_str().unwrap(),
            "--ring",
            "gmnRingNato",
            "--lang-module",
            inputs.path("language.ttl").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("cliNatoDatum"))
        .stdout(predicate::str::contains("cliTrustedDatum").not())
        .stdout(predicate::str::contains("cliCoreDatum").not())
        .stderr(predicate::str::contains("admitted 1/4 claims, excluded 3"));
}

/// A tiny `--budget` forces whole-claim elision, disclosed on stderr — never a silent cut.
#[test]
fn gmn_project_budget_discloses_elision_on_the_cli() {
    let inputs = Inputs::new();
    gmeow()
        .args([
            "gmn",
            "project",
            inputs.path("rings.ttl").to_str().unwrap(),
            "--ring",
            "gmnRingTrusted",
            "--budget",
            "20",
            "--lang-module",
            inputs.path("language.ttl").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("admitted claims elided"))
        .stderr(predicate::str::contains("never silently dropped"));
}

/// An unresolvable `--ring` hard-fails (`lang:GmnRingLatticeMalformed`) — no degraded default.
#[test]
fn gmn_project_unknown_ring_hard_fails_on_the_cli() {
    let inputs = Inputs::new();
    gmeow()
        .args([
            "gmn",
            "project",
            inputs.path("rings.ttl").to_str().unwrap(),
            "--ring",
            "gmnRingNotAThing",
            "--lang-module",
            inputs.path("language.ttl").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "not a resolvable gmeow:GmnSecurityRing",
        ));
}

// ── verify: HARD-FAIL on a corrupted corpus and a tampered pack ──────────────────

/// A deliberately corrupted one-vector input directory (one frozen `.gmn` byte tampered) makes
/// `gmn verify` exit NON-ZERO with the byte-mismatch diagnostic — the byte-exact
/// tooth, proven falsifiable.
#[test]
fn gmn_verify_fails_on_a_corrupted_vectors_dir() {
    let inputs = Inputs::new();
    let corrupt = inputs.path("");
    // Append junk to a frozen positive output so its recomputed encoding no longer
    // matches byte-for-byte.
    let target = corrupt.join("claim-basic.gmn");
    let mut bytes = fs::read(&target).expect("read frozen .gmn");
    bytes.extend_from_slice(b"CORRUPT");
    fs::write(&target, bytes).expect("write tampered .gmn");

    gmeow()
        .args(["gmn", "verify", "--vectors", corrupt.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("positives 0/1"))
        .stderr(predicate::str::contains("byte mismatch"));
}

/// A tampered `gmeow:gmnPackRoot` in a supplied pack file makes `gmn verify` exit
/// NON-ZERO — the pack-root tooth.
#[test]
fn gmn_verify_fails_on_a_tampered_pack_root() {
    let inputs = Inputs::new();
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let pack = tmp.path().join("pack.ttl");
    fs::write(
        &pack,
        "<https://blackcatinformatics.ca/gmeow/gmnPackCurrent> \
         <https://blackcatinformatics.ca/gmeow/gmnPackRoot> \"blake3:deadbeef\" .\n",
    )
    .expect("write tampered pack");

    gmeow()
        .args([
            "gmn",
            "verify",
            "--vectors",
            inputs.path("").to_str().unwrap(),
            "--pack",
            pack.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("gmnPackRoot"));
}

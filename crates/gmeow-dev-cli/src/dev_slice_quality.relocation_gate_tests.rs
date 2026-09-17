// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::process::Command;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";

/// The slice IRI a fixture slice local name resolves to — the on-disk shape every
/// real manifest uses, so the gate's joins behave exactly as they do in the repo.
fn slice_iri(local: &str) -> String {
    format!("{NS}slices/{local}")
}

/// The term IRI a fixture shape local name resolves to — the relocation-invariant
/// witness anchor the residue counter records for `gmeow:<local> a sh:NodeShape`.
fn term_iri(local: &str) -> String {
    format!("{NS}{local}")
}

/// One fixture slice: its directory under `slices/`, its local name, and the SHACL
/// node-shape local names its `shapes.ttl` authors (each one ungrounded residue
/// anchored on its own term IRI). A local name of `_` authors an ANONYMOUS
/// `[ sh:path … ]` block instead — a blank-subject construct with no named ancestor,
/// which is [`gmeow_slice_quality::Witness::NonRelocatable`] and can never witness a
/// relocation.
#[derive(Clone)]
struct SliceSpec {
    dir: String,
    local: String,
    shapes: Vec<String>,
}

impl SliceSpec {
    fn new(local: &str, shapes: &[&str]) -> Self {
        Self {
            dir: format!("demo/{local}"),
            local: local.to_owned(),
            shapes: shapes.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

/// One authored `gmeow:CeilingRelocation` in a fixture rubric.
#[derive(Clone)]
struct RelocSpec {
    local: String,
    terms: Vec<String>,
    from: String,
    to: String,
}

impl RelocSpec {
    fn new(local: &str, terms: &[&str], from: &str, to: &str) -> Self {
        Self {
            local: local.to_owned(),
            terms: terms.iter().map(|s| (*s).to_owned()).collect(),
            from: from.to_owned(),
            to: to.to_owned(),
        }
    }
}

/// One whole repository state: the slices and their residue, the committed
/// `(slice local, ceiling count)` cells, and the authored relocation declarations.
#[derive(Clone, Default)]
struct State {
    slices: Vec<SliceSpec>,
    ceilings: Vec<(String, u64)>,
    relocations: Vec<RelocSpec>,
}

impl State {
    fn ceiling(mut self, local: &str, count: u64) -> Self {
        self.ceilings.push((local.to_owned(), count));
        self
    }
    fn reloc(mut self, spec: RelocSpec) -> Self {
        self.relocations.push(spec);
        self
    }
}

fn state(slices: &[SliceSpec]) -> State {
    State {
        slices: slices.to_vec(),
        ..State::default()
    }
}

/// A fixture repository whose tree lives in an owned temp directory: dropping the
/// fixture drops the [`tempfile::TempDir`], which removes the tree — on success, on
/// early return, and while unwinding from a failed assertion alike.
struct RepoFixture {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

fn git(root: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .env("HOME", root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The fixture's `crates/` tree: ONE Rust file defining an item per implemented
/// axis primitive, so the gate's axis→producer BINDING gate (which resolves every
/// rubric producer to a real Rust item under `<root>/crates`) is satisfied. The two
/// exemption producer symbols are deliberately ABSENT so the staleness gate stays
/// silent — an exemption whose producer resolved would red.
fn producer_stub_source() -> String {
    let mut out = String::from("// fixture producer stubs\n");
    for producer in gmeow_slice_quality::axes::IMPLEMENTED {
        out.push_str(&format!("fn {producer}() {{}}\n"));
    }
    out
}

/// A structurally-complete fixture rubric module: a one-rung ladder, one
/// `gmeow:QualityAxis` per implemented primitive (each with a `0.0` threshold so
/// nothing is floored out), the two dated exemptions the completeness gate demands
/// for the unlanded `gmn` / `docs-panels` projection surfaces, the guarded `sh`
/// vocabulary registry, and this state's ceiling commitments + relocation
/// declarations.
fn rubric_module(state: &State) -> String {
    let mut out = format!(
        r#"@prefix gmeow: <{NS}> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:thr0 a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularyNamespace "http://www.w3.org/ns/shacl#"^^xsd:anyURI ;
    gmeow:vocabularySubsumedBy <{LOGIC_SLICE}> ;
    gmeow:vocabularyOwner <{LOGIC_SLICE}> ;
    gmeow:vocabularyCountKind "countKindShape" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:soundUnder .
"#
    );
    for producer in gmeow_slice_quality::axes::IMPLEMENTED {
        out.push_str(&format!(
                "gmeow:axis-{producer} a gmeow:QualityAxis ; gmeow:axisProducer \"{producer}\" ; gmeow:axisDimension gmeow:dimFixture ; gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thr0 .\n"
            ));
    }
    // The two projection surfaces with no landed axis must each carry a dated
    // exemption naming their producer symbol, or the completeness gate reds.
    for (local, producer) in [
        ("exGmn", "GmnProjectionTarget"),
        ("exPanels", "DocMaturityPanels"),
    ] {
        out.push_str(&format!(
                "gmeow:{local} a gmeow:AxisExemption ; gmeow:exemptsAxis gmeow:axis-grounding_axis ; gmeow:exemptionReason \"the producer is genuinely unlanded in this fixture\" ; gmeow:exemptionDate \"2026-07-28\" ; gmeow:exemptionProducer \"{producer}\" .\n"
            ));
    }
    for (local, count) in &state.ceilings {
        out.push_str(&format!(
                "gmeow:pcc-{local}-sh a gmeow:ProjectionCeilingCommitment ; gmeow:ceilingSlice <{}> ; gmeow:ceilingVocabulary gmeow:projVocab-sh ; gmeow:ceilingCount {count} .\n",
                slice_iri(local)
            ));
    }
    for r in &state.relocations {
        let terms = r
            .terms
            .iter()
            .map(|t| format!("<{}>", term_iri(t)))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
                "gmeow:{} a gmeow:CeilingRelocation ; gmeow:relocationTerm {terms} ; gmeow:relocationFromSlice <{}> ; gmeow:relocationToSlice <{}> ; gmeow:relocationDate \"2026-07-28\" .\n",
                r.local,
                slice_iri(&r.from),
                slice_iri(&r.to)
            ));
    }
    out
}

/// A slice `shapes.ttl` authoring one ungrounded residue construct per local name.
fn shapes_doc(shapes: &[String]) -> String {
    let mut out = format!("@prefix sh: <http://www.w3.org/ns/shacl#> .\n@prefix gmeow: <{NS}> .\n");
    for local in shapes {
        if local == "_" {
            // A blank subject with no named sh:property/sh:node ancestor — a
            // NonRelocatable construct that can never witness a relocation.
            out.push_str("[] sh:path gmeow:anonymousPath .\n");
        } else {
            out.push_str(&format!("gmeow:{local} a sh:NodeShape .\n"));
        }
    }
    out
}

/// Write a whole repository state onto `root` (creating every directory).
fn write_state(root: &std::path::Path, state: &State) {
    let rubric_dir = root.join("slices/core/slice-quality-rubric");
    std::fs::create_dir_all(&rubric_dir).unwrap();
    std::fs::write(
        rubric_dir.join("manifest.ttl"),
        format!(
            "@prefix gmeow: <{NS}> .\n<{}> a gmeow:Slice .\n",
            slice_iri("slice-quality-rubric")
        ),
    )
    .unwrap();
    std::fs::write(rubric_dir.join("module.ttl"), rubric_module(state)).unwrap();
    std::fs::create_dir_all(root.join("crates")).unwrap();
    std::fs::write(root.join("crates/producers.rs"), producer_stub_source()).unwrap();

    // Rewrite every demo slice from scratch so a state transition can DELETE a
    // shape (the departure half of the relocation witness) rather than only add.
    let demo_root = root.join("slices/demo");
    let _ = std::fs::remove_dir_all(&demo_root);
    for s in &state.slices {
        let dir = root.join("slices").join(&s.dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.ttl"),
            format!(
                "@prefix gmeow: <{NS}> .\n<{}> a gmeow:Slice .\n",
                slice_iri(&s.local)
            ),
        )
        .unwrap();
        std::fs::write(dir.join("module.ttl"), format!("@prefix gmeow: <{NS}> .\n")).unwrap();
        std::fs::write(dir.join("shapes.ttl"), shapes_doc(&s.shapes)).unwrap();
    }
}

/// Build the fixture repository at `base`, commit it, point `origin/main` at the
/// commit (the comparand [`resolve_base_ref`] resolves), then overwrite the working
/// tree with `working`. Returns the fixture; the caller drives
/// [`slice_quality_gate_at`] over `fixture.root`.
fn fixture(base: &State, working: &State) -> RepoFixture {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-reloc-")
        .tempdir()
        .expect("create temp dir");
    let root = tmp.path().to_path_buf();
    let fx = RepoFixture {
        _tmp: tmp,
        root: root.clone(),
    };

    write_state(&root, base);
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    // The gate diffs against `git merge-base HEAD origin/main`; in a fixture repo
    // that ref must exist or the whole comparison is a loud SKIP.
    git(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);

    write_state(&root, working);
    record_quality_corpus(&root);
    fx
}

/// Project the recorded quality-assessment corpus into the fixture, exactly as the
/// pipeline's DAG-root sweep does.
///
/// The gate no longer scores slices itself: it LOADS `graph/quality-assessment` and
/// hard-fails when the record is absent or stale (that refusal is the whole warrant
/// for reading a record instead of recomputing). A fixture repository therefore has
/// to carry the record too, or every scenario below reds on the missing projection
/// before it ever reaches the ceiling passes it is testing — and the scenarios that
/// EXPECT a non-zero exit would go on passing for entirely the wrong reason.
///
/// It is written from [`gmeow_slice_quality::assessment_artifacts`], the same single
/// producer the pipeline stage uses, so the fixture record is what the pipeline would
/// have written and never a hand-built stand-in. It runs LAST, after the working tree
/// is final, because the corpus stamps a freshness fingerprint over the sources it
/// scored and the gate recomputes that fingerprint over the tree it finds.
fn record_quality_corpus(root: &std::path::Path) {
    let artifacts =
        gmeow_slice_quality::assessment_artifacts(root).expect("score the fixture corpus");
    let recorded = root.join(gmeow_slice_quality::read::RECORDED_CORPUS_PATH);
    std::fs::create_dir_all(recorded.parent().expect("the corpus path has a parent")).unwrap();
    std::fs::write(&recorded, artifacts.nquads.as_bytes()).unwrap();
}

/// The two slices every scenario uses: `alpha` (the source) and `beta` (the
/// destination).
fn alpha(shapes: &[&str]) -> SliceSpec {
    SliceSpec::new("alpha", shapes)
}
fn beta(shapes: &[&str]) -> SliceSpec {
    SliceSpec::new("beta", shapes)
}

fn reloc_s1() -> RelocSpec {
    RelocSpec::new("relocS1", &["S1"], "alpha", "beta")
}

/// The relocation-aware ceiling comparator's OWN structured verdict for `root` —
/// the same sequence [`slice_quality_gate_at`] composes internally
/// (`load_repo_rubric` → `score_slices_with_rubric` → `ceilings_from_rubric` /
/// `measure_repo_residue_constructs` → `resolve_base_ref` → `measure_base_residues`
/// / `derive_edge_reasons` → `projection_ceiling_monotonicity`), called directly
/// here so a scenario can assert on its OWN violation message — a bare
/// `assert_ne!(slice_quality_gate_at(...), 0, ...)` cannot distinguish "this
/// scenario's intended refusal" from an unrelated gate (binding/completeness/coat)
/// reddening first, which would leave every one of these fixtures green on the
/// assertion while silently exercising nothing.
///
/// The comparison itself — base residue measurement, edge-reason derivation,
/// default-ceiling projection, and the `projection_ceiling_monotonicity` call —
/// is [`ceiling_rebalance`], the SAME function [`slice_quality_gate_at`] calls in
/// production; only the surrounding rubric/score/measure plumbing (identical in
/// shape to production, but without its other five checks interleaved) and the
/// `needed` pre-filter (every fixture slice, not just the implicated ones) are
/// re-composed here.
fn rebalance_for(root: &std::path::Path) -> gmeow_slice_quality::gate::CeilingRebalance {
    let rubric = gmeow_slice_quality::load_repo_rubric(root).expect("fixture rubric loads");
    let vocabularies = &rubric.floors.vocabularies;
    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    let score_results = gmeow_slice_quality::score_slices_with_rubric(root, &dirs, &rubric);
    let mut slice_dirs: Vec<(&Path, String)> = Vec::with_capacity(dirs.len());
    for (dir, result) in dirs.iter().zip(&score_results) {
        let report = result.as_ref().expect("fixture slice scores");
        slice_dirs.push((dir.as_path(), report.assessment.slice.clone()));
    }
    let working_ceilings = ceilings_from_rubric(&rubric);
    let working_constructs =
        gmeow_slice_quality::measure_repo_residue_constructs(root, vocabularies)
            .expect("fixture working residue measures");
    let working_residues: std::collections::BTreeMap<(String, String), u64> = working_constructs
        .iter()
        .map(|(key, constructs)| (key.clone(), constructs.len() as u64))
        .collect();
    let base = match resolve_base_ref(root) {
        BaseRef::Resolved(base) => base,
        BaseRef::NoUpstream(reason) => panic!("fixture must resolve a base ref: {reason}"),
        BaseRef::Unresolvable(reason) => panic!("fixture base ref unresolvable: {reason}"),
    };
    let declarations = &rubric.floors.relocations;
    // Every fixture slice — the fixtures are tiny (2-3 slices), so there is no
    // benefit to the production path's "only implicated slices" pre-filter.
    let needed: std::collections::BTreeSet<String> =
        slice_dirs.iter().map(|(_, iri)| iri.clone()).collect();
    let base_rubric = base_rubric_at(root, &base)
        .expect("fixture base rubric reads")
        .expect("fixture base rubric is present (the fixture always commits one)");
    let base_ceilings = ceilings_from_rubric(&base_rubric);
    ceiling_rebalance(&RebalanceInputs {
        root,
        base: &base,
        vocabularies,
        slice_dirs: &slice_dirs,
        declarations,
        base_ceilings: &base_ceilings,
        working_ceilings: &working_ceilings,
        working_residues: &working_residues,
        working_constructs: &working_constructs,
        needed: &needed,
    })
    .expect("fixture ceiling rebalance composes")
}

/// Assert `root`'s relocation-aware rebalance carries a violation containing
/// `needle` — the scenario's OWN reason, not merely SOME red somewhere.
#[track_caller]
fn assert_violation_contains(root: &std::path::Path, needle: &str) {
    let rebalance = rebalance_for(root);
    assert!(
        rebalance.violations.iter().any(|v| v.contains(needle)),
        "expected a violation containing {needle:?}; got: {:#?}",
        rebalance.violations
    );
}

#[test]
fn declared_witnessed_and_paid_transfer_is_accepted() {
    // S1 genuinely DEPARTS alpha and ARRIVES at beta; alpha's ceiling falls by
    // exactly one and beta's brand-new ceiling is pinned to its measured residue.
    // The base ceiling is re-projected through the declared relocation and the
    // unchanged lower-only comparison then holds — the gate is green.
    let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
    let working = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 1)
        .reloc(reloc_s1());
    let fx = fixture(&base, &working);
    assert_eq!(
        slice_quality_gate_at(&fx.root),
        0,
        "a declared, witnessed, funded, and pinned transfer must be accepted"
    );
}

#[test]
fn a_copy_rather_than_a_move_is_rejected() {
    // S1 is COPIED: it stays in alpha AND appears in beta — two second sources of
    // truth, strictly worse than one. Nothing departed, so the departure half of
    // the witness is empty and the raise is unwitnessed. Alpha's ceiling is
    // unchanged (nothing left it), so nothing funds beta either.
    let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
    let working = state(&[alpha(&["S1", "S2", "S3"]), beta(&["S1"])])
        .ceiling("alpha", 3)
        .ceiling("beta", 1)
        .reloc(reloc_s1());
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "a construct COPIED into a second slice must never be netted as a transfer"
    );
    // The SPECIFIC refusal: the declaration names S1 but NONE of it departed
    // alpha — a fixture drift that reds some unrelated gate first would still
    // pass the exit-code check above without ever exercising this reason.
    assert_violation_contains(
        &fx.root,
        &format!("but NONE of them departed {}", slice_iri("alpha")),
    );
}

#[test]
fn a_lowering_with_no_shared_key_is_rejected() {
    // S3 genuinely DEPARTS alpha (so the declaration itself is corroborated) and
    // alpha's ceiling duly falls by one — but what beta gained is S9, freshly
    // authored there, not S3. `departed(alpha) ∩ arrived(beta)` is empty, so the
    // edge carries no capacity and beta's raise is unwitnessed.
    let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
    let working = state(&[alpha(&["S1", "S2"]), beta(&["S9"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 1)
        .reloc(RelocSpec::new("relocS3", &["S3"], "alpha", "beta"));
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "a lowering that shares no witnessed key with the raise funds nothing"
    );
    // The SPECIFIC refusal: S9 arrived at beta but no declaration covers IT
    // (the declaration names S3, which never arrived anywhere).
    assert_violation_contains(
        &fx.root,
        &format!(
            "undeclared: term {} moved but no relocation declaration covers it",
            term_iri("S9")
        ),
    );
}

#[test]
fn lowering_stale_headroom_buys_nothing() {
    // Alpha's committed ceiling is 9 against a measured residue of 3 — six units of
    // DEAD headroom. It lowers to 2 (a five-unit drop) while only ONE construct
    // actually departed, so the supply clamp (`min(lowering, |departed ∩ declared|)`)
    // caps the credit at one. Beta asks for three (S1 arrived plus two freshly
    // authored), so two units are unpaid.
    let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 9);
    let working = state(&[alpha(&["S2", "S3"]), beta(&["S1", "N1", "N2"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 3)
        .reloc(reloc_s1());
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "lowering dead headroom surrenders no authoring and must never buy live headroom"
    );
    // The SPECIFIC refusal: only ONE of the THREE units beta asks for is
    // witnessed (the supply clamp caps the credit at the one construct that
    // actually departed alpha).
    assert_violation_contains(&fx.root, "unwitnessed: 1 of 3");
}

#[test]
fn a_raise_not_pinned_to_measured_is_rejected() {
    // The transfer is fully witnessed and fully funded — S1 departs alpha, arrives
    // at beta, and alpha's ceiling falls by exactly one — but beta ALSO deletes two
    // of its own pre-existing constructs and commits 4 against a measured residue
    // of 2. The flow saturates, the aggregate total is unchanged (6 → 6), and the
    // ONLY thing wrong is that the relocation banked two units of durable surplus
    // headroom, spendable forever with no witness.
    let base = state(&[alpha(&["S1", "A2", "A3"]), beta(&["B1", "B2", "B3"])])
        .ceiling("alpha", 3)
        .ceiling("beta", 3);
    let working = state(&[alpha(&["A2", "A3"]), beta(&["S1", "B1"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 4)
        .reloc(reloc_s1());
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "a raised ceiling must equal the destination's measured residue"
    );
    // The SPECIFIC refusal: the transfer is fully witnessed and fully funded
    // (demand is satisfied, residual 0) — the ONLY thing wrong is the pin.
    assert_violation_contains(&fx.root, "is not pinned to its measured residue");
}

#[test]
fn an_undeclared_move_is_rejected() {
    // Exactly the accepted scenario with the gmeow:CeilingRelocation deleted: the
    // witness alone authorizes nothing, because the declaration is a MAINTAINER
    // decision the tool never writes.
    let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
    let working = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 1);
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "an undeclared move authorizes no adjustment — the tool never writes the declaration"
    );
    // The SPECIFIC refusal: S1 genuinely arrived at beta, but no declaration
    // names it — the witness alone authorizes nothing.
    assert_violation_contains(
        &fx.root,
        &format!(
            "undeclared: term {} moved but no relocation declaration covers it",
            term_iri("S1")
        ),
    );
}

#[test]
fn a_stale_declaration_is_rejected() {
    // S1 already sits at beta on BOTH sides and nothing departs alpha: the
    // relocation is fully ABSORBED at the merge base. The declaration is dead and
    // must red until deleted, or declarations accumulate into standing permits.
    let base = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 1);
    let working = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 1)
        .reloc(reloc_s1());
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "a declaration whose relocation is fully absorbed at base is dead and must red"
    );
    // The SPECIFIC refusal: staleness, named as such — never conflated with an
    // ordinary unwitnessed/unpaid shortfall.
    assert_violation_contains(&fx.root, "stale-declaration");
    assert_violation_contains(&fx.root, "fully ABSORBED at the merge base");
}

#[test]
fn a_blank_subject_construct_cannot_witness_a_relocation() {
    // S1 genuinely DEPARTS alpha (the declaration is corroborated on its source
    // side) and alpha's ceiling falls by one — but what beta actually gained is an
    // anonymous `[ sh:path … ]` block: a blank subject with no named
    // sh:property/sh:node ancestor, hence NO cross-view identity at all. It can
    // never be the arrival half of a witness, so beta's raise is unwitnessed and
    // the refusal says so by name.
    let base = state(&[alpha(&["S1", "_"]), beta(&[])]).ceiling("alpha", 2);
    let working = state(&[alpha(&["_"]), beta(&["_"])])
        .ceiling("alpha", 1)
        .ceiling("beta", 1)
        .reloc(reloc_s1());
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "a blank-subject construct with no named anchor has no cross-view identity"
    );
    // The SPECIFIC refusal: named as non-relocatable, not merely "unwitnessed"
    // (which would also be true, but would not name WHY).
    assert_violation_contains(&fx.root, "non-relocatable: 1 blank-subject construct");
}

#[test]
fn one_source_cannot_fund_two_destinations() {
    // The exact case a per-destination GREEDY sum gets wrong. Alpha lowers by
    // THREE, and the three constructs it lost landed in BOTH beta and gamma, each
    // of which raises by three. Every arrival at both destinations is genuinely
    // witnessed (each key departed alpha and arrived there), so a greedy
    // per-destination accounting sees `witnessed >= demand` twice and accepts both
    // — then the aggregate conservation check reds with "Σ increased", a verdict
    // that contradicts its own audit lines and names no culprit.
    //
    // The transport solution instead saturates ONE destination, refuses the other,
    // names the blocking edge, and prints the residual demand.
    let base = state(&[
        alpha(&["S1", "S2", "S3", "A4"]),
        beta(&[]),
        SliceSpec::new("gamma", &[]),
    ])
    .ceiling("alpha", 4);
    let working = state(&[
        alpha(&["A4"]),
        beta(&["S1", "S2", "S3"]),
        SliceSpec::new("gamma", &["S1", "S2", "S3"]),
    ])
    .ceiling("alpha", 1)
    .ceiling("beta", 3)
    .ceiling("gamma", 3)
    .reloc(RelocSpec::new(
        "relocBeta",
        &["S1", "S2", "S3"],
        "alpha",
        "beta",
    ))
    .reloc(RelocSpec::new(
        "relocGamma",
        &["S1", "S2", "S3"],
        "alpha",
        "gamma",
    ));
    let fx = fixture(&base, &working);
    assert_ne!(
        slice_quality_gate_at(&fx.root),
        0,
        "one three-unit lowering can fund exactly one three-unit arrival, never two"
    );
    // The SPECIFIC refusal: the transport solution names the BLOCKING edge (the
    // destination its single source could not also fund) — never the
    // aggregate-conservation "Σ increased" verdict a greedy per-destination sum
    // would produce instead.
    assert_violation_contains(&fx.root, "blocking edge");
}

#[test]
fn a_relocation_into_a_brand_new_cell_passes_the_grandfather_gate() {
    // The destination cell has NO committed ceiling at base at all, so the path the
    // raise actually takes is invariant 3 (the grandfather gate), not the
    // monotonicity comparator. The same declaration, witness, and flow apply — a
    // rule that held at one ceiling gate and not the other would not be a rule.
    // (This is the accepted scenario stated from the grandfather side, with TWO
    // terms moving so the transported amount is more than a single unit.)
    let base = state(&[alpha(&["S1", "S4", "S2"]), beta(&[])]).ceiling("alpha", 3);
    let working = state(&[alpha(&["S2"]), beta(&["S1", "S4"])])
        .ceiling("alpha", 1)
        .ceiling("beta", 2)
        .reloc(RelocSpec::new("relocPair", &["S1", "S4"], "alpha", "beta"));
    let fx = fixture(&base, &working);
    assert_eq!(
        slice_quality_gate_at(&fx.root),
        0,
        "the grandfather gate honours the same relocation adjustment the monotonicity comparator does"
    );
}

#[test]
fn a_legitimate_grandfathered_addition_is_not_red_by_the_conservation_check() {
    // The workflow the ratchet documentation advertises: a slice with PRE-EXISTING
    // residue commits a matching ceiling for the first time. Nothing moved and no
    // declaration exists; the addition is governed by invariant 3 alone. An
    // UNSCOPED Σ would rise here and false-red — the conservation check is scoped
    // to base ∩ working precisely so it does not.
    let base = state(&[alpha(&["S1", "S2"]), beta(&["B1"])]).ceiling("alpha", 2);
    let working = state(&[alpha(&["S1", "S2"]), beta(&["B1"])])
        .ceiling("alpha", 2)
        .ceiling("beta", 1);
    let fx = fixture(&base, &working);
    assert_eq!(
        slice_quality_gate_at(&fx.root),
        0,
        "a new ceiling grandfathering pre-existing base residue is exactly what invariant 3 permits"
    );
}

#[test]
fn an_empty_declaration_set_reproduces_the_pre_relocation_behaviour() {
    // With no gmeow:CeilingRelocation anywhere, inflow is identically zero and the
    // rule degenerates to the original comparator. A hold is clean; a bare raise on
    // a shared key reds; a lowering is clean.
    let base = state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2);
    let held = fixture(
        &base,
        &state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2),
    );
    assert_eq!(
        slice_quality_gate_at(&held.root),
        0,
        "holding a ceiling is clean with no declarations"
    );

    let lowered_base = state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2);
    let lowered = fixture(
        &lowered_base,
        &state(&[alpha(&["S1"]), beta(&[])]).ceiling("alpha", 1),
    );
    assert_eq!(
        slice_quality_gate_at(&lowered.root),
        0,
        "lowering a ceiling to the new measured residue is clean with no declarations"
    );

    let raised_base = state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2);
    let raised = fixture(
        &raised_base,
        &state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3),
    );
    assert_ne!(
        slice_quality_gate_at(&raised.root),
        0,
        "a bare raise on a shared key still reds with no declarations — unchanged behaviour"
    );
}

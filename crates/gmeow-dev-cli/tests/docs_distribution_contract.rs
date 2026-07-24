// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-gated distribution contract for the external documentation
//! distribution. Every acceptance criterion becomes a self-policing gate here so a
//! future edit that quietly regresses AC2 (segmentation/bijection), AC3
//! (source-backed export), AC4 (zstd-rsyncable L12), AC5 (forbidden-embed / no size
//! budget / no carrier digest), or AC6 (single authority) fails loudly at
//! `cargo nextest run -p gmeow-dev-cli`, not only at an expensive `make check`.
//!
//! Modeled on `crates/gmeow-dev-cli/tests/make_gate_contract.rs`: structural
//! assertions read committed source/config files via `std::fs` under `repo_root()`;
//! the one runtime group (F1, the consumer verb) drives the real production API
//! end-to-end over a temp directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ── repo-anchored file readers (mirrors make_gate_contract.rs) ─────────────────────

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn makefile() -> String {
    read("Makefile")
}

fn pages_workflow() -> String {
    read(".github/workflows/pages.yml")
}

fn dev_project_source() -> String {
    read("crates/gmeow-dev-cli/src/dev_project.rs")
}

fn distribution_catalog_source() -> String {
    read("crates/pipeline/src/stages/distribution_catalog.rs")
}

fn docs_distribution_source() -> String {
    read("crates/pipeline/src/docs_distribution.rs")
}

fn bundle_blobs_source() -> String {
    read("crates/pipeline/src/bundle_blobs.rs")
}

fn reasoning_graphs_source() -> String {
    read("crates/logic/src/reasoning_graphs.rs")
}

fn gts_profile_source() -> String {
    read("crates/pipeline/src/gts_profile.rs")
}

// ── Makefile target parsing (mirrors make_gate_contract.rs) ────────────────────────

fn target_header_index(source: &str, target: &str) -> usize {
    let prefix = format!("{target}:");
    source
        .lines()
        .position(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing Make target {target}"))
}

fn target_header<'a>(source: &'a str, target: &str) -> &'a str {
    source
        .lines()
        .nth(target_header_index(source, target))
        .expect("target header index is in bounds")
}

fn target_recipe(source: &str, target: &str) -> String {
    source
        .lines()
        .skip(target_header_index(source, target) + 1)
        .take_while(|line| line.starts_with('\t') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// ── AC3 — source-backed export preserved ────────────────────────────────────────────

#[test]
fn ac3_pages_workflow_renders_from_source_and_uploads_ontology_docs() {
    let source = pages_workflow();
    assert!(
        source.contains("run: make sync SYNC_MODE=update SYNC_OUTPUTS=docs"),
        "AC3 (source-backed export): .github/workflows/pages.yml must render the \
         Pages site from canonical sources via the exact step `run: make sync SYNC_MODE=update \
         SYNC_OUTPUTS=docs` — it must never publish a stale or hand-copied tree"
    );
    assert!(
        source.contains("uses: actions/upload-pages-artifact"),
        "AC3: pages.yml must upload the freshly rendered tree via an \
         `uses: actions/upload-pages-artifact` step"
    );
    assert!(
        source.contains("path: ontology-docs"),
        "AC3: the upload-pages-artifact step must upload `path: ontology-docs` (the \
         source-rendered site tree written by `sync_docs`), never a different or generated/ path"
    );
}

#[test]
fn ac3_makefile_sync_delegates_to_gmeow_dev_sync() {
    let source = makefile();
    let recipe = target_recipe(&source, "sync");
    assert!(
        recipe.contains("$(GMEOW_DEV) sync"),
        "AC3: the standalone Makefile `sync:` recipe must delegate to the single \
         `$(GMEOW_DEV) sync` producer binary invocation; recipe was: {recipe:?}"
    );
}

#[test]
fn ac3_makefile_release_publish_attaches_docs_tar_via_docs_package() {
    let source = makefile();
    let recipe = target_recipe(&source, "release-publish");
    assert!(
        recipe.contains("$(GMEOW_DEV) docs-package"),
        "AC3: `release-publish` must package the external docs distribution \
         via `$(GMEOW_DEV) docs-package`; recipe was: {recipe:?}"
    );
    assert!(
        recipe.contains("dist/gmeow-docs.tar"),
        "AC3: `release-publish` must attach `dist/gmeow-docs.tar` as a \
         content-addressed release asset; recipe was: {recipe:?}"
    );
}

// ── AC2 / AC6 — segmentation, single authority, bijection, boundary ────────────────

/// The eight canonical distribution slugs this contract ships: the four doc-render
/// formats (`gmeow_docs::formats::DocFormat::ALL`) plus the four serialization
/// formats.
const CANONICAL_SLUGS: [&str; 8] = [
    "site", "mdbook", "pdf", "snippets", "pydantic", "okf", "jsonld", "yamlld",
];

/// Extract every `dist/gmeow-docs/<slug>` literal's `<slug>` from `source`
/// (deduped). Used against `dev_project.rs` to recover the set of destinations
/// `sync_docs` actually reconciles to disk — an extraction, not a hardcoded list,
/// so this test tracks the real destinations array rather than a copy of it.
fn dist_gmeow_docs_slugs(source: &str) -> BTreeSet<String> {
    let needle = "dist/gmeow-docs/";
    source
        .match_indices(needle)
        .filter_map(|(pos, matched)| {
            let tail = &source[pos + matched.len()..];
            let slug: String = tail
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            (!slug.is_empty()).then_some(slug)
        })
        .collect()
}

#[test]
fn ac2_ac6_eight_canonical_slugs_bijection_between_producers() {
    let expected: BTreeSet<String> = CANONICAL_SLUGS.iter().map(|s| s.to_string()).collect();

    // Producer 1: the rendered destinations `sync_docs` reconciles to disk.
    let dev_project = dev_project_source();
    let mut rendered_slugs = dist_gmeow_docs_slugs(&dev_project);
    let had_manifest = rendered_slugs.remove("manifest");
    assert!(
        had_manifest,
        "AC2/AC6: dev_project.rs `sync_docs` must reconcile a \
         `dist/gmeow-docs/manifest` destination for the DCAT release manifest, separate from the \
         8 canonical distributions — slug extraction found none"
    );
    assert_eq!(
        rendered_slugs, expected,
        "AC2/AC6 (single segmentation authority): sync_docs's rendered \
         `dist/gmeow-docs/<slug>` destinations must be EXACTLY the 8 canonical distribution slugs \
         {CANONICAL_SLUGS:?}; got {rendered_slugs:?}"
    );

    // Producer 2: the catalog schema — the WHOLE declared set, so a ninth declared
    // distribution is caught as a set mismatch instead of being silently subsumed by a
    // subset-presence check.
    let catalog_slugs: BTreeSet<String> =
        gmeow_pipeline::stages::distribution_catalog::declared_distribution_slugs()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
    assert_eq!(
        catalog_slugs, expected,
        "AC2/AC6: distribution_catalog.rs must declare EXACTLY the 8 canonical distribution \
         slugs {CANONICAL_SLUGS:?}; found {catalog_slugs:?}"
    );

    // Bijection both directions: rendered slugs == catalog-declared slugs == canonical set.
    assert_eq!(
        rendered_slugs, catalog_slugs,
        "AC2/AC6: the rendered docs destinations and the distribution catalog schema must \
         name the SAME slug set (bijection catalog <-> rendered); rendered={rendered_slugs:?} \
         catalog={catalog_slugs:?}"
    );
}

#[test]
fn site_sub_assets_are_priced_but_never_enter_the_eight_slug_bijection() {
    let bijection: BTreeSet<String> = CANONICAL_SLUGS.iter().map(|s| s.to_string()).collect();
    let sub_assets: BTreeSet<String> =
        gmeow_pipeline::stages::distribution_catalog::declared_site_sub_asset_slugs()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
    // The interactive engines + browser bundle ARE priced as first-class sub-assets…
    assert!(
        !sub_assets.is_empty(),
        "the vendored interactive engines + browser bundle must be priced as site sub-assets"
    );
    // …but they are SUB-ASSETS of `site`, never top-level distributions, so the
    // eight-slug bijection is untouched (a sub-asset leaking into it would be a ninth
    // distribution, which the bijection test above would also catch).
    assert!(
        sub_assets.is_disjoint(&bijection),
        "AC2/AC6: site sub-assets {sub_assets:?} must be DISJOINT from the eight-slug \
         distribution bijection {CANONICAL_SLUGS:?} — they are sub-assets, not distributions"
    );
    // The release-time digest producer prices exactly the declared set (one authority).
    let priced: BTreeSet<String> =
        gmeow_pipeline::stages::distribution_catalog::site_sub_asset_pricing()
            .into_iter()
            .map(|(slug, _, _)| slug.to_string())
            .collect();
    assert_eq!(
        priced, sub_assets,
        "the release-time sub-asset pricing set must equal the catalog-declared sub-asset set"
    );
}

/// The exact `let destinations = [ ... ];` array-literal slice of `sync_docs`,
/// isolated from the rest of the file (which legitimately references unrelated
/// `generated/…` INPUT sources, e.g. the `AXIOMS` array feeding the print render).
fn destinations_block(source: &str) -> &str {
    let start = source
        .find("let destinations = [")
        .expect("dev_project.rs sync_docs declares `let destinations = [...]`");
    let tail = &source[start..];
    let end = tail
        .find("];")
        .expect("the `destinations` array literal is closed with `];`");
    &tail[..end]
}

#[test]
fn ac2_ac6_docs_destinations_stay_under_dist_or_ontology_docs_never_generated() {
    let source = dev_project_source();
    let block = destinations_block(&source);
    assert!(
        block.contains("\"ontology-docs\"") && block.contains("\"dist/gmeow-docs/"),
        "AC2/AC6: `destinations` array extraction looks broken (found neither an \
         ontology-docs nor a dist/gmeow-docs base): {block}"
    );
    assert!(
        !block.contains("generated/"),
        "AC2/AC6 (boundary): every docs destination base in sync_docs's \
         `destinations` array must live under `dist/` or `ontology-docs`, NEVER `generated/`; \
         found a `generated/` literal in the destinations block: {block}"
    );
}

#[test]
fn ac2_ac6_single_serializer_authority_no_reimplementation() {
    let dev_project = dev_project_source();
    assert!(
        dev_project.contains("render_serialization_distributions"),
        "AC2/AC6 (single serializer authority): dev_project.rs must render the OKF \
         distribution through `gmeow_pipeline::docs_distribution::render_serialization_distributions`, \
         never a re-implemented serializer"
    );
    for banned in [
        "fn render_okf",
        "fn serialize_graph(",
        "fn serialize_graph_yaml(",
    ] {
        assert!(
            !dev_project.contains(banned),
            "AC2/AC6: dev_project.rs must not re-implement `{banned}` — okf/JSON-LD/YAML-LD \
             serialization has exactly one authority and dev_project.rs must only call it"
        );
    }
    // JSON-LD-star / YAML-LD-star are NOT re-serialized in the docs fanout — `make
    // build` already wrote dist/gmeow.jsonld / dist/gmeow.yamlld off the identical
    // committed-bundle authority, and sync_docs must only REFERENCE that single build
    // output, never call the serializer itself.
    for banned_call in [
        "yaml_ld::serialize_graph(",
        "yaml_ld::serialize_graph_yaml(",
    ] {
        assert!(
            !dev_project.contains(banned_call),
            "AC2/AC6: dev_project.rs's sync_docs must not call `{banned_call}` — it must \
             reference the single `make build` output (dist/gmeow.jsonld / dist/gmeow.yamlld) \
             via `read_build_serialization_tree`, never re-serialize"
        );
    }
    assert!(
        dev_project.contains("read_build_serialization_tree"),
        "AC2/AC6: dev_project.rs's sync_docs must reference the jsonld/yamlld docs-distribution \
         trees via `gmeow_pipeline::docs_distribution::read_build_serialization_tree` over the \
         build-produced dist/gmeow.jsonld / dist/gmeow.yamlld outputs"
    );
    for build_output_path in ["yaml_ld::JSON_LD_PATH", "yaml_ld::YAML_LD_PATH"] {
        assert!(
            dev_project.contains(build_output_path),
            "AC2/AC6: dev_project.rs must source the jsonld/yamlld build-output paths from the \
             single declared constant `{build_output_path}` (`gmeow_pipeline::stages::yaml_ld`), \
             never a re-typed literal path that could drift"
        );
    }

    let docs_distribution = docs_distribution_source();
    assert!(
        docs_distribution.contains("okf::render_okf"),
        "AC2/AC6: docs_distribution.rs's render_serialization_distributions must call the \
         single authority `okf::render_okf`, never a re-derived serializer"
    );
    for banned in [
        "fn render_okf",
        "fn serialize_graph(",
        "fn serialize_graph_yaml(",
    ] {
        assert!(
            !docs_distribution.contains(banned),
            "AC2/AC6: docs_distribution.rs must not locally re-implement `{banned}` — it is a \
             CALLER of the single serializer authority, never a second source of truth"
        );
    }
    // docs_distribution.rs must no longer CALL the JSON-LD-star / YAML-LD-star
    // serializer at all — that would be the exact duplicate render this gap closes. It
    // may (and does) still NAME the serializer functions in its module doc comment to
    // explain why they are absent, so this checks for a call form (trailing `(`), not a
    // bare substring.
    for banned_call in [
        "yaml_ld::serialize_graph(",
        "yaml_ld::serialize_graph_yaml(",
    ] {
        assert!(
            !docs_distribution.contains(banned_call),
            "AC2/AC6: docs_distribution.rs must not call `{banned_call}` — \
             render_serialization_distributions renders OKF ONLY; jsonld/yamlld are referenced \
             from the `make build` output elsewhere, never re-serialized here"
        );
    }
    assert!(
        docs_distribution.contains("pub fn read_build_serialization_tree"),
        "AC2/AC6: docs_distribution.rs must expose `read_build_serialization_tree`, the single \
         function that references the build-produced dist/gmeow.jsonld / dist/gmeow.yamlld \
         bytes for the docs distribution"
    );
}

/// AC2/AC6 — the jsonld/yamlld docs-distribution trees are sourced by READING the exact
/// build-output files, never a re-render, exercised via the real production
/// `read_build_serialization_tree` end-to-end (byte-identical to the file on disk,
/// hard-fails when the build output is absent).
#[test]
fn ac2_ac6_docs_distribution_jsonld_yamlld_are_byte_identical_to_the_build_output() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let jsonld_bytes = b"{\"@context\": \"https://blackcatinformatics.ca/gmeow/\", \"@graph\": []}";
    let build_output = tmp.path().join("gmeow.jsonld");
    std::fs::write(&build_output, jsonld_bytes).expect("write fake build output");

    let tree = gmeow_pipeline::docs_distribution::read_build_serialization_tree(
        &build_output,
        "gmeow.jsonld",
    )
    .expect("reference a present build output");
    assert_eq!(
        tree.get("gmeow.jsonld").map(Vec::as_slice),
        Some(jsonld_bytes.as_slice()),
        "the docs-distribution jsonld member must be BYTE-IDENTICAL to the canonical \
         dist/gmeow.jsonld build output, never a re-rendered copy: {tree:?}"
    );

    let missing = tmp.path().join("does-not-exist.jsonld");
    let err =
        gmeow_pipeline::docs_distribution::read_build_serialization_tree(&missing, "gmeow.jsonld")
            .expect_err("an absent build output must hard-fail, never silently render a fallback");
    let message = format!("{err}");
    assert!(
        message.contains("make build"),
        "AC2/AC6: a missing dist/gmeow.jsonld build output must point the operator at \
         `make build`, never silently degrade: {message}"
    );
}

// ── AC5 — forbidden-embed + no size gate + no carrier digest ───────────────────────

#[test]
fn ac5_forbidden_embed_gate_stays_present() {
    let source = bundle_blobs_source();
    assert!(
        source.contains("fn documentation_projections_are_absent"),
        "AC5 (forbidden-embed): crates/pipeline/src/bundle_blobs.rs must keep the \
         `documentation_projections_are_absent` gate — the shipped gmeow.gts bundle must never \
         embed a documentation projection"
    );
}

/// Recursively collect every `.rs` file under `dir` (skipping `target/` build
/// output directories, which are not committed source).
fn walk_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("read a directory entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            walk_rust_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn ac5_bundle_size_ceiling_stays_removed() {
    let crates_dir = repo_root().join("crates");
    let mut files = Vec::new();
    walk_rust_files(&crates_dir, &mut files);
    assert!(
        files.len() > 100,
        "AC5: the crates/ Rust source walk looks broken: found only {} .rs files",
        files.len()
    );

    let banned = ["TOTAL_CEILING", "REP_CEILINGS", "bundle_size_budget"];
    // This gate's own source necessarily names the banned identifiers (to look for
    // them) — exclude only THIS file from the walk, never any other.
    let self_path = PathBuf::from(file!());
    let self_name = self_path
        .file_name()
        .and_then(|n| n.to_str())
        .expect("this test file has a file name");
    let mut offenders: Vec<String> = Vec::new();
    for path in &files {
        if path.file_name().and_then(|n| n.to_str()) == Some(self_name) {
            continue;
        }
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for term in banned {
            if text.contains(term) {
                offenders.push(format!("{}: {term}", path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "AC5: none of {banned:?} may reappear anywhere under crates/ — the \
         bundle size-budget gate stays removed; found: {offenders:?}"
    );
}

#[test]
fn ac5_catalog_is_digest_free_and_outside_reasoning_closure() {
    let catalog = distribution_catalog_source();
    assert!(
        !catalog.contains("iri(GMEOW_NS, \"contentDigest\")"),
        "AC5: the distribution catalog schema \
         (crates/pipeline/src/stages/distribution_catalog.rs) must stay digest-free — a \
         release-time `gmeow:contentDigest` belongs only to the release instance \
         (crate::docs_distribution's release_instance_ntriples), never the carrier-time schema \
         graph the catalog builds"
    );

    let reasoning_graphs = reasoning_graphs_source();
    let array_start = reasoning_graphs
        .find("pub const OBJECT_LEVEL_NAMED_GRAPHS")
        .expect("crates/logic/src/reasoning_graphs.rs declares OBJECT_LEVEL_NAMED_GRAPHS");
    let array_tail = &reasoning_graphs[array_start..];
    let array_end = array_tail
        .find("];")
        .expect("OBJECT_LEVEL_NAMED_GRAPHS array literal is closed with `];`");
    let array = &array_tail[..array_end];
    assert!(
        !array.contains("distribution-catalog"),
        "AC5: the distribution-catalog named graph must stay OUT of \
         OBJECT_LEVEL_NAMED_GRAPHS (the object-level reasoning EDB boundary) — it is meta-level \
         schema content, never reasoning input; found `distribution-catalog` inside: {array}"
    );
}

// ── AC4 — zstd-rsyncable L12 preserved ──────────────────────────────────────────────

#[test]
fn ac4_gts_frame_profile_gate_and_zstd_level_12_preserved() {
    let source = makefile();
    let header = target_header(&source, "gts-frame-profile-gate");
    assert_eq!(
        header,
        "gts-frame-profile-gate: ## Enforce zstd-rsyncable level 12 on every materialized GTS \
         payload frame.",
        "AC4: the `gts-frame-profile-gate` Make target header changed"
    );
    let recipe = target_recipe(&source, "gts-frame-profile-gate");
    assert!(
        recipe.contains("$(GMEOW_DEV) gts-frame-profile generated/dist/gmeow.gts"),
        "AC4: `gts-frame-profile-gate` must positively validate EVERY payload frame \
         of the shipped bundle via `$(GMEOW_DEV) gts-frame-profile generated/dist/gmeow.gts`; \
         recipe was: {recipe:?}"
    );

    let gts_profile = gts_profile_source();
    assert!(
        gts_profile.contains("pub const GMEOW_GTS_ZSTD_LEVEL: i32 = 12;"),
        "AC4: gts_profile.rs must keep pinning `GMEOW_GTS_ZSTD_LEVEL` to 12"
    );
    assert!(
        gts_profile.contains("pub fn validate_mandated_frames"),
        "AC4: gts_profile.rs must keep `validate_mandated_frames`, the function that \
         positively validates every payload frame's zstd-rsyncable-L12 transform"
    );

    // Positively RUN the mandated-frame validator over the shipped bundle — not merely
    // assert the gate is wired. Every payload frame must carry the zstd-rsyncable-L12
    // transform; a torn CBOR sequence or a non-conforming frame is a hard failure here.
    let bundle_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generated/dist/gmeow.gts");
    let bundle = std::fs::read(&bundle_path).unwrap_or_else(|e| {
        panic!(
            "cannot read the shipped bundle {} ({e}); materialize it first with \
             `make sync SYNC_OUTPUTS=generated`",
            bundle_path.display()
        )
    });
    gmeow_pipeline::validate_mandated_frames(&bundle).unwrap_or_else(|e| {
        panic!("shipped bundle failed mandated zstd-rsyncable-L12 frame validation: {e}")
    });
}

// ── F1 — the consumer verb is exercised end-to-end (RUNTIME, no bundle needed) ────

/// Compile the REAL `dcat.rq` from the authored `dsl/mappings/projections/dcat.ttl`
/// source (a pure function of committed, tracked sources — no dependency on a prior
/// `make sync` materializing the git-ignored `generated/` tree) and fold it into a
/// minimal synthetic GTS snapshot carrying just the `queries-archive` blob, exactly
/// as the real bundle carries it. Mirrors the equivalent private test helper in
/// `crates/pipeline/src/docs_distribution.rs`, built here from ONLY the public API
/// this external test crate can reach.
fn synthetic_gts_with_dcat_query() -> Vec<u8> {
    let root = repo_root();
    let compiled = gmeow_pipeline::stages::mappings::compile_mappings(&root)
        .expect("compile mappings from committed dsl/mappings sources");
    let dcat_rq_path = format!("{}/dcat.rq", gmeow_pipeline::stages::mappings::QUERIES_DIR);
    let dcat_rq = compiled
        .artifacts
        .get(&dcat_rq_path)
        .unwrap_or_else(|| panic!("compiled mappings missing {dcat_rq_path}"))
        .clone();

    let archive = purrdf::ustar::write_archive(&[("dcat.rq".to_string(), dcat_rq)])
        .expect("tar the synthetic queries archive");
    let builder = purrdf::gts_compose::SnapshotBuilder::new();
    purrdf::gts_compose::emit_gts(
        &builder,
        "dist",
        Some(vec!["zstd-rsyncable".to_string()]),
        vec![purrdf::gts_compose::BlobRow {
            data: archive,
            media_type: "application/x-tar".to_string(),
            rep: gmeow_pipeline::bundle_blobs::REP_QUERIES.to_string(),
        }],
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .expect("frame the synthetic GTS snapshot")
}

#[test]
fn f1_consumer_verb_verify_exercises_real_manifest_end_to_end() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let docs_dir = tmp.path();
    let site_dir = docs_dir.join("site");
    std::fs::create_dir_all(&site_dir).expect("mkdir site");
    std::fs::write(site_dir.join("index.html"), b"<html>hello</html>").expect("write index.html");
    std::fs::write(site_dir.join("about.html"), b"<html>about</html>").expect("write about.html");

    // Content-address the fake rendered tree through the SAME producer
    // `verify_docs_distribution` recomputes against (`package_docs_dir`), then build
    // the manifest through the SAME production entry point `sync_docs` uses
    // (`build_docs_distribution_manifest`) — never a hand-rolled manifest that could
    // drift from the real parser.
    let (_, digest) =
        gmeow_pipeline::docs_distribution::package_docs_dir(&site_dir).expect("package site tree");
    let entries = vec![gmeow_pipeline::docs_distribution::DistributionEntry {
        slug: "site".to_string(),
        rel_path: "dist/gmeow-docs/site".to_string(),
        blake3: digest,
        media_type: "text/html".to_string(),
    }];
    let gts_bytes = synthetic_gts_with_dcat_query();
    let manifest = gmeow_pipeline::docs_distribution::build_docs_distribution_manifest(
        &entries,
        &[],
        &gts_bytes,
    )
    .expect("build the real docs distribution manifest");
    let manifest_dir = docs_dir.join("manifest");
    std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
    std::fs::write(manifest_dir.join("docs-manifest.ttl"), &manifest).expect("write manifest");

    let verdicts =
        gmeow_pipeline::docs_distribution::verify_docs_distribution(docs_dir, Some("site"))
            .expect("F1 (`gmeow docs verify`): verify a freshly rendered distribution");
    assert_eq!(
        verdicts.len(),
        1,
        "F1: expected exactly one verdict for --format site: {verdicts:?}"
    );
    assert!(
        verdicts[0].ok,
        "F1: a freshly rendered, untampered docs distribution must verify clean: \
         {:?}",
        verdicts[0]
    );

    // Flip a byte in the packaged tree — the consumer verb must be falsifiable: it
    // must catch tampering, never silently pass a mismatched digest.
    std::fs::write(site_dir.join("index.html"), b"<html>HELLO</html>").expect("tamper index.html");
    let verdicts =
        gmeow_pipeline::docs_distribution::verify_docs_distribution(docs_dir, Some("site"))
            .expect("F1: verify a tampered distribution (still parses/recomputes)");
    assert_eq!(verdicts.len(), 1);
    assert!(
        !verdicts[0].ok,
        "F1 (falsifiability): a tampered docs distribution must FAIL verification, \
         never silently pass: {:?}",
        verdicts[0]
    );
}

// ── Idempotent release packaging — no stray sidecar inside the archived tree ──────

/// A `gmeow-dev docs-package` invocation anchored at `root` via `GMEOW_ROOT`,
/// exactly mirroring the `dev_cmd()` helper in `tests/cli_parity.rs` — this drives
/// the REAL production binary end-to-end, never a reimplementation of its
/// packaging/sidecar logic.
fn docs_package_cmd(root: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("gmeow-dev").expect("gmeow-dev binary");
    cmd.env("GMEOW_ROOT", root);
    cmd.args(["docs-package", "--out", "dist/gmeow-docs.tar"]);
    cmd
}

#[test]
fn docs_package_repackaging_with_no_intervening_sync_is_byte_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("dist").join("gmeow-docs");
    let site_dir = docs_dir.join("site");
    std::fs::create_dir_all(&site_dir).expect("mkdir site");
    std::fs::write(site_dir.join("index.html"), b"<html>hello</html>").expect("write index.html");
    let manifest_dir = docs_dir.join("manifest");
    std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
    std::fs::write(
        manifest_dir.join("docs-manifest.ttl"),
        b"<urn:x> <urn:y> <urn:z> .\n",
    )
    .expect("write docs-manifest.ttl");

    let out_path = root.join("dist").join("gmeow-docs.tar");
    let archive_sidecar_path = root.join("dist").join("gmeow-docs.tar.blake3");
    let manifest_sidecar_path = root.join("dist").join("gmeow-docs.manifest.ttl.blake3");

    // Run 1.
    docs_package_cmd(root).assert().success();
    let archive1 = std::fs::read(&out_path).expect("read archive after run 1");
    let archive_digest1 =
        std::fs::read_to_string(&archive_sidecar_path).expect("read archive sidecar after run 1");
    let manifest_digest1 = std::fs::read_to_string(&manifest_sidecar_path).expect(
        "the manifest digest sidecar must land BESIDE the tar (dist/gmeow-docs.manifest.ttl.blake3), \
         outside the archived dist/gmeow-docs/ tree",
    );

    // Run 2, with NO intervening mutation (no `sync` between runs — the exact
    // real-world `make release-publish` cadence when re-running `docs-package`
    // alone). Before the fix, run 1's manifest digest sidecar landed INSIDE
    // `dist/gmeow-docs/manifest/`, so run 2's own packaging pass would archive that
    // stray file too, changing the tar bytes and both digests with no
    // documentation change whatsoever.
    docs_package_cmd(root).assert().success();
    let archive2 = std::fs::read(&out_path).expect("read archive after run 2");
    let archive_digest2 =
        std::fs::read_to_string(&archive_sidecar_path).expect("read archive sidecar after run 2");
    let manifest_digest2 =
        std::fs::read_to_string(&manifest_sidecar_path).expect("read manifest sidecar after run 2");

    assert_eq!(
        archive1, archive2,
        "docs-package run twice with no intervening sync must produce a BYTE-IDENTICAL tar — a \
         sidecar written inside the archived tree would change the tar bytes on the very next run"
    );
    assert_eq!(
        archive_digest1, archive_digest2,
        "the archive BLAKE3 sidecar must stay stable across a repeated docs-package run"
    );
    assert_eq!(
        manifest_digest1, manifest_digest2,
        "the manifest BLAKE3 sidecar must stay stable across a repeated docs-package run"
    );

    assert!(
        !manifest_dir.join("docs-manifest.ttl.blake3").exists(),
        "the manifest digest sidecar must NEVER land under the archived dist/gmeow-docs/ tree — \
         that is the exact non-idempotency defect this test guards against"
    );
}

// ── Determinism — PDF cross-environment ─────────────────────────────────────────────

#[test]
fn determinism_embedded_font_digest_is_stable_and_nonempty() {
    let d1 = docs_print::embedded_font_digest();
    let d2 = docs_print::embedded_font_digest();
    assert!(
        !d1.is_empty(),
        "PDF cross-environment determinism (AC4 sibling guarantee): \
         docs_print::embedded_font_digest() must be non-empty — it pins the exact embedded font \
         set a compiled PDF draws from"
    );
    assert_eq!(
        d1, d2,
        "PDF cross-environment determinism: docs_print::embedded_font_digest() must be stable \
         across calls — a byte-reproducible PDF requires a byte-stable embedded font set"
    );
}

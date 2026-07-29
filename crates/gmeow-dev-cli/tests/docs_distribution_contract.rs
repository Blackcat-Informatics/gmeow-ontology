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

/// Read a committed source/config file this gate scrapes.
///
/// Scraping gates DIAGNOSE rather than panic. A bare `panic!`/`expect` here reports "the
/// file moved" as an unlabelled crash with no remediation, which is exactly the failure
/// mode that makes a structural gate look broken instead of informative: the reader cannot
/// tell a real regression from a rename. Every reader below therefore fails through an
/// `assert!` carrying the path it wanted and what a maintainer should do.
fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => panic!(
            "distribution contract: cannot read the committed source {rel} at {} ({e}). This \
             gate reads that file to check a structural property; if the file MOVED, update \
             this reader to its new path — do not delete the assertion.",
            path.display()
        ),
    }
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
    read("crates/bundle-view/src/bundle_blobs.rs")
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
    let found = source.lines().position(|line| line.starts_with(&prefix));
    assert!(
        found.is_some(),
        "distribution contract: the Makefile declares no `{target}:` target. This gate \
         checks that target's recipe; if the target was RENAMED, point this reader at the \
         new name — if it was removed, the contract it enforced is gone and that needs a \
         decision, not a deleted assertion."
    );
    found.unwrap_or_default()
}

fn target_header<'a>(source: &'a str, target: &str) -> &'a str {
    let index = target_header_index(source, target);
    let header = source.lines().nth(index);
    assert!(
        header.is_some(),
        "distribution contract: the `{target}:` header index {index} is out of bounds — the \
         Makefile reader is broken, not the Makefile"
    );
    header.unwrap_or_default()
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
        source.contains("run: make regen SYNC_MODE=update SYNC_OUTPUTS=docs"),
        "AC3 (source-backed export): .github/workflows/pages.yml must render the \
         Pages site from canonical sources via the exact step `run: make regen SYNC_MODE=update \
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

    // …and the interactive console RIDES that upload. The chain is three positive reads of
    // production sources, each of which is where the fact actually lives:
    //   1. pages.yml uploads `ontology-docs`                                (just above);
    //   2. `sync_docs` reconciles `ontology-docs` from the SITE tree        (dev_project.rs);
    //   3. the site render folds the console producer into that tree        (render.rs).
    // Without (3) the console would be a `dist/` distribution only, and the published site
    // would 404 on the `console/index.html` its own navigation links to.
    let dev_project = dev_project_source();
    assert!(
        dev_project.contains("(\"ontology-docs\", &site)"),
        "AC3: `sync_docs` must reconcile the Pages upload root `ontology-docs` FROM the \
         rendered site tree — that is what puts the console inside the published site"
    );
    assert!(
        read("crates/docs/src/render.rs")
            .contains("files.extend(crate::console::console_files(exec))"),
        "AC3 (Task 12.6): the site render must fold `crate::console::console_files(exec)` \
         into its own tree, or the console does not ride the `ontology-docs` Pages upload \
         at all. If that fold moved, retarget this reader — do not delete the assertion."
    );
    // Non-vacuity, at run time: the console producer really does emit a `console/` tree.
    // A structural read of a fold that folds in NOTHING would pass while shipping no
    // console, which is precisely the failure this gate exists to catch.
    let console = gmeow_docs::console_files(&interactive_exec());
    assert!(
        console.contains_key("console/index.html"),
        "AC3 (Task 12.6): `console_files` must emit `console/index.html`; got {:?}",
        console.keys().collect::<Vec<_>>()
    );
}

/// An `exec` that makes the console producer interactive.
///
/// The producer only checks non-emptiness of the bundle field and content-addresses the
/// bytes, so fixed sentinels are sufficient — mirroring `interactive_exec()` in
/// `crates/docs/tests/console_producer.rs`, and keeping this gate independent of a
/// materialized `generated/dist/gmeow.gts`.
fn interactive_exec() -> gmeow_docs::ExecutableDocsData {
    gmeow_docs::ExecutableDocsData {
        full_bundle_gts: b"gts-bundle-sentinel-bytes".to_vec(),
        conjectures_ttl: b"@prefix ex: <http://example/> . ex:c a ex:Conjecture .\n".to_vec(),
        ..Default::default()
    }
}

#[test]
fn ac3_makefile_regen_delegates_to_gmeow_dev_sync() {
    let source = makefile();
    // `make sync` was removed; the standalone regenerate lane is `make regen`.
    let recipe = target_recipe(&source, "regen");
    assert!(
        recipe.contains("$(GMEOW_DEV) sync"),
        "AC3: the standalone Makefile `regen:` recipe must delegate to the single \
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
    // The console is one of the nine distributions that archive must carry — proved at run
    // time, through the real binary, by
    // `docs_package_archives_the_console_alongside_every_other_distribution` below.
    assert!(
        gmeow_pipeline::stages::distribution_catalog::distribution_row("console")
            .is_some_and(|row| row.rel_path == "dist/gmeow-docs/console"),
        "AC3 (Task 12.6): the console must be a declared distribution under \
         `dist/gmeow-docs/`, or it cannot ride `dist/gmeow-docs.tar` at all"
    );
}

// ── AC2 / AC6 — segmentation, single authority, bijection, boundary ────────────────

/// The nine canonical distribution slugs, READ OFF the exported table rather than
/// restated here.
///
/// This used to be a `const CANONICAL_SLUGS: [&str; 8]` literal — a third copy of the same
/// nine facts, sitting beside a text-scrape of `dev_project.rs`'s destinations array. That
/// shape could only ever assert "the copies still agree", and a genuine new distribution
/// made the gate red for the wrong reason. Reading
/// [`gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS`] makes this gate check
/// the real property: the ONE table drives the rendered destinations, and the counts are
/// non-vacuous.
fn canonical_slugs() -> BTreeSet<String> {
    gmeow_pipeline::stages::distribution_catalog::declared_distribution_slugs()
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[test]
fn ac2_ac6_canonical_slugs_bijection_between_producers() {
    use gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS;

    let expected = canonical_slugs();
    assert_eq!(
        expected.len(),
        9,
        "AC2/AC6: the distribution catalog must declare exactly nine distributions (the \
         eight documents/serializations plus the interactive console); found {expected:?}"
    );
    assert!(
        expected.contains("console"),
        "AC2/AC6: the standalone interactive console is a shipped distribution and must be \
         in the bijection: {expected:?}"
    );

    // Producer 1: the destinations `sync_docs` reconciles to disk. These are the table's
    // OWN `rel_path` column now — `sync_docs` iterates `DISTRIBUTIONS` and cannot render a
    // destination the catalog does not declare (nor skip one it does), which is exactly the
    // property the old `dist/gmeow-docs/<slug>` text-scrape was a proxy for.
    let rendered_slugs: BTreeSet<String> = DISTRIBUTIONS
        .iter()
        .map(|row| {
            let slug = row.rel_path.strip_prefix("dist/gmeow-docs/");
            assert!(
                slug.is_some(),
                "AC2/AC6: distribution {:?} ships at {:?}, outside the shared \
                 `dist/gmeow-docs/` docs-distribution base",
                row.slug,
                row.rel_path
            );
            slug.unwrap_or_default().to_string()
        })
        .collect();
    assert_eq!(
        rendered_slugs, expected,
        "AC2/AC6 (single segmentation authority): every declared distribution's rel_path \
         tail must be its own slug; rendered={rendered_slugs:?} declared={expected:?}"
    );

    // Producer 2: `sync_docs` really does consume that table, rather than carrying its own
    // destinations literal. This is the ONE structural claim about dev_project.rs left, and
    // it is a positive read of the shared symbol, not a scrape of a restated list.
    let dev_project = dev_project_source();
    assert!(
        dev_project.contains("distribution_catalog::DISTRIBUTIONS"),
        "AC2/AC6: dev_project.rs's sync_docs must derive its docs destinations by iterating \
         `gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS`, never a local array \
         literal that restates the slug/path pairs"
    );
    assert!(
        dev_project.contains("dist/gmeow-docs/manifest"),
        "AC2/AC6: dev_project.rs `sync_docs` must reconcile a `dist/gmeow-docs/manifest` \
         destination for the DCAT release manifest, separate from the declared distributions"
    );
}

#[test]
fn sub_assets_are_priced_but_never_enter_the_nine_slug_bijection() {
    use gmeow_pipeline::stages::distribution_catalog as catalog;

    let bijection = canonical_slugs();
    let sub_assets: BTreeSet<String> = catalog::declared_sub_asset_slugs()
        .into_iter()
        .map(str::to_string)
        .collect();
    // The interactive engines + browser bundle ARE priced as first-class sub-assets…
    assert!(
        !sub_assets.is_empty(),
        "the vendored interactive engines + browser bundle must be priced as sub-assets"
    );
    // …but they are SUB-ASSETS, never top-level distributions, so the nine-slug bijection
    // is untouched (a sub-asset leaking into it would be a tenth distribution, which the
    // bijection test above would also catch).
    assert!(
        sub_assets.is_disjoint(&bijection),
        "AC2/AC6: sub-assets {sub_assets:?} must be DISJOINT from the nine-slug distribution \
         bijection {bijection:?} — they are sub-assets, not distributions"
    );

    // Ownership is distribution-parameterized and every owner is itself a declared
    // distribution: the release-time producer prices each sub-asset out of each owner's own
    // tree, so a site-only pricing can no longer leave the console's identical copy of a
    // 7 MB wasm image with no release digest.
    let owners: BTreeSet<String> = catalog::sub_asset_owner_slugs()
        .into_iter()
        .map(str::to_string)
        .collect();
    assert!(
        owners.is_subset(&bijection),
        "AC2/AC6: every sub-asset owner must be a declared distribution; owners={owners:?}"
    );
    assert!(
        owners.contains("site") && owners.contains("console"),
        "AC2/AC6: the two interactive surfaces both ship the shared engine set and must both \
         own it; owners={owners:?}"
    );

    // The release-time digest producer prices exactly the declared set, for every owner
    // (one authority, parameterized — never a second sub-asset list).
    let priced = catalog::sub_asset_pricing();
    let priced_slugs: BTreeSet<String> = priced
        .iter()
        .map(|(_, slug, _, _)| (*slug).to_string())
        .collect();
    assert_eq!(
        priced_slugs, sub_assets,
        "the release-time sub-asset pricing set must equal the catalog-declared sub-asset set"
    );
    assert_eq!(
        priced.len(),
        owners.len() * sub_assets.len(),
        "the pricing must cover every (owner, sub-asset) pair: {priced:?}"
    );
}

/// AC2/AC6 (boundary) — every docs destination lives under `dist/` or `ontology-docs`,
/// NEVER `generated/`.
///
/// This used to isolate `dev_project.rs`'s `let destinations = [ … ];` array literal by
/// substring search and `expect` on both delimiters, which meant a refactor that made the
/// destinations DERIVED (as they now are) crashed the gate with "the `destinations` array
/// literal is closed with `];`" — a panic about the scraper, telling a maintainer nothing
/// about the boundary it was supposed to protect. The destinations are now the table's own
/// `rel_path` column, so the boundary is checked where it actually lives.
#[test]
fn ac2_ac6_docs_destinations_stay_under_dist_or_ontology_docs_never_generated() {
    use gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS;
    for row in DISTRIBUTIONS {
        assert!(
            row.rel_path.starts_with("dist/"),
            "AC2/AC6 (boundary): distribution {:?} ships at {:?}; every docs destination must \
             live under `dist/` (or the `ontology-docs` Pages root)",
            row.slug,
            row.rel_path
        );
        assert!(
            !row.rel_path.contains("generated/"),
            "AC2/AC6 (boundary): distribution {:?} ships at {:?} — a docs destination must \
             NEVER be written into `generated/`, which is the pipeline's own output tree",
            row.slug,
            row.rel_path
        );
    }
    // The destination bases that are NOT table rows — the Pages upload root and the
    // manifest subdir — are PARSED OUT of `sync_docs`'s own `destinations` seed/push forms
    // and then checked. The check used to run against the test's own search literal
    // (`assert!(!base.contains("generated/"))`), which is a tautology over a constant this
    // file wrote: it could only ever pass, and it would have gone on passing while
    // `sync_docs` reconciled straight into `generated/`. Reading the literal off the
    // production source is what makes the boundary claim falsifiable.
    //
    // A blanket "no `generated/` anywhere in this file" scan would be the opposite error:
    // `sync_docs` legitimately READS `generated/…` inputs (the axiom set and the
    // bibliography feeding the print render), and conflating an input read with a
    // reconciliation base is exactly the kind of proxy that reds for the wrong reason.
    let source = dev_project_source();
    let literal_bases = non_table_destination_bases(&source);
    for (base, role) in [
        ("ontology-docs", "the Pages upload root"),
        (
            "dist/gmeow-docs/manifest",
            "the DCAT release-manifest subdir",
        ),
    ] {
        assert!(
            literal_bases.contains(base),
            "AC2/AC6 (boundary): sync_docs must reconcile {role} as the destination base \
             `{base}`; parsed bases were {literal_bases:?}. If that base was renamed, \
             retarget this reader — do not delete the assertion."
        );
    }
    // The boundary itself, over every literal base the production source actually declares
    // (including any added since this gate was written).
    for base in &literal_bases {
        assert!(
            !base.contains("generated/"),
            "AC2/AC6 (boundary): sync_docs reconciles a destination base {base:?} inside \
             `generated/`, which is the pipeline's own output tree and has exactly one \
             writer; every docs destination must live under `dist/` or `ontology-docs`"
        );
        assert!(
            base == "ontology-docs" || base.starts_with("dist/"),
            "AC2/AC6 (boundary): sync_docs reconciles a destination base {base:?} outside \
             both `dist/` and the `ontology-docs` Pages root"
        );
    }
}

/// Every reconciliation base `sync_docs` names as a STRING LITERAL, parsed out of
/// `dev_project.rs`.
///
/// The table rows are covered by the `DISTRIBUTIONS` walk above (they are `row.rel_path`,
/// not literals, and this parser skips them by construction: it only reads a tuple whose
/// first element opens with `"`). What is left is exactly the bases that have no catalog
/// row and so would otherwise be checked by nothing at all.
fn non_table_destination_bases(source: &str) -> BTreeSet<String> {
    const ANCHORS: [&str; 2] = [
        "destinations: Vec<(&str, &BTreeMap<String, Vec<u8>>)> = vec![(",
        "destinations.push((",
    ];
    let mut out = BTreeSet::new();
    for anchor in ANCHORS {
        for (index, _) in source.match_indices(anchor) {
            let tail = &source[index + anchor.len()..];
            let Some(quoted) = tail.strip_prefix('"') else {
                continue; // a `row.rel_path` push — a table row, covered above.
            };
            let Some(end) = quoted.find('"') else {
                continue;
            };
            out.insert(quoted[..end].to_string());
        }
    }
    assert!(
        !out.is_empty(),
        "AC2/AC6 (boundary): the `destinations` reader parsed NO literal base out of \
         dev_project.rs — the reader is broken, and a broken reader would make every \
         boundary assertion below pass vacuously. Retarget it at the new seed/push form."
    );
    out
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
        "AC5 (forbidden-embed): crates/bundle-view/src/bundle_blobs.rs must keep the \
         `documentation_projections_are_absent` gate — the shipped gmeow.gts bundle must never \
         embed a documentation projection"
    );
}

/// Recursively collect every `.rs` file under `dir` (skipping `target/` build
/// output directories, which are not committed source).
fn walk_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir);
    assert!(
        entries.is_ok(),
        "AC5: cannot walk {} for the banned-identifier sweep ({:?}) — the sweep must cover \
         every committed crate, so a partial walk would pass vacuously",
        dir.display(),
        entries.as_ref().err()
    );
    for entry in entries.into_iter().flatten() {
        let Ok(entry) = entry else {
            panic!(
                "AC5: cannot read a directory entry under {} — the banned-identifier sweep \
                 must be complete, and a skipped entry would let a reintroduced size-budget \
                 gate pass unnoticed",
                dir.display()
            )
        };
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
    let array_start = reasoning_graphs.find("pub const OBJECT_LEVEL_NAMED_GRAPHS");
    assert!(
        array_start.is_some(),
        "AC5: crates/logic/src/reasoning_graphs.rs no longer declares \
         `pub const OBJECT_LEVEL_NAMED_GRAPHS` — this gate checks that the distribution \
         catalog stays OUT of the object-level reasoning EDB. If the constant was renamed or \
         moved, retarget this reader; if the EDB boundary is now expressed some other way, \
         re-express this check against it rather than dropping it."
    );
    let array_tail = &reasoning_graphs[array_start.unwrap_or_default()..];
    let array_end = array_tail.find("];");
    assert!(
        array_end.is_some(),
        "AC5: the `OBJECT_LEVEL_NAMED_GRAPHS` declaration in \
         crates/logic/src/reasoning_graphs.rs is no longer an array literal closed with `];` \
         — this gate's extraction needs retargeting to the new form, not deleting"
    );
    let array = &array_tail[..array_end.unwrap_or(array_tail.len())];
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
             `make regen SYNC_OUTPUTS=generated`",
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
/// `make regen` materializing the git-ignored `generated/` tree) and fold it into a
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

/// Task 12.6 — the console RIDES `dist/gmeow-docs.tar`.
///
/// Driven through the real `gmeow-dev docs-package` binary over a materialized
/// `dist/gmeow-docs/` tree, and read back out of the produced archive: the tar's member
/// list must name the console's files under `console/`. `release-publish` attaching the
/// tar (asserted structurally above) means nothing about the console unless the console is
/// actually inside it — and `package_docs_dir` walking the whole directory is the property
/// that makes it so, which only a real packaging run can demonstrate.
#[test]
fn docs_package_archives_the_console_alongside_every_other_distribution() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("dist").join("gmeow-docs");

    // One subdirectory per declared distribution, at the catalog's own `rel_path` tails —
    // the shape `sync_docs` reconciles. Naming them off the table rather than by hand is
    // what makes "the console is in there" a claim about the catalog, not about a fixture.
    for row in gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS {
        let slug = row
            .rel_path
            .strip_prefix("dist/gmeow-docs/")
            .unwrap_or_else(|| panic!("{} ships outside dist/gmeow-docs/", row.slug));
        let dir = docs_dir.join(slug);
        std::fs::create_dir_all(&dir).expect("mkdir distribution");
        std::fs::write(dir.join("index.html"), format!("<html>{slug}</html>"))
            .expect("write distribution file");
    }
    let manifest_dir = docs_dir.join("manifest");
    std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
    std::fs::write(
        manifest_dir.join("docs-manifest.ttl"),
        b"<urn:x> <urn:y> <urn:z> .\n",
    )
    .expect("write docs-manifest.ttl");

    docs_package_cmd(root).assert().success();
    let archive = std::fs::read(root.join("dist").join("gmeow-docs.tar")).expect("read the tar");
    let members: BTreeSet<String> = purrdf::ustar::read_archive(&archive)
        .expect("read the packaged tar")
        .into_iter()
        .map(|(name, _)| name)
        .collect();

    assert!(
        members.contains("console/index.html"),
        "Task 12.6: `dist/gmeow-docs.tar` must carry the interactive console under \
         `console/` — the release asset ships every distribution or it ships a lie; \
         members were {members:?}"
    );
    // …and not only the console: every declared distribution's tail is in the archive, so
    // this cannot pass by the console being special-cased into a tar that lost the rest.
    for row in gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS {
        let slug = row.rel_path.trim_start_matches("dist/gmeow-docs/");
        assert!(
            members.contains(&format!("{slug}/index.html")),
            "Task 12.6: distribution {slug:?} is missing from the packaged archive: \
             {members:?}"
        );
    }
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

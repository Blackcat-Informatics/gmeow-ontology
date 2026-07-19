// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External documentation / serialization distribution rendering, content-addressing,
//! and the release-time DCAT manifest (issue #1491 Task 3).
//!
//! This module is the SOLE producer [`crate::stages::okf`] / [`crate::stages::yaml_ld`]
//! results route through on their way to the `dist/gmeow-docs/{okf,jsonld,yamlld}`
//! external destinations `gmeow-dev sync` writes (`crates/gmeow-dev-cli`'s `sync_docs`).
//! It never re-implements a serializer — [`render_serialization_distributions`] calls
//! ONLY [`crate::stages::export::collect_term_surface`] + [`crate::stages::okf::render_okf`]
//! (OKF) and [`crate::stages::yaml_ld::serialize_graph`] /
//! [`crate::stages::yaml_ld::serialize_graph_yaml`] (JSON-LD-star / YAML-LD-star) — the
//! same single-authority calls [`crate::docs_measure`] already makes off the SAME carrier
//! dataset shape.
//!
//! [`distribution_blake3`] is the shared content-addressing idiom every rendered
//! distribution tree goes through before it is named in the release manifest: pack the
//! tree into one deterministic USTAR archive (mirroring [`crate::docs_measure`]'s
//! packing convention so the digests stay directly comparable) and `blake3` the archive
//! bytes, formatted `blake3:<hex>`.
//!
//! [`build_docs_distribution_manifest`] builds the release-time DCAT catalog instance:
//! one `gmeow:Corpus` node whose `gmeow:corpusMember` rows ARE the Task-2 canonical
//! catalog subjects (`https://blackcatinformatics.ca/gmeow/distribution/dist/<slug>`,
//! [`crate::stages::distribution_catalog::dist_iri`]) — so the release-time instance
//! references the exact same carrier-time schema subject, never a re-minted IRI — each
//! carrying its rendered tree's `gmeow:contentDigest` + `gmeow:sourceLocation`. The
//! instance graph is projected to DCAT/SPDX through the bundle's compiled `dcat.rq`
//! CONSTRUCT via the single projection authority ([`crate::projections::project_graph`]).
//! No-optionality: a missing `dcat.rq` or a projection failure is a HARD FAIL, never a
//! silently empty manifest.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_errors::Diag;
use purrdf::RdfDataset;

use crate::error::DocsDistribution as DocsDistributionError;
use crate::projections::{TagMap, project_graph};
use crate::stages::distribution_catalog::{dist_iri, iri, triple, triple_lit};

fn err(message: impl Into<String>) -> Diag {
    Diag::of_kind(DocsDistributionError {
        message: message.into(),
    })
}

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The single `gmeow:Corpus` subject every release manifest instance hangs its
/// `gmeow:corpusMember` distribution rows off. Stable across releases (this is a
/// release-instance identity, not a per-release-content digest).
const RELEASE_CORPUS_IRI: &str = "https://blackcatinformatics.ca/gmeow/distribution/release";

// ── serialization-family rendering ──────────────────────────────────────────────────

/// The three serialization-family trees rendered off a carrier dataset. Members are
/// relative paths (no `dist/gmeow-docs/<format>` destination prefix) — the caller
/// (`gmeow-dev sync`'s `sync_docs`) supplies that base when reconciling to disk.
#[derive(Debug, Default, Clone)]
pub struct SerializationTrees {
    pub okf: BTreeMap<String, Vec<u8>>,
    pub jsonld: BTreeMap<String, Vec<u8>>,
    pub yamlld: BTreeMap<String, Vec<u8>>,
}

/// Render the OKF / JSON-LD-star / YAML-LD-star serialization distributions off
/// `dataset` through the single production serializer authorities (see the module doc
/// comment). `render_okf` keys its members under `dist/gmeow-okf/…`; that prefix is
/// stripped here so [`SerializationTrees::okf`] carries plain relative members, ready
/// to reconcile under the caller's OWN `dist/gmeow-docs/okf` base.
pub fn render_serialization_distributions(
    dataset: &RdfDataset,
) -> Result<SerializationTrees, Diag> {
    let (title, version, terms) = crate::stages::export::collect_term_surface(dataset)
        .map_err(|e| err(format!("collect the OKF/serialization term surface: {e}")))?;
    let okf_raw = crate::stages::okf::render_okf(&title, &version, &terms)
        .map_err(|e| err(format!("render the OKF bundle: {e}")))?;

    let okf_prefix = format!("dist/{}/", crate::stages::okf::OKF_DIR_NAME);
    let mut okf = BTreeMap::new();
    for (path, bytes) in okf_raw {
        let rel = path.strip_prefix(&okf_prefix).ok_or_else(|| {
            err(format!(
                "OKF member {path:?} does not carry the expected {okf_prefix:?} prefix"
            ))
        })?;
        okf.insert(rel.to_string(), bytes);
    }
    if okf.is_empty() {
        return Err(err(
            "render_okf produced an empty tree — refusing to publish an empty OKF distribution",
        ));
    }

    let jsonld_text = crate::stages::yaml_ld::serialize_graph(dataset)
        .map_err(|e| err(format!("serialize the JSON-LD-star document: {e}")))?;
    let yamlld_text = crate::stages::yaml_ld::serialize_graph_yaml(dataset, None)
        .map_err(|e| err(format!("serialize the YAML-LD-star document: {e}")))?;

    Ok(SerializationTrees {
        okf,
        jsonld: BTreeMap::from([("gmeow.jsonld".to_string(), jsonld_text.into_bytes())]),
        yamlld: BTreeMap::from([("gmeow.yamlld".to_string(), yamlld_text.into_bytes())]),
    })
}

// ── content-addressing ──────────────────────────────────────────────────────────────

/// Pack `tree` into one deterministic USTAR archive (sorted members — `tree` is
/// already a `BTreeMap`) and `blake3` the archive bytes, formatted `blake3:<hex>`.
/// Mirrors [`crate::docs_measure`]'s packing convention exactly, so a digest computed
/// here is directly comparable to one computed there.
///
/// Returns `Err` (never panics) on the same rare, well-formed-input-only USTAR
/// construction failure [`purrdf::ustar::write_archive`] itself documents —
/// no-optionality forbids silently swallowing that failure into a bogus digest.
pub fn distribution_blake3(tree: &BTreeMap<String, Vec<u8>>) -> Result<String, Diag> {
    let members: Vec<(String, Vec<u8>)> =
        tree.iter().map(|(name, data)| (name.clone(), data.clone())).collect();
    let archive = purrdf::ustar::write_archive(&members)
        .map_err(|e| err(format!("tar a distribution tree for content-addressing: {e}")))?;
    Ok(format!("blake3:{}", blake3::hash(&archive).to_hex()))
}

/// `blake3:<hex>` of a bare byte slice — the same content-addressing idiom
/// [`distribution_blake3`] applies to a tarred tree, exposed standalone for callers
/// that need to digest one already-materialized file (e.g. the release-time DCAT
/// manifest sidecar) rather than a whole tree.
pub fn blake3_of(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

// ── release publication packaging ───────────────────────────────────────────────────

/// Recursively read every file under `dir` into a member map keyed by its path
/// RELATIVE to `dir` (forward-slash separated, naturally sorted — the map is a
/// `BTreeMap`), pack the tree into one deterministic USTAR archive via
/// [`purrdf::ustar::write_archive`] (the exact packing convention
/// [`distribution_blake3`] uses, so the digest stays directly comparable), and
/// `blake3` the resulting archive bytes.
///
/// This is the release-publication counterpart to [`distribution_blake3`]: where
/// that function digests an in-memory tree already held by the pipeline,
/// `package_docs_dir` digests a tree the caller has already materialized to disk
/// (the `dist/gmeow-docs/` external documentation distribution the release
/// publish path attaches as a content-addressed asset).
///
/// No-optionality: a missing `dir` or an empty tree is a HARD FAIL — the caller
/// must materialize the docs distribution first (`make sync SYNC_OUTPUTS=docs`);
/// this function never silently produces an empty archive.
pub fn package_docs_dir(dir: &Path) -> Result<(Vec<u8>, String), Diag> {
    if !dir.is_dir() {
        return Err(err(format!(
            "docs distribution directory {} is missing — materialize it first with \
             `make sync SYNC_MODE=update SYNC_OUTPUTS=docs`",
            dir.display()
        )));
    }
    let mut tree = BTreeMap::new();
    collect_docs_files(dir, dir, &mut tree)?;
    if tree.is_empty() {
        return Err(err(format!(
            "docs distribution directory {} is empty — refusing to package an empty archive",
            dir.display()
        )));
    }
    let members: Vec<(String, Vec<u8>)> = tree.into_iter().collect();
    let archive = purrdf::ustar::write_archive(&members)
        .map_err(|e| err(format!("tar the docs distribution tree: {e}")))?;
    let digest = blake3_of(&archive);
    Ok((archive, digest))
}

/// Recursively walk `dir` (relative to `root`), inserting every regular file's
/// bytes into `out` keyed by its `root`-relative, forward-slash path.
fn collect_docs_files(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), Diag> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| err(format!("read directory {}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry
            .map_err(|e| err(format!("read a directory entry under {}: {e}", dir.display())))?;
        let path = entry.path();
        if path.is_dir() {
            collect_docs_files(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| err(format!("compute the relative path of {}: {e}", path.display())))?;
        let rel_str: String = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let bytes = std::fs::read(&path).map_err(|e| err(format!("read {}: {e}", path.display())))?;
        out.insert(rel_str, bytes);
    }
    Ok(())
}

// ── release-time DCAT manifest ──────────────────────────────────────────────────────

/// One rendered distribution's manifest row: its Task-2 catalog slug (`site`, `okf`,
/// …), the release-relative path it ships at, its [`distribution_blake3`] content
/// digest, and its declared media type
/// ([`crate::stages::distribution_catalog::media_type_for_slug`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionEntry {
    pub slug: String,
    pub rel_path: String,
    pub blake3: String,
    pub media_type: String,
}

/// Emit the sorted, deduped, byte-stable N-Triples release-instance graph: one
/// `gmeow:Corpus` node ([`RELEASE_CORPUS_IRI`]) with one `gmeow:corpusMember` per
/// entry, each member node BEING its Task-2 catalog subject
/// ([`dist_iri`]) and carrying `gmeow:sourceLocation`, `gmeow:contentDigest`, and
/// `gmeow:artifactMediaType`.
fn release_instance_ntriples(entries: &[DistributionEntry]) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(entries.len() * 4 + 1);
    lines.push(triple(
        RELEASE_CORPUS_IRI,
        RDF_TYPE,
        &iri(GMEOW_NS, "Corpus"),
    ));
    for entry in entries {
        let member = dist_iri(&entry.slug);
        lines.push(triple(
            RELEASE_CORPUS_IRI,
            &iri(GMEOW_NS, "corpusMember"),
            &member,
        ));
        lines.push(triple_lit(
            &member,
            &iri(GMEOW_NS, "sourceLocation"),
            &entry.rel_path,
        ));
        lines.push(triple_lit(
            &member,
            &iri(GMEOW_NS, "contentDigest"),
            &entry.blake3,
        ));
        lines.push(triple_lit(
            &member,
            &iri(GMEOW_NS, "artifactMediaType"),
            &entry.media_type,
        ));
    }
    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Build the release-time DCAT distribution manifest: the [`release_instance_ntriples`]
/// graph for `entries`, projected through the bundle's compiled `dcat.rq` CONSTRUCT
/// (folded into `gts_bytes`'s `queries-archive` blob — [`crate::bundle_blobs::bundled_queries`])
/// via the single projection authority ([`project_graph`]). Returns the projected
/// N-Triples.
///
/// No-optionality: a `gts_bytes` snapshot with no bundled `dcat.rq` (any key ending in
/// `dcat.rq`) is a HARD FAIL — never a silently empty or partial manifest.
pub fn build_docs_distribution_manifest(
    entries: &[DistributionEntry],
    gts_bytes: &[u8],
) -> Result<String, Diag> {
    let instance_nt = release_instance_ntriples(entries);

    let queries = crate::bundle_blobs::bundled_queries(gts_bytes)
        .map_err(|e| err(format!("load the bundled projection queries: {e}")))?;
    let dcat_rq_bytes = queries
        .iter()
        .find(|(key, _)| key.ends_with("dcat.rq"))
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| {
            err(
                "bundled dcat.rq projection query is missing — cannot build the release \
                 distribution manifest",
            )
        })?;
    let dcat_rq = std::str::from_utf8(dcat_rq_bytes)
        .map_err(|e| err(format!("bundled dcat.rq is not valid UTF-8: {e}")))?;

    project_graph(&instance_nt, dcat_rq, &TagMap::default())
        .map_err(|e| err(format!("project the release distribution instance through dcat.rq: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("workspace root")
    }

    /// Compile the REAL `dcat.rq` from the authored `dsl/mappings/projections/dcat.ttl`
    /// source (a pure function of committed, tracked sources — no dependency on a
    /// prior `make sync` materializing the git-ignored `generated/` tree) and fold it
    /// into a minimal synthetic GTS snapshot carrying just the `queries-archive` blob,
    /// exactly as the real bundle carries it (`REP_QUERIES`, basename-keyed member
    /// `dcat.rq`). This is the same fixture-construction idiom
    /// [`crate::docs_measure`]'s GTS-framing tests use.
    fn synthetic_gts_with_dcat_query() -> Vec<u8> {
        let root = repo_root();
        let compiled = crate::stages::mappings::compile_mappings(&root).expect("compile mappings");
        let dcat_rq_path = format!("{}/dcat.rq", crate::stages::mappings::QUERIES_DIR);
        let dcat_rq = compiled
            .artifacts
            .get(&dcat_rq_path)
            .unwrap_or_else(|| panic!("compiled mappings missing {dcat_rq_path}"))
            .clone();

        let archive = purrdf::ustar::write_archive(&[("dcat.rq".to_string(), dcat_rq)])
            .expect("tar the synthetic queries archive");
        let builder = purrdf::gts_compose::SnapshotBuilder::new();
        crate::gts_profile::emit_gmeow_gts(
            &builder,
            vec![purrdf::gts_compose::BlobRow {
                data: archive,
                media_type: "application/x-tar".to_string(),
                rep: crate::bundle_blobs::REP_QUERIES.to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("frame the synthetic GTS snapshot")
    }

    fn sample_entries() -> Vec<DistributionEntry> {
        vec![
            DistributionEntry {
                slug: "site".to_string(),
                rel_path: "dist/gmeow-docs/site".to_string(),
                blake3: "blake3:aaaa".to_string(),
                media_type: "text/html".to_string(),
            },
            DistributionEntry {
                slug: "okf".to_string(),
                rel_path: "dist/gmeow-docs/okf".to_string(),
                blake3: "blake3:bbbb".to_string(),
                media_type: "application/json".to_string(),
            },
        ]
    }

    #[test]
    fn distribution_blake3_is_deterministic_and_content_sensitive() {
        let mut tree = BTreeMap::new();
        tree.insert("a.txt".to_string(), b"hello".to_vec());
        tree.insert("b.txt".to_string(), b"world".to_vec());
        let d1 = distribution_blake3(&tree).expect("digest 1");
        let d2 = distribution_blake3(&tree).expect("digest 2");
        assert_eq!(d1, d2, "distribution_blake3 must be deterministic");
        assert!(d1.starts_with("blake3:"), "digest must carry the blake3: prefix: {d1}");

        tree.insert("a.txt".to_string(), b"HELLO".to_vec());
        let d3 = distribution_blake3(&tree).expect("digest 3");
        assert_ne!(d1, d3, "changing content must change the digest");
    }

    #[test]
    fn package_docs_dir_is_deterministic_and_content_sensitive() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("sub");
        std::fs::create_dir_all(&sub).expect("create subdir");
        std::fs::write(tmp.path().join("a.txt"), b"hello").expect("write a.txt");
        std::fs::write(sub.join("b.txt"), b"world").expect("write sub/b.txt");

        let (archive1, digest1) = package_docs_dir(tmp.path()).expect("package 1");
        let (archive2, digest2) = package_docs_dir(tmp.path()).expect("package 2");
        assert_eq!(archive1, archive2, "package_docs_dir must be byte-reproducible");
        assert_eq!(digest1, digest2, "package_docs_dir digest must be deterministic");
        assert!(
            digest1.starts_with("blake3:"),
            "digest must carry the blake3: prefix: {digest1}"
        );

        std::fs::write(sub.join("b.txt"), b"WORLD").expect("mutate sub/b.txt");
        let (archive3, digest3) = package_docs_dir(tmp.path()).expect("package 3");
        assert_ne!(archive1, archive3, "changing content must change the archive bytes");
        assert_ne!(digest1, digest3, "changing content must change the digest");
    }

    #[test]
    fn package_docs_dir_fails_closed_on_missing_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("does-not-exist");
        let err = package_docs_dir(&missing).expect_err("a missing directory must hard-fail");
        assert!(
            format!("{err}").contains("missing"),
            "failure must name the missing directory: {err}"
        );
    }

    #[test]
    fn package_docs_dir_fails_closed_on_empty_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = package_docs_dir(tmp.path()).expect_err("an empty directory must hard-fail");
        assert!(
            format!("{err}").contains("empty"),
            "failure must name the empty directory: {err}"
        );
    }

    #[test]
    fn manifest_is_deterministic() {
        let gts_bytes = synthetic_gts_with_dcat_query();
        let entries = sample_entries();
        let m1 = build_docs_distribution_manifest(&entries, &gts_bytes).expect("manifest 1");
        let m2 = build_docs_distribution_manifest(&entries, &gts_bytes).expect("manifest 2");
        assert_eq!(m1, m2, "the release distribution manifest must be byte-reproducible");
    }

    #[test]
    fn manifest_carries_a_checksum_and_catalog_link_per_entry() {
        let gts_bytes = synthetic_gts_with_dcat_query();
        let entries = sample_entries();
        let manifest = build_docs_distribution_manifest(&entries, &gts_bytes).expect("manifest");

        assert!(
            manifest.contains("http://spdx.org/rdf/terms#checksumValue"),
            "manifest must project at least one spdx:checksumValue triple:\n{manifest}"
        );
        for entry in &entries {
            let needle = format!("\"blake3:{}\"", entry.blake3.trim_start_matches("blake3:"));
            assert!(
                manifest.contains(&needle),
                "manifest must carry the verbatim digest for {}: {needle}\n{manifest}",
                entry.slug
            );
            let catalog_iri = dist_iri(&entry.slug);
            assert!(
                manifest.contains(&format!("<{catalog_iri}>")),
                "manifest must link entry {} to its Task-2 catalog subject {catalog_iri}:\n{manifest}",
                entry.slug
            );
        }
    }

    #[test]
    fn manifest_fails_closed_without_a_bundled_dcat_query() {
        // A snapshot carrying SOME unrelated blob (so `Bundle::from_snapshot` itself
        // succeeds) but no `queries-archive` blob at all — `bundled_queries` legitimately
        // returns an empty map (the "wheel-only-install" contract), and this module must
        // still hard-fail rather than silently build an empty manifest.
        let builder = purrdf::gts_compose::SnapshotBuilder::new();
        let gts_bytes = crate::gts_profile::emit_gmeow_gts(
            &builder,
            vec![purrdf::gts_compose::BlobRow {
                data: b"unrelated".to_vec(),
                media_type: "application/octet-stream".to_string(),
                rep: "not-queries".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("frame a GTS snapshot with no queries-archive blob");
        let err = build_docs_distribution_manifest(&sample_entries(), &gts_bytes)
            .expect_err("a bundle with no queries-archive blob must hard-fail");
        assert!(
            format!("{err}").contains("dcat.rq"),
            "failure must name the missing dcat.rq: {err}"
        );
    }

    #[test]
    fn release_instance_ntriples_is_sorted_and_deduped() {
        let nt = release_instance_ntriples(&sample_entries());
        let lines: Vec<&str> = nt.lines().collect();
        let mut sorted = lines.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(lines, sorted, "release instance N-Triples must be sorted+deduped");
    }
}

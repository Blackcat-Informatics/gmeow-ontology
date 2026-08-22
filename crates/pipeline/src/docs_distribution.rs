// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External documentation / serialization distribution rendering, content-addressing,
//! and the release-time DCAT manifest.
//!
//! This module is the SOLE producer [`crate::stages::okf`] results route through on
//! their way to the `dist/gmeow-docs/okf` external destination `gmeow-dev sync` writes
//! (`crates/gmeow-dev-cli`'s `sync_docs`). It never re-implements a serializer —
//! [`render_serialization_distributions`] calls ONLY
//! [`crate::stages::export::collect_term_surface`] + [`crate::stages::okf::render_okf`]
//! (OKF) — the same single-authority call [`crate::docs_measure`] already makes off the
//! SAME carrier dataset shape.
//!
//! JSON-LD-star / YAML-LD-star are DELIBERATELY absent here: `make build`
//! (`crates/gmeow-dev-cli`'s `dev_transpile::build`) already renders `dist/gmeow.jsonld`
//! / `dist/gmeow.yamlld` off the identical committed-bundle authority through
//! [`crate::stages::yaml_ld::serialize_graph`] / [`crate::stages::yaml_ld::serialize_graph_yaml`].
//! Re-serializing them a second time here would be a duplicate render with its own
//! on-disk channel that could silently diverge from the build output — instead
//! [`read_build_serialization_tree`] REFERENCES that single build output by reading it
//! off disk, so the docs distribution's `jsonld`/`yamlld` members are always the exact
//! bytes `make build` produced, never a second serialization pass.
//!
//! [`distribution_blake3`] is the shared content-addressing idiom every rendered
//! distribution tree goes through before it is named in the release manifest: pack the
//! tree into one deterministic USTAR archive (mirroring [`crate::docs_measure`]'s
//! packing convention so the digests stay directly comparable) and `blake3` the archive
//! bytes, formatted `blake3:<hex>`.
//!
//! [`build_docs_distribution_manifest`] builds the release-time DCAT catalog instance:
//! one `gmeow:Corpus` node whose `gmeow:corpusMember` rows ARE the canonical
//! distribution-catalog subjects (`https://blackcatinformatics.ca/gmeow/distribution/dist/<slug>`,
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
use purrdf::{RdfDataset, RdfTerm};

use crate::error::DocsDistribution as DocsDistributionError;
use crate::projections::{TagMap, project_graph};
use crate::stages::distribution_catalog::{dist_iri, iri, sub_asset_iri, triple, triple_lit};

fn err(message: impl Into<String>) -> Diag {
    Diag::of_kind(DocsDistributionError {
        message: message.into(),
    })
}

use gmeow_ns::GMEOW_NS;
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The single `gmeow:Corpus` subject every release manifest instance hangs its
/// `gmeow:corpusMember` distribution rows off. Stable across releases (this is a
/// release-instance identity, not a per-release-content digest).
const RELEASE_CORPUS_IRI: &str = "https://blackcatinformatics.ca/gmeow/distribution/release";

// ── serialization-family rendering ──────────────────────────────────────────────────

/// Render the OKF serialization distribution off `dataset` through the single
/// production serializer authority (see the module doc comment). `render_okf` keys its
/// members under `dist/gmeow-okf/…`; that prefix is stripped here so the returned tree
/// carries plain relative members, ready to reconcile under the caller's OWN
/// `dist/gmeow-docs/okf` base.
///
/// JSON-LD-star / YAML-LD-star are NOT rendered here — see the module doc comment;
/// their docs-distribution trees come from [`read_build_serialization_tree`] instead.
pub fn render_serialization_distributions(
    dataset: &RdfDataset,
) -> Result<BTreeMap<String, Vec<u8>>, Diag> {
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
    Ok(okf)
}

/// Reference a single build-produced serialization output (`dist/gmeow.jsonld` /
/// `dist/gmeow.yamlld` — [`crate::stages::yaml_ld::JSON_LD_PATH`] /
/// [`crate::stages::yaml_ld::YAML_LD_PATH`], written by `make build` /
/// `gmeow-dev build` off the SAME committed-bundle carrier dataset the docs fanout
/// reads) as a one-member docs-distribution tree keyed `member_name` (e.g.
/// `"gmeow.jsonld"`), ready to reconcile under the caller's OWN
/// `dist/gmeow-docs/{jsonld,yamlld}` base.
///
/// The docs fanout must NEVER re-serialize JSON-LD-star / YAML-LD-star — it reads the
/// exact bytes `make build` already wrote. No-optionality: a missing `build_output` is a
/// HARD FAIL naming the file and telling the operator to run `make build` first — never
/// a silent skip and never a fallback re-render.
pub fn read_build_serialization_tree(
    build_output: &Path,
    member_name: &str,
) -> Result<BTreeMap<String, Vec<u8>>, Diag> {
    let bytes = std::fs::read(build_output).map_err(|e| {
        err(format!(
            "read the build-produced serialization output {}: {e} — run `make build` first \
             (the docs distribution references that single build output rather than \
             re-rendering it)",
            build_output.display()
        ))
    })?;
    Ok(BTreeMap::from([(member_name.to_string(), bytes)]))
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
    let members: Vec<(String, Vec<u8>)> = tree
        .iter()
        .map(|(name, data)| (name.clone(), data.clone()))
        .collect();
    let archive = purrdf::ustar::write_archive(&members).map_err(|e| {
        err(format!(
            "tar a distribution tree for content-addressing: {e}"
        ))
    })?;
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
/// must materialize the docs distribution first (`make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`);
/// this function never silently produces an empty archive.
pub fn package_docs_dir(dir: &Path) -> Result<(Vec<u8>, String), Diag> {
    if !dir.is_dir() {
        return Err(err(format!(
            "docs distribution directory {} is missing — materialize it first with \
             `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`",
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
        let entry = entry.map_err(|e| {
            err(format!(
                "read a directory entry under {}: {e}",
                dir.display()
            ))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_docs_files(root, &path, out)?;
            continue;
        }
        // Belt-and-suspenders: a `.blake3` digest sidecar must never itself become an
        // archive member. The one sidecar this module's own callers used to write
        // under this tree (the release manifest's digest) has moved OUTSIDE it
        // (`gmeow-dev-cli`'s `docs_package`); skipping any stray `.blake3` file here
        // too means a future regression that re-introduces an in-tree sidecar still
        // cannot silently break packaging idempotency.
        if path.extension().and_then(|ext| ext.to_str()) == Some("blake3") {
            continue;
        }
        let rel = path.strip_prefix(root).map_err(|e| {
            err(format!(
                "compute the relative path of {}: {e}",
                path.display()
            ))
        })?;
        let rel_str: String = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let bytes =
            std::fs::read(&path).map_err(|e| err(format!("read {}: {e}", path.display())))?;
        out.insert(rel_str, bytes);
    }
    Ok(())
}

// ── release-time DCAT manifest ──────────────────────────────────────────────────────

/// One rendered distribution's manifest row: its distribution-catalog slug (`site`, `okf`,
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
/// entry, each member node BEING its distribution-catalog subject
/// ([`dist_iri`]) and carrying `gmeow:sourceLocation`, `gmeow:contentDigest`, and
/// `gmeow:artifactMediaType`.
fn release_instance_ntriples(
    entries: &[DistributionEntry],
    sub_asset_entries: &[DistributionEntry],
) -> String {
    let mut lines: Vec<String> =
        Vec::with_capacity((entries.len() + sub_asset_entries.len()) * 4 + 1);
    lines.push(triple(
        RELEASE_CORPUS_IRI,
        RDF_TYPE,
        &iri(GMEOW_NS, "Corpus"),
    ));
    // Each top-level distribution member IS its distribution-catalog subject.
    for entry in entries {
        emit_member(&mut lines, dist_iri(&entry.slug), entry);
    }
    // Each shared sub-asset (the vendored interactive engines, the browser bundle, the
    // demo library) is a corpus member keyed by its sub_asset subject, so its per-release
    // content digest rides HERE — the release-instance manifest — and NOT in the carrier
    // catalog (which stays digest-free; a render-derived digest there is a non-converging
    // fixpoint). The subject is SHARED across owning distributions, so one sub-asset shipped
    // by both `site` and `console` contributes one digest and two `gmeow:sourceLocation`
    // values — an honest "one component, two locations", not two components.
    for entry in sub_asset_entries {
        emit_member(&mut lines, sub_asset_iri(&entry.slug), entry);
    }
    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Emit one corpus-member's four release-instance triples (membership, source location,
/// content digest, media type) onto `member`.
fn emit_member(lines: &mut Vec<String>, member: String, entry: &DistributionEntry) {
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

/// Build the release-time DCAT distribution manifest: the [`release_instance_ntriples`]
/// graph for `entries`, projected through the bundle's compiled `dcat.rq` CONSTRUCT
/// (folded into `gts_bytes`'s `queries-archive` blob — [`crate::bundle_blobs::bundled_queries`])
/// via the single projection authority ([`project_graph`]). Returns the projected
/// N-Triples.
///
/// No-optionality: a `gts_bytes` snapshot with no bundled `dcat.rq` (the key
/// `dcat.rq`, or any `…/dcat.rq`) is a HARD FAIL — never a silently empty or partial
/// manifest.
pub fn build_docs_distribution_manifest(
    entries: &[DistributionEntry],
    sub_asset_entries: &[DistributionEntry],
    gts_bytes: &[u8],
) -> Result<String, Diag> {
    let instance_nt = release_instance_ntriples(entries, sub_asset_entries);

    let queries = crate::bundle_blobs::bundled_queries(gts_bytes)
        .map_err(|e| err(format!("load the bundled projection queries: {e}")))?;
    let dcat_rq_bytes = queries
        .iter()
        .find(|(key, _)| key.as_str() == "dcat.rq" || key.ends_with("/dcat.rq"))
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| {
            err(
                "bundled dcat.rq projection query is missing — cannot build the release \
                 distribution manifest",
            )
        })?;
    let dcat_rq = std::str::from_utf8(dcat_rq_bytes)
        .map_err(|e| err(format!("bundled dcat.rq is not valid UTF-8: {e}")))?;

    project_graph(&instance_nt, dcat_rq, &TagMap::default()).map_err(|e| {
        err(format!(
            "project the release distribution instance through dcat.rq: {e}"
        ))
    })
}

// ── consumer-side catalog matrix (`gmeow docs matrix`) ────────────────────────────

/// The per-format consumer-need matrix reader and its row type, re-exported from the
/// wasm-clean [`gmeow_docs_catalog`] leaf at their historical paths.
///
/// The reader MOVED there because it has consumers that must not inherit this build
/// executor: `gmeow docs matrix` on the consumer CLI, and the MCP `distribution_matrix`
/// tool, which is a bundle-only leaf. Reading nine rows out of a shipped catalog graph is
/// a pure function of snapshot bytes and needs neither the stage DAG nor the scheduler nor
/// the release signer. There is exactly ONE definition site — over there — and this
/// re-export keeps `gmeow_pipeline::docs_distribution::read_distribution_matrix` spelled
/// the way every existing caller already spells it.
///
/// The `gmeow:DocumentationDistribution` filter is unchanged by the move: `gmeow docs
/// matrix` lists exactly the declared distributions
/// ([`crate::stages::distribution_catalog::DISTRIBUTIONS`], nine of them since the console
/// became a first-class distribution).
pub use gmeow_docs_catalog::{DistributionRow, read_distribution_matrix};

// ── manifest verification (`gmeow docs verify`) ───────────────────────────────────

/// One format's blake3 verification outcome: the manifest's declared digest vs. the
/// digest recomputed by re-packaging `<dir>/<slug>` through [`package_docs_dir`] — the
/// SAME content-addressing idiom the release manifest was built from
/// ([`distribution_blake3`]/[`package_docs_dir`]), so `ok` is a genuine byte-content
/// proof, never a metadata-only comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionVerdict {
    pub slug: String,
    pub declared: String,
    pub actual: String,
    pub ok: bool,
}

/// Verify a materialized documentation distribution's blake3 content digests against
/// its DCAT manifest (`<dir>/manifest/docs-manifest.ttl`) — the consumer-side twin of
/// the release-time manifest build ([`build_docs_distribution_manifest`]).
///
/// Reads every `dcat:Distribution` the manifest declares (its catalog subject IRI
/// `…/distribution/dist/<slug>` yields the slug; its `spdx:checksum`/
/// `spdx:checksumValue` pair yields the declared VERBATIM `blake3:<hex>` digest), then
/// for each — optionally restricted to the single slug `only` — recomputes
/// [`package_docs_dir`]`(dir.join(slug)).1` and compares.
///
/// No-optionality: a missing/unparsable manifest, a manifest with zero declared
/// distributions, an `only` slug the manifest does not name, or a referenced format
/// directory that is missing/empty under `dir`, is a HARD FAIL — never a silently
/// skipped or partial verdict list.
pub fn verify_docs_distribution(
    dir: &Path,
    only: Option<&str>,
) -> Result<Vec<DistributionVerdict>, Diag> {
    let manifest_path = dir.join("manifest").join("docs-manifest.ttl");
    let bytes = std::fs::read(&manifest_path).map_err(|e| {
        err(format!(
            "read the DCAT distribution manifest {}: {e} — materialize it first with \
             `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`",
            manifest_path.display()
        ))
    })?;
    let dataset = purrdf::parse_dataset(&bytes, "application/n-triples", None).map_err(|e| {
        err(format!(
            "parse {} as N-Triples: {e}",
            manifest_path.display()
        ))
    })?;
    let quads = purrdf::flat_rdf_quads_from_dataset(dataset.as_ref());

    const DIST_BASE: &str = "https://blackcatinformatics.ca/gmeow/distribution/dist/";
    const DCAT_DISTRIBUTION: &str = "http://www.w3.org/ns/dcat#Distribution";
    const SPDX_CHECKSUM: &str = "http://spdx.org/rdf/terms#checksum";
    const SPDX_CHECKSUM_VALUE: &str = "http://spdx.org/rdf/terms#checksumValue";

    let mut declared: BTreeMap<String, String> = BTreeMap::new();
    for quad in &quads {
        if quad.predicate != RDF_TYPE {
            continue;
        }
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let RdfTerm::Iri(object) = &quad.object else {
            continue;
        };
        if object != DCAT_DISTRIBUTION {
            continue;
        }
        let Some(slug) = subject.strip_prefix(DIST_BASE) else {
            continue;
        };

        let checksum_node = quads
            .iter()
            .find(|q| q.subject == quad.subject && q.predicate == SPDX_CHECKSUM)
            .ok_or_else(|| {
                err(format!(
                    "distribution {slug} is missing spdx:checksum in {}",
                    manifest_path.display()
                ))
            })?;
        let checksum_value = quads
            .iter()
            .find(|q| q.subject == checksum_node.object && q.predicate == SPDX_CHECKSUM_VALUE)
            .and_then(|q| match &q.object {
                RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                err(format!(
                    "distribution {slug} checksum node is missing spdx:checksumValue in {}",
                    manifest_path.display()
                ))
            })?;
        declared.insert(slug.to_string(), checksum_value);
    }
    if declared.is_empty() {
        return Err(err(format!(
            "{} declares zero dcat:Distribution rows — nothing to verify",
            manifest_path.display()
        )));
    }

    let slugs: Vec<String> = match only {
        Some(slug) => {
            if !declared.contains_key(slug) {
                return Err(err(format!(
                    "{} names no distribution {slug:?}",
                    manifest_path.display()
                )));
            }
            vec![slug.to_string()]
        }
        None => declared.keys().cloned().collect(),
    };

    let mut verdicts = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let declared_digest = declared
            .get(&slug)
            .expect("slug was sourced from the declared map's own keys")
            .clone();
        let format_dir = dir.join(&slug);
        let (_, actual_digest) = package_docs_dir(&format_dir)
            .map_err(|e| err(format!("recompute the digest for distribution {slug}: {e}")))?;
        let ok = declared_digest == actual_digest;
        verdicts.push(DistributionVerdict {
            slug,
            declared: declared_digest,
            actual: actual_digest,
            ok,
        });
    }
    verdicts.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(verdicts)
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
    /// prior `make check` materializing the git-ignored `generated/` tree) and fold it
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
        gmeow_gts_profile::emit_gmeow_gts(
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
        assert!(
            d1.starts_with("blake3:"),
            "digest must carry the blake3: prefix: {d1}"
        );

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
        assert_eq!(
            archive1, archive2,
            "package_docs_dir must be byte-reproducible"
        );
        assert_eq!(
            digest1, digest2,
            "package_docs_dir digest must be deterministic"
        );
        assert!(
            digest1.starts_with("blake3:"),
            "digest must carry the blake3: prefix: {digest1}"
        );

        std::fs::write(sub.join("b.txt"), b"WORLD").expect("mutate sub/b.txt");
        let (archive3, digest3) = package_docs_dir(tmp.path()).expect("package 3");
        assert_ne!(
            archive1, archive3,
            "changing content must change the archive bytes"
        );
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
        let m1 = build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("manifest 1");
        let m2 = build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("manifest 2");
        assert_eq!(
            m1, m2,
            "the release distribution manifest must be byte-reproducible"
        );
    }

    #[test]
    fn manifest_carries_a_checksum_and_catalog_link_per_entry() {
        let gts_bytes = synthetic_gts_with_dcat_query();
        let entries = sample_entries();
        let manifest =
            build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("manifest");

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
                "manifest must link entry {} to its distribution-catalog subject {catalog_iri}:\n{manifest}",
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
        let gts_bytes = gmeow_gts_profile::emit_gmeow_gts(
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
        let err = build_docs_distribution_manifest(&sample_entries(), &[], &gts_bytes)
            .expect_err("a bundle with no queries-archive blob must hard-fail");
        assert!(
            format!("{err}").contains("dcat.rq"),
            "failure must name the missing dcat.rq: {err}"
        );
    }

    #[test]
    fn site_sub_asset_digests_ride_the_release_instance_on_the_sub_asset_subject() {
        use crate::stages::distribution_catalog::sub_asset_iri;
        let sub_assets = vec![DistributionEntry {
            slug: "mcp-wasm".to_string(),
            rel_path: "dist/gmeow-docs/site/assets/mcp/".to_string(),
            blake3: "blake3:deadbeef".to_string(),
            media_type: "application/wasm".to_string(),
        }];
        let nt = release_instance_ntriples(&sample_entries(), &sub_assets);
        let node = sub_asset_iri("mcp-wasm");
        // The sub-asset digest hangs off its site_sub_asset subject (NOT a dist_iri),
        // as a corpus member — so dcat.rq projects it exactly like a distribution.
        assert!(
            nt.contains(&triple(
                RELEASE_CORPUS_IRI,
                &iri(GMEOW_NS, "corpusMember"),
                &node
            )),
            "sub-asset must be a corpus member of the release: {nt}"
        );
        assert!(
            nt.contains(&triple_lit(
                &node,
                &iri(GMEOW_NS, "contentDigest"),
                "blake3:deadbeef"
            )),
            "sub-asset content digest must ride on its site_sub_asset subject: {nt}"
        );
    }

    #[test]
    fn build_manifest_projects_site_sub_asset_digests_through_dcat_rq() {
        use crate::stages::distribution_catalog::sub_asset_iri;
        // Exercise the FULL projection (release_instance_ntriples -> dcat.rq), not just
        // the pre-projection input the sibling test checks: a dcat.rq regression that
        // dropped the SiteSubAsset corpus-member branch would publish no sub-asset digest
        // in the shipped manifest yet leave that pre-projection test green.
        let gts_bytes = synthetic_gts_with_dcat_query();
        let sub_assets = vec![DistributionEntry {
            slug: "mcp-wasm".to_string(),
            rel_path: "dist/gmeow-docs/site/assets/mcp/".to_string(),
            blake3: "blake3:deadbeef".to_string(),
            media_type: "application/wasm".to_string(),
        }];
        let manifest = build_docs_distribution_manifest(&sample_entries(), &sub_assets, &gts_bytes)
            .expect("manifest with a site sub-asset");
        let node = sub_asset_iri("mcp-wasm");
        assert!(
            manifest.contains(&format!("<{node}>")),
            "the projected manifest must carry the site_sub_asset subject {node}:\n{manifest}"
        );
        assert!(
            manifest.contains("\"blake3:deadbeef\""),
            "the sub-asset digest must survive the dcat.rq projection into the shipped \
             manifest — a dropped SiteSubAsset row is a missing release digest:\n{manifest}"
        );
    }

    #[test]
    fn release_instance_ntriples_is_sorted_and_deduped() {
        let nt = release_instance_ntriples(&sample_entries(), &[]);
        let lines: Vec<&str> = nt.lines().collect();
        let mut sorted = lines.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            lines, sorted,
            "release instance N-Triples must be sorted+deduped"
        );
    }

    // ── read_distribution_matrix ────────────────────────────────────────────────

    /// Fold the REAL [`crate::stages::distribution_catalog::build_distribution_catalog`]
    /// output (a pure function of committed sources — no `make check` dependency) into a
    /// minimal synthetic GTS snapshot carrying just that named graph, exactly as the
    /// real bundle carries it.
    fn synthetic_gts_with_catalog() -> Vec<u8> {
        let dataset = crate::stages::distribution_catalog::build_distribution_catalog()
            .expect("build distribution catalog");
        let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
        builder
            .add_dataset(&dataset)
            .expect("add catalog dataset to snapshot builder");
        gmeow_gts_profile::emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
            .expect("frame the synthetic GTS snapshot")
    }

    #[test]
    fn read_distribution_matrix_over_a_synthetic_bundle_returns_all_nine_slugs() {
        use crate::stages::distribution_catalog::DISTRIBUTIONS;
        let gts_bytes = synthetic_gts_with_catalog();
        let rows = read_distribution_matrix(&gts_bytes).expect("read distribution matrix");
        let slugs: Vec<&str> = rows.iter().map(|r| r.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec![
                "console", "jsonld", "mdbook", "okf", "pdf", "pydantic", "site", "snippets",
                "yamlld"
            ],
            "matrix must carry exactly the nine declared distributions, sorted by slug"
        );

        // Every row's facets ARE the table's — no restated expectations here.
        for row in &rows {
            let declared = DISTRIBUTIONS
                .iter()
                .find(|d| d.slug == row.slug)
                .unwrap_or_else(|| panic!("matrix row {} is not a declared row", row.slug));
            assert_eq!(row.family, declared.family.slug(), "{}", row.slug);
            assert_eq!(row.media_type, declared.media_type, "{}", row.slug);
            assert_eq!(
                row.consumers,
                vec![declared.consumer.to_string()],
                "{}",
                row.slug
            );
            // The dropped set is DERIVED from the surface lattice, never authored — and
            // its SPELLING is read back off the emitter's own `capability_iri`, so a
            // renamed capability individual moves both sides at once. A local
            // capability→local-name table here would be a second authority: rename one
            // arm and this gate would go on asserting a spelling no emitter produces.
            let expected: Vec<String> = match declared.surface {
                Some(surface) => {
                    let mut dropped: Vec<String> =
                        gmeow_docs::formats::surface_capabilities(surface)
                            .dropped
                            .into_iter()
                            .map(|cap| {
                                gmeow_docs_catalog::identity::local_name(
                                    &crate::stages::distribution_catalog::capability_iri(cap),
                                )
                            })
                            .collect();
                    dropped.sort();
                    dropped
                }
                None => Vec::new(),
            };
            assert_eq!(
                row.dropped_capabilities, expected,
                "{}: the matrix's dropped set must be the lattice's",
                row.slug
            );
        }

        // The console is a genuine row with a DERIVED, non-empty loss set — the property
        // that most easily regresses to "declared but empty".
        let console = rows
            .iter()
            .find(|r| r.slug == "console")
            .expect("console row");
        assert_eq!(console.family, "interactive-runtime");
        assert_eq!(console.media_type, "text/html");
        assert_eq!(
            console.consumers,
            vec!["consumerInteractiveConsole".to_string()]
        );
        assert_eq!(
            console.dropped_capabilities,
            vec![
                "capabilityCrossLinkFidelity".to_string(),
                "capabilitySearchIndex".to_string()
            ],
            "the console's dropped capabilities must be derived from the surface lattice"
        );

        let okf = rows.iter().find(|r| r.slug == "okf").expect("okf row");
        assert!(
            okf.dropped_capabilities.is_empty(),
            "serialization family declares no loss: {okf:?}"
        );
    }

    /// The EMITTER half of the concept-lattice pair, read back through the reader that was
    /// written against the shape. Over the REAL catalog this returns the four concepts the
    /// authored surface × capability incidence admits — an empty result would now mean the
    /// emitter regressed, not that a lattice is optional.
    #[test]
    fn read_concept_lattice_over_a_synthetic_bundle_returns_the_derived_lattice() {
        let gts_bytes = synthetic_gts_with_catalog();
        let rows =
            gmeow_docs_catalog::read_concept_lattice(&gts_bytes).expect("read concept lattice");
        let rendered: Vec<(Vec<&str>, Vec<&str>)> = rows
            .iter()
            .map(|row| {
                (
                    row.extent.iter().map(String::as_str).collect(),
                    row.intent.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        // Rows sort by concept IRI, whose tail is the extent joined in surface order
        // (`site`, `site+mdbook`, `site+mdbook+console`, `site+mdbook+pdf+snippets+console`)
        // — so the rows arrive bottom-up along the lattice. Extents and intents come back
        // as ALPHABETICALLY sorted local names, which is the reader's own convention.
        assert_eq!(
            rendered,
            vec![
                (
                    vec!["site"],
                    vec![
                        "capabilityCrossLinkFidelity",
                        "capabilityDiagrams",
                        "capabilityInteractivity",
                        "capabilityLiveReasoning",
                        "capabilityLiveSparql",
                        "capabilitySearchIndex",
                    ]
                ),
                (
                    vec!["mdbook", "site"],
                    vec![
                        "capabilityCrossLinkFidelity",
                        "capabilityDiagrams",
                        "capabilityInteractivity",
                        "capabilityLiveReasoning",
                        "capabilityLiveSparql",
                    ]
                ),
                (
                    vec!["console", "mdbook", "site"],
                    vec![
                        "capabilityDiagrams",
                        "capabilityInteractivity",
                        "capabilityLiveReasoning",
                        "capabilityLiveSparql",
                    ]
                ),
                (
                    vec!["console", "mdbook", "pdf", "site", "snippets"],
                    Vec::<&str>::new()
                ),
            ],
            "the emitted lattice must be the four concepts the incidence admits"
        );

        // Emitting the lattice must not widen the matrix: the matrix is exactly the
        // DECLARED table, no more. The console appears in both now — as an object of the
        // lattice and as the ninth shipped distribution — but a lattice concept node is
        // still never a matrix row.
        use crate::stages::distribution_catalog::DISTRIBUTIONS;
        let matrix = read_distribution_matrix(&gts_bytes).expect("read distribution matrix");
        assert_eq!(
            matrix.len(),
            DISTRIBUTIONS.len(),
            "the matrix must be exactly the declared table: {:?}",
            matrix.iter().map(|r| &r.slug).collect::<Vec<_>>()
        );
        assert!(
            matrix.iter().any(|row| row.slug == "console"),
            "the console is a shipped distribution and an object of the lattice"
        );
        for row in &rows {
            assert!(
                !matrix.iter().any(|m| m.slug == row.concept),
                "a formal concept must never surface as a distribution row: {}",
                row.concept
            );
        }
    }

    #[test]
    fn read_distribution_matrix_fails_closed_without_the_catalog_graph() {
        let builder = purrdf::gts_compose::SnapshotBuilder::new();
        let gts_bytes = gmeow_gts_profile::emit_gmeow_gts(
            &builder,
            vec![purrdf::gts_compose::BlobRow {
                data: b"unrelated".to_vec(),
                media_type: "application/octet-stream".to_string(),
                rep: "not-catalog".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("frame a GTS snapshot with no distribution-catalog graph");
        let err = read_distribution_matrix(&gts_bytes)
            .expect_err("a bundle with no distribution-catalog graph must hard-fail");
        assert!(
            format!("{err}").contains("distribution-catalog"),
            "failure must name the missing catalog graph: {err}"
        );
    }

    // ── verify_docs_distribution ────────────────────────────────────────────────

    #[test]
    fn verify_docs_distribution_passes_fresh_then_fails_on_tamper() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let docs_dir = tmp.path();
        let sample_dir = docs_dir.join("sample");
        std::fs::create_dir_all(&sample_dir).expect("mkdir sample");
        std::fs::write(sample_dir.join("a.txt"), b"hello").expect("write a.txt");

        let (_, digest) = package_docs_dir(&sample_dir).expect("package sample");
        let entries = vec![DistributionEntry {
            slug: "sample".to_string(),
            rel_path: "dist/gmeow-docs/sample".to_string(),
            blake3: digest.clone(),
            media_type: "text/plain".to_string(),
        }];
        let gts_bytes = synthetic_gts_with_dcat_query();
        let manifest =
            build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("build manifest");
        let manifest_dir = docs_dir.join("manifest");
        std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
        std::fs::write(manifest_dir.join("docs-manifest.ttl"), &manifest).expect("write manifest");

        let verdicts = verify_docs_distribution(docs_dir, None).expect("verify fresh package");
        assert_eq!(
            verdicts.len(),
            1,
            "expected exactly one verdict: {verdicts:?}"
        );
        assert_eq!(verdicts[0].slug, "sample");
        assert_eq!(verdicts[0].declared, digest);
        assert_eq!(verdicts[0].actual, digest);
        assert!(
            verdicts[0].ok,
            "a freshly packaged tree must verify clean: {:?}",
            verdicts[0]
        );

        // Flip a byte in the packaged tree — the recomputed digest must diverge and the
        // verdict must flip to failed, never silently pass.
        std::fs::write(sample_dir.join("a.txt"), b"HELLO").expect("tamper a.txt");
        let verdicts = verify_docs_distribution(docs_dir, None).expect("verify tampered package");
        assert_eq!(verdicts.len(), 1);
        assert_ne!(
            verdicts[0].actual, verdicts[0].declared,
            "tampering must change the recomputed digest"
        );
        assert!(
            !verdicts[0].ok,
            "a tampered tree must fail verification: {:?}",
            verdicts[0]
        );

        // `only` filtering on the sole declared slug still finds it.
        let filtered =
            verify_docs_distribution(docs_dir, Some("sample")).expect("verify filtered by slug");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].slug, "sample");
    }

    #[test]
    fn verify_docs_distribution_fails_closed_on_unknown_only_slug() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let docs_dir = tmp.path();
        let sample_dir = docs_dir.join("sample");
        std::fs::create_dir_all(&sample_dir).expect("mkdir sample");
        std::fs::write(sample_dir.join("a.txt"), b"hello").expect("write a.txt");
        let (_, digest) = package_docs_dir(&sample_dir).expect("package sample");
        let entries = vec![DistributionEntry {
            slug: "sample".to_string(),
            rel_path: "dist/gmeow-docs/sample".to_string(),
            blake3: digest,
            media_type: "text/plain".to_string(),
        }];
        let gts_bytes = synthetic_gts_with_dcat_query();
        let manifest =
            build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("build manifest");
        let manifest_dir = docs_dir.join("manifest");
        std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
        std::fs::write(manifest_dir.join("docs-manifest.ttl"), &manifest).expect("write manifest");

        let err = verify_docs_distribution(docs_dir, Some("does-not-exist"))
            .expect_err("an unknown --format slug must hard-fail");
        assert!(
            format!("{err}").contains("does-not-exist"),
            "failure must name the unknown slug: {err}"
        );
    }

    #[test]
    fn verify_docs_distribution_fails_closed_on_missing_manifest() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = verify_docs_distribution(tmp.path(), None)
            .expect_err("a missing manifest must hard-fail");
        assert!(
            format!("{err}").contains("docs-manifest.ttl"),
            "failure must name the missing manifest: {err}"
        );
    }

    // ── read_build_serialization_tree (reference the build output, don't re-render) ──

    #[test]
    fn read_build_serialization_tree_carries_the_exact_build_output_bytes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let build_output = tmp.path().join("gmeow.jsonld");
        let bytes = b"{\"@context\": {}, \"@graph\": []}".to_vec();
        std::fs::write(&build_output, &bytes).expect("write fake build output");

        let tree = read_build_serialization_tree(&build_output, "gmeow.jsonld")
            .expect("read a present build output");
        assert_eq!(
            tree.get("gmeow.jsonld"),
            Some(&bytes),
            "the docs-distribution member must carry the build output's bytes byte-for-byte, \
             never a re-rendered copy: {tree:?}"
        );
        assert_eq!(
            tree.len(),
            1,
            "a single build output must yield exactly one tree member: {tree:?}"
        );
    }

    #[test]
    fn read_build_serialization_tree_fails_closed_when_the_build_output_is_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("gmeow.yamlld");
        let err = read_build_serialization_tree(&missing, "gmeow.yamlld")
            .expect_err("a missing build output must hard-fail, never silently skip");
        let message = format!("{err}");
        assert!(
            message.contains("gmeow.yamlld") && message.contains("make build"),
            "failure must name the missing build output and point at `make build`: {message}"
        );
    }
}

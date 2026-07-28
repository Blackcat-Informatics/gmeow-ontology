// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-anchored projection / description commands: `describe`, the docs fanout
//! used by `sync`, `temporal`, `import-foundation`, `crossref`, and
//! `compliance-report`.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_cli_core::{DocsProjectionReport, reconcile_docs_projection_tree};

use crate::dev_common::{emit_error, fail, fail_code, note, project_root, snapshot_bytes};
use crate::error;

/// Aggregate render and filesystem-reconciliation outcome for the complete
/// external documentation fanout selected by `gmeow-dev sync`.
#[derive(Debug, Default)]
pub struct DocsSyncReport {
    pub output_paths: Vec<String>,
    pub reconciliation: DocsProjectionReport,
}

/// `gmeow-dev describe TERM [--gts --lang]` — render one term card.
pub fn describe(term: &str, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    let root = project_root();
    let bytes = match gts {
        Some(path) => match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        },
        None => match snapshot_bytes(&root) {
            Ok(b) => b,
            Err(code) => return code,
        },
    };
    let resolved: Option<String> = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    // The JSON Schema `$defs` key set folded into THIS bundle — the
    // model-existence signal `build_card` gates a class's `python_model` link on
    // (a class with no `$defs` entry has no generated Pydantic model, so the link
    // must never be fabricated: issue "Pydantic model surface", finding F3).
    let modeled_defs = match gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&bytes)
        .and_then(|bundle| bundle.modeled_def_keys())
    {
        Ok(defs) => defs,
        Err(e) => {
            return fail(format!(
                "cannot read the bundled JSON Schema for the model-existence gate: {e}"
            ));
        }
    };
    let (text, status) = gmeow_docs::describe(
        term,
        &bytes,
        resolved.as_deref(),
        gmeow_docs::card::CardFormat::Prose,
        &modeled_defs,
    );
    // Map each backend failure kind to its OWN typed diagnostic code — a
    // resolution miss, a cross-namespace ambiguity, an unknown language, and a
    // bundle-load failure are distinct, greppable codes (mirrors the shipped
    // `gmeow describe` mapping in `gmeow-cli::commands::describe`).
    use gmeow_docs::DescribeStatus;
    match status {
        DescribeStatus::Ok => println!("{text}"),
        DescribeStatus::Unresolved => emit_error("gmeow-dev.describe.unresolved", text),
        DescribeStatus::Ambiguous => emit_error("gmeow-dev.describe.ambiguous", text),
        DescribeStatus::UnknownLanguage => emit_error("gmeow-dev.lang.unknown", text),
        DescribeStatus::LoadFailed => emit_error("gmeow-dev.describe.load-failed", text),
    }
    status.exit_code()
}

/// The repo-relative member the curated conjecture demo library rides under in the
/// bundle's `examples-archive` — the same path it occupies in the working tree, because
/// [`REP_EXAMPLES`](gmeow_pipeline::bundle_blobs::REP_EXAMPLES) keys members repo-relative.
pub(crate) const CONJECTURE_LIBRARY_MEMBER: &str =
    "slices/grounding/logic/examples/conjectures.ttl";

/// Read the curated `logic:Conjecture` demo library out of a `gmeow.gts` snapshot.
///
/// The bundle is the source; the working tree is not consulted. A snapshot that predates
/// the examples fold is a hard failure naming the regenerate that produces it, so a stale
/// local bundle is reported rather than silently papered over with the disk copy.
fn bundled_conjecture_library(snapshot: &[u8]) -> Result<Vec<u8>, i32> {
    let bundle = gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(snapshot)
        .map_err(|e| fail(format!("cannot read bundle blobs: {e}")))?;
    let examples = bundle
        .examples()
        .map_err(|e| fail(format!("cannot read the bundle's examples archive: {e}")))?;
    examples
        .get(CONJECTURE_LIBRARY_MEMBER)
        .cloned()
        .ok_or_else(|| {
            fail(format!(
                "the bundle carries no {CONJECTURE_LIBRARY_MEMBER} member in its examples \
                 archive — the curated conjecture demo library is a mandatory site sub-asset, \
                 and this snapshot predates the examples fold. Run `make regen` to \
                 re-materialize generated/dist/gmeow.gts; the disk copy is NOT a fallback \
                 (reading it is what let the bundle and the rendered playground diverge)."
            ))
        })
}

/// Build the site render's [`gmeow_docs::ExecutableDocsData`] from the committed
/// `gmeow.gts` bundle — which IS the queryable site asset, shipped verbatim.
///
/// This function used to derive two further query assets from these same bytes: a TriG
/// projection for the playground and an object-level N-Quads projection for the explorer,
/// 311 MB between them. Both are retired. The browser engine boots over the bundle, so
/// every question those assets existed to answer is answered from the bundle directly, and
/// the projections could only ever disagree with it.
///
/// A missing or unreadable committed bundle is a hard fail (no-optionality): the `Err`
/// carries the console exit code the caller returns.
fn playground_exec_from_bundle(root: &Path) -> Result<gmeow_docs::ExecutableDocsData, i32> {
    let gts_path = root.join(crate::dev_common::GTS_SNAPSHOT_REL);
    let bytes = std::fs::read(&gts_path).map_err(|e| {
        fail(format!(
            "cannot read committed bundle {}: {e}",
            gts_path.display()
        ))
    })?;
    // The W4 conjecture-playground demo library: the curated `logic:Conjecture` corpus,
    // read OUT OF THE BUNDLE (the `examples-archive` blob), not off disk.
    //
    // It used to be a `std::fs::read` of the slice source. That made the shipped
    // `gmeow.gts` an incomplete carrier of its own documentation surface: the site render
    // needed a file the bundle did not contain, so a repo-free render was impossible and
    // the bundle and the rendered playground could disagree with nothing to catch it. The
    // corpus is now folded into the bundle by `stage-snapshot` and read back here through
    // the ONE bundle reader.
    //
    // No-optionality: a bundle with no such member is a HARD FAIL naming the regenerate
    // that folds it — never a fallback to the disk copy, which would restore exactly the
    // divergence this removes.
    let conjectures_ttl = bundled_conjecture_library(&bytes)?;
    Ok(gmeow_docs::ExecutableDocsData {
        full_bundle_gts: bytes,
        conjectures_ttl,
        ..Default::default()
    })
}

/// The output bases `console-assemble` REFUSES to write into.
///
/// Both are materialized by exactly one writer — `make regen SYNC_OUTPUTS=docs` — which
/// reconciles them as whole trees. A second command dropping a partial console tree into
/// either would look like drift to the reconciler and would be silently reverted, or worse
/// would be reconciled AWAY along with real output. Refusing is the honest behaviour, and
/// the refusal names the one writer so the reader knows what to run instead.
pub(crate) const CONSOLE_REFUSED_BASES: &[&str] = &["ontology-docs", "dist/gmeow-docs"];

/// The single writer of the refused bases, named in every refusal.
pub(crate) const CONSOLE_REFUSAL_WRITER: &str = "make regen SYNC_OUTPUTS=docs";

/// Whether `out` is equal to, or inside, one of [`CONSOLE_REFUSED_BASES`].
///
/// Compared on NORMALIZED path components (no string prefix matching), so
/// `ontology-docs`, `./ontology-docs/console`, and an absolute path under the repo's
/// `ontology-docs/` all refuse, while a sibling like `ontology-docs-scratch` does not.
#[must_use]
pub(crate) fn console_out_is_refused(root: &Path, out: &Path) -> Option<&'static str> {
    let absolute = if out.is_absolute() {
        out.to_path_buf()
    } else {
        root.join(out)
    };
    // Normalize away `.` and `..` without touching the filesystem (the directory need not
    // exist yet, so `canonicalize` is not available).
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other.as_os_str().to_os_string()),
        }
    }
    let normalized: std::path::PathBuf = parts.iter().collect();
    CONSOLE_REFUSED_BASES
        .iter()
        .find(|base| normalized.starts_with(root.join(base)))
        .copied()
}

/// `gmeow-dev console-assemble --out <dir>` — write the standalone console tree.
///
/// The tree is [`gmeow_docs::console_files`] over the SAME `exec`
/// ([`playground_exec_from_bundle`]) the site render uses, so the assembled console and
/// the site's `console/` subtree are the identical bytes by construction rather than by
/// a copy step.
pub fn console_assemble(out: &Path) -> i32 {
    let root = project_root();
    if let Some(base) = console_out_is_refused(&root, out) {
        return fail_code(
            format!(
                "refusing to write the console into {} (it is {base}/ or inside it): that base has \
                 exactly one writer, `{CONSOLE_REFUSAL_WRITER}`, which reconciles it as a whole \
                 tree. Choose a scratch --out (the pinned default is dist/console-smoke).",
                out.display()
            ),
            2,
        );
    }
    let exec = match playground_exec_from_bundle(&root) {
        Ok(exec) => exec,
        Err(code) => return code,
    };
    let files = gmeow_docs::console_files(&exec);
    if files.is_empty() {
        return fail(
            "the console assembled to zero files — the bundle-backed exec is not interactive, so \
             there is no engine to ship (a console without its engine is broken, not smaller)"
                .to_string(),
        );
    }
    let site = gmeow_docs::Site { files };
    match gmeow_docs::render::write_site(&site, out) {
        Ok(written) => {
            note(
                "gmeow-dev.console.assembled",
                format!("{} files under {}", written.len(), out.display()),
            );
            println!("{}", out.join("console/index.html").display());
            0
        }
        Err(e) => fail(format!(
            "cannot write the console tree to {}: {e}",
            out.display()
        )),
    }
}

/// Render every external documentation projection once for `gmeow-dev sync`.
/// Update mode reconciles the trees to their canonical destinations; check mode
/// performs the same complete render in memory and never touches the workspace.
pub fn sync_docs(update: bool, lang: Option<&str>) -> Result<DocsSyncReport, i32> {
    let root = project_root();

    let mut model = match gmeow_docs::DocsModel::discover(&root) {
        Ok(model) => model,
        Err(e) => return Err(fail(format!("cannot build documentation model: {e}"))),
    };
    // Attach the per-term JSON-Schema / OpenAPI fragment digest so the per-term
    // Python (Pydantic) + Rust example tabs actually render on this — the sole —
    // production docs surface (`make regen SYNC_OUTPUTS=docs` fanout). The standalone render has no
    // live pipeline product, so the digest is sourced off the committed
    // `generated/schemas/*.json`, the projection of the same
    // `stage-export-json-schema` emitter output the in-pipeline reader consumes.
    // Hard-fails if the required committed schema source is missing (no silent
    // None — no-optionality).
    match gmeow_pipeline::stages::docs_render::schema_fragments_from_generated(&root, &model.terms)
    {
        Ok(digest) => model.attach_schema_fragments(digest),
        Err(e) => return Err(fail(format!("cannot build schema-fragment digest: {e}"))),
    }
    let source_lang = pick_source_lang(lang, &model.translations)?;

    // The reasoned SPARQL playground is a SITE surface: build its `ExecutableDocsData`
    // from the committed bundle — which already carries the documentation graph, the
    // reasoned closure, and the chase-invented-null (Skolem witness) subgraph — ONLY for
    // the formats that render the site. No-optionality: for a site render a missing or
    // unreadable committed bundle is a HARD FAIL (mirrors the schema-fragment gate above),
    // never a silently empty playground; non-site formats never touch the bundle.
    let exec = playground_exec_from_bundle(&root)?;

    // One model discovery and ONE site render feed both site destinations and
    // the snippets projection. The previous workflow rendered this same site
    // three times (`all` site, snippets, then ontology-docs).
    let site = gmeow_docs::render_site_lang_exec(&model, &source_lang, &exec).files;
    let snippets =
        source_snippets(&site).map_err(|e| fail(format!("cannot render snippets: {e}")))?;
    let mdbook = render_source_book(&model);
    let pdf = render_source_print(&root, &model)
        .map_err(|e| fail(format!("cannot render print docs: {e}")))?;
    let pydantic = gmeow_pipeline::stages::pydantic::render_models_python_package(&root)
        .map_err(|e| fail(format!("cannot render Pydantic docs: {e}")))?;
    // The standalone interactive console — the ninth distribution. Assembled off the SAME
    // `exec` the site render uses, through the SAME single producer `console-assemble`
    // calls, so the console distribution and the site's `console/` subtree are the
    // identical bytes by construction rather than by a copy step. Its keys keep their
    // `console/…` + `assets/…` prefixes: the generated service-worker SHELL resolves
    // `../assets/…` relative to `console/sw.mjs`, so flattening the prefix here would
    // silently break the offline cache of the tree we ship.
    let console = gmeow_docs::console_files(&exec);

    // The OKF serialization distribution (AC2 payload segmentation): rendered off the
    // SAME committed-bundle carrier dataset the site's reasoned playground reads,
    // through the single production serializer authority
    // (`gmeow_pipeline::docs_distribution`) — never re-implemented here.
    let gts_path = root.join(crate::dev_common::GTS_SNAPSHOT_REL);
    let gts_bytes = std::fs::read(&gts_path).map_err(|e| {
        fail(format!(
            "cannot read committed bundle {}: {e}",
            gts_path.display()
        ))
    })?;
    let carrier_graph = purrdf::gts::read_all_segments(&gts_bytes)
        .map_err(|e| fail(format!("cannot read GTS segments from bundle: {e}")))?;
    let carrier_dataset = purrdf::gts::dataset_from_gts_graph(&carrier_graph)
        .map_err(|e| fail(format!("cannot fold GTS dataset from bundle: {e}")))?;
    let okf =
        gmeow_pipeline::docs_distribution::render_serialization_distributions(&carrier_dataset)
            .map_err(|e| {
                fail(format!(
                    "cannot render the OKF serialization distribution: {e}"
                ))
            })?;

    // JSON-LD-star / YAML-LD-star are NOT re-rendered here — `make build` already wrote
    // `dist/gmeow.jsonld` / `dist/gmeow.yamlld` off the identical committed-bundle
    // authority (`gmeow_pipeline::stages::yaml_ld`). The docs distribution REFERENCES
    // that single build output rather than re-serializing it a second time.
    // No-optionality: an absent build output is a hard fail naming the missing file and
    // pointing at `make build`, never a silent skip or a fallback re-render.
    let jsonld = gmeow_pipeline::docs_distribution::read_build_serialization_tree(
        &root.join(gmeow_pipeline::stages::yaml_ld::JSON_LD_PATH),
        "gmeow.jsonld",
    )
    .map_err(|e| {
        fail(format!(
            "cannot reference the JSON-LD-star build output: {e}"
        ))
    })?;
    let yamlld = gmeow_pipeline::docs_distribution::read_build_serialization_tree(
        &root.join(gmeow_pipeline::stages::yaml_ld::YAML_LD_PATH),
        "gmeow.yamlld",
    )
    .map_err(|e| {
        fail(format!(
            "cannot reference the YAML-LD-star build output: {e}"
        ))
    })?;

    // The PRODUCER registry: slug → the bytes that realize it. This is the one binding
    // that genuinely belongs here and nowhere else — it restates no facet of the catalog
    // (no family, media type, path, or consumer), and `content_address_distributions`
    // iterates the CATALOG rather than this map, so a declared distribution missing a
    // producer is a hard fail instead of a silently absent release row.
    let rendered: RenderedTrees<'_> = BTreeMap::from([
        ("site", &site),
        ("mdbook", &mdbook),
        ("pdf", &pdf),
        ("snippets", &snippets),
        ("console", &console),
        ("pydantic", &pydantic),
        ("okf", &okf),
        ("jsonld", &jsonld),
        ("yamlld", &yamlld),
    ]);

    // Content-address every declared distribution and build the release-time DCAT
    // manifest linking each to its distribution-catalog subject. Rendered in memory
    // unconditionally (even in check mode) — no-optionality forbids a silent skip of the
    // manifest.
    let entries = content_address_distributions(&rendered).map_err(fail)?;
    let sub_asset_entries = price_sub_assets(&rendered).map_err(fail)?;
    let manifest_nt = gmeow_pipeline::docs_distribution::build_docs_distribution_manifest(
        &entries,
        &sub_asset_entries,
        &gts_bytes,
    )
    .map_err(|e| fail(format!("cannot build the docs distribution manifest: {e}")))?;
    let manifest = BTreeMap::from([("docs-manifest.ttl".to_string(), manifest_nt.into_bytes())]);

    // The reconciliation destinations, DERIVED from the same catalog table: `ontology-docs`
    // (the Pages upload root, which is the site tree under a second base), then one base
    // per declared distribution at its declared `rel_path`, then the manifest.
    let mut destinations: Vec<(&str, &BTreeMap<String, Vec<u8>>)> = vec![("ontology-docs", &site)];
    for row in gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS {
        let tree = rendered.get(row.slug).copied().ok_or_else(|| {
            fail(format!(
                "distribution {:?} is declared in the catalog but sync_docs renders no tree \
                 for it",
                row.slug
            ))
        })?;
        destinations.push((row.rel_path, tree));
    }
    // The manifest gets its OWN subdir base — never the shared `dist/gmeow-docs`
    // parent, which `reconcile_docs_projection_tree` would otherwise prune of
    // every sibling format's files not present in THIS tree.
    destinations.push(("dist/gmeow-docs/manifest", &manifest));

    let mut outputs = Vec::new();
    let mut reconciliation = DocsProjectionReport::default();
    for (base, tree) in destinations {
        outputs.extend(tree.keys().map(|rel| format!("{base}/{rel}")));
        if update {
            let report = reconcile_docs_projection_tree(&root.join(base), tree).map_err(fail)?;
            println!("docs -> {}", root.join(base).display());
            reconciliation.produced += report.produced;
            reconciliation.written += report.written;
            reconciliation.unchanged += report.unchanged;
            reconciliation.removed += report.removed;
        } else {
            reconciliation.produced += tree.len();
        }
    }
    outputs.sort();
    outputs.dedup();
    Ok(DocsSyncReport {
        output_paths: outputs,
        reconciliation,
    })
}

/// The rendered documentation trees `sync_docs` produced, keyed by distribution slug.
type RenderedTrees<'a> = BTreeMap<&'a str, &'a BTreeMap<String, Vec<u8>>>;

/// Content-address every distribution the catalog DECLARES against the trees `sync_docs`
/// rendered, producing the release-manifest rows.
///
/// The loop runs over
/// [`DISTRIBUTIONS`](gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS) — the one
/// table — and never over the producer registry, so the bijection is enforced in BOTH
/// directions at run time: a declared distribution with no rendered tree is a hard fail,
/// and a rendered tree no row declares is a hard fail too.
///
/// **An empty tree is refused BEFORE it is content-addressed.** An empty `BTreeMap` tars
/// and hashes perfectly happily, so the digest is exactly the wrong thing to notice with:
/// a distribution that rendered nothing would sail into the release manifest carrying a
/// well-formed `blake3:` of an empty archive, and `gmeow docs verify` would then confirm
/// the emptiness as correct. This mirrors the sub-asset guard in [`price_sub_assets`].
pub(crate) fn content_address_distributions(
    rendered: &RenderedTrees<'_>,
) -> Result<Vec<gmeow_pipeline::docs_distribution::DistributionEntry>, gmeow_errors::Diag> {
    use gmeow_pipeline::stages::distribution_catalog::{
        DISTRIBUTIONS, declared_distribution_slugs,
    };

    let declared = declared_distribution_slugs();
    for slug in rendered.keys() {
        if !declared.contains(slug) {
            return Err(error::sync(format!(
                "sync_docs rendered a tree for {slug:?}, which the distribution catalog does \
                 not declare — a shipped surface outside the catalog has no media type, no \
                 audience, and no release row. Add it to \
                 `gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS`."
            )));
        }
    }

    let mut entries = Vec::with_capacity(DISTRIBUTIONS.len());
    for row in DISTRIBUTIONS {
        let tree = rendered.get(row.slug).copied().ok_or_else(|| {
            error::sync(format!(
                "distribution {:?} is declared in the distribution catalog but sync_docs \
                 renders no tree for it — a declared distribution with no producer would ship \
                 as a missing release row, not as a smaller release",
                row.slug
            ))
        })?;
        if tree.is_empty() {
            return Err(error::sync(format!(
                "declared distribution {:?} rendered an EMPTY tree: refusing to \
                 content-address it. Its render is a mandatory output of this selected \
                 profile, and an empty tree still hashes to a perfectly well-formed digest — \
                 publishing it would make a dropped distribution and a shipped one \
                 indistinguishable in the release manifest",
                row.slug
            )));
        }
        let blake3 = gmeow_pipeline::docs_distribution::distribution_blake3(tree).map_err(|e| {
            error::sync(format!(
                "cannot content-address the {} distribution: {e}",
                row.slug
            ))
        })?;
        entries.push(gmeow_pipeline::docs_distribution::DistributionEntry {
            slug: row.slug.to_string(),
            rel_path: row.rel_path.to_string(),
            blake3,
            media_type: row.media_type.to_string(),
        });
    }
    Ok(entries)
}

/// Price the shared sub-assets (the vendored interactive engines, the object-level browser
/// bundle, the conjecture demo library) into the release-instance manifest.
///
/// Pricing is DISTRIBUTION-PARAMETERIZED: `sub_asset_pricing()` yields one
/// `(owner, sub-asset, prefix, media type)` row per owning distribution, and each is
/// content-addressed out of THAT OWNER'S rendered tree. A site-only pricing would have
/// left the console's copy of the same 7 MB wasm image with no release digest at all.
///
/// The digest hangs off the SHARED `sub_asset_iri` subject the carrier catalog prices
/// digest-free. Because that subject is shared, two owners pricing one sub-asset to two
/// different digests is a contradiction — the site and the console assemble the identical
/// engine set from the identical `interactive_asset_files` producer — so a disagreement is
/// refused here rather than published as an ambiguous release row.
///
/// This release render is unconditionally interactive (`exec` is hard-required by
/// `sync_docs`), so every declared sub-asset is a mandatory output: one that produced zero
/// files is a HARD FAIL, never a silent skip, because a silent skip makes a shipped engine
/// and a dropped engine indistinguishable on the release path.
pub(crate) fn price_sub_assets(
    rendered: &RenderedTrees<'_>,
) -> Result<Vec<gmeow_pipeline::docs_distribution::DistributionEntry>, gmeow_errors::Diag> {
    use gmeow_pipeline::stages::distribution_catalog::{distribution_row, sub_asset_pricing};

    let mut entries = Vec::new();
    let mut digest_by_slug: BTreeMap<&str, String> = BTreeMap::new();
    for (owner, slug, prefix, media_type) in sub_asset_pricing() {
        let owner_row = distribution_row(owner).ok_or_else(|| {
            error::sync(format!(
                "sub-asset owner {owner:?} is not a declared distribution"
            ))
        })?;
        let tree = rendered.get(owner).copied().ok_or_else(|| {
            error::sync(format!(
                "sub-asset owner {owner:?} has no rendered tree — its sub-assets cannot be \
                 content-addressed"
            ))
        })?;
        let subtree: BTreeMap<String, Vec<u8>> = tree
            .iter()
            .filter(|(p, _)| p.as_str() == prefix || p.starts_with(prefix))
            .map(|(p, b)| (p.clone(), b.clone()))
            .collect();
        if subtree.is_empty() {
            return Err(error::sync(format!(
                "declared sub-asset {slug:?} produced no files under {prefix:?} in the \
                 {owner:?} distribution: this render is interactive, so the engine/bundle is a \
                 mandatory output — its absence is a degraded surface with a missing release \
                 digest, not a silent skip"
            )));
        }
        let blake3 =
            gmeow_pipeline::docs_distribution::distribution_blake3(&subtree).map_err(|e| {
                error::sync(format!(
                    "cannot content-address the {slug} sub-asset of {owner}: {e}"
                ))
            })?;
        if let Some(previous) = digest_by_slug.get(slug)
            && previous != &blake3
        {
            return Err(error::sync(format!(
                "sub-asset {slug:?} prices to {blake3} in the {owner:?} distribution but to \
                 {previous} in an earlier owner: the owners share ONE catalog subject, so two \
                 digests for it is a contradiction. Every owner assembles this asset from the \
                 same `interactive_asset_files` producer — a disagreement means one of them \
                 re-cut or post-processed the bytes."
            )));
        }
        digest_by_slug.insert(slug, blake3.clone());
        entries.push(gmeow_pipeline::docs_distribution::DistributionEntry {
            slug: slug.to_string(),
            rel_path: format!("{}/{prefix}", owner_row.rel_path),
            blake3,
            media_type: media_type.to_string(),
        });
    }
    Ok(entries)
}

fn pick_source_lang(
    lang: Option<&str>,
    translations: &gmeow_docs::Translations,
) -> Result<String, i32> {
    let available = gmeow_docs::available_languages(translations);
    let requested = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    if let Some(requested) = requested {
        for tag in requested.split(',').map(str::trim) {
            if let Some(found) = available.iter().find(|candidate| {
                candidate.as_str() == tag || translations.internal_tag(candidate) == tag
            }) {
                return Ok(found.clone());
            }
        }
        return Err(fail(format!(
            "requested documentation language profile {requested:?} is unavailable; available: {}",
            available.join(", ")
        )));
    }
    Ok(gmeow_docs::i18n::ENGLISH.to_string())
}

fn render_source_book(model: &gmeow_docs::DocsModel) -> BTreeMap<String, Vec<u8>> {
    gmeow_docs::mdbook::render_book(model, &gmeow_docs::ExecutableDocsData::default()).files
}

fn render_source_print(
    root: &Path,
    model: &gmeow_docs::DocsModel,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    const AXIOMS: [&str; 4] = [
        "generated/owl/gmeow-dl.ttl",
        "generated/owl/gmeow-el.ttl",
        "generated/logic/gmeow.logic.rdf12.ttl",
        "generated/datalog/gmeow.dl",
    ];
    let mut axioms = BTreeMap::new();
    for rel in AXIOMS {
        axioms.insert(rel.to_string(), std::fs::read(root.join(rel))?);
    }
    let bib = std::fs::read(root.join("generated/references/references.bib"))?;
    // `DocFormat::ALL`, never a re-typed variant list: the PDF's loss appendix must cover
    // EVERY rendered format, and a hand-written array silently omits any format added
    // later — the appendix would then claim a complete cross-format loss table while
    // missing a row, with nothing to catch it.
    let losses = gmeow_docs::formats::DocFormat::ALL
        .into_iter()
        .map(gmeow_docs::formats::format_capabilities)
        .collect::<Vec<_>>();
    let typ = docs_print::render_typ(model, &axioms, &bib, &losses);
    let pdf = docs_print::compile_pdf(&typ, &bib)?;
    Ok(BTreeMap::from([
        ("gmeow.pdf".to_string(), pdf),
        ("gmeow.typ".to_string(), typ.into_bytes()),
    ]))
}

fn source_snippets(
    site: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let mut snippets = site
        .iter()
        .filter_map(|(path, bytes)| {
            let rest = path.strip_prefix("terms/")?;
            let slug = rest.strip_suffix("/card.md")?;
            Some((format!("terms/{slug}.md"), bytes.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    if snippets.is_empty() {
        return Err(error::source(
            "source documentation render produced no term-card snippets",
        ));
    }
    // Emit the corpus README describing what an agent/tool is looking at and how
    // to consume it. One well-known root file, always present alongside the cards.
    snippets.insert("README.md".to_string(), SNIPPETS_README.as_bytes().to_vec());
    Ok(snippets)
}

/// The README written at the root of the `--format snippets` export tree. It
/// describes the corpus — one prompt-ready Markdown card per term — as the
/// offline, agent-ingestible projection of the bundle documentation, and how a
/// tool consumes it. Fixed text, deterministic from source.
const SNIPPETS_README: &str = "\
# GMEOW documentation snippets

This directory is the **offline, agent-ingestible projection** of the GMEOW bundle
documentation. It contains one prompt-ready Markdown card per vocabulary term at
`terms/<slug>.md`. Each card is self-contained plain Markdown (metadata,
definition, and usage advice) with no site chrome and no cross-page links, so it
can be dropped straight into a prompt or a retrieval index without further
rendering.

## How to consume it

- Read a single term card directly: `terms/<slug>.md`, where `<slug>` is the
  lower-cased local name of the term.
- Ingest the whole corpus: concatenate or index every `terms/*.md` file; the set
  is complete (one card per documented term) and deterministically named.
- Regenerate the corpus from canonical sources with
  `gmeow-dev sync --mode update --outputs docs`.

The cards here are the same per-term surface the published documentation renders;
this projection simply flattens them for offline agent use.
";

/// `gmeow-dev temporal QUERY [--data --focus --window-* --valid-at --as-of]`.
#[allow(clippy::too_many_arguments)]
pub fn temporal(
    query: &str,
    data: Option<&Path>,
    focus: Option<&str>,
    window_start: Option<&str>,
    window_end: Option<&str>,
    valid_at: Option<&str>,
    as_of: Option<&str>,
) -> i32 {
    let root = project_root();
    let query_dir = root.join("slices/core/temporal/queries/tql");
    let queries = gmeow_pipeline::cli_ops::temporal::temporal_queries();
    if !queries.contains_key(query) {
        note(
            "gmeow-dev.temporal.available",
            format!("unknown TQL query {query:?}. Available:"),
        );
        for (name, q) in &queries {
            note(
                "gmeow-dev.temporal.available",
                format!("  {name:<20} {}", q.summary),
            );
        }
        return fail(format!("unknown TQL query {query:?}"));
    }

    // The events graph = the authored temporal sources merged with any --data file.
    let mut source_ttl = String::new();
    if let Some(path) = data {
        match std::fs::read_to_string(path) {
            Ok(s) => source_ttl.push_str(&s),
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        }
    }
    // Merge the committed temporal module so the query has an events model even
    // without a --data file.
    let module = root.join("slices/core/temporal/module.ttl");
    if let Ok(s) = std::fs::read_to_string(&module) {
        source_ttl.push('\n');
        source_ttl.push_str(&s);
    }

    const XSD_DT: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
    let mut bindings: Vec<(String, purrdf::TermValue)> = Vec::new();
    if let Some(f) = focus {
        bindings.push(("focus".to_owned(), purrdf::TermValue::iri(f)));
    }
    if let Some(v) = window_start {
        bindings.push((
            "windowStart".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }
    if let Some(v) = window_end {
        bindings.push((
            "windowEnd".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }
    if let Some(v) = valid_at {
        bindings.push((
            "validAt".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }
    if let Some(v) = as_of {
        bindings.push((
            "asOf".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }

    match gmeow_pipeline::cli_ops::temporal::run_temporal_query(
        &query_dir,
        query,
        &source_ttl,
        &bindings,
    ) {
        Ok(solutions) => {
            for row in &solutions.rows {
                let rendered: Vec<String> = row
                    .iter()
                    .map(|v| v.as_ref().map(|t| format!("{t:?}")).unwrap_or_default())
                    .collect();
                println!("{}", rendered.join(" "));
            }
            println!("{query}: {} row(s)", solutions.rows.len());
            0
        }
        Err(e) => fail(format!("temporal query failed: {e}")),
    }
}

/// `gmeow-dev import-foundation JSONL --out --nq`.
pub fn import_foundation(jsonl: &Path, out_dir: &Path, nq: Option<&Path>) -> i32 {
    match gmeow_foundation_corpus::run_import(jsonl, out_dir, nq) {
        Ok((_dataset, budget)) => {
            println!("{}", budget.as_text());
            println!("artifacts -> {}", out_dir.display());
            0
        }
        Err(e) => fail(format!("import-foundation failed: {e}")),
    }
}

/// `gmeow-dev crossref` — generate CrossRef DOI deposit XML from self-description.
pub fn crossref() -> i32 {
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot fold snapshot: {e}")),
    };
    let meta = match gmeow_validate::self_desc::load_self_description_from_dataset(&dataset) {
        Ok(m) => m,
        Err(e) => return fail(format!("self-description unavailable: {e}")),
    };
    let lint_json = match gmeow_validate::self_desc::lint_input_json(&meta, None, None) {
        Ok(j) => j,
        Err(e) => return fail(format!("cannot assemble lint input: {e}")),
    };
    match gmeow_validate::crossref::lint_deposit(&lint_json) {
        Ok(problems) if problems.is_empty() => {}
        Ok(problems) => {
            for p in &problems {
                note("gmeow-dev.crossref.doi-lint", format!("doi-lint {p}"));
            }
            return fail(format!("{} doi-lint problem(s)", problems.len()));
        }
        Err(e) => return fail(format!("doi-lint failed: {e}")),
    }
    let (ts, batch) = gmeow_validate::self_desc::live_stamp(&meta);
    let deposit_json = match gmeow_validate::self_desc::deposit_input_json(&meta) {
        Ok(j) => j,
        Err(e) => return fail(format!("cannot assemble deposit input: {e}")),
    };
    let xml = match gmeow_validate::crossref::build_deposit_xml(&deposit_json, &ts, &batch) {
        Ok(x) => x,
        Err(e) => return fail(format!("cannot build deposit XML: {e}")),
    };
    let out = root.join("dist").join("crossref-deposit.xml");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, format!("{xml}\n")) {
        return fail(format!("cannot write {}: {e}", out.display()));
    }
    println!("wrote {}", out.display());
    0
}

/// `gmeow-dev compliance-report [--from-passing-check]`.
pub fn compliance_report(from_passing_check: bool) -> i32 {
    let root = project_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");
    let gate_runs: BTreeMap<String, gmeow_validate::compliance::GateRun> = BTreeMap::new();
    let evidence_mode = if from_passing_check {
        "from-passing-check"
    } else {
        "in-process"
    };
    let report = match gmeow_validate::compliance::compliance_report(
        &manifest,
        &constitution,
        &root,
        &gate_runs,
        env!("CARGO_PKG_VERSION"),
        evidence_mode,
    ) {
        Ok(r) => r,
        Err(e) => return fail(format!("compliance-report failed: {e}")),
    };
    let out = root.join("dist").join("compliance-report.ttl");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, report) {
        return fail(format!("cannot write {}: {e}", out.display()));
    }
    println!("compliance report written to {}", out.display());
    0
}

// ── up-projection-audit ─────────────────────────────────────────────────────

/// `gmeow-dev up-projection-audit [--report --gaps]` — the correspondence-gate
/// verdict ledger over the vendored real-world corpus.
pub fn up_projection_audit(report_path: Option<&Path>, show_gaps: bool) -> i32 {
    let root = project_root();
    let snapshot = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // The SSSOM lift maps + projection/EDOAL TTLs folded into the bundle.
    let sssom_texts: Vec<String> = match gmeow_pipeline::bundle_blobs::bundled_sssom(&snapshot) {
        Ok(m) => m
            .into_values()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .collect(),
        Err(e) => return fail(format!("cannot read bundled SSSOM: {e}")),
    };
    let projection_ttls: Vec<String> =
        match gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&snapshot) {
            Ok(b) => match b.archive(gmeow_pipeline::bundle_blobs::REP_MAPPINGS) {
                Ok(a) => a
                    .into_iter()
                    .filter(|(k, _)| k.ends_with(".ttl"))
                    .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
                    .collect(),
                Err(e) => return fail(format!("cannot read bundled mappings: {e}")),
            },
            Err(e) => return fail(format!("cannot fold bundle: {e}")),
        };
    // The fixed real-world corpus snapshots (never authored fixtures).
    let mut corpus: Vec<(String, String)> = Vec::new();
    for name in ["bii", "paudley"] {
        let path = root
            .join("tests/fixtures/coverage/external")
            .join(format!("{name}.ttl"));
        match std::fs::read_to_string(&path) {
            Ok(text) => corpus.push((name.to_owned(), text)),
            Err(e) => return fail(format!("cannot read corpus {}: {e}", path.display())),
        }
    }

    let (ledger, markdown) = match gmeow_pipeline::cli_ops::confirmations::up_projection_gate_audit(
        &sssom_texts,
        &projection_ttls,
        &corpus,
    ) {
        Ok(pair) => pair,
        Err(e) => return fail(format!("up-projection-audit failed: {e}")),
    };
    if let Some(path) = report_path {
        if let Err(e) = std::fs::write(path, &markdown) {
            return fail(format!("cannot write {}: {e}", path.display()));
        }
        println!("wrote {}", path.display());
    }
    let liftable = ledger.totals.liftable();
    let total = ledger.totals.total();
    let pct = liftable
        .checked_mul(100)
        .and_then(|n| n.checked_div(total))
        .unwrap_or(0);
    println!(
        "liftable {liftable}/{total} ({pct}%) · proved {} · claimed {} · excluded {} · unsupported {}",
        ledger.totals.proved,
        ledger.totals.claimed,
        ledger.totals.red_excluded,
        ledger.totals.unsupported
    );
    println!("gaps {} distinct terms", ledger.gaps.len());
    if show_gaps {
        for term in &ledger.gaps {
            note("gmeow-dev.up-projection-audit.gap", format!("gap {term}"));
        }
    }
    0
}

// ── refresh-target-axioms ────────────────────────────────────────────────────

/// One IMPORT_OK target's canonical source document (mirrors the pipeline's
/// `TARGET_SOURCES` — reference-only targets are fetched live at lint time, never
/// vendored, so they are absent here).
struct TargetSource {
    prefix: &'static str,
    url: &'static str,
    media_type: &'static str,
}

/// The vendorable target-axiom sources (IMPORT_OK license family only).
const TARGET_SOURCES: &[TargetSource] = &[
    TargetSource {
        prefix: "org",
        url: "https://www.w3.org/ns/org.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "foaf",
        url: "http://xmlns.com/foaf/spec/index.rdf",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "vcard",
        url: "https://www.w3.org/2006/vcard/ns.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "prov",
        url: "https://www.w3.org/ns/prov-o.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "time",
        url: "https://www.w3.org/2006/time.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "geo",
        url: "https://opengeospatial.github.io/ogc-geosparql/geosparql11/geo.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "bfo",
        url: "http://purl.obolibrary.org/obo/bfo.owl",
        media_type: "application/rdf+xml",
    },
];

/// The structural predicates a vendored target snapshot keeps: domain/range,
/// inverse, and the property-type declarations.
const STRUCTURAL_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#domain",
    "http://www.w3.org/2000/01/rdf-schema#range",
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
    "http://www.w3.org/2002/07/owl#inverseOf",
];

/// `gmeow-dev refresh-target-axioms [--target]` — re-vendor minimal target-axiom
/// snapshots into `imports/targets/`. Network; IMPORT_OK targets only.
pub fn refresh_target_axioms(target: &str) -> i32 {
    let root = project_root();
    let out_dir = root.join("imports").join("targets");
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return fail(format!("cannot create {}: {e}", out_dir.display()));
    }
    let selected: Vec<&TargetSource> = if target == "all" {
        TARGET_SOURCES.iter().collect()
    } else {
        TARGET_SOURCES
            .iter()
            .filter(|s| s.prefix == target)
            .collect()
    };
    if selected.is_empty() {
        // A named target that is not IMPORT_OK is skipped with a clear note, never
        // vendored (reference-only targets are fetched live at lint time).
        note(
            "gmeow-dev.refresh-target-axioms.skip",
            format!(
                "skip {target}: not an IMPORT_OK vendorable target (reference-only or unknown)"
            ),
        );
        return 0;
    }
    let mut written = 0usize;
    for source in selected {
        match refresh_one(source, &out_dir) {
            Ok(path) => {
                println!("{}", path.display());
                written += 1;
            }
            Err(e) => return fail_code(format!("fetch failed for {}: {e}", source.prefix), 2),
        }
    }
    println!("refreshed {written} target snapshot(s)");
    0
}

/// Fetch, structurally filter, and write one target's axiom snapshot.
fn refresh_one(source: &TargetSource, out_dir: &Path) -> gmeow_errors::Result<std::path::PathBuf> {
    // A network vendor step must fail fast rather than hang: cap the whole
    // request/response with a global timeout so an unreachable or stalled remote
    // surfaces as an error instead of blocking the CLI indefinitely.
    let body = ureq::get(source.url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .map_err(|e| error::refresh(format!("HTTP {e}")))?
        .into_body()
        .read_to_string()
        .map_err(|e| error::refresh(format!("read body: {e}")))?;
    let media = if source.media_type.contains("rdf+xml") {
        "application/rdf+xml"
    } else {
        "text/turtle"
    };
    let dataset = purrdf::parse_dataset(body.as_bytes(), media, None)
        .map_err(|e| error::refresh(format!("parse: {e}")))?;

    // Keep only the structural-axiom quads (domain / range / subPropertyOf /
    // inverseOf, plus property-type declarations) — a minimal, deterministic
    // vendored snapshot. Filtering the parsed quads in memory (rather than a
    // serialize → line-match → re-parse round trip) is exact: it matches on the
    // predicate term itself, so a literal that happens to embed a predicate URI
    // can never masquerade as a structural axiom.
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let mut filtered_quads = Vec::new();
    for quad in dataset.owned_quads() {
        let pred = quad.predicate.as_str();
        let keep = STRUCTURAL_PREDICATES.contains(&pred)
            || (pred == rdf_type
                && matches!(&quad.object, purrdf::RdfTerm::Iri(iri) if iri.ends_with("Property")));
        if keep {
            filtered_quads.push(quad);
        }
    }
    let filtered = purrdf::flat_dataset_from_quads(&filtered_quads)
        .map_err(|e| error::refresh(format!("flatten filtered: {e}")))?;
    let prefixes = vec![(source.prefix.to_owned(), namespace_for(source.prefix))];
    let ttl = purrdf::turtle_normalize::render(&filtered, &prefixes);
    let path = out_dir.join(format!("{}.ttl", source.prefix));
    std::fs::write(&path, ttl)
        .map_err(|e| error::refresh(format!("write {}: {e}", path.display())))?;
    Ok(path)
}

/// A best-effort namespace binding for a target prefix (cosmetic in the snapshot).
fn namespace_for(prefix: &str) -> String {
    match prefix {
        "org" => "http://www.w3.org/ns/org#",
        "foaf" => "http://xmlns.com/foaf/0.1/",
        "vcard" => "http://www.w3.org/2006/vcard/ns#",
        "prov" => "http://www.w3.org/ns/prov#",
        "time" => "http://www.w3.org/2006/time#",
        "geo" => "http://www.opengis.net/ont/geosparql#",
        "bfo" => "http://purl.obolibrary.org/obo/",
        _ => "http://example.org/",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_readme_describes_the_offline_corpus() {
        // The README must name the corpus, its per-term card layout, its offline
        // agent-ingestible role, and how to (re)generate it.
        assert!(SNIPPETS_README.starts_with("# GMEOW documentation snippets"));
        assert!(SNIPPETS_README.contains("offline, agent-ingestible projection"));
        assert!(SNIPPETS_README.contains("one prompt-ready Markdown card per vocabulary term"));
        assert!(SNIPPETS_README.contains("terms/<slug>.md"));
        assert!(SNIPPETS_README.contains("gmeow-dev sync --mode update --outputs docs"));
    }

    #[test]
    fn source_snippets_flattens_cards_and_emits_the_readme() {
        // A minimal site tree: two term cards plus unrelated files that must be
        // dropped by the snippets projection.
        let mut site: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        site.insert("terms/foo/card.md".to_string(), b"# gmeow:Foo".to_vec());
        site.insert("terms/bar/card.md".to_string(), b"# gmeow:Bar".to_vec());
        site.insert("terms/foo/index.html".to_string(), b"<html/>".to_vec());
        site.insert("index.html".to_string(), b"<html/>".to_vec());

        let out = source_snippets(&site).expect("cards present → snippets projection succeeds");

        // The cards are flattened to `terms/<slug>.md`; nothing else leaks through.
        assert_eq!(
            out.get("terms/foo.md").map(Vec::as_slice),
            Some(&b"# gmeow:Foo"[..])
        );
        assert_eq!(
            out.get("terms/bar.md").map(Vec::as_slice),
            Some(&b"# gmeow:Bar"[..])
        );
        assert!(!out.contains_key("terms/foo/index.html"));
        assert!(!out.contains_key("index.html"));

        // The corpus README is emitted at the tree root with the corpus paragraph.
        let readme = out
            .get("README.md")
            .expect("snippets export writes a README");
        let readme = std::str::from_utf8(readme).expect("README is UTF-8");
        assert!(readme.contains("offline, agent-ingestible projection"));
        assert!(readme.contains("terms/<slug>.md"));
    }

    #[test]
    fn source_snippets_hard_fails_without_cards() {
        // No `terms/*/card.md` in the tree → a hard error, never a silent empty tree.
        let mut site: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        site.insert("index.html".to_string(), b"<html/>".to_vec());
        assert!(source_snippets(&site).is_err());
    }

    #[test]
    fn explicit_unknown_docs_language_hard_fails() {
        assert!(pick_source_lang(Some("not-a-language"), &Default::default()).is_err());
    }

    // ── the release-row producers, over the REAL catalog table ─────────────────────

    /// A non-empty tree carrying every declared sub-asset prefix, so a fixture built from
    /// it exercises the real pricing loop rather than a stub.
    fn interactive_tree() -> BTreeMap<String, Vec<u8>> {
        let mut tree = BTreeMap::from([("index.html".to_string(), b"<html/>".to_vec())]);
        for (_, slug, prefix, _) in
            gmeow_pipeline::stages::distribution_catalog::sub_asset_pricing()
        {
            // A directory prefix ends in `/`; a file prefix IS the key.
            let key = if prefix.ends_with('/') {
                format!("{prefix}engine.wasm")
            } else {
                prefix.to_string()
            };
            tree.insert(key, format!("bytes-for-{slug}").into_bytes());
        }
        tree
    }

    /// Every declared distribution gets a tree, so the happy path is genuinely complete.
    fn full_rendered<'a>(
        trees: &'a BTreeMap<&'static str, BTreeMap<String, Vec<u8>>>,
    ) -> RenderedTrees<'a> {
        trees.iter().map(|(slug, tree)| (*slug, tree)).collect()
    }

    fn every_slug_rendered() -> BTreeMap<&'static str, BTreeMap<String, Vec<u8>>> {
        gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS
            .iter()
            .map(|row| (row.slug, interactive_tree()))
            .collect()
    }

    /// The happy path: one release row per DECLARED distribution, each carrying the
    /// catalog's own `rel_path` and media type — no restated table on this side of the
    /// crate seam.
    #[test]
    fn content_addressing_emits_one_row_per_declared_distribution() {
        let trees = every_slug_rendered();
        let entries =
            content_address_distributions(&full_rendered(&trees)).expect("full render prices");
        let declared = gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS;
        assert_eq!(entries.len(), declared.len());
        for (entry, row) in entries.iter().zip(declared.iter()) {
            assert_eq!(entry.slug, row.slug);
            assert_eq!(entry.rel_path, row.rel_path);
            assert_eq!(entry.media_type, row.media_type);
            assert!(entry.blake3.starts_with("blake3:"));
        }
        assert!(
            entries.iter().any(|e| e.slug == "console"),
            "the console must get a release row: {entries:?}"
        );
    }

    /// An EMPTY console tree is refused, by slug, BEFORE it is content-addressed — an
    /// empty tree hashes fine, so the digest can never be what notices.
    #[test]
    fn an_empty_console_tree_hard_fails_naming_the_slug() {
        let mut trees = every_slug_rendered();
        trees.insert("console", BTreeMap::new());
        let err = content_address_distributions(&full_rendered(&trees))
            .expect_err("an empty declared distribution must hard-fail")
            .to_string();
        assert!(
            err.contains("console"),
            "the refusal must name the empty distribution: {err}"
        );
        assert!(
            err.contains("EMPTY"),
            "the refusal must say what went wrong: {err}"
        );
    }

    /// The same guard is not console-specific: it holds for every declared slug.
    #[test]
    fn an_empty_tree_hard_fails_for_every_declared_distribution() {
        for row in gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS {
            let mut trees = every_slug_rendered();
            trees.insert(row.slug, BTreeMap::new());
            let err = content_address_distributions(&full_rendered(&trees))
                .unwrap_err()
                .to_string();
            assert!(
                err.contains(row.slug),
                "an empty {} tree must hard-fail naming the slug: {err}",
                row.slug
            );
        }
    }

    /// A declared distribution with no producer, and a producer with no declared row, are
    /// both hard fails — the bijection is enforced in both directions at run time.
    #[test]
    fn the_producer_registry_and_the_catalog_must_be_a_bijection() {
        let mut trees = every_slug_rendered();
        trees.remove("console");
        let err = content_address_distributions(&full_rendered(&trees))
            .expect_err("a declared distribution with no producer must hard-fail")
            .to_string();
        assert!(err.contains("console") && err.contains("no tree"), "{err}");

        let mut trees = every_slug_rendered();
        trees.insert("not-a-distribution", interactive_tree());
        let err = content_address_distributions(&full_rendered(&trees))
            .expect_err("a producer with no catalog row must hard-fail")
            .to_string();
        assert!(err.contains("not-a-distribution"), "{err}");
    }

    /// Sub-assets are priced from EVERY owner's own tree, onto the shared subject, with
    /// the cross-owner byte-identity invariant enforced.
    #[test]
    fn sub_assets_are_priced_from_every_owners_tree() {
        use gmeow_pipeline::stages::distribution_catalog as catalog;
        let trees = every_slug_rendered();
        let entries = price_sub_assets(&full_rendered(&trees)).expect("full render prices");
        let owners = catalog::sub_asset_owner_slugs();
        let subs = catalog::declared_sub_asset_slugs();
        assert_eq!(
            entries.len(),
            owners.len() * subs.len(),
            "one row per (owner, sub-asset) pair: {entries:?}"
        );
        for owner in &owners {
            let row = catalog::distribution_row(owner).expect("owner is declared");
            for sub in &subs {
                assert!(
                    entries
                        .iter()
                        .any(|e| e.slug == *sub && e.rel_path.starts_with(row.rel_path)),
                    "sub-asset {sub:?} is unpriced in the {owner:?} tree: {entries:?}"
                );
            }
        }
        assert!(
            entries
                .iter()
                .any(|e| e.rel_path.starts_with("dist/gmeow-docs/console/")),
            "the console's copy of the shared engines must be priced: {entries:?}"
        );
    }

    /// An owner whose tree is missing a declared sub-asset hard-fails naming both.
    #[test]
    fn a_missing_sub_asset_hard_fails_naming_the_owner_and_the_asset() {
        let mut trees = every_slug_rendered();
        trees.insert(
            "console",
            BTreeMap::from([("index.html".to_string(), b"<html/>".to_vec())]),
        );
        let err = price_sub_assets(&full_rendered(&trees))
            .expect_err("a console tree with no engines must hard-fail")
            .to_string();
        assert!(err.contains("console"), "{err}");
        assert!(err.contains("mandatory output"), "{err}");
    }

    /// Two owners whose copies of one shared sub-asset DIFFER is refused: the subject is
    /// shared, so two digests for it is a contradiction rather than two release rows.
    #[test]
    fn divergent_copies_of_a_shared_sub_asset_are_refused() {
        let mut trees = every_slug_rendered();
        let mut console = interactive_tree();
        for (_, _, prefix, _) in gmeow_pipeline::stages::distribution_catalog::sub_asset_pricing() {
            let key = if prefix.ends_with('/') {
                format!("{prefix}engine.wasm")
            } else {
                prefix.to_string()
            };
            console.insert(key, b"DIFFERENT BYTES".to_vec());
        }
        trees.insert("console", console);
        let err = price_sub_assets(&full_rendered(&trees))
            .expect_err("two digests for one shared subject must be refused")
            .to_string();
        assert!(err.contains("contradiction"), "{err}");
    }
}

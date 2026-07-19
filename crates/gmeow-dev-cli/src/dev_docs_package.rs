// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev docs-package` — package the materialized `dist/gmeow-docs/` external
//! documentation distribution (issue #1491 Task 3's `sync_docs` output) into one
//! deterministic content-addressed release asset, alongside a standalone digest of
//! its DCAT release manifest.
//!
//! This is the release-publication counterpart to `docs-measure`/`sync_docs`: it
//! NEVER renders or reconciles the docs tree itself (that authority stays with
//! `gmeow-dev sync --outputs docs`) — it only reads what is already on disk under
//! `dist/gmeow-docs/`, packages it via the single tar+blake3 authority
//! ([`gmeow_pipeline::docs_distribution::package_docs_dir`]), and writes the tar +
//! its `.blake3` sidecar plus a `.blake3` sidecar for the manifest file. `make
//! release-publish` runs `make sync SYNC_OUTPUTS=docs` immediately before this
//! command so the tree it packages is always freshly materialized.
//!
//! No-optionality: a missing `dist/gmeow-docs/` tree or a missing
//! `dist/gmeow-docs/manifest/docs-manifest.ttl` file is a HARD FAIL — never a
//! silently skipped asset.

use std::path::PathBuf;

use crate::dev_common::{fail, project_root};

/// The materialized external documentation distribution root, relative to the
/// project root (the base `sync_docs` reconciles every rendered tree + the
/// release-time manifest under — see `crates/gmeow-dev-cli/src/dev_project.rs`).
const DOCS_DIST_REL: &str = "dist/gmeow-docs";

/// The release-time DCAT manifest file, relative to the project root.
const DOCS_MANIFEST_REL: &str = "dist/gmeow-docs/manifest/docs-manifest.ttl";

/// `gmeow-dev docs-package [--out PATH]`.
pub fn docs_package(out: &std::path::Path) -> i32 {
    let root = project_root();
    let docs_dir = root.join(DOCS_DIST_REL);
    let manifest_path = root.join(DOCS_MANIFEST_REL);

    let (archive, archive_digest) =
        match gmeow_pipeline::docs_distribution::package_docs_dir(&docs_dir) {
            Ok(packaged) => packaged,
            Err(e) => return fail(format!("cannot package {}: {e}", docs_dir.display())),
        };

    let out_path = if out.is_absolute() {
        out.to_path_buf()
    } else {
        root.join(out)
    };
    if let Some(parent) = out_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(format!("cannot create {}: {e}", parent.display()));
    }
    if let Err(e) = std::fs::write(&out_path, &archive) {
        return fail(format!("cannot write {}: {e}", out_path.display()));
    }
    let archive_digest_path = sidecar_path(&out_path);
    if let Err(e) = std::fs::write(&archive_digest_path, format!("{archive_digest}\n")) {
        return fail(format!("cannot write {}: {e}", archive_digest_path.display()));
    }

    let manifest_bytes = match std::fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return fail(format!(
                "cannot read release manifest {} ({e}); materialize it first with \
                 `make sync SYNC_MODE=update SYNC_OUTPUTS=docs`",
                manifest_path.display()
            ));
        }
    };
    let manifest_digest = gmeow_pipeline::docs_distribution::blake3_of(&manifest_bytes);
    let manifest_digest_path = sidecar_path(&manifest_path);
    if let Err(e) = std::fs::write(&manifest_digest_path, format!("{manifest_digest}\n")) {
        return fail(format!("cannot write {}: {e}", manifest_digest_path.display()));
    }

    let mut produced: Vec<(PathBuf, String)> = vec![
        (out_path, archive_digest),
        (archive_digest_path, String::new()),
        (manifest_digest_path, manifest_digest),
    ];
    produced.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, digest) in &produced {
        if digest.is_empty() {
            println!("{}", path.display());
        } else {
            println!("{}  {digest}", path.display());
        }
    }
    0
}

/// The `.blake3` sidecar path for `path` (`<path>.blake3`, matching the
/// `release-publish` `.sha256` sidecar convention already used for the signed GTS).
fn sidecar_path(path: &std::path::Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".blake3");
    PathBuf::from(name)
}

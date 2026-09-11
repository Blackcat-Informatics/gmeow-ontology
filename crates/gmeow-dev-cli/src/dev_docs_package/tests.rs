// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic archive and sidecar behavior through the production packager.
//! No repository corpus is read, rendered, or materialized by these tests.

use std::collections::BTreeSet;
use std::path::Path;

use super::package_at_root;

/// The console RIDES `dist/gmeow-docs.tar`.
///
/// Exercise the admitted command's shared packager on a synthetic documentation
/// tree, then read the produced archive back: the tar's member
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

    assert_eq!(package_at_root(root, Path::new("dist/gmeow-docs.tar")), 0);
    let archive = std::fs::read(root.join("dist").join("gmeow-docs.tar")).expect("read the tar");
    let members: BTreeSet<String> = purrdf::ustar::read_archive(&archive)
        .expect("read the packaged tar")
        .into_iter()
        .map(|(name, _)| name)
        .collect();

    assert!(
        members.contains("console/index.html"),
        "Archive contract: `dist/gmeow-docs.tar` must carry the interactive console under \
         `console/` — the release asset ships every distribution or it ships a lie; \
         members were {members:?}"
    );
    // …and not only the console: every declared distribution's tail is in the archive, so
    // this cannot pass by the console being special-cased into a tar that lost the rest.
    for row in gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS {
        let slug = row.rel_path.trim_start_matches("dist/gmeow-docs/");
        assert!(
            members.contains(&format!("{slug}/index.html")),
            "Archive contract: distribution {slug:?} is missing from the packaged archive: \
             {members:?}"
        );
    }
}

/// Keep archive and digest bytes stable by writing sidecars outside the packaged tree.
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
    assert_eq!(package_at_root(root, Path::new("dist/gmeow-docs.tar")), 0);
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
    assert_eq!(package_at_root(root, Path::new("dist/gmeow-docs.tar")), 0);
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

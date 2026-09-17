// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::fs;

use tempfile::TempDir;

use super::*;

const RENDERED_BOOT_PATH: &str = "mdbook-boot-deadbeef.js";

struct Fixture {
    _temp: TempDir,
    source: PathBuf,
    rendered: PathBuf,
}

fn write(root: &Path, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture file has a parent")).unwrap();
    fs::write(path, bytes).unwrap();
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let rendered = temp.path().join("rendered");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&rendered).unwrap();

    let boot = b"import(new URL(\"assets/gmeow-docs.js\", document.currentScript.src));\n";
    write(&source, BOOT_PATH, boot);
    write(&rendered, RENDERED_BOOT_PATH, boot);

    let mut controller = String::new();
    for engine in required_mdbook_engines() {
        for (filename, _) in engine.emitted_files {
            let bytes: &[u8] = if filename.ends_with(".wasm") {
                b"\0asm\x01\0\0\0"
            } else {
                b"export default true;\n"
            };
            let relative = Path::new("assets").join(engine.name).join(filename);
            write(&source, Path::new("src").join(&relative), bytes);
            write(&rendered, &relative, bytes);
            controller.push_str(&format!(
                "const _{} = './{}/{}';\n",
                engine.name, engine.name, filename
            ));
        }
    }
    write(
        &source,
        Path::new("src").join(CONTROLLER_PATH),
        controller.as_bytes(),
    );
    write(&rendered, CONTROLLER_PATH, controller.as_bytes());

    write(&rendered, "book.css", b"body {}\n");
    write(&rendered, "image.svg", b"<svg id=\"image\"></svg>\n");
    write(
            &rendered,
            "index.html",
            br##"<!doctype html><html><head><link href="book.css"><script src="mdbook-boot-deadbeef.js"></script></head><body id="home section"><a href="guide/#details">Guide</a><a href="#home%20section">Home</a></body></html>"##,
        );
    write(
            &rendered,
            "guide/index.html",
            br##"<!doctype html><html><head><script src="../mdbook-boot-deadbeef.js"></script></head><body><h2 id="details">Details</h2><a href="../index.html#home%20section">Home</a><img src="../image.svg"></body></html>"##,
        );

    Fixture {
        _temp: temp,
        source,
        rendered,
    }
}

fn has_code(audit: &RenderedBookAudit, code: &str) -> bool {
    audit
        .report
        .findings
        .iter()
        .any(|finding| finding.code == code)
}

#[test]
fn complete_rendered_corpus_and_four_engine_chain_pass() {
    let fixture = fixture();
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(audit.report.ok(), "{:?}", audit.report.legacy_errors());
    assert_eq!(audit.html_pages, 2);
    assert!(audit.local_references >= 7);
    assert_eq!(audit.wasm_engines, 4);
    assert_eq!(
        required_mdbook_engines()
            .iter()
            .map(|asset| asset.name)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["gmn", "query", "reason", "validate"])
    );
}

#[test]
fn missing_target_and_cross_page_fragment_fail() {
    let fixture = fixture();
    write(
            &fixture.rendered,
            "guide/index.html",
            br#"<html><script src="../mdbook-boot-deadbeef.js"></script><a href="missing.html">Missing</a><a href="../index.html#absent">Bad anchor</a></html>"#,
        );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(has_code(&audit, "docs/mdbook-dangling-reference"));
    assert!(has_code(&audit, "docs/mdbook-broken-fragment"));
}

#[test]
fn root_escape_fails_even_when_the_outside_file_exists() {
    let fixture = fixture();
    fs::write(
        fixture.rendered.parent().unwrap().join("outside.html"),
        b"outside",
    )
    .unwrap();
    write(
            &fixture.rendered,
            "guide/index.html",
            br#"<html><script src="../mdbook-boot-deadbeef.js"></script><a href="../../outside.html">Escape</a></html>"#,
        );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(has_code(&audit, "docs/mdbook-root-escape"));
}

#[test]
fn missing_and_mutated_engine_payloads_fail() {
    let missing = fixture();
    let engine = required_mdbook_engines()[0];
    let wasm = engine
        .emitted_files
        .iter()
        .map(|(filename, _)| *filename)
        .find(|filename| filename.ends_with(".wasm"))
        .unwrap();
    fs::remove_file(missing.rendered.join("assets").join(engine.name).join(wasm)).unwrap();
    let audit = audit_rendered_book(&missing.source, &missing.rendered);
    assert!(has_code(&audit, "docs/mdbook-missing-asset"));

    let mutated = fixture();
    write(
        &mutated.rendered,
        Path::new("assets").join(engine.name).join(wasm),
        b"\0asmDIFFERENT",
    );
    let audit = audit_rendered_book(&mutated.source, &mutated.rendered);
    assert!(has_code(&audit, "docs/mdbook-asset-drift"));

    let empty = fixture();
    let js = engine
        .emitted_files
        .iter()
        .map(|(filename, _)| *filename)
        .find(|filename| filename.ends_with(".js"))
        .unwrap();
    write(
        &empty.rendered,
        Path::new("assets").join(engine.name).join(js),
        b"",
    );
    let audit = audit_rendered_book(&empty.source, &empty.rendered);
    assert!(has_code(&audit, "docs/mdbook-empty-engine-asset"));
}

#[test]
fn broken_boot_and_controller_loading_chain_fails() {
    let broken_boot = fixture();
    write(
        &broken_boot.rendered,
        RENDERED_BOOT_PATH,
        b"console.log('no controller');\n",
    );
    let audit = audit_rendered_book(&broken_boot.source, &broken_boot.rendered);
    assert!(has_code(&audit, "docs/mdbook-boot-controller"));

    let broken_controller = fixture();
    write(
        &broken_controller.rendered,
        CONTROLLER_PATH,
        b"export default true;\n",
    );
    let audit = audit_rendered_book(&broken_controller.source, &broken_controller.rendered);
    assert!(has_code(&audit, "docs/mdbook-controller-engine"));
}

#[test]
fn every_page_must_load_the_boot_shim() {
    let fixture = fixture();
    write(
        &fixture.rendered,
        "guide/index.html",
        br#"<html><a href="../index.html">Home</a></html>"#,
    );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(has_code(&audit, "docs/mdbook-missing-boot-reference"));
}

#[test]
fn mdbook_auxiliary_toc_is_link_checked_without_requiring_boot() {
    let fixture = fixture();
    write(
        &fixture.rendered,
        MDBOOK_AUXILIARY_TOC_PATH,
        br#"<html><body class="sidebar-iframe-inner"><a href="index.html">Home</a></body></html>"#,
    );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(audit.report.ok(), "{:?}", audit.report.legacy_errors());
    assert_eq!(audit.html_pages, 3);

    write(
            &fixture.rendered,
            MDBOOK_AUXILIARY_TOC_PATH,
            br#"<html><body class="sidebar-iframe-inner"><a href="missing.html">Missing</a></body></html>"#,
        );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(has_code(&audit, "docs/mdbook-dangling-reference"));
}

#[test]
fn ordinary_toc_path_does_not_bypass_the_boot_requirement() {
    let fixture = fixture();
    write(
        &fixture.rendered,
        MDBOOK_AUXILIARY_TOC_PATH,
        br#"<html><body><a href="index.html">Home</a></body></html>"#,
    );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(has_code(&audit, "docs/mdbook-missing-boot-reference"));
}

#[test]
fn raw_script_markup_is_ignored_and_unquoted_paths_are_checked() {
    let fixture = fixture();
    write(
            &fixture.rendered,
            "index.html",
            br#"<html><script src=mdbook-boot-deadbeef.js></script><script>const example = '<a href="missing.html">';</script><body id="home section"><a href=guide/index.html#details>Guide</a></body></html>"#,
        );
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(audit.report.ok(), "{:?}", audit.report.legacy_errors());
}

#[test]
fn an_empty_rendered_html_corpus_fails() {
    let fixture = fixture();
    fs::remove_file(fixture.rendered.join("index.html")).unwrap();
    fs::remove_file(fixture.rendered.join("guide/index.html")).unwrap();
    let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
    assert!(has_code(&audit, "docs/mdbook-empty-html"));
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The standalone-console producer's named acceptance assertions, plus the
//! two-shell agreement gate.
//!
//! Every test here is a gate blocker. They run over the REAL site and book renders of the
//! cached documentation fixture, not over a synthetic tree, so what is asserted is what
//! ships.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_docs::ExecutableDocsData;
use gmeow_docs::console::{CONSOLE_PREFIX, console_files, generated_shell};
use gmeow_docs::mdbook::render_book;
use gmeow_docs::render::{
    Page, body_carries_interactive_control, interactive_asset_files, render_site_lang_exec,
    term_slug,
};

mod common;

/// An `exec` that makes the render interactive. The renderer only checks non-emptiness of
/// each field and content-addresses the bytes, so fixed sentinels are sufficient and keep
/// the test independent of a built bundle.
fn interactive_exec() -> ExecutableDocsData {
    ExecutableDocsData {
        full_bundle_gts: b"gts-bundle-sentinel-bytes".to_vec(),
        conjectures_ttl: b"@prefix ex: <http://example/> . ex:c a ex:Conjecture .\n".to_vec(),
        ..Default::default()
    }
}

// ── Task 8: the producer ────────────────────────────────────────────────────

/// Given the same `exec`, `console_files` twice yields byte-identical maps.
#[test]
fn console_files_twice_is_byte_identical() {
    let exec = interactive_exec();
    assert_eq!(console_files(&exec), console_files(&exec));
}

/// Every key present in BOTH `interactive_asset_files` and `console_files` has identical
/// bytes — the console never re-cuts a shared engine asset.
#[test]
fn shared_keys_carry_identical_bytes() {
    let exec = interactive_exec();
    let console = console_files(&exec);
    let shared = interactive_asset_files(&exec);
    assert!(!shared.is_empty());
    let overlap: Vec<&String> = shared.keys().filter(|k| console.contains_key(*k)).collect();
    assert_eq!(
        overlap.len(),
        shared.len(),
        "console_files must carry EVERY interactive asset key"
    );
    for key in overlap {
        assert_eq!(shared[key], console[key], "shared asset {key} differs");
    }
}

/// The rendered site tree's `console/` + `assets/` subset is byte-identical to
/// `console_files(exec)`.
#[test]
fn the_site_subset_equals_the_console_tree() {
    let model = common::cached_model();
    let exec = interactive_exec();
    let site = render_site_lang_exec(&model, "english", &exec);
    let expected = console_files(&exec);
    let actual: BTreeMap<String, Vec<u8>> = site
        .files
        .iter()
        .filter(|(path, _)| path.starts_with(CONSOLE_PREFIX) || path.starts_with("assets/"))
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect();
    // `assets/gmeow.css` is a site-chrome asset, not part of the console tree; every other
    // `assets/` key is. Compare the exact expected set both ways.
    let extra: Vec<&String> = actual
        .keys()
        .filter(|k| !expected.contains_key(*k) && k.as_str() != "assets/gmeow.css")
        .collect();
    assert!(
        extra.is_empty(),
        "site emits console/assets keys the producer does not: {extra:?}"
    );
    for (key, bytes) in &expected {
        assert_eq!(
            actual.get(key),
            Some(bytes),
            "the site tree's {key} is not byte-identical to the console producer's"
        );
    }
}

/// The generated `SHELL` set EQUALS the assembled key set, in both directions.
#[test]
fn the_generated_shell_equals_the_assembled_key_set() {
    let files = console_files(&interactive_exec());
    let generated: BTreeSet<String> = generated_shell(&files).into_iter().collect();
    let assembled: BTreeSet<String> = files
        .keys()
        .map(|key| match key.strip_prefix(CONSOLE_PREFIX) {
            Some(rest) => format!("./{rest}"),
            None => format!("../{key}"),
        })
        .collect();
    assert_eq!(generated, assembled);
}

/// A non-interactive render emits NO `console/` key.
#[test]
fn a_non_interactive_render_emits_no_console_key() {
    let model = common::cached_model();
    let site = render_site_lang_exec(&model, "english", &ExecutableDocsData::default());
    let console: Vec<&String> = site
        .files
        .keys()
        .filter(|k| k.starts_with(CONSOLE_PREFIX))
        .collect();
    assert!(
        console.is_empty(),
        "a static render leaked console keys: {console:?}"
    );
    assert!(console_files(&ExecutableDocsData::default()).is_empty());
}

// ── D-i: the controller reaches every page that carries a control ───────────

/// A term page that emits `.gmeow-run-validation` controls MUST load the controller.
///
/// This is the D-i regression. The controls were emitted on term pages whenever the render
/// was bundle-backed, but the script tag was injected only on the three interactive host
/// pages — so on the static site every fixture button was inert, while under mdbook (which
/// injects the boot shim on every chapter) the identical button worked.
#[test]
fn every_page_with_a_control_loads_the_controller() {
    let model = common::cached_model();
    let exec = interactive_exec();
    let site = render_site_lang_exec(&model, "english", &exec);
    let mut checked = 0usize;
    for (path, bytes) in &site.files {
        if !path.ends_with("/index.html") && path != "index.html" {
            continue;
        }
        let html = String::from_utf8_lossy(bytes);
        if !body_carries_interactive_control(&html) {
            continue;
        }
        checked += 1;
        assert!(
            html.contains("assets/docs-controller.mjs"),
            "{path} carries an interactive control but loads no controller"
        );
    }
    assert!(
        checked > 0,
        "the fixture render must contain at least one page with an interactive control"
    );
}

/// The bundle explorer and the conjecture playground are reachable from the static nav.
#[test]
fn the_static_nav_reaches_every_interactive_page() {
    let model = common::cached_model();
    let exec = interactive_exec();
    let site = render_site_lang_exec(&model, "english", &exec);
    let landing = String::from_utf8(site.files["index.html"].clone()).unwrap();
    for (dir, label) in [
        (Page::BundleExplorer.dir(), "Explorer"),
        (Page::ConjecturePlayground.dir(), "Conjectures"),
        ("console".to_string(), "Console"),
    ] {
        assert!(
            landing.contains(&format!("{dir}/index.html")),
            "the nav does not link {dir}/ ({label})"
        );
    }
    // …and each linked page really exists (no dangling nav entry).
    for page in [Page::BundleExplorer, Page::ConjecturePlayground] {
        assert!(
            site.files.contains_key(&page.html_path()),
            "nav links {} but the page is not emitted",
            page.dir()
        );
    }
    assert!(site.files.contains_key("console/index.html"));
}

// ── Assertion 7 ─────────────────────────────────────────────────────────────

/// One `.gmeow-run-validation` control, reduced to what activating it determines.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Activation {
    /// The fixture identity the control carries (`data-origin`).
    origin: String,
    /// The MCP tool the controller dispatches for this control.
    tool: &'static str,
    /// The decoded Turtle the control hands that tool — the `data` argument.
    data: String,
    /// The `format` argument.
    format: &'static str,
}

/// Decode a base64 payload (the `data-turtle` attribute's encoding).
fn base64_decode(input: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let Some(index) = ALPHABET.iter().position(|c| *c == byte) else {
            continue;
        };
        buffer = (buffer << 6) | index as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

/// Every activation a rendered body determines, in a stable order.
///
/// This mirrors `validationRequest()` in `assets/docs-controller.mjs` exactly: read the
/// control's `data-turtle`, base64-decode it, and dispatch `validate_local` with
/// `{data, format: "turtle"}`. There is one controller and one mapping, so computing it
/// here is computing what the browser would send.
fn activations(body: &str) -> Vec<Activation> {
    let mut out = Vec::new();
    for chunk in body.split("class=\"gmeow-run-validation\"").skip(1) {
        let Some(turtle) = attribute(chunk, "data-turtle") else {
            continue;
        };
        let origin = attribute(chunk, "data-origin").unwrap_or_default();
        out.push(Activation {
            origin,
            tool: "validate_local",
            data: String::from_utf8_lossy(&base64_decode(&turtle)).into_owned(),
            format: "turtle",
        });
    }
    out.sort();
    out
}

/// The value of `name="…"` at the start of `chunk`.
fn attribute(chunk: &str, name: &str) -> Option<String> {
    let start = chunk.find(&format!("{name}=\""))? + name.len() + 2;
    let end = start + chunk[start..].find('"')?;
    Some(chunk[start..end].to_string())
}

/// Every `id="gmeow-…"` an interactive body declares, sorted and deduped.
fn control_ids(body: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for chunk in body.split("id=\"gmeow-").skip(1) {
        if let Some(end) = chunk.find('"') {
            ids.insert(format!("gmeow-{}", &chunk[..end]));
        }
    }
    ids
}

/// **`static_and_mdbook_shells_agree`.**
///
/// The same term page, rendered into both shells, must agree on three things:
///
/// 1. the interactive control ids it declares;
/// 2. the set of modules the shell dynamically imports to drive them;
/// 3. the result of activating each `.gmeow-run-validation` control.
///
/// (3) is asserted as the pairwise equality of the ACTIVATION each control determines —
/// the tool name and the exact argument object the one shared controller dispatches — plus
/// the byte-identity of the controller module and the engine assets both shells ship.
/// Equal request + equal engine is equal result; the two are checked separately so a
/// divergence in either is named for what it is.
#[test]
fn static_and_mdbook_shells_agree() {
    let model = common::cached_model();
    let exec = interactive_exec();
    let site = render_site_lang_exec(&model, "english", &exec);
    let book = render_book(&model, &exec);

    // Pick the term page that carries the most controls, so the comparison is not vacuous.
    let mut best: Option<(String, Vec<Activation>)> = None;
    for term in &model.terms {
        let slug = term_slug(term);
        let path = format!("terms/{slug}/index.html");
        let Some(bytes) = site.files.get(&path) else {
            continue;
        };
        let found = activations(&String::from_utf8_lossy(bytes));
        if best.as_ref().is_none_or(|(_, a)| found.len() > a.len()) {
            best = Some((slug, found));
        }
    }
    let (slug, static_activations) = best.expect("the fixture model has term pages");
    assert!(
        !static_activations.is_empty(),
        "no term page carried a .gmeow-run-validation control — the comparison would be vacuous"
    );

    let static_html =
        String::from_utf8(site.files[&format!("terms/{slug}/index.html")].clone()).unwrap();
    let book_md =
        String::from_utf8(book.files[&format!("src/terms/{slug}/index.md")].clone()).unwrap();

    // 1. Interactive control ids, pairwise equal.
    assert_eq!(
        control_ids(&static_html),
        control_ids(&book_md),
        "the two shells declare different interactive control ids on terms/{slug}"
    );

    // 2. The dynamically-imported module set. The static shell injects the controller with
    //    a module <script>; the book's `additional-js` boot shim dynamic-imports the same
    //    path. Both sets must be the single controller module.
    let boot = String::from_utf8(book.files["mdbook-boot.js"].clone()).unwrap();
    let static_modules: BTreeSet<&str> = static_html
        .contains("assets/docs-controller.mjs")
        .then_some("assets/docs-controller.mjs")
        .into_iter()
        .collect();
    let book_modules: BTreeSet<&str> = boot
        .contains("assets/docs-controller.mjs")
        .then_some("assets/docs-controller.mjs")
        .into_iter()
        .collect();
    assert_eq!(
        static_modules, book_modules,
        "the two shells import different controller modules"
    );
    assert!(
        !static_modules.is_empty(),
        "neither shell imports a controller"
    );

    // 3. The activation of each control, pairwise equal …
    assert_eq!(
        static_activations,
        activations(&book_md),
        "activating the same control in the two shells would dispatch different requests"
    );

    // … over a byte-identical controller and a byte-identical engine.
    for (path, bytes) in interactive_asset_files(&exec) {
        assert_eq!(
            site.files.get(&path),
            Some(&bytes),
            "the static site's {path} is not the shared asset"
        );
        assert_eq!(
            book.files.get(&format!("src/{path}")),
            Some(&bytes),
            "the book's src/{path} is not the shared asset"
        );
    }
}

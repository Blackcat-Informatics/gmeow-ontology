// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The standalone-console producer's named acceptance assertions, plus the
//! two-shell agreement gate.
//!
//! Every test here is a gate blocker. They run over the REAL site and book renders of the
//! cached documentation fixture, not over a synthetic tree, so what is asserted is what
//! ships.

use std::collections::BTreeSet;

use gmeow_docs::ExecutableDocsData;
use gmeow_docs::console::{
    ByteReport, CONSOLE_PREFIX, Fetch, NO_SIZE_GATE_DISCLOSURE, PRECACHE_CEILING_FACTOR,
    SITE_SECTIONS_MARKER, console_files, fetch_tier, generated_build_digest, generated_shell,
    hand_authored_byte_magnitudes, precache_ceiling,
};
use gmeow_docs::render::interactive_asset_files;

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

// ── Console producer ────────────────────────────────────────────────────────

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

/// One assembled key, expressed the way the service worker spells it.
fn sw_relative(key: &str) -> String {
    match key.strip_prefix(CONSOLE_PREFIX) {
        Some(rest) => format!("./{rest}"),
        None => format!("../{key}"),
    }
}

/// The generated `SHELL` set EQUALS the PRE-CACHED tiers, in both directions.
///
/// Equality, not containment, in both directions: a shell member that is not a pre-cached
/// asset is a byte downloaded at install for nothing, and a pre-cached asset that is not a
/// shell member is a console that boots online and dies offline.
#[test]
fn the_generated_shell_equals_the_precached_tiers() {
    let files = console_files(&interactive_exec());
    let generated: BTreeSet<String> = generated_shell(&files).into_iter().collect();
    let precached: BTreeSet<String> = files
        .keys()
        .filter(|key| fetch_tier(key).is_precached())
        .map(|key| sw_relative(key))
        .collect();
    assert_eq!(generated, precached);
}

/// The PAGE-LOAD tier is a PROPER subset of what the worker pre-caches.
///
/// The defect this closes was a published heading, not a number: a table of everything the
/// service worker stores at install was headed "First load — everything fetched before any
/// pane runs", so 8 MB of vendored purrdf, a PWA manifest and four icons were published as
/// bytes a reader pays to open the page. No page load fetches any of them. The two sets are
/// distinct now, so a future asset that joins the pre-cache cannot inflate the page-load
/// figure by sitting in the same bucket.
#[test]
fn the_page_load_tier_is_strictly_inside_the_pre_cache() {
    let files = console_files(&interactive_exec());
    let page_load: BTreeSet<&String> = files
        .keys()
        .filter(|key| fetch_tier(key) == Fetch::PageLoad)
        .collect();
    let precached: BTreeSet<&String> = files
        .keys()
        .filter(|key| fetch_tier(key).is_precached())
        .collect();
    assert!(
        page_load.is_subset(&precached),
        "a page-load asset that is not pre-cached is a console that dies offline"
    );
    assert!(
        page_load.len() < precached.len(),
        "the pre-cache must carry members no page load fetches — otherwise the two published \
         numbers are one number under two headings"
    );
    // Named, so this cannot pass on some incidental difference: these are the assets the
    // published table counts as "fetched before any pane runs". The documentation site's
    // per-capability wasm engines are deliberately absent: the console dispatches JSON-RPC
    // to the MCP segments and reaches none of them, so they are Fetch::Never and belong to
    // neither published number.
    for key in [
        "console/manifest.webmanifest",
        "console/icon-maskable-512.png",
        "console/sw.mjs",
    ] {
        assert_eq!(
            fetch_tier(key),
            Fetch::InstallOnly,
            "{key} is pre-cached but no page load fetches it"
        );
        assert!(precached.contains(&key.to_string()));
    }
}

/// The install-time pre-cache does NOT carry the demand-loaded reasoning segment.
///
/// This is the whole of the demand-loading decision, expressed over the artifact. The
/// generated `SHELL` used to be the entire assembled key set, so `cache.addAll` fetched the
/// whole reasoning image — the largest single asset in the tree — on install for every
/// reader, including one who never reasons,
/// while the README claimed the segment was "fetched only when a pane first needs a
/// reasoning-segment tool". The measurement that motivated demand loading was falsified by
/// the artifact that was supposed to implement it.
#[test]
fn the_install_shell_omits_the_demand_loaded_segment() {
    let files = console_files(&interactive_exec());
    let shell: BTreeSet<String> = generated_shell(&files).into_iter().collect();

    let segment: Vec<&String> = files
        .keys()
        .filter(|key| key.starts_with("assets/mcp/"))
        .collect();
    assert!(
        !segment.is_empty(),
        "the fixture must actually emit a reasoning segment for this to be a real check"
    );
    for key in segment {
        assert!(
            !shell.contains(&sw_relative(key)),
            "the install-time SHELL pre-caches the demand-loaded reasoning segment ({key})"
        );
    }
    // …and the two site-only assets the console never asks for.
    for key in ["assets/conjectures.ttl", "assets/gmeow-docs.js"] {
        assert!(
            files.contains_key(key),
            "{key} must be in the assembled tree for this check to be meaningful"
        );
        assert!(
            !shell.contains(&sw_relative(key)),
            "the install-time SHELL pre-caches {key}, which the console never fetches"
        );
    }
}

/// The producer ships NO dev-only scaffolding.
///
/// `smoke/package.json` and `smoke/package-lock.json` were `include_bytes!`d into the shell
/// file set, so the private Playwright manifest pair was deployed to the public site and
/// pre-cached into every reader's offline storage — while the README said nothing under
/// `smoke/` is part of the shipped console.
#[test]
fn the_producer_ships_no_dev_only_scaffolding() {
    let files = console_files(&interactive_exec());
    let leaked: Vec<&String> = files.keys().filter(|key| key.contains("/smoke/")).collect();
    assert!(
        leaked.is_empty(),
        "dev-only scaffolding is deployed to the site: {leaked:?}"
    );
}

/// The service worker's cache name is a CONTENT digest, not a shape hash.
///
/// The cache used to be named from the shell's entry count and the length of its joined
/// paths, so any rebuild that changed bytes without changing paths — which is nearly every
/// rebuild — reused the previous cache and served a returning reader stale code for ever.
#[test]
fn the_cache_name_is_a_content_digest() {
    let one = console_files(&interactive_exec());
    let mut changed = interactive_exec();
    // Same LENGTH, different bytes: the substitution a shape-based key cannot see.
    changed.full_bundle_gts = b"gts-bundle-sentinel-byteS".to_vec();
    let two = console_files(&changed);
    assert_eq!(
        one.keys().collect::<Vec<_>>(),
        two.keys().collect::<Vec<_>>(),
        "the two trees must carry identical paths for this to be the interesting case"
    );
    let first = generated_build_digest(&one);
    assert_ne!(
        first,
        generated_build_digest(&two),
        "two trees with the same paths and different bytes share a cache name"
    );
    let sw = String::from_utf8(one[&format!("{CONSOLE_PREFIX}sw.mjs")].clone()).unwrap();
    assert!(
        sw.contains(&format!("const BUILD = \"{first}\"")),
        "the worker must carry the generated digest verbatim"
    );
}

/// The console README's measured-byte table is GENERATED, and the ceiling it publishes is
/// the DERIVED one — the factor between the two published numbers is an identity.
///
/// The hand-authored table this replaced was wrong on nearly every row and omitted two
/// assets the console fetches on every boot. A table typed into a document is a second
/// source of truth for numbers that move on every build; this one is measured over the
/// bytes that ship.
#[test]
fn the_published_byte_table_is_measured_over_the_shipped_bytes() {
    let files = console_files(&interactive_exec());
    let readme = String::from_utf8(files["console/README.md"].clone()).unwrap();
    let report = ByteReport::of(&files);

    assert!(
        !readme.contains("__GMEOW_CONSOLE_BYTE_TABLE__"),
        "the shipped README still carries the unsubstituted table marker"
    );
    // Every pre-cached row the report measured is published, with the measured number.
    for (key, bytes, tier) in &report.rows {
        if !tier.is_precached() {
            continue;
        }
        assert_eq!(
            files[key].len() as u64,
            *bytes,
            "the report's {key} row is not the emitted length"
        );
        assert!(
            readme.contains(&format!("| `{key}` |")),
            "the published table omits the pre-cached asset {key}"
        );
    }
    // The heading over the page-load table says what that table is, and the pre-cache table
    // says what IT is. The one this replaced headed the pre-cache set "First load —
    // everything fetched before any pane runs", 40 lines above the same document's
    // admission that nothing in the console fetches the vendored engine it counted.
    assert!(
        readme.contains("**Page load** — what the page itself fetches on a first visit"),
        "the page-load table must be headed as the page load"
    );
    assert!(
        readme.contains("**Pre-cached at install, not fetched by a page load**"),
        "the install-only table must be headed as what it is: bytes the worker stores that \
         no page load asks for"
    );
    assert!(
        !readme.contains("everything fetched before any pane runs"),
        "the false heading is back over a table of the install pre-cache"
    );
    // The two published numbers and the published FACTOR between them agree, read back out
    // of the shipped prose. This is the D15 regression: the ceiling was a hand-typed
    // constant (`47 534 469 × 1.1`) whose measurement had since moved to 47 704 211, so the
    // sentence "the measurement above plus ten percent of headroom" sat directly under two
    // numbers whose real ratio was 1.09609. Nothing bound the three together, so nothing
    // noticed. Parsed here rather than restated, so this test cannot be the fourth place
    // the factor is written down.
    let published_page_load = grouped_number(&readme, "| **Page-load total** | **")
        .expect("the README publishes a page-load total");
    let published_install_only = grouped_number(&readme, "| **Install-only total** | **")
        .expect("the README publishes an install-only total");
    let published_precache = grouped_number(&readme, "| **Install pre-cache total** | **")
        .expect("the README publishes an install pre-cache total");
    let published_ceiling = grouped_number(&readme, "The install pre-cache ceiling is **")
        .expect("the README publishes an install pre-cache ceiling");
    assert_eq!(
        published_page_load, report.page_load_total,
        "the published page-load total is not the measured one"
    );
    assert_eq!(
        published_install_only, report.install_only_total,
        "the published install-only total is not the measured one"
    );
    assert_eq!(
        published_precache,
        published_page_load + published_install_only,
        "the published pre-cache total is not the sum of the two published sections — a \
         reader adding the numbers up must arrive at the number the ceiling bounds"
    );
    assert_eq!(
        published_precache,
        report.precache_total(),
        "the published pre-cache total is not the measured one"
    );
    assert!(
        published_page_load < published_precache,
        "the page load must cost strictly less than the install pre-cache — publishing them \
         as one number is the defect this split exists to close"
    );
    assert_eq!(
        published_ceiling,
        precache_ceiling(report.precache_total()),
        "the published ceiling is not the derived one"
    );
    assert_eq!(
        published_ceiling,
        published_precache * PRECACHE_CEILING_FACTOR,
        "the ratio between the two published numbers is not the declared factor"
    );
    assert!(
        readme.contains(&format!("× {PRECACHE_CEILING_FACTOR},")),
        "the published sentence must STATE the factor it is derived with, so a reader can \
         check the two numbers above it against each other"
    );

    // The document must say what that derived number is NOT. It used to read "it bounds
    // what `cache.addAll` downloads at install" — a claim of enforcement over a figure that
    // is a fixed multiple of the very measurement it claimed to bound. The reasoning
    // segment grew by megabytes, the ceiling floated up with it, and nothing could red:
    // every check over the pair was `2n > n` or `measured ≤ 2 × measured`. The tautological
    // assertions are gone; this is what replaced them, and unlike them it CAN red — restore
    // the budget language, or drop the disclosure, and this fails.
    assert!(
        readme.contains(NO_SIZE_GATE_DISCLOSURE),
        "the shipped README no longer discloses that the pre-cache ceiling is a derived \
         ratio and NOT a size-regression gate — a reader takes an unqualified 'ceiling' for \
         a budget something enforces, and nothing here enforces one"
    );
    assert!(
        !readme.contains("of headroom"),
        "the README describes the derived ceiling as headroom again. Headroom is space \
         beneath a limit; this number has no limit beneath it, because it is computed from \
         the measurement it sits above and moves whenever that measurement does"
    );
    assert!(
        !readme.contains("It bounds what"),
        "the README claims the derived ceiling BOUNDS the install pre-cache. It does not: \
         it is that measurement times a constant, so it rises with every byte added and \
         bounds nothing"
    );
}

/// The spliced deployed-site sections state NO hand-authored byte magnitude.
///
/// The gate on the exact defect that came back. `crates/docs/assets/console-site-readme.md`
/// said "pre-caching a 10 MB image at install" twenty-two lines above a GENERATED table
/// reading `12 373 564` for that same image, under a heading asserting "Nothing in this
/// section is typed in" — the image had grown and the prose had not moved. An earlier commit
/// had already closed this defect class once; the commit after it reopened it.
///
/// The producer refuses to splice sections carrying a magnitude at all, so this asserts over
/// the SHIPPED document rather than over the authored source: whatever a reader ends up
/// holding is what is checked. The only numbers the emitted README may state as sizes are
/// the ones the measured table generates.
#[test]
fn the_spliced_site_sections_state_no_hand_authored_byte_magnitude() {
    let files = console_files(&interactive_exec());
    let readme = String::from_utf8(files["console/README.md"].clone()).unwrap();

    let start = readme
        .find("## Offline")
        .expect("the spliced site sections open with the Offline heading");
    let end = readme
        .find("<!-- __GMEOW_CONSOLE_BYTE_TABLE__ -->")
        .or_else(|| readme.find("*Generated by `crates/docs/src/console.rs`"))
        .expect("the measured table is substituted into the spliced sections");
    let prose = &readme[start..end];
    assert!(
        prose.contains("demand-loaded"),
        "the slice must actually cover the offline contract for this check to be meaningful"
    );
    assert_eq!(
        hand_authored_byte_magnitudes(prose),
        Vec::<String>::new(),
        "the deployed-site sections state a byte magnitude in prose. Every byte figure in \
         that document is generated from the assembled tree; a typed one is a second source \
         of truth that goes stale the next time an engine is re-vendored — which is what \
         happened to the reasoning segment's size, in prose sitting directly above the \
         measurement that contradicted it"
    );
}

/// The npm README and the SITE README are different documents, and each describes only its
/// own distribution.
///
/// The published tarball shipped the site's document verbatim: it documented a service
/// worker, a PWA manifest, four icons, a byte table full of `assets/…` rows and a dev-only
/// smoke lane, and the package contains none of that. The site-only sections are substituted
/// by the producer now, so the authored document — which is what npm packs — carries the
/// marker and nothing else.
#[test]
fn the_site_readme_carries_the_site_sections_and_the_authored_one_does_not() {
    let authored = include_str!("../assets/console/README.md");
    assert!(
        authored.contains(SITE_SECTIONS_MARKER),
        "the authored README must carry the site-sections marker"
    );
    for site_only in [
        "## Offline",
        "## Measured bytes",
        "__GMEOW_CONSOLE_BYTE_TABLE__",
        "assets/query/",
        "icon-maskable-512.png",
    ] {
        assert!(
            !authored.contains(site_only),
            "the authored README — the bytes npm publishes — documents {site_only}, which \
             the package does not ship"
        );
    }

    let files = console_files(&interactive_exec());
    let site = String::from_utf8(files["console/README.md"].clone()).unwrap();
    assert!(
        !site.contains(SITE_SECTIONS_MARKER),
        "the site README still carries the unsubstituted site-sections marker"
    );
    for site_only in ["## Offline", "## Measured bytes", "assets/query/"] {
        assert!(
            site.contains(site_only),
            "the site README is missing the substituted section {site_only}"
        );
    }
    // The shared half is in both, and the substitution kept the document's order.
    for shared in ["## Panes", "## The four verbs", "## Install"] {
        assert!(authored.contains(shared) && site.contains(shared));
    }
    assert!(
        site.find("## Measured bytes") < site.find("## Install"),
        "the site sections must be substituted where the marker sits, above Install"
    );
}

/// Every backticked path a SHIPPED README names exists in the distribution that ships it.
///
/// The byte table is already gated this way — every measured row is published — and this is
/// the same rule over the prose. On the site the one reference that dangled was
/// `assets/query/PROVENANCE.md`: the producer emits the engine files and never that one,
/// so the sentence pointing a reader at it was a 404 for every deployed reader.
#[test]
fn every_path_the_site_readme_names_exists_in_the_site_tree() {
    let files = console_files(&interactive_exec());
    let readme = String::from_utf8(files["console/README.md"].clone()).unwrap();
    let distribution: BTreeSet<String> = files.keys().cloned().collect();
    let missing = common::unresolved_readme_paths(&readme, &distribution, CONSOLE_PREFIX, &[]);
    assert!(
        missing.is_empty(),
        "the deployed console README names paths the assembled tree does not carry: \
         {missing:?}"
    );
}

/// A space-grouped byte count published immediately after `marker`, as a number.
///
/// The README's own formatting (`52 287 915`), so the assertions above read the published
/// numbers rather than trusting that they were rendered from the report.
fn grouped_number(readme: &str, marker: &str) -> Option<u64> {
    let start = readme.find(marker)? + marker.len();
    let digits: String = readme[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ' ')
        .filter(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The engine worker's transport specifier RESOLVES inside the emitted tree.
///
/// The published package could not boot because `engine.worker.mjs` imported
/// `../assets/mcp-transport.mjs`, which resolves out of the package to a 404. The one
/// specifier it now spells has to name a file the producer actually emits — checked here
/// over the emitted bytes rather than over the authored source.
#[test]
fn the_worker_transport_specifier_names_an_emitted_file() {
    let files = console_files(&interactive_exec());
    let worker = String::from_utf8(files["console/engine.worker.mjs"].clone()).unwrap();
    assert!(
        worker.contains("from \"./pkg/mcp-transport.mjs\""),
        "the worker must import its transport from ./pkg/, which resolves inside both the \
         site tree and the published package"
    );
    assert!(
        !worker.contains("from \"../assets/mcp-transport.mjs\""),
        "the worker must not import out of its own directory — that is the specifier that \
         404s once the console is installed from npm"
    );
    let forwarder = String::from_utf8(files["console/pkg/mcp-transport.mjs"].clone()).unwrap();
    assert!(
        forwarder.contains("export * from \"../../assets/mcp-transport.mjs\";"),
        "the site's forwarder must re-export the ONE shared transport, not carry a copy"
    );
    // The forwarder's own target is emitted too, so nothing about the chain dangles.
    assert!(files.contains_key("assets/mcp-transport.mjs"));
}

/// A non-interactive render emits NO `console/` key.
#[test]
fn a_non_interactive_render_emits_no_console_key() {
    let site = common::cached_site();
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

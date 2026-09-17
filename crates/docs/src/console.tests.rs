// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// An `exec` that makes a render interactive (the renderer only checks non-emptiness).
fn interactive_exec() -> ExecutableDocsData {
    ExecutableDocsData {
        full_bundle_gts: b"gts-bundle-sentinel-bytes".to_vec(),
        conjectures_ttl: b"@prefix ex: <http://example/> .".to_vec(),
        // The playground TriG too: `console_files` folds `interactive_asset_files`,
        // and the site emits its playground asset there. A fixture without it renders
        // a STRICT SUBSET of the interactive surface, so the exactness check below
        // would call a live tier row dead purely because the fixture never triggered
        // the asset it classifies.
        playground_trig: b"@prefix ex: <http://example/> .\nex:a ex:b ex:c .".to_vec(),
        ..Default::default()
    }
}

/// Determinism: the same `exec` yields byte-identical maps.
#[test]
fn console_files_is_deterministic() {
    let exec = interactive_exec();
    assert_eq!(console_files(&exec), console_files(&exec));
}

/// The console never re-cuts a shared asset: every key both maps carry is byte-equal.
#[test]
fn shared_asset_bytes_are_identical() {
    let exec = interactive_exec();
    let console = console_files(&exec);
    let shared = interactive_asset_files(&exec);
    assert!(!shared.is_empty(), "the fixture must be interactive");
    for (key, bytes) in &shared {
        assert_eq!(
            console.get(key),
            Some(bytes),
            "shared asset {key} differs between the console and the site"
        );
    }
}

/// The generated `SHELL` set EQUALS the PRE-CACHED tiers, in both directions.
#[test]
fn generated_shell_equals_the_precached_tiers() {
    let files = console_files(&interactive_exec());
    let generated: std::collections::BTreeSet<String> =
        generated_shell(&files).into_iter().collect();
    let expected: std::collections::BTreeSet<String> = files
        .keys()
        .filter(|key| fetch_tier(key).is_precached())
        .map(|key| sw_relative(key))
        .collect();
    assert_eq!(
        generated, expected,
        "the generated SHELL must equal the pre-cached tiers"
    );
    assert!(
        generated.contains("./sw.mjs"),
        "the worker caches its own script: {generated:?}"
    );
    assert!(
        generated.iter().any(|path| path.starts_with("../assets/")),
        "the out-of-scope engine assets must be pre-cached: {generated:?}"
    );
}

/// A non-interactive render emits NO console key at all.
#[test]
fn non_interactive_render_emits_no_console() {
    assert!(console_files(&ExecutableDocsData::default()).is_empty());
}

/// The substituted worker and README carry no marker residue.
#[test]
fn every_marker_is_fully_substituted() {
    let files = console_files(&interactive_exec());
    let sw = String::from_utf8(files[&format!("{CONSOLE_PREFIX}sw.mjs")].clone()).unwrap();
    assert!(
        !sw.contains("__GMEOW_CONSOLE_SHELL__") && !sw.contains("__GMEOW_CONSOLE_BUILD__"),
        "unsubstituted worker marker survived"
    );
    let readme = String::from_utf8(files[README_KEY].clone()).unwrap();
    assert!(
        !readme.contains("__GMEOW_CONSOLE_BYTE_TABLE__")
            && !readme.contains("__GMEOW_CONSOLE_SITE_SECTIONS__"),
        "unsubstituted README marker survived"
    );
}

/// Every declared tier row is USED, and no key matches two of them.
///
/// A pattern that covers nothing is a classification for an asset that no longer exists,
/// and a key covered twice is classified by table ORDER — which is the silent,
/// order-dependent tiering the `else` arm this table replaced already had.
#[test]
fn the_tier_tables_are_exact() {
    let files = console_files(&interactive_exec());
    for (pattern, _) in ASSET_TIERS {
        assert!(
            files.keys().any(|key| covers(pattern, key)),
            "the ASSET_TIERS row {pattern} covers no emitted key"
        );
    }
    for key in files.keys() {
        if key.starts_with(CONSOLE_PREFIX) {
            continue;
        }
        let matched: Vec<&str> = ASSET_TIERS
            .iter()
            .filter(|(pattern, _)| covers(pattern, key))
            .map(|(pattern, _)| *pattern)
            .collect();
        assert_eq!(
            matched.len(),
            1,
            "{key} is covered by {matched:?} — a key must have exactly one declared tier"
        );
    }
    for (name, _, _) in SHELL_FILES {
        assert!(
            GENERATED_CONSOLE_FILES
                .iter()
                .all(|(generated, _)| generated != name),
            "console/{name} is declared both as an authored shell file and as a \
                 generated one"
        );
    }
}

/// An emitted key with no declared tier is REFUSED, not swept into one.
#[test]
#[should_panic(expected = "no ASSET_TIERS entry covers")]
fn an_undeclared_asset_key_is_refused() {
    let _ = fetch_tier("assets/some-newly-vendored-engine/index.mjs");
}

/// The same refusal on the console side of the tree.
#[test]
#[should_panic(expected = "neither a declared shell file nor a generated console file")]
fn an_undeclared_console_key_is_refused() {
    let _ = fetch_tier("console/newly-added-pane.mjs");
}

/// The two published totals are the two SUMS, and the pre-cache set is their union.
#[test]
fn the_published_totals_partition_the_measured_tree() {
    let files = console_files(&interactive_exec());
    let report = ByteReport::of(&files);
    let summed = |tier: Fetch| -> u64 {
        files
            .iter()
            .filter(|(key, _)| fetch_tier(key) == tier)
            .map(|(_, bytes)| bytes.len() as u64)
            .sum()
    };
    assert_eq!(report.page_load_total, summed(Fetch::PageLoad));
    assert_eq!(report.install_only_total, summed(Fetch::InstallOnly));
    assert_eq!(
        report.precache_total(),
        summed(Fetch::PageLoad) + summed(Fetch::InstallOnly)
    );
    assert!(
        report.install_only_total > 0,
        "the fixture must carry install-only bytes for the split to be meaningful"
    );
    assert!(
        report.precache_total() > report.page_load_total,
        "the pre-cache set must be strictly larger than the page load — otherwise the \
             two published numbers are the same number under two headings"
    );
    // There is deliberately NO assertion here relating `ceiling()` to `precache_total()`.
    // `ceiling()` IS `precache_total() × PRECACHE_CEILING_FACTOR` by definition, so
    // `assert_eq!(report.ceiling(), report.precache_total() * 2)` — which stood here —
    // restated the definition against a second hard-coded copy of the factor and no
    // measurement could ever have reddened it. What the README publishes about that
    // derived number IS asserted, in the producer acceptance lane, against the shipped
    // prose: that it is the derived figure, and that the document says outright it is
    // not a budget.
}

/// The AUTHORED site sections carry no hand-typed byte magnitude, anywhere in them.
///
/// The total form of the gate — over the whole authored source, not over a slice of the
/// emitted document — because this test can see the private constant the producer
/// splices. `console_files` refuses to splice a source that fails this, so the check is
/// fail-closed at render time as well; this pins it as a named test so the reason it
/// exists is written down next to it rather than only inside a panic message.
#[test]
fn the_authored_site_sections_state_no_byte_magnitude() {
    assert_eq!(
        hand_authored_byte_magnitudes(SITE_SECTIONS_SOURCE),
        Vec::<String>::new(),
        "crates/docs/assets/console-site-readme.md states a byte magnitude in prose. It \
             carried \"a 10 MB image\" twenty-two lines above a GENERATED table measuring \
             that same image at 12 373 564 — under a heading asserting nothing in the \
             section is typed in — because the engine grew and the sentence did not. Refer \
             to the measured table; do not restate a number beside it"
    );
}

/// The scanner catches real hand-typed magnitudes and leaves real prose alone.
///
/// Pins the gate itself. A detector that finds nothing is indistinguishable from clean
/// prose, and the defect it exists to catch is one that already came back once.
#[test]
fn the_byte_magnitude_scanner_is_not_vacuous() {
    for typed in [
        "pre-caching a 10 MB image at install",
        "the 7 MB core image is not duplicated",
        "duplicating a 29 kB module",
        "cache.addAll over a 56MB tier",
        "a 1.5 GiB dataset",
        "12 373 564 bytes on disk",
        "an 8b header",
    ] {
        assert!(
            !hand_authored_byte_magnitudes(typed).is_empty(),
            "the scanner missed a hand-typed magnitude in {typed:?}"
        );
    }
    for clean in [
        "icon-192.png, icon-512.png and icon-maskable-512.png",
        "a BLAKE3 content digest of the assembled tree",
        "RDF-1.2 quoted triples over 38 tools",
        "`display: standalone`, `start_url: \".\"`",
        "the demand-loaded total in the table below",
        "a 10 Mbps link",
    ] {
        assert_eq!(
            hand_authored_byte_magnitudes(clean),
            Vec::<String>::new(),
            "the scanner flagged prose that states no byte magnitude: {clean:?}"
        );
    }
    assert_eq!(
        hand_authored_byte_magnitudes("a 10 MB image and a 7MB one"),
        vec!["10 MB".to_string(), "7MB".to_string()],
        "the scanner must report every magnitude it finds, in order"
    );
}

/// The cache name is a digest of the BYTES: same paths + different bytes ⇒ new cache.
#[test]
fn the_cache_name_changes_when_a_byte_changes() {
    let one = console_files(&interactive_exec());
    let mut other = interactive_exec();
    // Same length, one byte different — the substitution a length-based key accepts.
    other.full_bundle_gts = b"gts-bundle-sentinel-byteS".to_vec();
    let two = console_files(&other);
    assert_eq!(
        one.keys().collect::<Vec<_>>(),
        two.keys().collect::<Vec<_>>(),
        "the two trees must have identical paths for this to be the interesting case"
    );
    assert_ne!(
        generated_build_digest(&one),
        generated_build_digest(&two),
        "a byte-for-byte substitution must produce a different cache name"
    );
}

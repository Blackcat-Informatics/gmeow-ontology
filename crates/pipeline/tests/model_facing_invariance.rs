// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! ZERO MODEL-FACING CHANGE — the two legs that need no bundle.
//!
//! The medium axis re-CODES the bundle's bytes. Its whole claim is that coding is not
//! meaning, so nothing a model reads may move. Three legs carry that claim; the
//! artifact-invariance leg needs a live emission and lives with the identity gate
//! (`tests/medium_identity_gate.rs`), while the two here are cheap and run on every
//! `cargo nextest` pass:
//!
//! * **producer non-interference** — this branch's diff against the merge base must not
//!   intersect the GMN-dialect producer census, PLUS the companion that stops the
//!   ratchet freezing an incomplete census;
//! * **the llms clause** — the `llms.txt`-family SHAPE (skeleton, section headers,
//!   section ordering, notation conventions, the MCP consumer-index resource-list
//!   structure) is byte-identical to the merge base, while term entries follow the
//!   ontology and the resource list may grow only by the delta the ontology itself
//!   licenses (one resource per `gmeow:` surface this change declares and the merge base
//!   did not).
//!
//! Every leg is exercised against a targeted RED FIXTURE beside its live assertion: a
//! gate whose failure arm cannot be reached is not a gate, and a live tree that happens
//! to be clean proves nothing about the check that looked at it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_pipeline::branch_base::{
    BaseFile, BaseRef, ci_declared, git_show_base, resolve_base_ref,
};
use gmeow_pipeline::gmn_dialect::{
    self, ModelFacingReport, PINNED_GMN_DIALECT_PRODUCERS, ProducedPath,
    check_producer_census_is_complete,
};

/// The llms-family SHAPE freeze, `#[path]`-included exactly as the shared MEDIUM negative
/// controls in `support/medium_tamper.rs` are: nothing in the shipped pipeline library
/// calls any of it, so it is test support rather than a `crates/pipeline/src` module.
#[path = "support/llms_shape.rs"]
mod llms_shape;

use llms_shape::{
    FROZEN_LLMS_SHAPE, ItemRef, MCP_RESOURCE_CONTRIBUTORS, SurfaceMatch, check_frozen_item,
    check_resource_list, declared_surfaces, extract_item,
};

/// Run one check over a fresh report and return it — every leg below asserts on the
/// COLLECTED census rather than on a first failure, so one run names every problem.
fn run(check: impl FnOnce(&mut ModelFacingReport)) -> ModelFacingReport {
    let mut report = ModelFacingReport::default();
    check(&mut report);
    report
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Run `git` in `root` and return stdout, hard-failing on anything but success.
///
/// A git invocation that fails is never a reason to pass: both legs are DEFINED as a
/// comparison against the merge base, and a gate that cannot obtain its comparand has
/// not performed its check.
fn git(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("could not run `git {}`: {err}", args.join(" ")));
    assert!(
        out.status.success(),
        "`git {}` failed ({}): {}",
        args.join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Every repo-relative path this branch changes relative to the merge base — committed
/// AND uncommitted.
///
/// `git diff <base>...HEAD` alone would read only what has been committed, so a
/// working-tree edit to a GMN-dialect producer would sail past the gate on the very run
/// that introduced it. The union with the index/worktree diff and the untracked set is
/// the honest reading of "what this change touches".
fn changed_paths(root: &Path, base: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for line in git(root, &["diff", "--name-only", base]).lines() {
        out.insert(line.trim().to_string());
    }
    for line in git(root, &["ls-files", "--others", "--exclude-standard"]).lines() {
        out.insert(line.trim().to_string());
    }
    out.remove("");
    out
}

/// The merge base, or a LOUD skip when `origin/main` genuinely is not in this checkout AND
/// the run is an interactive one.
///
/// An automated run is the case that matters: nobody reads a `SKIP:` line scrolling past
/// in a log, so a CI checkout without `origin/main` would take every leg below through its
/// early return and the whole model-facing clause would pass by never having compared
/// anything. CI builds the PR merged into `main`, so `origin/main` is present there by
/// construction — which means an absent upstream under [`ci_declared`] is a
/// mis-provisioned checkout, not a legitimate bare clone, and the honest report is a hard
/// failure.
fn base_ref(root: &Path) -> Option<String> {
    match resolve_base_ref(root) {
        BaseRef::Resolved(sha) => Some(sha),
        BaseRef::NoUpstream(why) if ci_declared() => panic!(
            "CI is declared, so this run is automated and a skip nobody reads is a vacuous \
             pass: {why}. CI builds the PR merged into `main`, so `origin/main` is present \
             there by construction — an absent upstream here is a mis-provisioned checkout, \
             and the model-facing legs would grade nothing"
        ),
        BaseRef::NoUpstream(why) => {
            println!("SKIP: {why} (interactive run in a bare clone — set CI=true to require it)");
            None
        }
        BaseRef::Unresolvable(why) => panic!(
            "the model-facing gates are DEFINED as a comparison against the merge base, so a \
             comparand that cannot be obtained is unfinished work rather than a pass: {why}"
        ),
    }
}

// ── Leg 2: producer non-interference ─────────────────────────────────────────

#[test]
fn leg2_the_branch_diff_touches_no_gmn_dialect_producer() {
    let root = repo_root();
    let Some(base) = base_ref(&root) else { return };
    let changed = changed_paths(&root, &base);
    assert!(
        !changed.is_empty(),
        "the branch diff against {base} is empty — the gate would pass by looking at nothing"
    );
    println!("leg 2: {} changed path(s) against {base}", changed.len());
    let touched: Vec<&str> = changed
        .iter()
        .map(String::as_str)
        .filter(|path| gmn_dialect::is_gmn_dialect_producer(path))
        .collect();
    if touched.is_empty() {
        return;
    }
    // A producer was touched, so the leg has to decide whether the DIALECT moved or only the
    // code that produces it. A binding can only relocate between files that both appear in
    // the diff — the file that loses it and the file that gains it — so the union over the
    // touched producers captures a move exactly, without enumerating the census at the base.
    println!(
        "leg 2: {} GMN-dialect producer(s) touched; comparing the glyph/cost surface",
        touched.len()
    );
    let mut base_bindings = BTreeMap::new();
    let mut work_bindings = BTreeMap::new();
    for path in &touched {
        if let Some(text) = base_text(&root, &base, path) {
            base_bindings.extend(gmn_dialect::glyph_cost_bindings(&text));
        }
        if let Ok(text) = std::fs::read_to_string(root.join(path)) {
            work_bindings.extend(gmn_dialect::glyph_cost_bindings(&text));
        }
    }
    let report = run(|r| {
        gmn_dialect::check_dialect_content_invariance(&base_bindings, &work_bindings, r);
    });
    assert!(report.is_clean(), "{report}");
}

/// A moved COST reds the leg — the fixture perturbs one binding rather than one path,
/// because a path touch is exactly what this leg no longer treats as the violation.
#[test]
fn leg2_red_fixture_a_moved_glyph_cost_reds() {
    let base = BTreeMap::from([("¬".to_string(), 1), ("⊑".to_string(), 3)]);
    let repriced = BTreeMap::from([("¬".to_string(), 2), ("⊑".to_string(), 3)]);
    let report = run(|r| gmn_dialect::check_dialect_content_invariance(&base, &repriced, r));
    assert!(!report.is_clean(), "a repriced glyph must red the leg");
    assert!(report.to_string().contains("1 -> 2"), "{report}");

    let dropped = BTreeMap::from([("⊑".to_string(), 3)]);
    let report = run(|r| gmn_dialect::check_dialect_content_invariance(&base, &dropped, r));
    assert!(!report.is_clean(), "an unpriced glyph must red the leg");
    assert!(report.to_string().contains("now UNPRICED"), "{report}");

    let added = BTreeMap::from([
        ("¬".to_string(), 1),
        ("⊑".to_string(), 3),
        ("◉".to_string(), 2),
    ]);
    let report = run(|r| gmn_dialect::check_dialect_content_invariance(&base, &added, r));
    assert!(!report.is_clean(), "a newly priced glyph must red the leg");
    assert!(report.to_string().contains("NEWLY priced"), "{report}");

    // And the move this branch actually makes — a table relocating between two census
    // crates with every binding intact — must NOT red, or the leg is unsatisfiable again.
    let moved = base.clone();
    let report = run(|r| gmn_dialect::check_dialect_content_invariance(&base, &moved, r));
    assert!(
        report.is_clean(),
        "a pure relocation must not red: {report}"
    );
}

/// Every `crates/*/src/**/*.rs` file, sorted, repo-relative with forward slashes.
fn workspace_sources(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let crates = root.join("crates");
    let mut crate_dirs: Vec<PathBuf> = std::fs::read_dir(&crates)
        .expect("crates/ is a required source tree")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir() && !path.is_symlink())
        .collect();
    crate_dirs.sort();
    for dir in crate_dirs {
        collect_rs(&dir.join("src"), root, &mut out);
    }
    out.sort();
    out
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| !path.is_symlink())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

/// The `EmittedArtifact::path_suffix` templates one source file mints, each resolved to
/// the committed path family it becomes.
///
/// `crates/pipeline/src/stages/lang_projection.rs` is the ONE place the
/// `generated/projections/lang` prefix is joined onto a suffix, so a `path_suffix:`
/// initializer IS the complete minting surface for that tree; the assertion below pins
/// that so this derivation cannot quietly become partial.
fn produced_lang_paths(root: &Path) -> BTreeSet<ProducedPath> {
    let mut out: BTreeSet<ProducedPath> = BTreeSet::new();
    for rel in workspace_sources(root) {
        let text = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
        for line in text.lines() {
            let Some(after) = line.split_once("path_suffix:").map(|(_, rest)| rest) else {
                continue;
            };
            let Some(template) = first_string_literal(after) else {
                // `pub path_suffix: String,` — the field DECLARATION, not a minting site.
                continue;
            };
            // A `format!` interpolation stands for "any value here": collapse it so the
            // family, not one instantiation, is what the predicate is asked about.
            let mut family = String::new();
            let mut depth = 0usize;
            for ch in template.chars() {
                match ch {
                    '{' => {
                        depth += 1;
                        if depth == 1 {
                            family.push('*');
                        }
                    }
                    '}' => depth = depth.saturating_sub(1),
                    _ if depth == 0 => family.push(ch),
                    _ => {}
                }
            }
            out.insert(ProducedPath {
                source: rel.clone(),
                path: format!("{}{family}", gmn_dialect::LANG_PROJECTION_PREFIX),
            });
        }
    }
    out
}

/// The content of the first `"…"` literal in `text`.
fn first_string_literal(text: &str) -> Option<String> {
    let open = text.find('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in text[open + 1..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            _ => out.push(ch),
        }
    }
    None
}

/// The companion that keeps leg 2 from freezing an incomplete census.
///
/// It filters the derived minting sites through the SAME `is_gmn_dialect_path` predicate
/// leg 1 uses — NOT through "every path the fanout manifest carries", which would drag in
/// `tei.rs` / `nif.rs` / `conllu.rs` / `ontolex.rs` / `semaf.rs` / `bcp47.rs` and turn a
/// dialect gate into a whole-`lang:`-slice freeze.
#[test]
fn leg2_companion_every_gmn_dialect_path_producer_is_in_the_pinned_census() {
    let root = repo_root();

    // The single-prefix premise this derivation rests on, pinned rather than assumed.
    let projection_stage =
        std::fs::read_to_string(root.join("crates/pipeline/src/stages/lang_projection.rs"))
            .expect("the lang-projection stage is readable");
    assert!(
        projection_stage
            .contains("pub const LANG_PROJECTION_DIR: &str = \"generated/projections/lang\";"),
        "the committed lang-projection prefix moved — the path_suffix → committed-path \
         derivation below no longer resolves the family it claims to"
    );

    let produced = produced_lang_paths(&root);
    assert!(
        !produced.is_empty(),
        "no path_suffix minting site was derived at all — the completeness companion would \
         pass by examining nothing"
    );

    let dialect: BTreeSet<&ProducedPath> = produced
        .iter()
        .filter(|row| gmn_dialect::is_gmn_dialect_path(&row.path))
        .collect();
    println!("leg 2 companion: derived dialect minting sites {dialect:?}");
    assert!(
        !dialect.is_empty(),
        "the derivation found NO GMN-dialect minting site — the companion is vacuous"
    );

    // DISCRIMINATION, in both directions: the emitter of the grammar / GMN-1 families is
    // derived as a dialect producer, and the non-GMN `lang:` emitters are NOT (or the
    // "census" would be a whole-slice freeze that happened to pass).
    let dialect_sources: BTreeSet<&str> = dialect.iter().map(|row| row.source.as_str()).collect();
    assert!(
        dialect_sources.contains("crates/lang-bridge/src/registry.rs"),
        "the projection registry mints the grammar + GMN-1 families; got {dialect_sources:?}"
    );
    for neighbour in [
        "crates/lang-bridge/src/tei.rs",
        "crates/lang-bridge/src/nif.rs",
        "crates/lang-bridge/src/conllu.rs",
        "crates/lang-bridge/src/ontolex.rs",
        "crates/lang-bridge/src/semaf.rs",
        "crates/lang-bridge/src/bcp47.rs",
    ] {
        assert!(
            !dialect_sources.contains(neighbour),
            "{neighbour} mints only non-GMN lang projections, so sweeping it into the dialect \
             census would freeze the whole lang: slice under a dialect gate's name"
        );
    }

    let report = run(|r| check_producer_census_is_complete(&produced, r));
    assert!(report.is_clean(), "{report}");

    // The pin itself is SHRINK-ONLY, so an entry matching no live file must not red — but
    // it must not be silently useless either. Report the live coverage so a retirement
    // that leaves a stale row is visible.
    let sources = workspace_sources(&root);
    for pin in PINNED_GMN_DIALECT_PRODUCERS {
        let live = sources.iter().filter(|rel| rel.starts_with(pin)).count();
        println!("leg 2 pin {pin}: {live} live source file(s)");
    }
}

/// The red fixture for the completeness companion: the SAME derived set with one
/// unpinned file minting a dialect path.
#[test]
fn leg2_companion_red_fixture_an_unpinned_dialect_producer_reds() {
    let root = repo_root();
    let mut perturbed = produced_lang_paths(&root);
    perturbed.insert(ProducedPath {
        source: "crates/lang-bridge/src/tei.rs".to_string(),
        path: "generated/projections/lang/gmn1/v*/smuggled.gmn".to_string(),
    });
    let report = run(|r| check_producer_census_is_complete(&perturbed, r));
    assert!(
        !report.is_clean(),
        "an unpinned dialect minting site must red the completeness companion"
    );
    assert!(report.to_string().contains("tei.rs"), "{report}");
    assert!(
        report.to_string().contains("freezes the incompleteness"),
        "{report}"
    );
}

// ── Leg 4: the llms clause ───────────────────────────────────────────────────

/// Read one frozen file at the merge base, hard-failing on any git error.
fn base_text(root: &Path, base: &str, rel: &str) -> Option<String> {
    match git_show_base(root, base, rel) {
        BaseFile::Contents(text) => Some(text),
        BaseFile::Absent => None,
        BaseFile::Error(why) => panic!(
            "the llms shape freeze cannot obtain its comparand for {rel}, so it cannot perform \
             the comparison it is defined to perform: {why}"
        ),
    }
}

#[test]
fn leg4_the_llms_family_shape_is_frozen_against_the_merge_base() {
    let root = repo_root();
    let Some(base) = base_ref(&root) else { return };

    let mut checked = 0usize;
    for item in FROZEN_LLMS_SHAPE {
        let work = std::fs::read_to_string(root.join(item.path))
            .unwrap_or_else(|err| panic!("{}: unreadable on this branch: {err}", item.path));
        let Some(base_text) = base_text(&root, &base, item.base_lookup_path()) else {
            panic!(
                "{}: absent at the merge base. Every frozen llms-shape source predates this \
                 change; a missing comparand means the freeze list drifted from the tree",
                item.path
            );
        };
        // NON-VACUITY: the item must actually be found on both sides, or the comparison
        // would be between two `None`s.
        assert!(
            extract_item(&work, item.item).is_some(),
            "{}: {} was not found on this branch — the freeze list names an item the file no \
             longer has",
            item.path,
            item.item.label()
        );
        let report = run(|r| check_frozen_item(item, &base_text, &work, r));
        assert!(report.is_clean(), "{report}");
        checked += 1;
    }
    assert_eq!(
        checked,
        FROZEN_LLMS_SHAPE.len(),
        "every frozen llms-shape item must have been compared"
    );

    // The MCP consumer index: STRUCTURE frozen, list allowed to grow only by the delta the
    // ONTOLOGY licenses — one resource per `gmeow:` surface this change declares and the
    // merge base did not.
    let base_body = mcp_base_body(&root, &base);
    let work_body = mcp_work_body(&root);
    let surfaces = declared_surfaces(&root, &base);
    // NON-VACUITY, both sides: a derivation that read no term at all would license every
    // addition as "undeclared-but-unchecked" or refuse every one of them for the wrong
    // reason, and either way the permitted delta would be decided by nothing.
    assert!(
        !surfaces.working.is_empty() && !surfaces.base.is_empty(),
        "the declared-surface derivation read {} gmeow: term(s) on this branch and {} at the \
         merge base — the slice-module scan is vacuous",
        surfaces.working.len(),
        surfaces.base.len()
    );
    let report = run(|r| check_resource_list(&base_body, &work_body, &surfaces, r));
    assert!(report.is_clean(), "{report}");
    println!(
        "leg 4: {} resource(s) at the base, {} on this branch; the ontology declares {} \
         gmeow: term(s) here vs {} at the base, {} of them newly",
        llms_shape::resource_entries(&base_body).len(),
        llms_shape::resource_entries(&work_body).len(),
        surfaces.working.len(),
        surfaces.base.len(),
        surfaces.newly_declared().len()
    );
}

/// The acceptance criterion's first red fixture: reorder a section header.
///
/// It perturbs the LIVE working text of the real frozen item, so the failure arm is
/// reached through exactly the comparison the live assertion performs.
#[test]
fn leg4_red_fixture_reordering_a_section_header_reds() {
    let root = repo_root();
    let Some(base) = base_ref(&root) else { return };
    let item = FROZEN_LLMS_SHAPE
        .iter()
        .find(|item| item.item == ItemRef::Function("llms_sections"))
        .expect("the section-heading item is frozen");
    let work = std::fs::read_to_string(root.join(item.path)).expect("readable");
    let base_source =
        base_text(&root, &base, item.base_lookup_path()).expect("present at the base");

    // The live ordering is Classes, Properties, Individuals. Swap the first two.
    let reordered = work.replacen(
        "        section(\"Classes\", \"class\"),\n        section(\"Properties\", \"property\"),",
        "        section(\"Properties\", \"property\"),\n        section(\"Classes\", \"class\"),",
        1,
    );
    assert_ne!(
        reordered, work,
        "the red fixture must actually perturb the live section list — if this ever stops \
         matching, the fixture has gone vacuous and the leg is no longer demonstrably a gate"
    );
    let report = run(|r| check_frozen_item(item, &base_source, &reordered, r));
    assert!(
        !report.is_clean(),
        "a reordered section list must red the llms freeze"
    );
    assert!(report.to_string().contains("SHAPE moved"), "{report}");
}

/// The live MCP resource-list bodies at the base and on this branch.
fn mcp_bodies(root: &Path, base: &str) -> (String, String) {
    (mcp_base_body(root, base), mcp_work_body(root))
}

/// The complete consumer-index body at the merge base.
///
/// The index is assembled from the same contributor census on both sides of the comparison.
/// Reading only the working contributors while treating one base function as the whole index
/// would compare different surfaces after a contributor split landed.
fn mcp_base_body(root: &Path, base: &str) -> String {
    let mut uri_consts: BTreeMap<String, String> = BTreeMap::new();
    let mut bodies: Vec<String> = Vec::new();
    for (path, item) in MCP_RESOURCE_CONTRIBUTORS {
        let text = base_text(root, base, path)
            .unwrap_or_else(|| panic!("{path}: absent at the merge base"));
        uri_consts.extend(str_consts(&text));
        bodies.push(
            extract_item(&text, *item)
                .unwrap_or_else(|| panic!("{path}: {} is absent at the merge base", item.label())),
        );
    }
    resolve_mcp_consts(&uri_consts, &bodies)
}

/// Every contributing site's text, concatenated: the advertised surface is assembled from
/// several of them now, and reading one would grade a fragment as if it were the whole.
fn mcp_work_body(root: &Path) -> String {
    let mut uri_consts: BTreeMap<String, String> = BTreeMap::new();
    let mut bodies: Vec<String> = Vec::new();
    for (path, item) in MCP_RESOURCE_CONTRIBUTORS {
        let text = std::fs::read_to_string(root.join(path))
            .unwrap_or_else(|e| panic!("{path}: unreadable on this branch: {e}"));
        uri_consts.extend(str_consts(&text));
        bodies.push(
            extract_item(&text, *item)
                .unwrap_or_else(|| panic!("{path}: {} is absent on this branch", item.label())),
        );
    }
    resolve_mcp_consts(&uri_consts, &bodies)
}

/// Resolve URI constants before comparing the assembled base and working surfaces.
///
/// A URI may be named by a `const` because multiple hosts register the same descriptor.
/// Resolving the name back to its value makes the entry comparison see one surface rather than
/// a source-level refactor.
fn resolve_mcp_consts(uri_consts: &BTreeMap<String, String>, bodies: &[String]) -> String {
    let mut body = bodies.join("\n");
    for (name, value) in uri_consts {
        body = body.replace(name, &format!("\"{value}\""));
    }
    body
}

/// Every `const NAME: &str = "value";` in `text`, longest name first so a substitution never
/// eats a prefix of a longer name.
fn str_consts(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("pub const ")
            .or(line.strip_prefix("const "))
        else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(": &str = ") else {
            continue;
        };
        let value = rest.trim_end_matches(';').trim().trim_matches('"');
        if !value.is_empty() {
            out.push((name.trim().to_string(), value.to_string()));
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
    out
}

/// The entry every resource-list red fixture splices in front of, found by its URI rather
/// than by a pinned indentation: the list has already moved once from a method body to a free
/// function, and a fixture that dies on whitespace is a fixture that stops grading the rule.
const RESOURCE_ANCHOR_URI: &str = r#""gmeow://ontology/okf-index","#;

/// `work_body` with one extra `resource(...)` entry named `slug`, spliced at the anchor.
fn with_extra_resource(work_body: &str, slug: &str) -> String {
    let uri_at = work_body.find(RESOURCE_ANCHOR_URI).unwrap_or_else(|| {
        panic!("the red fixtures' anchor URI is gone; the live list is:\n{work_body}")
    });
    // Back up to the `resource(` that opens the anchored entry, and reuse ITS indentation so
    // the splice reads exactly like the entries around it.
    let open_at = work_body[..uri_at]
        .rfind("resource(")
        .expect("the anchor URI sits inside a resource(...) entry");
    let line_at = work_body[..open_at].rfind('\n').map_or(0, |nl| nl + 1);
    let indent = &work_body[line_at..open_at];
    let extra = format!(
        "{indent}resource(\n{indent}    \"gmeow://ontology/{slug}\",\n{indent}    \"{slug}\",\n\
         {indent}    \"A red fixture.\",\n{indent}    \"application/json\",\n{indent}),\n"
    );
    let grown = format!("{}{extra}{}", &work_body[..line_at], &work_body[line_at..]);
    assert_ne!(
        grown, work_body,
        "the red fixture must actually perturb the list"
    );
    grown
}

/// The acceptance criterion's second red fixture, re-aimed at the DERIVED rule: an MCP
/// resource that names no `gmeow:` term the ontology declares.
///
/// This is the arm the retired `uri.contains("medium")` rule could not have: it perturbs
/// the LIVE list and the LIVE declared-surface derivation, so the refusal comes from the
/// ontology having nothing to say about the added name rather than from a literal in the
/// gate.
#[test]
fn leg4_red_fixture_an_mcp_resource_with_no_declared_surface_reds() {
    let root = repo_root();
    let Some(base) = base_ref(&root) else { return };
    let (base_body, work_body) = mcp_bodies(&root, &base);
    let surfaces = declared_surfaces(&root, &base);

    // Derived, not assumed: the slug is only a valid fixture while the ontology genuinely
    // declares no term matching it.
    const SLUG: &str = "changelog";
    assert_eq!(
        surfaces.resolve(&format!("gmeow://ontology/{SLUG}")),
        SurfaceMatch::Undeclared,
        "the fixture slug {SLUG:?} now names a declared gmeow: term — pick one the ontology \
         does not declare, or the fixture proves nothing"
    );

    let report = run(|r| {
        check_resource_list(
            &base_body,
            &with_extra_resource(&work_body, SLUG),
            &surfaces,
            r,
        )
    });
    assert!(
        !report.is_clean(),
        "an MCP resource with no declared surface must red the derived delta"
    );
    assert!(
        report.to_string().contains("names NO gmeow: term"),
        "{report}"
    );
}

/// The third red fixture: an MCP resource that surfaces vocabulary the merge base ALREADY
/// declared.
///
/// The delta is the vocabulary THIS change adds, so a resource exposing a long-standing
/// term is a model-facing addition with nothing behind it. The term is read out of the
/// derivation itself rather than named here — a hardcoded term would rot the moment the
/// slice that declares it is renamed.
#[test]
fn leg4_red_fixture_an_mcp_resource_for_preexisting_vocabulary_reds() {
    let root = repo_root();
    let Some(base) = base_ref(&root) else { return };
    let (base_body, work_body) = mcp_bodies(&root, &base);
    let surfaces = declared_surfaces(&root, &base);

    // A base-declared term whose skeleton is unique among declared terms and is not
    // already an entry of the live list — the fixture must add a resource, not collide
    // with one.
    let live: BTreeSet<String> = llms_shape::resource_entries(&work_body)
        .iter()
        .map(|entry| entry.uri.clone())
        .collect();
    let slug = surfaces
        .base
        .iter()
        .map(|local| local.to_ascii_lowercase())
        .find(|slug| {
            !live.contains(&format!("gmeow://ontology/{slug}"))
                && matches!(
                    surfaces.resolve(&format!("gmeow://ontology/{slug}")),
                    SurfaceMatch::Preexisting(_)
                )
        })
        .expect("the merge base declares gmeow: vocabulary this fixture can surface");

    let report = run(|r| {
        check_resource_list(
            &base_body,
            &with_extra_resource(&work_body, &slug),
            &surfaces,
            r,
        )
    });
    assert!(
        !report.is_clean(),
        "an MCP resource surfacing pre-existing vocabulary ({slug}) must red the derived delta"
    );
    assert!(report.to_string().contains("ALREADY declared"), "{report}");
}

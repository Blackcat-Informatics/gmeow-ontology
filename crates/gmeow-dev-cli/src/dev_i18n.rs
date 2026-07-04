// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmeow-dev i18n {extract,sync-english,merge,export-csv,export-xliff}`
//! internationalization toolchain, over the native `gmeow_docs::i18n_compile`.

use std::path::{Path, PathBuf};

use gmeow_docs::i18n_compile;

use crate::dev_common::{fail, project_root};

/// `gmeow-dev i18n extract [--root --output-dir --lang --terms-only]`.
pub fn extract(
    root: Option<&Path>,
    output_dir: Option<&Path>,
    lang: Option<&str>,
    terms_only: bool,
) -> i32 {
    let root = root.map(Path::to_path_buf).unwrap_or_else(project_root);
    let output_dir = output_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("dist").join("i18n"));
    match i18n_compile::extract_catalog(&root, &output_dir, lang, terms_only) {
        Ok(report) => {
            println!(
                "wrote {} term catalog(s) ({} keys) to {}",
                report.groups,
                report.total_keys,
                output_dir.display()
            );
            0
        }
        Err(e) => fail(format!("i18n extract failed: {e}")),
    }
}

/// `gmeow-dev i18n sync-english [--root --dry-run]`.
pub fn sync_english(root: Option<&Path>, dry_run: bool) -> i32 {
    let root = root.map(Path::to_path_buf).unwrap_or_else(project_root);
    let mut po_files: Vec<PathBuf> = Vec::new();
    collect_po(&root.join("slices"), &mut po_files);
    po_files.sort();

    let mut changed: Vec<PathBuf> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut unchanged = 0usize;
    let mut processed = 0usize;

    for po_path in &po_files {
        let Some(slice_dir) = po_path.parent().and_then(Path::parent) else {
            continue;
        };
        let file_name = po_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let source_paths: Vec<PathBuf> = if file_name == "en.po" {
            vec![slice_dir.join("module.ttl"), slice_dir.join("manifest.ttl")]
        } else if file_name.ends_with(".md.po") {
            vec![slice_dir.join(&file_name[..file_name.len() - 3])]
        } else {
            continue;
        };
        for source_path in source_paths {
            if !source_path.is_file() {
                continue;
            }
            match i18n_compile::sync_english_file(po_path, &source_path, dry_run) {
                Ok(report) => {
                    processed += 1;
                    changed.extend(report.changed_files);
                    conflicts.extend(report.conflicts);
                    skipped.extend(report.skipped);
                    unchanged += report.unchanged.len();
                }
                Err(e) => return fail(format!("i18n sync-english failed: {e}")),
            }
        }
    }

    changed.sort();
    changed.dedup();
    for path in &changed {
        let status = if dry_run { "would change" } else { "changed" };
        println!("{status} {}", path.display());
    }
    for conflict in &conflicts {
        eprintln!("conflict {conflict}");
    }
    for skip in &skipped {
        eprintln!("skip {skip}");
    }
    if !conflicts.is_empty() {
        return fail(format!(
            "{} conflict(s), {} file(s) changed, {unchanged} unchanged, {} skipped",
            conflicts.len(),
            changed.len(),
            skipped.len()
        ));
    }
    let note = if dry_run { " (dry run)" } else { "" };
    println!(
        "{note} {processed} source(s) synced: {} changed, 0 conflicts, {} skipped, {unchanged} unchanged",
        changed.len(),
        skipped.len()
    );
    0
}

/// Recursively collect every `slices/**/i18n/*.po` file under `dir`.
fn collect_po(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_po(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("po")
            && path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("i18n")
        {
            out.push(path);
        }
    }
}

/// `gmeow-dev i18n merge [--root --output --lang]`.
pub fn merge(root: Option<&Path>, output: Option<&Path>, lang: Option<&str>) -> i32 {
    let root = root.map(Path::to_path_buf).unwrap_or_else(project_root);
    match i18n_compile::merge_terms(&root, output, lang) {
        Ok(report) => {
            if output.is_none() {
                print!("{}", report.turtle);
            }
            eprintln!(
                "merged {} PO file(s), {} translated triple(s) added -> {}",
                report.po_files, report.added, report.output_note
            );
            0
        }
        Err(e) => fail(format!("i18n merge failed: {e}")),
    }
}

/// `gmeow-dev i18n export-csv [--root --output]`.
pub fn export_csv(root: Option<&Path>, output: Option<&Path>) -> i32 {
    let root = root.map(Path::to_path_buf).unwrap_or_else(project_root);
    match i18n_compile::export_csv(&root, output) {
        Ok(text) => {
            if output.is_none() {
                print!("{text}");
            }
            0
        }
        Err(e) => fail(format!("i18n export-csv failed: {e}")),
    }
}

/// `gmeow-dev i18n export-xliff [--root --output]`.
pub fn export_xliff(root: Option<&Path>, output: Option<&Path>) -> i32 {
    let root = root.map(Path::to_path_buf).unwrap_or_else(project_root);
    match i18n_compile::export_xliff(&root, output) {
        Ok(text) => {
            if output.is_none() {
                print!("{text}");
            }
            0
        }
        Err(e) => fail(format!("i18n export-xliff failed: {e}")),
    }
}

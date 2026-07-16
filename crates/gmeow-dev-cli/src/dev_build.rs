// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Focused build commands outside the canonical `sync` workflow: fanout,
//! normalize, mappings, compile-gts, and release-bundle.

use std::path::{Path, PathBuf};

use gmeow_cli_core::ConsoleMode;
use gmeow_pipeline::fanout::fanout as run_fanout;
use gmeow_pipeline::run::{RunMode, run_full};

use crate::dev_common::{
    GTS_SNAPSHOT_REL, fail, project_root, reporter_for, resolve_console, resolve_jobs,
    write_timings_json,
};

/// `gmeow-dev fanout [-j --timings-json]` — project the flat tree out of gmeow.gts.
pub fn fanout(
    jobs: Option<usize>,
    timings_json: Option<&Path>,
    console: Option<ConsoleMode>,
) -> i32 {
    let jobs = match resolve_jobs(jobs) {
        Ok(j) => j,
        Err(code) => return code,
    };
    let root = project_root();
    let reporter = reporter_for(resolve_console(console));
    let report = match run_fanout(&root, jobs) {
        Ok(r) => r,
        Err(e) => return fail(format!("fanout failed: {e}")),
    };
    reporter.summary(&gmeow_errors::Report::new("fanout"));
    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "fanout",
            "produced": report.produced,
            "written": report.written,
            "skipped": report.skipped,
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    println!(
        "pipeline fanout: produced {}, written {}, unchanged {}",
        report.produced, report.written, report.skipped
    );
    0
}

/// `gmeow-dev normalize` — canonicalize the authored ontology sources.
pub fn normalize() -> i32 {
    let root = project_root();
    let prefixes = standard_prefixes();
    let mut modules: Vec<PathBuf> = Vec::new();
    collect_named(&root.join("slices"), "module.ttl", &mut modules);
    modules.sort();
    let mut changed = 0usize;
    for path in &modules {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        };
        let canonical =
            match gmeow_pipeline::cli_ops::confirmations::canonicalize_turtle(&bytes, &prefixes) {
                Ok(c) => c,
                Err(e) => return fail(format!("normalize {}: {e}", path.display())),
            };
        if canonical != bytes {
            if let Err(e) = std::fs::write(path, &canonical) {
                return fail(format!("cannot write {}: {e}", path.display()));
            }
            println!("normalized {}", path.display());
            changed += 1;
        }
    }
    if changed == 0 {
        println!("sources already canonical");
    }
    0
}

/// The standard GMEOW prefix bindings emitted into normalized Turtle.
fn standard_prefixes() -> Vec<(String, String)> {
    [
        ("gmeow", "https://blackcatinformatics.ca/gmeow/"),
        ("logic", "https://blackcatinformatics.ca/logic/"),
        ("math", "https://blackcatinformatics.ca/math/"),
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("sh", "http://www.w3.org/ns/shacl#"),
        ("xsd", "http://www.w3.org/2001/XMLSchema#"),
        ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ]
    .into_iter()
    .map(|(p, n)| (p.to_owned(), n.to_owned()))
    .collect()
}

/// Recursively collect every file named `name` under `dir`.
fn collect_named(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_named(&path, name, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            out.push(path);
        }
    }
}

/// `gmeow-dev mappings` — compile the alignment/linkset families from SSSOM.
pub fn mappings() -> i32 {
    let root = project_root();
    match gmeow_pipeline::cli_ops::confirmations::compile_mappings(&root) {
        Ok(_compiled) => {
            println!("mappings compiled");
            0
        }
        Err(e) => fail(format!("mappings failed: {e}")),
    }
}

/// `gmeow-dev compile-gts [-o --sign-key --public-key]` — reproduce (and
/// optionally sign) the committed dist snapshot.
pub fn compile_gts(out: Option<&Path>, sign_key: Option<&Path>, public_key: Option<&Path>) -> i32 {
    if sign_key.is_some() != public_key.is_some() {
        return fail("--sign-key and --public-key must be supplied together");
    }
    let root = project_root();
    let jobs = match resolve_jobs(None) {
        Ok(j) => j,
        Err(code) => return code,
    };
    // The pipeline (the build authority) folds the snapshot at its single gts_sink.
    if let Err(e) = run_full(&root, jobs, RunMode::Update) {
        return fail(format!("regenerate failed: {e}"));
    }
    let snapshot = root.join(GTS_SNAPSHOT_REL);
    let bytes = match std::fs::read(&snapshot) {
        Ok(b) => b,
        Err(e) => return fail(format!("cannot read {}: {e}", snapshot.display())),
    };

    if let (Some(sk), Some(pk)) = (sign_key, public_key) {
        let signed = match sign_snapshot(&bytes, sk, pk) {
            Ok(s) => s,
            Err(code) => return code,
        };
        let target = out.unwrap_or(&snapshot);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(target, &signed) {
            return fail(format!("cannot write {}: {e}", target.display()));
        }
        println!("{} ({} bytes, signed)", target.display(), signed.len());
        return 0;
    }

    if let Some(target) = out {
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(target, &bytes) {
            return fail(format!("cannot write {}: {e}", target.display()));
        }
        println!("{} ({} bytes)", target.display(), bytes.len());
    } else {
        println!("{} ({} bytes)", snapshot.display(), bytes.len());
    }
    0
}

/// Re-emit a folded snapshot with an embedded release transport key + signature.
fn sign_snapshot(bytes: &[u8], sign_key: &Path, public_key: &Path) -> Result<Vec<u8>, i32> {
    let secret_armor = std::fs::read_to_string(sign_key).map_err(|e| {
        fail(format!(
            "cannot read --sign-key {}: {e}",
            sign_key.display()
        ))
    })?;
    let public_armor = std::fs::read_to_string(public_key).map_err(|e| {
        fail(format!(
            "cannot read --public-key {}: {e}",
            public_key.display()
        ))
    })?;
    let signer = purrdf::gts::openpgp::parse_secret_signing_key(&secret_armor, None)
        .map_err(|e| fail(format!("cannot load signer: {e}")))?;
    let (signing_key, kid) = signer.into_parts();
    // Re-emit through the release fold with no added evidence: this signs the
    // committed snapshot content with the transport key embedded in metadata.
    gmeow_pipeline::stages::release::fold_release_bundle(
        bytes,
        Vec::new(),
        "https://blackcatinformatics.ca/gmeow/agent/release-lane",
        "1970-01-01T00:00:00Z",
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
        signing_key.to_bytes(),
        &kid,
        &public_armor,
    )
    .map_err(|e| fail(format!("signing failed: {e}")))
}

/// `gmeow-dev release-bundle …` — fold evidence into a SIGNED gmeow.gts.
#[allow(clippy::too_many_arguments)]
pub fn release_bundle(
    out: &Path,
    sign_key: &Path,
    public_key: &Path,
    source: &Path,
    issued_at: &str,
    attester: &str,
    release_subject: &str,
    evidence: &[String],
) -> i32 {
    let snapshot = match std::fs::read(source) {
        Ok(b) => b,
        Err(e) => {
            return fail(format!(
                "source snapshot {} is unreadable: {e}",
                source.display()
            ));
        }
    };
    let secret_armor = match std::fs::read_to_string(sign_key) {
        Ok(s) => s,
        Err(e) => {
            return fail(format!(
                "signing key {} is unreadable: {e}",
                sign_key.display()
            ));
        }
    };
    let public_armor = match std::fs::read_to_string(public_key) {
        Ok(s) => s,
        Err(e) => {
            return fail(format!(
                "public key {} is unreadable: {e}",
                public_key.display()
            ));
        }
    };
    let signer = match purrdf::gts::openpgp::parse_secret_signing_key(&secret_armor, None) {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot load signer: {e}")),
    };
    let (signing_key, kid) = signer.into_parts();

    let mut rows = Vec::with_capacity(evidence.len());
    for spec in evidence {
        match parse_evidence_spec(spec) {
            Ok(row) => rows.push(row),
            Err(code) => return code,
        }
    }

    let signed = match gmeow_pipeline::stages::release::fold_release_bundle(
        &snapshot,
        rows,
        attester,
        issued_at,
        release_subject,
        signing_key.to_bytes(),
        &kid,
        &public_armor,
    ) {
        Ok(s) => s,
        Err(e) => return fail(format!("release fold failed: {e}")),
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(out, &signed) {
        return fail(format!("cannot write {}: {e}", out.display()));
    }
    println!(
        "signed release bundle: {} ({} evidence artifact(s), {} bytes)",
        out.display(),
        evidence.len(),
        signed.len()
    );
    0
}

/// Parse one `path:media_type:attestation_type:rep:label` evidence spec, reading
/// the artifact file (HARD-fails on a missing/unreadable file, per §18).
fn parse_evidence_spec(spec: &str) -> Result<gmeow_pipeline::stages::release::EvidenceInput, i32> {
    // Split from the right: the four trailing metadata fields are colon-free, so
    // the path (which may itself contain a colon — a Windows drive letter, or a
    // URL-like local path) is the whole remainder and stays intact. `rsplitn`
    // yields the parts in reverse, hence the descending indices below.
    let parts: Vec<&str> = spec.rsplitn(5, ':').collect();
    if parts.len() != 5 {
        return Err(fail(format!(
            "malformed --evidence spec {spec:?}; expected path:media_type:attestation_type:rep:label"
        )));
    }
    let path = parts[4];
    let data = std::fs::read(path)
        .map_err(|e| fail(format!("evidence artifact {path:?} is unreadable: {e}")))?;
    Ok(gmeow_pipeline::stages::release::EvidenceInput {
        data,
        media_type: parts[3].to_owned(),
        attestation_type_iri: parts[2].to_owned(),
        rep: parts[1].to_owned(),
        subject_label: parts[0].to_owned(),
    })
}

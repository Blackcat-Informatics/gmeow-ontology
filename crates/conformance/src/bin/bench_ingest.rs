// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `bench-ingest` — convert a fetched Nemo-examples-style program tree into the
//! bench-corpus `nary-existential` layout the `gmeow-bench-engines` harness runs.
//!
//! # Input model
//!
//! A fetched Nemo-examples ChaseBench-family scenario is a directory tree containing
//! one or more Nemo `.rls` PROGRAMS whose n-ary EDB relations are pulled in by
//! `@import <rel> :- <format> { resource = "<file>" } .` directives (the resource is a
//! CSV / TSV, optionally gzip-compressed, path RELATIVE to the `.rls` file). This tool
//! discovers every `*.rls` under `--input` (a scenario per file, deterministic sorted
//! order), bounded by `--cap`, and for each writes one bench-corpus CASE directory:
//!
//! * `program.rls` — the rule text with `@import` / `@output` directives STRIPPED (the
//!   harness loads the EDB from `data/` by relation name, not through Nemo import
//!   resolution). `@prefix` / `@base` declarations and the rule bodies are preserved
//!   VERBATIM, so the same predicate surface drives both engines.
//! * `data/<rel>.csv[.tsv][.gz]` — each `@import`ed resource file, COPIED VERBATIM
//!   (bytes untouched, so a `.gz` stays gzipped) and named by the imported RELATION
//!   (the file stem the harness's [`gmeow_logic::nary_rls::load_nary_data_file`] keys
//!   the relation on). A `@import` whose resource cannot be resolved is a HARD FAIL
//!   (named), never a silently-dropped relation.
//! * `profile.json` — the synthesized `{ "fragment": "nary-existential",
//!   "engines": ["native", "nemo"] }`.
//! * `expected/result.json` — the reference golden. A fetched corpus has NO
//!   hand-derived golden, so the golden count is the Nemo REFERENCE reasoner's
//!   de-reified closure size over the converted program + EDB (Nemo is the established
//!   oracle). This makes the harness's native-vs-golden count check a REAL cross-engine
//!   comparison (not a native self-echo); the independent native-vs-nemo null-blind
//!   fingerprint check runs in the harness on top.
//!
//! One `corpus.json` ([`gmeow_conformance::external::corpus::CorpusMeta`]) is written at
//! `<out>/<corpus-name>/corpus.json` declaring the SPDX license, source URL, pinned
//! commit/tag, refresh command, and lane `b`. The harness's loader
//! ([`gmeow_conformance::bench_corpus::load_bench_corpora_from`]) audits that license
//! with the SAME [`gmeow_conformance::external::corpus::audit_vendorable`] gate the
//! committed corpora use — a non-vendorable license is a hard fail there.
//!
//! # Refusal discipline (no silent skip)
//!
//! Each converted program is validated through
//! [`gmeow_logic::nary_rls::parse_nary_rls_program`] BEFORE any output is written for
//! it, so a construct the native n-ary fragment refuses (a negated body literal, a body
//! operation, or a Skolem-FUNCTION existential — a non-range-restricted head argument
//! shared with no other head atom, the shape the ChaseBench `deep` / `doctors` rule sets
//! carry) HARD-FAILS here with the engine's named message. Nothing is silently dropped
//! or mis-lowered.

use std::path::{Path, PathBuf};

use serde_json::json;

use gmeow_errors::Diag;

use gmeow_conformance::error::{CorpusInvalid, Io, Vendor};

/// Wrap a bin-local condition as a typed diagnostic on the shared substrate.
fn be(detail: String) -> Diag {
    Diag::of_kind(Vendor { detail })
}

/// Parsed CLI arguments (all required; a missing flag is a hard fail — no defaults that
/// could silently mis-declare a corpus's license or provenance).
struct Args {
    input: PathBuf,
    out: PathBuf,
    corpus_name: String,
    cap: usize,
    source_url: String,
    commit: String,
    refresh: String,
    license: String,
}

fn parse_args() -> gmeow_errors::Result<Args> {
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut corpus_name: Option<String> = None;
    let mut cap: Option<usize> = None;
    let mut source_url: Option<String> = None;
    let mut commit: Option<String> = None;
    let mut refresh: Option<String> = None;
    let mut license: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut take = |flag: &str| -> gmeow_errors::Result<String> {
            args.next()
                .ok_or_else(|| be(format!("{flag} requires a value")))
        };
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(take("--input")?)),
            "--out" => out = Some(PathBuf::from(take("--out")?)),
            "--corpus-name" => corpus_name = Some(take("--corpus-name")?),
            "--cap" => {
                let v = take("--cap")?;
                cap = Some(
                    v.parse::<usize>()
                        .map_err(|e| be(format!("--cap must be a non-negative integer: {e}")))?,
                );
            }
            "--source-url" => source_url = Some(take("--source-url")?),
            "--commit" => commit = Some(take("--commit")?),
            "--refresh" => refresh = Some(take("--refresh")?),
            "--license" => license = Some(take("--license")?),
            other => return Err(be(format!("unknown argument: {other}"))),
        }
    }

    let req = |o: Option<String>, name: &str| -> gmeow_errors::Result<String> {
        o.ok_or_else(|| be(format!("missing required argument {name}")))
    };
    Ok(Args {
        input: input.ok_or_else(|| be("missing required argument --input".to_string()))?,
        out: out.ok_or_else(|| be("missing required argument --out".to_string()))?,
        corpus_name: req(corpus_name, "--corpus-name")?,
        cap: cap.ok_or_else(|| be("missing required argument --cap".to_string()))?,
        source_url: req(source_url, "--source-url")?,
        commit: req(commit, "--commit")?,
        refresh: req(refresh, "--refresh")?,
        license: req(license, "--license")?,
    })
}

fn main() -> gmeow_errors::Result<()> {
    let args = parse_args()?;

    if !args.input.is_dir() {
        return Err(be(format!(
            "--input {} is not a directory; expected a fetched Nemo-examples program tree",
            args.input.display()
        )));
    }

    // Discover every `.rls` scenario under --input, sorted for deterministic order.
    let mut scenarios = discover_rls(&args.input)?;
    scenarios.sort();
    if scenarios.is_empty() {
        return Err(be(format!(
            "no *.rls program found under {} — the fetched tree carries no scenario to \
             convert (nothing to do). Point --input at a Nemo-examples scenarios directory.",
            args.input.display()
        )));
    }
    scenarios.truncate(args.cap);

    // The single corpus directory: <out>/<corpus-name>/.
    let corpus_dir = args.out.join(&args.corpus_name);
    mkdir_all(&corpus_dir)?;

    let mut converted = 0usize;
    for rls_path in &scenarios {
        let case = scenario_case_name(&args.input, rls_path)?;
        convert_scenario(rls_path, &corpus_dir, &case)?;
        converted += 1;
    }

    // Write the one corpus.json (Apache-2.0 / lane b). Ordered keys via the serde_json
    // BTreeMap-backed Value, so the bytes are deterministic.
    let corpus_json = json!({
        "name": args.corpus_name,
        "spdx_license": args.license,
        "source_url": args.source_url,
        "version_or_commit": args.commit,
        "refresh_command": args.refresh,
        "lane": "b",
    });
    let corpus_text = serde_json::to_string_pretty(&corpus_json)
        .map_err(|e| be(format!("serializing corpus.json: {e}")))?
        + "\n";
    write_file(&corpus_dir.join("corpus.json"), corpus_text.as_bytes())?;

    println!(
        "bench-ingest: converted {converted} scenario(s) from {} into {} (corpus {:?}, lane b)",
        args.input.display(),
        corpus_dir.display(),
        args.corpus_name
    );
    Ok(())
}

/// Recursively collect every `*.rls` file under `root`.
fn discover_rls(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("reading {}: {e}", root.display()),
        })
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("dir entry: {e}"),
            })
        })?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(discover_rls(&path)?);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rls") {
            out.push(path);
        }
    }
    Ok(out)
}

/// A deterministic, filesystem-safe case name from the `.rls` path relative to `input`.
///
/// The relative path (minus the `.rls` extension) has its separators and any character
/// outside `[A-Za-z0-9._-]` mapped to `-`, with runs collapsed — so `programs/el/nemo/run.rls`
/// becomes `programs-el-nemo-run` and `deep/deep-100.rls` becomes `deep-deep-100`.
fn scenario_case_name(input: &Path, rls_path: &Path) -> gmeow_errors::Result<String> {
    let rel = rls_path.strip_prefix(input).unwrap_or(rls_path);
    let stem = rel.with_extension("");
    let raw = stem.to_str().ok_or_else(|| {
        be(format!(
            "scenario path {} is not valid UTF-8",
            rls_path.display()
        ))
    })?;
    let mut name = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            name.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            name.push('-');
            prev_dash = true;
        }
    }
    let name = name.trim_matches('-').to_string();
    if name.is_empty() {
        return Err(be(format!(
            "scenario path {} yields an empty case name",
            rls_path.display()
        )));
    }
    Ok(name)
}

/// One parsed `@import` directive: the imported relation name and its resource path
/// (relative to the `.rls` file).
struct Import {
    relation: String,
    resource: String,
}

/// Split a Nemo `.rls` PROGRAM into (kept program text, `@import` directives), stripping
/// every `@import` / `@output` statement. A statement runs from its `@import`/`@output`
/// keyword to the terminating `.`; every other line (rules, `@prefix`, `@base`, comments,
/// blanks) is preserved VERBATIM. A malformed `@import` (no relation / no `resource`) is a
/// hard fail.
fn split_program(rls: &str, case: &str) -> gmeow_errors::Result<(String, Vec<Import>)> {
    let mut kept = String::new();
    let mut imports = Vec::new();

    let mut pending: Option<String> = None; // Some(kind): "import" | "output"
    let mut buf = String::new();
    for line in rls.lines() {
        let trimmed = line.trim_start();
        if pending.is_none() {
            let kind = if trimmed.starts_with("@import") {
                Some("import")
            } else if trimmed.starts_with("@output") {
                Some("output")
            } else {
                None
            };
            match kind {
                None => {
                    kept.push_str(&normalize_comment_line(line));
                    kept.push('\n');
                }
                Some(k) => {
                    buf.clear();
                    buf.push_str(line);
                    if line.trim_end().ends_with('.') {
                        if k == "import" {
                            imports.push(parse_import(&buf, case)?);
                        }
                    } else {
                        pending = Some(k.to_string());
                    }
                }
            }
        } else {
            buf.push('\n');
            buf.push_str(line);
            if line.trim_end().ends_with('.') {
                let k = pending.take().expect("in-directive");
                if k == "import" {
                    imports.push(parse_import(&buf, case)?);
                }
            }
        }
    }
    if pending.is_some() {
        return Err(be(format!(
            "{case}: an @import/@output directive was not terminated by `.` before end of file"
        )));
    }
    Ok((kept, imports))
}

/// Normalize a KEPT comment line to a plain `%` line comment, preserving its content.
///
/// Nemo distinguishes a `%%%` DOC comment (which must attach to a following statement)
/// and a `%!` TOP-LEVEL comment from a plain `%` line comment. A doc comment that
/// documented a now-stripped `@import` would DANGLE (a parse error), so every kept
/// comment line has its leading run of `%` / `!` markers collapsed to a single `%`
/// (a line comment attaches to nothing and is always valid). Rule and `@prefix` lines
/// — whose first non-whitespace character is not `%` — are returned verbatim.
fn normalize_comment_line(line: &str) -> String {
    let ws_len = line.len() - line.trim_start().len();
    let (ws, rest) = line.split_at(ws_len);
    if !rest.starts_with('%') {
        return line.to_string();
    }
    let content = rest.trim_start_matches(['%', '!']);
    format!("{ws}%{content}")
}

/// Parse one `@import <rel> :- <fmt> { resource = "<path>" [ , ... ] } .` directive.
fn parse_import(text: &str, case: &str) -> gmeow_errors::Result<Import> {
    let after = text
        .trim_start()
        .strip_prefix("@import")
        .ok_or_else(|| be(format!("{case}: not an @import directive: {text:?}")))?;
    let (relation, _rest) = after
        .split_once(":-")
        .ok_or_else(|| be(format!("{case}: @import directive has no `:-`: {text:?}")))?;
    let relation = relation.trim().to_string();
    if relation.is_empty() {
        return Err(be(format!(
            "{case}: @import directive has an empty relation: {text:?}"
        )));
    }
    if relation.contains('/') || relation.contains(char::is_whitespace) {
        return Err(be(format!(
            "{case}: @import relation {relation:?} is not usable as a data-file stem \
             (contains a path separator or whitespace)"
        )));
    }
    let resource = extract_resource(text).ok_or_else(|| {
        be(format!(
            "{case}: @import directive has no `resource = \"...\"`: {text:?}"
        ))
    })?;
    Ok(Import { relation, resource })
}

/// Extract the first `resource = "<value>"` string from an import directive body.
fn extract_resource(text: &str) -> Option<String> {
    let idx = text.find("resource")?;
    let after_key = &text[idx + "resource".len()..];
    let eq = after_key.find('=')?;
    let after_eq = &after_key[eq + 1..];
    let start = after_eq.find('"')?;
    let rest = &after_eq[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Convert one scenario `.rls` into a bench-corpus case directory under `corpus_dir`.
fn convert_scenario(rls_path: &Path, corpus_dir: &Path, case: &str) -> gmeow_errors::Result<()> {
    let rls = std::fs::read_to_string(rls_path).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("reading {}: {e}", rls_path.display()),
        })
    })?;
    let (program, imports) = split_program(&rls, case)?;

    if imports.is_empty() {
        return Err(be(format!(
            "{case}: the program {} declares no @import EDB relation — an nary-existential \
             scenario must carry at least one imported n-ary relation",
            rls_path.display()
        )));
    }

    // Validate the stripped program through the native n-ary fragment BEFORE writing any
    // output for this scenario: a refused construct (negation / body operation /
    // Skolem-function existential) hard-fails here with the engine's named message.
    gmeow_logic::nary_rls::parse_nary_rls_program(&program).map_err(|e| {
        be(format!(
            "{case}: the native n-ary engine refuses this scenario's program: {e}"
        ))
    })?;

    let case_dir = corpus_dir.join(case);
    let data_dir = case_dir.join("data");
    mkdir_all(&data_dir)?;

    let rls_parent = rls_path.parent().unwrap_or_else(|| Path::new("."));
    let mut written = 0usize;
    for imp in &imports {
        let src = rls_parent.join(&imp.resource);
        if !src.is_file() {
            return Err(be(format!(
                "{case}: @import relation {:?} resource {:?} resolves to {}, which does not \
                 exist — the fetched tree is missing this EDB file (never a silent skip). The \
                 ChaseBench data for this scenario may not be committed to the Apache-2.0 \
                 Nemo-examples packaging (some sizes carry only a README pointing at the \
                 unlicensed dbunibas/chasebench).",
                imp.relation,
                imp.resource,
                src.display()
            )));
        }
        let ext = data_extension(&imp.resource).ok_or_else(|| {
            be(format!(
                "{case}: @import relation {:?} resource {:?} has an unrecognized extension \
                 (expected .csv / .tsv, optionally .gz)",
                imp.relation, imp.resource
            ))
        })?;
        let bytes = std::fs::read(&src).map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("reading {}: {e}", src.display()),
            })
        })?;
        // An imported relation whose data file is EMPTY (e.g. Galen has no role chains, so
        // subPropChain is empty) is legitimately empty. The n-ary loader refuses a zero-row
        // relation, so the converter OMITS it (an empty relation seeds no fact, so omitting
        // its file is semantically inert) — announced here, never silently dropped.
        if resource_is_empty(&bytes, &imp.resource, case)? {
            println!(
                "bench-ingest: {case}: relation {:?} ({}) is empty — omitting (an empty EDB \
                 relation contributes no fact)",
                imp.relation, imp.resource
            );
            continue;
        }
        write_file(&data_dir.join(format!("{}{ext}", imp.relation)), &bytes)?;
        written += 1;
    }
    if written == 0 {
        return Err(be(format!(
            "{case}: every @import EDB relation resolved to an EMPTY file — an nary-existential \
             scenario must carry at least one non-empty n-ary relation"
        )));
    }

    write_file(&case_dir.join("program.rls"), program.as_bytes())?;
    write_file(
        &case_dir.join("profile.json"),
        b"{ \"fragment\": \"nary-existential\", \"engines\": [\"native\", \"nemo\"] }\n",
    )?;

    // Reference golden: reload the written EDB exactly as the harness does, then run the
    // Nemo reference reasoner over the same program + EDB and record its de-reified
    // closure size as the golden count keyed by a synthesized world IRI.
    let golden_rows = nemo_reference_rows(&data_dir, &program, case)?;
    let world = format!("https://blackcatinformatics.ca/gmeow/bench/{}", case);
    let expected = json!({ world: { "rows": golden_rows } });
    let expected_text = serde_json::to_string_pretty(&expected)
        .map_err(|e| be(format!("{case}: serializing expected/result.json: {e}")))?
        + "\n";
    mkdir_all(&case_dir.join("expected"))?;
    write_file(
        &case_dir.join("expected").join("result.json"),
        expected_text.as_bytes(),
    )?;

    Ok(())
}

/// Whether an imported resource's data is empty (no non-blank record line). Gzip data
/// (`.gz`) is transparently decoded first.
fn resource_is_empty(bytes: &[u8], resource: &str, case: &str) -> gmeow_errors::Result<bool> {
    let decoded: Vec<u8> = if resource.to_ascii_lowercase().ends_with(".gz") {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut out)
            .map_err(|e| be(format!("{case}: cannot gunzip resource {resource:?}: {e}")))?;
        out
    } else {
        bytes.to_vec()
    };
    let text = String::from_utf8_lossy(&decoded);
    Ok(text.lines().all(|l| l.trim().is_empty()))
}

/// The bench-corpus data-file extension for a resource path (`.csv` / `.tsv`, optionally
/// `.gz`), or `None` for an unrecognized extension.
fn data_extension(resource: &str) -> Option<&'static str> {
    let lower = resource.to_ascii_lowercase();
    if lower.ends_with(".csv.gz") {
        Some(".csv.gz")
    } else if lower.ends_with(".tsv.gz") {
        Some(".tsv.gz")
    } else if lower.ends_with(".csv") {
        Some(".csv")
    } else if lower.ends_with(".tsv") {
        Some(".tsv")
    } else {
        None
    }
}

/// Load the written `data/` EDB (as the harness does) and run the Nemo reference reasoner
/// over `program`, returning the de-reified closure tuple count.
fn nemo_reference_rows(data_dir: &Path, program: &str, case: &str) -> gmeow_errors::Result<u64> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(data_dir)
        .map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("reading {}: {e}", data_dir.display()),
            })
        })?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut edb: Vec<gmeow_logic::nary::NaryTuple> = Vec::new();
    for path in &files {
        let tuples = gmeow_logic::nary_rls::load_nary_data_file(path).map_err(|e| {
            Diag::of_kind(CorpusInvalid {
                detail: format!("{case}: {} — {e}", path.display()),
            })
        })?;
        edb.extend(tuples);
    }

    let closure = gmeow_logic::nary::run_nemo_nary_forward(&edb, program).map_err(|e| {
        be(format!(
            "{case}: computing the Nemo reference golden failed: {e}"
        ))
    })?;
    Ok(closure.len() as u64)
}

/// Create a directory and every missing parent (hard-fail on I/O error).
fn mkdir_all(dir: &Path) -> gmeow_errors::Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("creating {}: {e}", dir.display()),
        })
    })
}

/// Write a file, creating parents as needed (hard-fail on I/O error).
fn write_file(path: &Path, bytes: &[u8]) -> gmeow_errors::Result<()> {
    if let Some(parent) = path.parent() {
        mkdir_all(parent)?;
    }
    std::fs::write(path, bytes).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("writing {}: {e}", path.display()),
        })
    })
}

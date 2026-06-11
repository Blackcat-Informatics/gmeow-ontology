// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! The `gts` command-line tool: inspect, fold, verify, and compose GTS files.
//!
//! `cat` and `verify` implement the §14.1 composition-tooling contract: raw
//! byte concatenation is always valid GTS (§3.1), but a publish-class tool
//! refuses pathological states instead of trusting them to be intentional.
//!
//! Exit codes: 0 clean; 1 diagnostics found or input refused; 2 usage/IO error.

use std::collections::HashSet;
use std::process::ExitCode;

use ciborium::value::Value;
use gts::model::{Graph, Suppression};
use gts::nquads::to_nquads;
use gts::reader::{read, read_file_segments, FileSegments};
use gts::wire::{digest_str, hex};

const USAGE: &str = "usage: gts <command> [args]

commands:
  info <file>...            per-segment composition ledger (§14.1)
  fold <file>               fold to N-Quads on stdout
  verify <file>...          verify chains; ledger + diagnostics; exit 1 on any
  ls <file>                 list inline blobs: digest, size, declared media type
  extract <file> <digest> [-o out] [--mt TYPE] [--include-suppressed]
                            extract one blob by content digest; --mt asserts
                            the declared media type (never converts)
  cat -o <out> <file>...    validating composer: refuse degenerate inputs,
                            then byte-concatenate (§3.1, §14.1)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "info" => cmd_info(&args[1..]),
        "fold" => cmd_fold(&args[1..]),
        "verify" => cmd_verify(&args[1..]),
        "ls" => cmd_ls(&args[1..]),
        "extract" => cmd_extract(&args[1..]),
        "cat" => cmd_cat(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("gts: unknown command '{other}'\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn load(path: &str) -> Result<Vec<u8>, ExitCode> {
    std::fs::read(path).map_err(|e| {
        eprintln!("gts: cannot read {path}: {e}");
        ExitCode::from(2)
    })
}

/// Print the per-segment composition ledger (§14.1 "SHOULD report").
fn print_ledger(path: &str, fs: &FileSegments) {
    println!(
        "{path}: {} segment(s){}",
        fs.segments.len(),
        match fs.torn {
            Some(off) => format!(", TORN at byte {off}"),
            None => String::new(),
        }
    );
    if let Some(fatal) = &fs.fatal {
        println!("  FATAL {}: {}", fatal.code, fatal.detail);
        return;
    }
    for (idx, seg) in fs.segments.iter().enumerate() {
        let head = seg
            .segment_heads
            .first()
            .map(|h| hex(h))
            .unwrap_or_else(|| "<none>".to_string());
        let profile = seg
            .segment_profiles
            .first()
            .map(String::as_str)
            .unwrap_or("<none>");
        let signers = seg
            .signatures
            .iter()
            .filter(|s| s.status != "invalid")
            .count();
        println!(
            "  segment {idx}: head {head} profile {profile} terms {} quads {} \
             reifies {} annot {} blobs {} suppress {} opaque {} sigs {signers}",
            seg.terms.len(),
            seg.quads.len(),
            seg.reifiers.len(),
            seg.annotations.len(),
            seg.blobs.len(),
            seg.suppressions.len(),
            seg.opaque.len(),
        );
        for o in &seg.opaque {
            println!("    opaque: {} ({})", o.frame_type, o.reason);
        }
        for d in &seg.diagnostics {
            println!(
                "    diagnostic {}: {}{}",
                d.code,
                d.detail,
                match d.frame_index {
                    Some(i) => format!(" [item {i}]"),
                    None => String::new(),
                }
            );
        }
    }
}

fn has_problems(fs: &FileSegments) -> bool {
    fs.fatal.is_some() || fs.torn.is_some() || fs.segments.iter().any(|s| !s.diagnostics.is_empty())
}

fn cmd_info(paths: &[String]) -> ExitCode {
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    for path in paths {
        let data = match load(path) {
            Ok(d) => d,
            Err(code) => return code,
        };
        print_ledger(path, &read_file_segments(&data));
    }
    ExitCode::SUCCESS
}

fn cmd_fold(paths: &[String]) -> ExitCode {
    let [path] = paths else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let data = match load(path) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let g = read(&data, true, None);
    for d in &g.diagnostics {
        eprintln!("gts: diagnostic {}: {}", d.code, d.detail);
    }
    print!("{}", to_nquads(&g));
    if g.segment_heads.is_empty() {
        // never reached segmentation — empty file or no leading header
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn cmd_verify(paths: &[String]) -> ExitCode {
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let mut problems = false;
    for path in paths {
        let data = match load(path) {
            Ok(d) => d,
            Err(code) => return code,
        };
        let fs = read_file_segments(&data);
        print_ledger(path, &fs);
        if has_problems(&fs) {
            problems = true;
        }
    }
    if problems {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn blob_mt(g: &Graph, digest: &str) -> Option<String> {
    g.blob_meta
        .iter()
        .find(|(d, _)| d == digest)
        .and_then(|(_, meta)| {
            if let Value::Map(entries) = meta {
                entries.iter().find_map(|(k, v)| match (k, v) {
                    (Value::Text(key), Value::Text(text)) if key == "mt" => Some(text.clone()),
                    _ => None,
                })
            } else {
                None
            }
        })
}

/// List inline blobs: digest, size, declared media type (tar's `t`).
fn cmd_ls(paths: &[String]) -> ExitCode {
    let [path] = paths else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let data = match load(path) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let g = read(&data, true, None);
    for d in &g.diagnostics {
        eprintln!("gts: diagnostic {}: {}", d.code, d.detail);
    }
    for (digest, bytes) in &g.blobs {
        let mt = blob_mt(&g, digest).unwrap_or_else(|| "-".to_string());
        println!("{digest}  {:>10}  {mt}", bytes.len());
    }
    ExitCode::SUCCESS
}

fn normalize_digest(digest: &str) -> String {
    if digest.starts_with("blake3:") {
        digest.to_string()
    } else {
        format!("blake3:{digest}")
    }
}

/// Digests hidden by `{"kind": "blob", "digest": …}` targets (§11).
fn suppressed_blob_digests(g: &Graph) -> HashSet<String> {
    let mut out = HashSet::new();
    for sup in &g.suppressions {
        for target in &sup.targets {
            if target_kind(target) != "blob" {
                continue;
            }
            if let Value::Map(entries) = target {
                for (k, v) in entries {
                    if matches!(k, Value::Text(t) if t == "digest") {
                        match v {
                            Value::Bytes(b) => {
                                out.insert(format!("blake3:{}", hex(b)));
                            }
                            Value::Text(t) => {
                                out.insert(normalize_digest(t));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    out
}

/// Extract one blob by content digest (tar's `x`), refuse-don't-trust:
/// verify bytes against the digest, honour §11 suppression unless overridden,
/// and treat `--mt` as an ASSERTION against the declared media type — never
/// a conversion.
fn cmd_extract(args: &[String]) -> ExitCode {
    let mut out_path: Option<&str> = None;
    let mut mt: Option<&str> = None;
    let mut include_suppressed = false;
    let mut positional: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--out" => match it.next() {
                Some(p) => out_path = Some(p),
                None => {
                    eprintln!("gts: -o requires a path\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--mt" => match it.next() {
                Some(m) => mt = Some(m),
                None => {
                    eprintln!("gts: --mt requires a media type\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--include-suppressed" => include_suppressed = true,
            other => positional.push(other),
        }
    }
    let [path, digest] = positional[..] else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let data = match load(path) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let g = read(&data, true, None);
    let digest = normalize_digest(digest);
    let Some((_, bytes)) = g.blobs.iter().find(|(d, _)| *d == digest) else {
        eprintln!("gts: no inline blob {digest} in {path}");
        return ExitCode::from(1);
    };
    if !include_suppressed && suppressed_blob_digests(&g).contains(&digest) {
        eprintln!(
            "gts: refusing {digest}: suppressed (§11); pass \
             --include-suppressed to extract anyway"
        );
        return ExitCode::from(1);
    }
    if let Some(asserted) = mt {
        let declared = blob_mt(&g, &digest);
        if declared.as_deref() != Some(asserted) {
            eprintln!(
                "gts: refusing {digest}: declared media type {declared:?} \
                 does not match asserted {asserted:?}"
            );
            return ExitCode::from(1);
        }
    }
    if digest_str(bytes) != digest {
        eprintln!("gts: integrity failure: {digest} bytes re-hash differently");
        return ExitCode::from(1);
    }
    match out_path {
        Some(p) => {
            if let Err(e) = std::fs::write(p, bytes) {
                eprintln!("gts: cannot write {p}: {e}");
                return ExitCode::from(2);
            }
        }
        None => {
            use std::io::Write;
            if let Err(e) = std::io::stdout().write_all(bytes) {
                eprintln!("gts: cannot write stdout: {e}");
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
}

/// The validating composer (§14.1): refuse-don't-trust, then `cat`.
fn cmd_cat(args: &[String]) -> ExitCode {
    let mut out_path: Option<&str> = None;
    let mut inputs: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-o" {
            match it.next() {
                Some(p) => out_path = Some(p),
                None => {
                    eprintln!("gts: -o requires a path\n{USAGE}");
                    return ExitCode::from(2);
                }
            }
        } else {
            inputs.push(a);
        }
    }
    if inputs.len() < 2 {
        eprintln!("gts: cat needs at least two inputs\n{USAGE}");
        return ExitCode::from(2);
    }

    let mut combined: Vec<u8> = Vec::new();
    for path in &inputs {
        let data = match load(path) {
            Ok(d) => d,
            Err(code) => return code,
        };
        let fs = read_file_segments(&data);
        if has_problems(&fs) {
            eprintln!("gts: refusing {path}: not a clean GTS input");
            print_ledger(path, &fs);
            return ExitCode::from(1);
        }
        // §14.1: a segment that contributes NOTHING (no quads, blobs,
        // reifier bindings, annotations, or suppressions) is almost always a
        // wiring bug — never a real package. Refuse, don't trust.
        for (idx, seg) in fs.segments.iter().enumerate() {
            let contributes = !seg.quads.is_empty()
                || !seg.blobs.is_empty()
                || !seg.reifiers.is_empty()
                || !seg.annotations.is_empty()
                || !seg.suppressions.is_empty();
            if !contributes {
                eprintln!(
                    "gts: refusing {path}: segment {idx} folds to nothing \
                     (no quads/blobs/reifies/annot/suppress) — wiring bug?"
                );
                return ExitCode::from(1);
            }
        }
        combined.extend_from_slice(&data);
    }

    // §14.1: refuse an output in which suppressions would hide every prior
    // frame — a composition that suppresses the whole graph is a mistake.
    let folded = read(&combined, true, None);
    if all_quads_suppressed(&folded) {
        eprintln!(
            "gts: refusing composition: suppressions hide every quad in the \
             folded output"
        );
        return ExitCode::from(1);
    }

    match out_path {
        Some(p) => {
            if let Err(e) = std::fs::write(p, &combined) {
                eprintln!("gts: cannot write {p}: {e}");
                return ExitCode::from(2);
            }
        }
        None => {
            use std::io::Write;
            if let Err(e) = std::io::stdout().write_all(&combined) {
                eprintln!("gts: cannot write stdout: {e}");
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
}

fn target_idx(target: &Value, key: &str) -> Option<usize> {
    let Value::Map(entries) = target else {
        return None;
    };
    let v = entries
        .iter()
        .find(|(k, _)| matches!(k, Value::Text(t) if t == key))
        .map(|(_, v)| v)?;
    if let Value::Integer(i) = v {
        usize::try_from(i128::from(*i)).ok()
    } else {
        None
    }
}

fn target_kind(target: &Value) -> &str {
    if let Value::Map(entries) = target {
        for (k, v) in entries {
            if matches!(k, Value::Text(t) if t == "kind") {
                if let Value::Text(t) = v {
                    return t;
                }
            }
        }
    }
    ""
}

/// True iff the folded graph has quads and EVERY one is hidden by a
/// suppression (a direct quad target, or a term target on any component).
fn all_quads_suppressed(g: &Graph) -> bool {
    if g.quads.is_empty() || g.suppressions.is_empty() {
        return false;
    }
    let mut term_sup: HashSet<usize> = HashSet::new();
    let mut quad_sup: HashSet<Vec<usize>> = HashSet::new();
    for sup in &g.suppressions {
        collect_suppressed(sup, &mut term_sup, &mut quad_sup);
    }
    g.quads.iter().all(|&(s, p, o, gq)| {
        let key = match gq {
            Some(gv) => vec![s, p, o, gv],
            None => vec![s, p, o],
        };
        quad_sup.contains(&key)
            || term_sup.contains(&s)
            || term_sup.contains(&p)
            || term_sup.contains(&o)
            || gq.is_some_and(|gv| term_sup.contains(&gv))
    })
}

fn collect_suppressed(
    sup: &Suppression,
    term_sup: &mut HashSet<usize>,
    quad_sup: &mut HashSet<Vec<usize>>,
) {
    for target in &sup.targets {
        match target_kind(target) {
            "term" | "reifier" => {
                if let Some(id) = target_idx(target, "id") {
                    term_sup.insert(id);
                }
            }
            "quad" => {
                if let Value::Map(entries) = target {
                    let q = entries
                        .iter()
                        .find(|(k, _)| matches!(k, Value::Text(t) if t == "q"))
                        .map(|(_, v)| v);
                    if let Some(Value::Array(ids)) = q {
                        let key: Option<Vec<usize>> = ids
                            .iter()
                            .map(|x| {
                                if let Value::Integer(i) = x {
                                    usize::try_from(i128::from(*i)).ok()
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if let Some(key) = key {
                            quad_sup.insert(key);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

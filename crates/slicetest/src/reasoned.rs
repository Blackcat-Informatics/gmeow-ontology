// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The merged ontology closed under OWL 2 RL — the graph competency questions
//! run over.
//!
//! T2 measured every competency exemplar's expected answer against the merged
//! ontology closed under OWL 2 RL (`gmeow_tools.native_rl.native_rl_closure`),
//! the same reasoned-graph lane `tests/test_competency.py` uses. The harness
//! reproduces that EXACTLY in Rust: it merges the same source-set
//! (`ontology/gmeow.ttl` + every `slices/*/*/module.ttl`, imports excluded),
//! runs the native [`rl_closure`] chase once, and serializes the closure back
//! to N-Triples.
//!
//! ## Caching — one chase, not one-per-test-process
//!
//! The Nemo chase is the expensive step (this is the same chase the Python
//! competency suite already pays today). nextest runs each test in its own
//! process, so a process [`OnceLock`] alone would re-chase for every
//! `competency.ttl` file. Two tiers avoid that:
//!
//! * a process [`OnceLock`] memoizes the result within one test process, and
//! * a **content-addressed disk cache** memoizes it across processes and runs:
//!   the cache key is the SHA-256 of the source-set bytes mixed with the test
//!   binary's own mtime, so a change to the ontology OR to the reasoning crate
//!   (which rebuilds the binary) invalidates it. The whole `make check` pays at
//!   most one chase regardless of how many slices carry competency specs.
//!
//! Caching the N-Triples *string* (which is `Sync`) rather than an oxigraph
//! `Store` keeps the static trivially shareable; each cell rebuilds a fresh
//! queryable store from the cached triples (cheap next to the chase).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use oxigraph::store::Store;
use sha2::{Digest, Sha256};

use gmeow_logic::reason::{rl_closure, RlClosure};
use gmeow_rdf::oxigraph::OxigraphStore;
use gmeow_validate::store::{build_store, build_store_from_nt};

use crate::paths;

/// Memoized N-Triples of the reasoned merged ontology (built once per process).
static REASONED_NT: OnceLock<Result<String, String>> = OnceLock::new();

/// The reasoned merged ontology as N-Triples, computed once and cached.
///
/// # Errors
///
/// Returns the (cloned) build error if assembling or closing the merged
/// ontology failed.
pub fn reasoned_nt() -> Result<&'static str, String> {
    match REASONED_NT.get_or_init(build_reasoned_nt) {
        Ok(nt) => Ok(nt.as_str()),
        Err(e) => Err(e.clone()),
    }
}

/// A fresh, SPARQL-queryable oxigraph store of the reasoned merged ontology.
///
/// Rebuilt from the cached [`reasoned_nt`] on each call — cheap relative to the
/// one-time RL chase.
///
/// # Errors
///
/// Returns `Err(String)` if the cached closure is an error or the N-Triples
/// fails to re-ingest.
pub fn reasoned_store() -> Result<Store, String> {
    build_store_from_nt(reasoned_nt()?)
}

/// Assemble the merged ontology, close it under OWL 2 RL, and serialize to NT,
/// reading from (and populating) the content-addressed disk cache.
fn build_reasoned_nt() -> Result<String, String> {
    let sources = source_files()?;
    let cache_path = cache_path(&sources)?;

    if let Some(path) = &cache_path {
        if let Ok(cached) = std::fs::read_to_string(path) {
            if !cached.is_empty() {
                return Ok(cached);
            }
        }
    }

    let merged = build_store(&sources).map_err(|e| format!("merging ontology sources: {e}"))?;
    let closure =
        rl_closure(&OxigraphStore::new(&merged)).map_err(|e| format!("OWL 2 RL closure: {e}"))?;
    let nt = closure_to_ntriples(&closure);

    if let Some(path) = &cache_path {
        // Best-effort: a cache write failure must never fail the test run.
        write_atomic(path, &nt);
    }
    Ok(nt)
}

/// The reasoned source-set: `ontology/gmeow.ttl` followed by every slice module,
/// imports excluded — identical to `iter_source_files(include_imports=False)`.
fn source_files() -> Result<Vec<PathBuf>, String> {
    let mut files = vec![paths::repo_root().join("ontology/gmeow.ttl")];
    files.extend(module_files()?);
    Ok(files.into_iter().filter(|f| f.is_file()).collect())
}

/// Every `slices/<group>/<name>/module.ttl`, in sorted (deterministic) order.
fn module_files() -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for group in sorted_subdirs(&paths::slices_root())? {
        for slice in sorted_subdirs(&group)? {
            let module = slice.join("module.ttl");
            if module.is_file() {
                out.push(module);
            }
        }
    }
    Ok(out)
}

/// Immediate subdirectories of `dir`, sorted by path for determinism.
fn sorted_subdirs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// Serialize an [`RlClosure`] to N-Triples.
///
/// `RlTriple::subject`/`predicate` are bare IRI strings (blank nodes were
/// skolemized to IRIs during the chase) and `object` is ALREADY in N-Triples
/// object form (`<iri>`, `"v"`, `"v"@lang`, `"v"^^<dt>`) — so each triple is a
/// straight `<s> <p> o .` line. The lenient parser re-ingests the private-use
/// `@x-gmeow-*` language tags.
fn closure_to_ntriples(closure: &RlClosure) -> String {
    let mut nt = String::new();
    for t in &closure.triples {
        nt.push('<');
        nt.push_str(&t.subject);
        nt.push_str("> <");
        nt.push_str(&t.predicate);
        nt.push_str("> ");
        nt.push_str(&t.object);
        nt.push_str(" .\n");
    }
    nt
}

// ── Disk cache ──────────────────────────────────────────────────────────────────

/// The cache file for the current source-set, or `None` if the key cannot be
/// derived (in which case the harness simply recomputes — caching is an
/// optimization, never a correctness dependency).
fn cache_path(sources: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let mut hasher = Sha256::new();
    for path in sources {
        let bytes = std::fs::read(path).map_err(|e| format!("hashing {}: {e}", path.display()))?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(path.to_string_lossy().as_bytes());
    }
    // Mix the test binary's mtime so a rebuild of the reasoning crate (which
    // rebuilds this binary) invalidates a cache that the source bytes alone
    // would not.
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(mtime) = std::fs::metadata(&exe).and_then(|m| m.modified()) {
            hasher.update(format!("{mtime:?}").as_bytes());
        }
    }
    let key = hex(&hasher.finalize());
    let dir = std::env::temp_dir().join("gmeow-slicetest-reasoned");
    if std::fs::create_dir_all(&dir).is_err() {
        return Ok(None);
    }
    Ok(Some(dir.join(format!("{key}.nt"))))
}

/// Write `contents` to `path` atomically (temp file + rename); best-effort.
fn write_atomic(path: &Path, contents: &str) {
    let Some(parent) = path.parent() else { return };
    let tmp = parent.join(format!(
        "{}.tmp.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("cache"),
        std::process::id()
    ));
    if std::fs::write(&tmp, contents).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_logic::reason::RlTriple;

    #[test]
    fn closure_serializes_iri_and_literal_objects() {
        // A synthetic closure exercises the NT serialization WITHOUT paying the
        // chase: an IRI object and a typed-literal object (already in NT form).
        let closure = RlClosure {
            triples: vec![
                RlTriple {
                    subject: "https://blackcatinformatics.ca/gmeow/roleAuthor".to_owned(),
                    predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    object: "<https://blackcatinformatics.ca/gmeow/ContributionRole>".to_owned(),
                    world: "default".to_owned(),
                    is_edb: true,
                    rule_name: None,
                },
                RlTriple {
                    subject: "https://blackcatinformatics.ca/gmeow/x".to_owned(),
                    predicate: "http://www.w3.org/2000/01/rdf-schema#label".to_owned(),
                    object: "\"hi\"@x-gmeow-english".to_owned(),
                    world: "default".to_owned(),
                    is_edb: true,
                    rule_name: None,
                },
            ],
        };
        let nt = closure_to_ntriples(&closure);
        // The serialized NT must re-ingest leniently (private lang tag included).
        let store = build_store_from_nt(&nt).expect("serialized closure must re-ingest");
        assert_eq!(store.len().expect("len"), 2);
        assert!(nt.contains("<https://blackcatinformatics.ca/gmeow/roleAuthor> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/ContributionRole> ."));
        assert!(nt.contains("\"hi\"@x-gmeow-english"));
    }
}

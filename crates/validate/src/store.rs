// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free Turtle parsing for the validation lints.
//!
//! The syntax check and the `owl:sameAs` ban scan overlapping source files. Both
//! only need parse success plus a per-quad scan, so this module parses each
//! Turtle file with oxigraph directly (the same parser the SHACL engine uses)
//! rather than building short-lived rdflib graphs.
//!
//! Parsing is **lenient** (`.lenient()`), preserving the legacy behavior of
//! pyoxigraph's default `parse()` exactly (#579 is a no-behavior-change port).
//! The real GMEOW ontology carries private-use `@x-gmeow-*` language tags whose
//! subtag exceeds BCP-47's 8-char limit (e.g. `@x-gmeow-afrikaans`); the strict
//! parser rejects the whole file on these (`imports/languages-reference.ttl`),
//! which would make the ontology un-syntax-checkable. Leniency skips that one
//! check while still surfacing every real Turtle syntax error — the same stance
//! the SHACL engine takes for the same files (#597).

use std::path::{Path, PathBuf};

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, NamedOrBlankNode, Quad, Term};
use oxigraph::store::{SerializerError, Store};

use crate::model::owl;

/// Parse a single Turtle file, returning either its quads or a syntax-error
/// string.
///
/// The error string is the `Display` form of the underlying parse error,
/// matching what the legacy pyoxigraph `parse()` raised as an exception. The
/// caller decides how to frame it (`syntax error in {path}: {err}` etc.).
///
/// # Errors
///
/// Returns `Err(message)` if the file cannot be read or the Turtle fails to
/// parse.
pub fn parse_file(path: &Path) -> Result<Vec<Quad>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut quads: Vec<Quad> = Vec::new();
    for triple in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes.as_slice())
    {
        let triple = triple.map_err(|e| e.to_string())?;
        quads.push(triple);
    }
    Ok(quads)
}

/// Parse every Turtle file in `paths` into one oxigraph [`Store`].
///
/// Lenient parsing (matching [`parse_file`]): a malformed file aborts with an
/// error naming the file, but the private-use `@x-gmeow-*` language tags are
/// accepted. This is the multi-file ingestion primitive future lint ports build
/// on; the current syntax/sameAs lints scan files individually via [`parse_file`]
/// so they can attribute every diagnostic to its source file.
///
/// # Errors
///
/// Returns `Err(message)` if any file fails to read or parse.
pub fn build_store(paths: &[PathBuf]) -> Result<Store, String> {
    load_sources_into_store(paths)
}

/// Alias for [`build_store`] used by the validation orchestration.
///
/// Loads every Turtle source in `paths` into a single oxigraph [`Store`] using
/// lenient parsing. See [`build_store`] for details.
pub fn load_sources_into_store(paths: &[PathBuf]) -> Result<Store, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for path in paths {
        let bytes =
            std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(bytes.as_slice())
        {
            let triple = triple.map_err(|e| format!("syntax error in {}: {e}", path.display()))?;
            store
                .insert(&triple)
                .map_err(|e| format!("store insert failed for {}: {e}", path.display()))?;
        }
    }
    Ok(store)
}

/// Render a subject term the way the legacy Python `_ox_term_display` did:
/// NamedNode → its value; BlankNode → `_:b`.
///
/// A triple subject is exactly an IRI or a blank node ([`NamedOrBlankNode`]);
/// the legacy Python `str(term)` fallback was unreachable, so there is no
/// catch-all here.
pub fn subject_display(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
        NamedOrBlankNode::BlankNode(b) => format!("_:{}", b.as_str()),
    }
}

/// Scan `quads` for Principle 5 `owl:sameAs`-to-external-entity violations.
///
/// A violation is every `owl:sameAs` triple whose object is a NamedNode that
/// does NOT start with `namespace`, unless `(subject_display, object)` is in
/// `allowlist`. Returns the `(subject_display, object)` pair for each violation,
/// in document order — the caller frames the user-facing message so the file
/// path can be interpolated exactly as the Python lint does.
pub fn sameas_violations(
    quads: &[Quad],
    namespace: &str,
    allowlist: &[(String, String)],
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for quad in quads {
        if quad.predicate.as_ref() != owl::SAME_AS {
            continue;
        }
        let obj = match &quad.object {
            Term::NamedNode(n) => n.as_str(),
            _ => continue,
        };
        if obj.starts_with(namespace) {
            continue;
        }
        let subject_text = subject_display(&quad.subject);
        if allowlist
            .iter()
            .any(|(s, o)| s == &subject_text && o == obj)
        {
            continue;
        }
        out.push((subject_text, obj.to_owned()));
    }
    out
}

/// Build an oxigraph [`Store`] from an N-Triples document.
///
/// The validation-path SHACL data and the reasoning test shims pass graphs as
/// N-Triples strings (the rdflib-free seam, #579) rather than as rdflib graphs.
/// N-Triples is a strict subset of the Turtle family, so the same lenient parser
/// [`build_store`] uses ingests it directly. Parsing is lenient for the same
/// reason (private-use `@x-gmeow-*` language tags).
///
/// # Errors
///
/// Returns `Err(message)` if the N-Triples fails to parse.
pub fn build_store_from_nt(data_nt: &str) -> Result<Store, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for triple in RdfParser::from_format(RdfFormat::NTriples)
        .lenient()
        .for_reader(data_nt.as_bytes())
    {
        let triple = triple.map_err(|e| format!("N-Triples parse error: {e}"))?;
        store
            .insert(&triple)
            .map_err(|e| format!("store insert failed: {e}"))?;
    }
    Ok(store)
}

/// Serialize a [`Store`]'s default graph to canonical N-Triples text.
///
/// Uses oxigraph's own N-Triples serializer (no hand-rolled literal escaping),
/// the same primitive the SHACL report uses. This is the rdflib-free replacement
/// for `rdflib.Graph.serialize(format="nt")` on the validation path (#579):
/// `merge_to_ntriples` builds the store from the Turtle sources and dumps it so
/// the SHACL data graph never touches rdflib.
///
/// # Errors
///
/// Returns `Err(SerializerError)` if the oxigraph serializer fails (e.g., an
/// unexpected `Storage` error from the underlying store).
pub fn dump_store_to_ntriples(store: &Store) -> Result<String, SerializerError> {
    let mut buf: Vec<u8> = Vec::new();
    store.dump_graph_to_writer(GraphNameRef::DefaultGraph, RdfFormat::NTriples, &mut buf)?;
    // SAFETY: The W3C N-Triples spec mandates US-ASCII output; oxigraph
    // escapes all non-ASCII codepoints, so the byte buffer is valid UTF-8.
    Ok(String::from_utf8(buf).expect("oxigraph N-Triples output is guaranteed UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    #[test]
    fn parse_file_rejects_bad_turtle() {
        let path = write_tmp("gmeow_validate_store_bad.ttl", "this is not turtle <<< @@@");
        let result = parse_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "malformed Turtle must parse-error");
    }

    #[test]
    fn parse_file_accepts_good_turtle() {
        let path = write_tmp(
            "gmeow_validate_store_good.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let result = parse_file(&path);
        std::fs::remove_file(&path).ok();
        let quads = result.expect("well-formed Turtle must parse");
        assert_eq!(quads.len(), 1);
    }

    #[test]
    fn build_store_loads_multiple_files() {
        let a = write_tmp(
            "gmeow_validate_store_multi_a.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let b = write_tmp(
            "gmeow_validate_store_multi_b.ttl",
            "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
        );
        let store = build_store(&[a.clone(), b.clone()]).expect("both files must load");
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
        assert_eq!(store.len().unwrap(), 2);
    }

    #[test]
    fn sameas_flags_external_object() {
        let path = write_tmp(
            "gmeow_validate_store_sameas_ext.ttl",
            "@prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a owl:sameAs ex:b .\n",
        );
        let quads = parse_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let violations = sameas_violations(&quads, NS, &[]);
        assert_eq!(
            violations,
            vec![(
                "https://example.org/a".to_owned(),
                "https://example.org/b".to_owned()
            )]
        );
    }

    #[test]
    fn sameas_skips_internal_and_allowlisted() {
        let path = write_tmp(
            "gmeow_validate_store_sameas_skip.ttl",
            &format!(
                "@prefix gmeow: <{NS}> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                 gmeow:A owl:sameAs gmeow:B .\n\
                 ex:a owl:sameAs ex:b .\n"
            ),
        );
        let quads = parse_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let allowlist = vec![(
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned(),
        )];
        assert!(sameas_violations(&quads, NS, &allowlist).is_empty());
    }

    #[test]
    fn nt_round_trips_through_store() {
        let nt = "<https://example.org/a> <https://example.org/p> <https://example.org/b> .\n";
        let store = build_store_from_nt(nt).expect("valid N-Triples must load");
        assert_eq!(store.len().unwrap(), 1);
        let dumped =
            dump_store_to_ntriples(&store).expect("in-memory store serialization must succeed");
        // oxigraph emits the same single triple (whitespace-normalized).
        assert!(dumped.contains("<https://example.org/a>"));
        assert!(dumped.contains("<https://example.org/p>"));
        assert!(dumped.contains("<https://example.org/b>"));
        // Re-ingesting the dump yields the identical triple count.
        let store2 = build_store_from_nt(&dumped).expect("dump must re-load");
        assert_eq!(store2.len().unwrap(), 1);
    }

    #[test]
    fn nt_rejects_malformed() {
        assert!(build_store_from_nt("this is not n-triples @@@").is_err());
    }
}

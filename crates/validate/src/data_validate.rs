// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-free Tier-1 conformance of an external RDF data file against the bundled
//! ontology's SHACL shapes and OntoUML disciplines.
//!
//! Where [`crate::validate_all`] is the slice-authoring dev gate (structural and
//! naming lint, example coverage, DSL phases) run over the repository sources,
//! this is the *consumer* path: it takes an arbitrary RDF data graph plus a
//! `gmeow.gts` bundle and runs only the two Tier-1 engines a downstream user
//! cares about —
//!
//! 1. **SHACL** against the data-graph shape union carried in the bundle's
//!    `shapes-archive` blob (every committed `shapes/*.ttl` and
//!    `generated/shapes/*.ttl` plus every per-slice `shapes.ttl`, minus the four
//!    DSL/manifest lint shapes that only target authoring sources, not the data
//!    graph); and
//! 2. the six **gUFO/OntoUML disciplines** ([`crate::gufo::reasoning_invariants`]).
//!
//! No reasoner runs here — full consistency is the separate `--deep` pass. The
//! bundle is the only input besides the data file, so the path is repo-free and
//! Docker-free: an installed wheel carrying the folded `gmeow.gts` is sufficient.
//!
//! The data-graph shape *selection* is authoritative here in Rust (the bundle
//! reader untars `shapes-archive` and applies the exclusion set) rather than in
//! the Python CLI surface, which passes only raw bytes.

use gmeow_diagnostics::Report;
use oxigraph::store::Store;

use crate::gufo::{self, GufoConfig};
use crate::store;
use crate::validate_all::{build_report, shacl_findings_from_report};

/// The blob `rep` label under which the snapshot stage folds the full SHACL shape
/// surface (`shapes-archive`). MUST match the writer in the pipeline snapshot
/// stage and the Python `bundle` reader.
const REP_SHAPES: &str = "shapes-archive";

/// Shape files in `shapes/` that lint *authoring* sources (the DSL modules and the
/// per-slice manifests), not the published data graph. They are excluded from the
/// data-graph validator union — exactly mirroring the dev validator's own
/// composition — so validating a user's instance graph never trips manifest- or
/// DSL-only constraints.
const NON_DATA_GRAPH_SHAPES: [&str; 4] = [
    "mapping-dsl-shapes.ttl",
    "statement-dsl-shapes.ttl",
    "test-dsl-shapes.ttl",
    "slice-manifest-shapes.ttl",
];

/// Run Tier-1 conformance of `data_bytes` (an RDF graph in `data_format`) against
/// the shapes and disciplines carried in `gts_bytes`.
///
/// `data_format` is a media type or short format id understood by
/// [`gmeow_rdf::parse_dataset`] (`turtle`/`ttl`, `trig`, `n-triples`/`nt`,
/// `n-quads`/`nq`, `rdf+xml`) or the JSON-LD ids `json-ld`/`jsonld`. `namespace`
/// is the GMEOW IRI prefix the discipline checks key on. `origin` is the data
/// file's display path, recorded as each SHACL finding's physical location so
/// SARIF `artifactLocation.uri` points at the user's file.
///
/// The data graph is validated in isolation (no ontology merge): every shape is
/// self-contained (`sh:targetClass` + constraints), so direct `rdf:type`
/// assertions resolve without the TBox, and the finding set reflects only the
/// user's graph. Named graphs in TriG/N-Quads are flattened to the default graph
/// so the shapes see every triple.
///
/// # Errors
///
/// Returns `Err` if the bundle carries no `shapes-archive` blob, the archive is
/// malformed, the shapes fail to parse, or the data graph fails to parse.
pub fn run(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    namespace: &str,
    origin: &str,
) -> Result<Report, String> {
    let shapes_ttl = data_graph_shapes_from_gts(gts_bytes)?;
    let store = data_store(data_bytes, data_format)?;

    let shapes = gmeow_shacl::engine::parse_shapes(&shapes_ttl)
        .map_err(|e| format!("bundled SHACL shapes failed to parse: {e}"))?;
    let shacl_report = gmeow_shacl::engine::validate(&store, &shapes);
    let shacl_findings = shacl_findings_from_report(&shacl_report, Some(origin));

    let cfg = GufoConfig {
        namespace: namespace.to_owned(),
    };
    let discipline_errors = gufo::reasoning_invariants(&store, &cfg);

    Ok(build_report(discipline_errors, Vec::new(), shacl_findings))
}

/// Build an in-memory oxigraph store from external RDF data bytes, flattening any
/// named graphs into the default graph so the shapes and discipline checks see the
/// whole graph.
fn data_store(data_bytes: &[u8], data_format: &str) -> Result<Store, String> {
    use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};

    if is_json_ld(data_format) {
        // JSON-LD has no native-codec media type; route it through the gmeow-gts
        // JSON-LD codec to GTS bytes, then fold to a flattened store.
        let text = std::str::from_utf8(data_bytes)
            .map_err(|e| format!("data file is not valid UTF-8: {e}"))?;
        let gts = gmeow_gts::from_yamlld::from_json_ld(text)
            .map_err(|e| format!("JSON-LD parse error: {e}"))?;
        let graph = store::read_gts_graph(&gts)?;
        return store::build_store_from_graph(&graph);
    }

    let dataset = gmeow_rdf::parse_dataset(data_bytes, data_format, None)
        .map_err(|e| format!("data graph parse error: {e}"))?;
    store_from_dataset(&dataset, GraphPolicy::FlattenToDefaultGraph).map_err(|e| e.to_string())
}

/// True for the JSON-LD format ids (handled outside the native-codec router).
fn is_json_ld(format: &str) -> bool {
    let f = format.trim().to_ascii_lowercase();
    matches!(
        f.as_str(),
        "json-ld" | "jsonld" | "application/ld+json" | "ld+json"
    )
}

/// Extract and assemble the data-graph SHACL shape union (one Turtle document)
/// from the bundle's `shapes-archive` blob.
fn data_graph_shapes_from_gts(gts_bytes: &[u8]) -> Result<String, String> {
    let mut graph = store::read_gts_graph(gts_bytes)?;

    // Resolve the digest of the blob declared with rep == "shapes-archive".
    // `blob_meta` values are CBOR maps (`ciborium::value::Value::Map`); read the
    // `rep` text field rather than indexing a JSON object.
    let digest = graph
        .blob_meta
        .iter()
        .find(|(_, meta)| cbor_text_field(meta, "rep") == Some(REP_SHAPES))
        .map(|(d, _)| d.clone())
        .ok_or_else(|| {
            format!("bundle carries no `{REP_SHAPES}` blob — cannot validate repo-free")
        })?;

    // Decode the blob bytes (forcing a lazy entry if the fold deferred it).
    let entry = graph
        .blobs
        .iter_mut()
        .find(|(d, _)| *d == digest)
        .map(|(_, e)| e)
        .ok_or_else(|| format!("`{REP_SHAPES}` blob metadata present but bytes missing"))?;
    let tar = entry
        .decode()
        .map_err(|e| format!("`{REP_SHAPES}` blob decode error: {e}"))?
        .to_vec();

    let mut members = untar(&tar)?;
    // Deterministic concatenation order regardless of archive member order.
    members.sort_by(|a, b| a.0.cmp(&b.0));

    let mut ttl = String::new();
    let mut included = 0usize;
    for (name, bytes) in &members {
        if !name.ends_with(".ttl") {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(name);
        if NON_DATA_GRAPH_SHAPES.contains(&base) {
            continue;
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|e| format!("shape `{name}` is not valid UTF-8: {e}"))?;
        ttl.push_str(text);
        ttl.push('\n');
        included += 1;
    }

    if included == 0 {
        return Err(format!(
            "`{REP_SHAPES}` blob held no data-graph shapes — the bundle is incomplete"
        ));
    }
    Ok(ttl)
}

/// Minimal reader for the byte-deterministic USTAR archive the snapshot stage
/// writes: per-member 512-byte header + 512-padded body, terminated by zero
/// blocks. Handles the GNU `'L'` (`LongLink`) record the writer emits for member
/// names longer than 100 bytes. Returns regular-file members as `(name, bytes)`.
fn untar(tar: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut i = 0usize;
    let mut long_name: Option<String> = None;

    while i + 512 <= tar.len() {
        let header = &tar[i..i + 512];
        if header.iter().all(|&b| b == 0) {
            break; // trailing zero block(s) — end of archive
        }
        let typeflag = header[156];
        let size = parse_octal(&header[124..136])
            .ok_or_else(|| "USTAR archive: unreadable size field".to_string())?;
        i += 512;
        let body_end = i
            .checked_add(size)
            .filter(|end| *end <= tar.len())
            .ok_or_else(|| "USTAR archive: member body overruns archive".to_string())?;
        let body = &tar[i..body_end];
        // Advance past the 512-padded body.
        i = body_end + (512 - size % 512) % 512;

        match typeflag {
            b'L' => {
                // GNU LongLink: the body is the full path, NUL-terminated.
                let name = String::from_utf8_lossy(body)
                    .trim_end_matches('\0')
                    .to_string();
                long_name = Some(name);
            }
            b'0' | 0 => {
                let name = long_name.take().unwrap_or_else(|| {
                    let nb = &header[0..100];
                    let end = nb.iter().position(|&b| b == 0).unwrap_or(nb.len());
                    String::from_utf8_lossy(&nb[..end]).to_string()
                });
                out.push((name, body.to_vec()));
            }
            _ => {
                // Non-file records (other than LongLink) are not emitted by the
                // writer; skip defensively without consuming a pending long name.
            }
        }
    }
    Ok(out)
}

/// Read a text-valued field out of a CBOR map (`ciborium::value::Value::Map`),
/// matching the string key `key`. Returns `None` for a non-map value or a
/// missing/non-text field.
fn cbor_text_field<'a>(meta: &'a ciborium::value::Value, key: &str) -> Option<&'a str> {
    let ciborium::value::Value::Map(entries) = meta else {
        return None;
    };
    for (k, v) in entries {
        if let ciborium::value::Value::Text(name) = k {
            if name == key {
                if let ciborium::value::Value::Text(text) = v {
                    return Some(text.as_str());
                }
                return None;
            }
        }
    }
    None
}

/// Parse a NUL/space-padded octal USTAR numeric field.
fn parse_octal(field: &[u8]) -> Option<usize> {
    let s: String = field
        .iter()
        .map(|&b| b as char)
        .take_while(|c| *c != '\0' && *c != ' ')
        .collect();
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    usize::from_str_radix(trimmed, 8).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_octal_reads_zero_padded_nul_terminated_field() {
        // The snapshot writer emits right-justified, zero-padded octal + NUL.
        let mut field = [0u8; 12];
        let octal = b"00000000142"; // 0o142 == 98
        field[..octal.len()].copy_from_slice(octal);
        assert_eq!(parse_octal(&field), Some(0o142));
    }

    #[test]
    fn parse_octal_empty_field_is_zero() {
        assert_eq!(parse_octal(&[0u8; 12]), Some(0));
    }

    #[test]
    fn is_json_ld_matches_ids_and_media_type() {
        assert!(is_json_ld("json-ld"));
        assert!(is_json_ld("jsonld"));
        assert!(is_json_ld("application/ld+json"));
        assert!(is_json_ld("  JSON-LD  "));
        assert!(!is_json_ld("turtle"));
        assert!(!is_json_ld("application/json"));
    }

    #[test]
    fn cbor_text_field_reads_rep_label() {
        use ciborium::value::Value;
        let meta = Value::Map(vec![
            (
                Value::Text("mt".into()),
                Value::Text("application/x-tar".into()),
            ),
            (
                Value::Text("rep".into()),
                Value::Text("shapes-archive".into()),
            ),
        ]);
        assert_eq!(cbor_text_field(&meta, "rep"), Some("shapes-archive"));
        assert_eq!(cbor_text_field(&meta, "absent"), None);
        assert_eq!(cbor_text_field(&Value::Null, "rep"), None);
    }

    #[test]
    fn untar_round_trips_a_minimal_archive() {
        // A 1-record USTAR archive: header (name + octal size + '0' typeflag) +
        // 512-padded body + two trailing zero blocks. Mirrors the snapshot writer.
        let name = b"shapes/x.ttl";
        let body = b"@prefix ex: <https://example.org/> .\n";
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name);
        let size_field = format!("{:011o}\0", body.len());
        header[124..136].copy_from_slice(size_field.as_bytes());
        header[156] = b'0';

        let mut tar = Vec::new();
        tar.extend_from_slice(&header);
        tar.extend_from_slice(body);
        tar.extend(std::iter::repeat_n(0u8, (512 - body.len() % 512) % 512));
        tar.extend(std::iter::repeat_n(0u8, 1024));

        let members = untar(&tar).expect("untar");
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].0, "shapes/x.ttl");
        assert_eq!(members[0].1, body);
    }
}

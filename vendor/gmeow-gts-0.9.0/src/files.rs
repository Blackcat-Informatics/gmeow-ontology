// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Files-profile pack/unpack/diff logic for GTS archives (§13.2, §14.2).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use ciborium::value::Value;

use crate::model::{Graph, TermKind};
use crate::writer::{digest_string, Writer};

const FILES_NS: &str = "https://w3id.org/gts/files#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

fn iri_term(value: &str) -> crate::model::Term {
    crate::model::Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn literal_term(value: &str, datatype: Option<usize>) -> crate::model::Term {
    crate::model::Term {
        kind: TermKind::Literal,
        value: Some(value.to_string()),
        datatype,
        lang: None,
        reifier: None,
    }
}

fn bnode_term(label: &str) -> crate::model::Term {
    crate::model::Term {
        kind: TermKind::Bnode,
        value: Some(label.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn safe_archive_path(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("empty archive path".to_string());
    }
    let normalized = name.replace('\\', "/");
    let drive_relative =
        name.len() >= 2 && name.as_bytes()[1] == b':' && name.as_bytes()[0].is_ascii_alphabetic();
    if drive_relative || normalized.starts_with('/') {
        return Err(format!(
            "absolute or drive-relative path not allowed in archive: {name}"
        ));
    }
    let parts: Vec<&str> = normalized.split('/').collect();
    for part in &parts {
        if *part == ".." {
            return Err(format!("path traversal not allowed in archive: {name}"));
        }
    }
    if name.contains('\\') {
        return Err(format!(
            "backslash path separator not allowed in archive: {name}"
        ));
    }
    if parts.iter().any(|part| part.is_empty() || *part == ".") {
        return Err(format!(
            "empty or current-directory path component not allowed in archive: {name}"
        ));
    }
    Ok(())
}

fn to_posix_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for c in path.components() {
        let s = c
            .as_os_str()
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 path component in {path:?}"))?;
        parts.push(s.to_string());
    }
    Ok(parts.join("/"))
}

fn walk_dir_sorted(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    fn recurse(out: &mut Vec<std::path::PathBuf>, dir: &Path) -> Result<(), std::io::Error> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|a| a.file_name());
        for entry in entries {
            let path = entry.path();
            let ftype = entry.file_type()?;
            if ftype.is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("symlink not supported: {}", path.display()),
                ));
            }
            if ftype.is_dir() {
                recurse(out, &path)?;
            } else if ftype.is_file() {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    recurse(&mut out, dir).map_err(|e| format!("walk {dir:?}: {e}"))?;
    Ok(out)
}

fn resolve_sources(sources: &[&Path]) -> Result<Vec<(std::path::PathBuf, String)>, String> {
    let mut entries: Vec<(std::path::PathBuf, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for src in sources {
        let meta = fs::symlink_metadata(src).map_err(|e| format!("{src:?}: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!("symlink not supported: {}", src.display()));
        }
        if meta.is_file() {
            let name = src
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("invalid source name: {src:?}"))?
                .to_string();
            safe_archive_path(&name)?;
            if !seen.insert(name.clone()) {
                return Err(format!("duplicate archive path: {name}"));
            }
            entries.push((src.to_path_buf(), name));
        } else if meta.is_dir() {
            let files = walk_dir_sorted(src)?;
            for fspath in files {
                let relpath = to_posix_path(
                    fspath
                        .strip_prefix(src)
                        .map_err(|_| format!("path outside source: {fspath:?}"))?,
                )?;
                safe_archive_path(&relpath)?;
                if !seen.insert(relpath.clone()) {
                    return Err(format!("duplicate archive path: {relpath}"));
                }
                entries.push((fspath, relpath));
            }
        } else {
            return Err(format!("unsupported source type: {src:?}"));
        }
    }
    entries.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(entries)
}

fn guess_media_type(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("txt") => "text/plain".to_string(),
        Some("html") | Some("htm") => "text/html".to_string(),
        Some("json") => "application/json".to_string(),
        Some("xml") => "application/xml".to_string(),
        Some("png") => "image/png".to_string(),
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("pdf") => "application/pdf".to_string(),
        Some("zip") => "application/zip".to_string(),
        Some("gz") => "application/gzip".to_string(),
        Some("tar") => "application/x-tar".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// Pack files/directories into a deterministic GTS files-profile archive.
pub fn pack(sources: &[&Path]) -> Result<Vec<u8>, String> {
    let mut w = Writer::new("files");

    let shared = vec![
        iri_term(&(FILES_NS.to_string() + "FileEntry")),
        iri_term(&(FILES_NS.to_string() + "path")),
        iri_term(&(FILES_NS.to_string() + "digest")),
        iri_term(&(FILES_NS.to_string() + "size")),
        iri_term(&(FILES_NS.to_string() + "mode")),
        iri_term(&(FILES_NS.to_string() + "modified")),
        iri_term(&(FILES_NS.to_string() + "mediaType")),
        iri_term(RDF_TYPE),
        iri_term(XSD_INTEGER),
        iri_term(XSD_DATETIME),
    ];
    w.add_terms(&shared);
    let file_entry_id: usize = 0;
    let path_id: usize = 1;
    let digest_id: usize = 2;
    let size_id: usize = 3;
    let mode_id: usize = 4;
    let modified_id: usize = 5;
    let media_type_id: usize = 6;
    let type_id: usize = 7;
    let xsd_integer_id: usize = 8;
    let xsd_datetime_id: usize = 9;

    let entries = resolve_sources(sources)?;

    let mut file_terms: Vec<crate::model::Term> = Vec::new();
    let mut quads: Vec<crate::model::Quad> = Vec::new();
    let mut blobs: Vec<(Vec<u8>, String, String)> = Vec::new();

    for (idx, (fspath, relpath)) in entries.iter().enumerate() {
        let data = fs::read(fspath).map_err(|e| format!("read {fspath:?}: {e}"))?;
        let digest = digest_string(&data);
        let meta = fs::metadata(fspath).map_err(|e| format!("stat {fspath:?}: {e}"))?;
        let size = meta.len();
        #[cfg(unix)]
        let mode = std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o7777;
        #[cfg(not(unix))]
        let mode = 0o644u32;
        let mtime = meta
            .modified()
            .map_err(|e| format!("mtime {fspath:?}: {e}"))?;
        let mt = guess_media_type(fspath);

        let entry_label = format!("f{idx}");
        let entry_term = bnode_term(&entry_label);
        let path_term = literal_term(relpath, None);
        let digest_term = literal_term(&digest, None);
        let size_term = literal_term(&size.to_string(), Some(xsd_integer_id));
        let mode_term = literal_term(&mode.to_string(), Some(xsd_integer_id));
        let modified_text =
            format_datetime(&mtime).map_err(|e| format!("datetime {fspath:?}: {e}"))?;
        let modified_term = literal_term(&modified_text, Some(xsd_datetime_id));
        let media_term = literal_term(&mt, None);

        let base = shared.len() + file_terms.len();
        file_terms.extend(vec![
            entry_term,
            path_term,
            digest_term,
            size_term,
            mode_term,
            modified_term,
            media_term,
        ]);
        let entry_id = base;
        quads.push((entry_id, type_id, file_entry_id, None));
        quads.push((entry_id, path_id, base + 1, None));
        quads.push((entry_id, digest_id, base + 2, None));
        quads.push((entry_id, size_id, base + 3, None));
        quads.push((entry_id, mode_id, base + 4, None));
        quads.push((entry_id, modified_id, base + 5, None));
        quads.push((entry_id, media_type_id, base + 6, None));
        blobs.push((data, digest, mt));
    }

    if !file_terms.is_empty() {
        w.add_terms(&file_terms);
    }
    if !quads.is_empty() {
        w.add_quads(&quads);
    }

    let mut seen: HashSet<String> = HashSet::new();
    for (data, _digest, mt) in blobs {
        if !seen.insert(digest_string(&data)) {
            continue;
        }
        w.add_blob(&data, Some(&mt), None);
    }

    Ok(w.to_bytes())
}

fn format_datetime(time: &std::time::SystemTime) -> Result<String, String> {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("mtime before unix epoch: {e}"))?;
    let secs = duration.as_secs();
    let dt = time::OffsetDateTime::from_unix_timestamp(secs as i64)
        .map_err(|e| format!("invalid mtime timestamp {secs}: {e}"))?;
    let text = dt
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| format!("format datetime: {e}"))?;
    Ok(text.replace("+00:00", "Z"))
}

fn read_file_entries(graph: &Graph) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut type_id: Option<usize> = None;
    let mut file_entry_id: Option<usize> = None;
    let mut field_ids: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, term) in graph.terms.iter().enumerate() {
        if term.kind != TermKind::Iri {
            continue;
        }
        let Some(value) = &term.value else {
            continue;
        };
        if value == RDF_TYPE {
            type_id = Some(idx);
        } else if *value == FILES_NS.to_string() + "FileEntry" {
            file_entry_id = Some(idx);
        } else if let Some(rest) = value.strip_prefix(FILES_NS) {
            field_ids.insert(rest.to_string(), idx);
        }
    }
    let type_id = type_id.ok_or("not a files-profile archive: missing rdf:type")?;
    let file_entry_id = file_entry_id.ok_or("not a files-profile archive: missing FileEntry")?;

    let mut entries: BTreeMap<usize, BTreeMap<String, String>> = BTreeMap::new();
    let mut file_entry_subjects: HashSet<usize> = HashSet::new();
    for &(s, p, o, _g) in &graph.quads {
        if p == type_id && o == file_entry_id {
            file_entry_subjects.insert(s);
            entries.entry(s).or_default();
        } else if let Some(field_name) = field_ids
            .iter()
            .find(|(_, &id)| id == p)
            .map(|(k, _)| k.clone())
        {
            let term = &graph.terms[o];
            let value = term.value.clone().unwrap_or_default();
            entries.entry(s).or_default().insert(field_name, value);
        }
    }

    let mut by_path: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (s, entry) in entries {
        if !file_entry_subjects.contains(&s) {
            continue;
        }
        if let Some(path) = entry.get("path") {
            if by_path.contains_key(path) {
                return Err(format!("duplicate files:path in archive: {path}"));
            }
            by_path.insert(path.clone(), entry);
        }
    }
    Ok(by_path)
}

fn dest_path(dest: &Path, archive_path: &str) -> Result<std::path::PathBuf, String> {
    safe_archive_path(archive_path)?;
    // Resolve the destination itself (e.g. `/tmp` -> `/private/tmp` on macOS),
    // then resolve the closest existing target ancestor so an existing
    // symlinked directory below the destination cannot redirect writes.
    let dest_canon = dest
        .canonicalize()
        .map_err(|e| format!("resolve destination {dest:?}: {e}"))?;
    let target = dest_canon.join(archive_path);

    let mut ancestor = target
        .parent()
        .unwrap_or(dest_canon.as_path())
        .to_path_buf();
    while !ancestor.exists() {
        let Some(parent) = ancestor.parent() else {
            break;
        };
        ancestor = parent.to_path_buf();
    }
    let ancestor_canon = ancestor
        .canonicalize()
        .map_err(|e| format!("resolve target ancestor {ancestor:?}: {e}"))?;
    if !ancestor_canon.starts_with(&dest_canon) {
        return Err(format!("path escapes destination: {archive_path}"));
    }
    Ok(target)
}

fn suppressed_blob_digests(graph: &Graph) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    for sup in &graph.suppressions {
        for target in &sup.targets {
            let Value::Map(entries) = target else {
                continue;
            };
            let mut kind = "";
            let mut digest: Option<String> = None;
            for (k, v) in entries {
                if let Value::Text(key) = k {
                    if key == "kind" {
                        if let Value::Text(val) = v {
                            kind = val.as_str();
                        }
                    } else if key == "digest" {
                        digest = Some(match v {
                            Value::Text(t) => t.clone(),
                            Value::Bytes(b) => format!("blake3:{}", hex(b)),
                            _ => continue,
                        });
                    }
                }
            }
            if kind == "blob" {
                if let Some(d) = digest {
                    out.insert(d);
                }
            }
        }
    }
    out
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

/// Extract FileEntry quads from a folded graph into dest.
pub fn unpack(graph: &Graph, dest: &Path, include_suppressed: bool) -> Result<(), String> {
    let entries = read_file_entries(graph)?;
    let suppressed = if include_suppressed {
        HashSet::new()
    } else {
        suppressed_blob_digests(graph)
    };
    fs::create_dir_all(dest).map_err(|e| format!("create {dest:?}: {e}"))?;
    let blob_entries: BTreeMap<&str, &crate::model::BlobEntry> =
        graph.blobs.iter().map(|(d, e)| (d.as_str(), e)).collect();
    let mut decoded_cache: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for (path, entry) in entries {
        let target = dest_path(dest, &path)?;

        let digest = entry
            .get("digest")
            .ok_or(format!("missing digest for {path}"))?;
        if suppressed.contains(digest) {
            continue;
        }
        let data = if let Some(cached) = decoded_cache.get(digest) {
            cached.clone()
        } else {
            let decoded = blob_entries
                .get(digest.as_str())
                .map(|entry| {
                    entry
                        .decoded_vec()
                        .map_err(|err| format!("decode inline blob for {path}: {err:?}"))
                })
                .transpose()?
                .ok_or(format!("missing inline blob for {path}: {digest}"))?;
            decoded_cache.insert(digest.clone(), decoded.clone());
            decoded
        };
        if digest_string(&data) != *digest {
            return Err(format!("integrity failure for {path}: {digest}"));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create dir {parent:?}: {e}"))?;
        }
        fs::write(&target, &data).map_err(|e| format!("write {target:?}: {e}"))?;

        #[cfg(unix)]
        if let Some(mode) = entry.get("mode") {
            if let Ok(m) = mode.parse::<u32>() {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&target, std::fs::Permissions::from_mode(m));
            }
        }

        if let Some(modified) = entry.get("modified") {
            if let Ok(ts) = parse_datetime(modified) {
                let _ =
                    filetime::set_file_mtime(&target, filetime::FileTime::from_unix_time(ts, 0));
            }
        }
    }
    Ok(())
}

fn parse_datetime(text: &str) -> Result<i64, String> {
    let text = text.strip_suffix('Z').unwrap_or(text);
    let dt = time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .or_else(|_| {
            time::OffsetDateTime::parse(
                &(text.to_string() + "+00:00"),
                &time::format_description::well_known::Rfc3339,
            )
        })
        .map_err(|e| format!("parse datetime {text}: {e}"))?;
    Ok(dt.unix_timestamp())
}

/// Compare an archive to a directory by content digest.
pub fn diff(graph: &Graph, directory: &Path) -> Result<Vec<String>, String> {
    let entries = read_file_entries(graph)?;
    let archive_digests: BTreeMap<String, String> = entries
        .iter()
        .map(|(p, e)| (p.clone(), e.get("digest").cloned().unwrap_or_default()))
        .collect();

    if !directory.exists() {
        return Err(format!("diff destination does not exist: {directory:?}"));
    }

    let mut disk_digests: BTreeMap<String, String> = BTreeMap::new();
    let files = walk_dir_sorted(directory)?;
    for fspath in files {
        let relpath = to_posix_path(
            fspath
                .strip_prefix(directory)
                .map_err(|_| format!("path outside directory: {fspath:?}"))?,
        )?;
        let data = fs::read(&fspath).map_err(|e| format!("read {fspath:?}: {e}"))?;
        disk_digests.insert(relpath, digest_string(&data));
    }

    let archive_paths: HashSet<&String> = archive_digests.keys().collect();
    let disk_paths: HashSet<&String> = disk_digests.keys().collect();

    let mut lines: Vec<String> = Vec::new();
    for path in archive_paths.difference(&disk_paths) {
        lines.push(format!("removed: {path}"));
    }
    for path in disk_paths.difference(&archive_paths) {
        lines.push(format!("added: {path}"));
    }
    for path in archive_paths.intersection(&disk_paths) {
        if archive_digests.get(*path) != disk_digests.get(*path) {
            lines.push(format!("modified: {path}"));
        }
    }
    lines.sort();
    Ok(lines)
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Relational exports for the folded graph (§14).
//!
//! The schema mirrors the Python `gts.db` module: five dictionary-encoded
//! tables (`terms`, `quads`, `reifiers`, `annotations`, `blobs`) whose rows keep
//! GTS integer term ids intact so query engines can resolve labels lazily.
//!
//! To avoid pulling database engines into the core Rust crate, these helpers use
//! command-line database tools at runtime: `sqlite3` for SQLite, plus `duckdb`
//! for DuckDB/Parquet when the optional `duckdb` feature is enabled.

use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{Graph, TermKind};

const SCHEMA: &[&str] = &[
    "CREATE TABLE terms (id INTEGER PRIMARY KEY, kind INTEGER, lex TEXT, datatype INTEGER, lang TEXT, reifier INTEGER)",
    "CREATE TABLE quads (s INTEGER, p INTEGER, o INTEGER, g INTEGER)",
    "CREATE TABLE reifiers (reifier INTEGER, s INTEGER, p INTEGER, o INTEGER)",
    "CREATE TABLE annotations (reifier INTEGER, predicate INTEGER, value INTEGER)",
    "CREATE TABLE blobs (digest TEXT PRIMARY KEY, bytes BLOB)",
];

const INDEXES: &[&str] = &[
    "CREATE INDEX quads_s ON quads (s)",
    "CREATE INDEX quads_p ON quads (p)",
    "CREATE INDEX quads_o ON quads (o)",
    "CREATE INDEX annot_reifier ON annotations (reifier)",
];

#[cfg(feature = "duckdb")]
const TABLES: &[&str] = &["terms", "quads", "reifiers", "annotations", "blobs"];

/// Export failure with a displayable message suitable for CLI output.
#[derive(Debug)]
pub struct DbExportError {
    detail: String,
}

impl DbExportError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for DbExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for DbExportError {}

#[derive(Clone, Copy)]
enum SqlDialect {
    Sqlite,
    #[cfg(feature = "duckdb")]
    Duckdb,
}

fn term_kind_int(kind: TermKind) -> u8 {
    match kind {
        TermKind::Iri => 0,
        TermKind::Literal => 1,
        TermKind::Bnode => 2,
        TermKind::Triple => 3,
    }
}

fn sql_io(err: io::Error) -> DbExportError {
    DbExportError::new(format!("cannot write SQL script: {err}"))
}

fn write_sql(writer: &mut dyn Write, bytes: &[u8]) -> Result<(), DbExportError> {
    writer.write_all(bytes).map_err(sql_io)
}

fn write_sql_fmt(writer: &mut dyn Write, args: fmt::Arguments<'_>) -> Result<(), DbExportError> {
    writer.write_fmt(args).map_err(sql_io)
}

fn write_sql_text(writer: &mut dyn Write, value: Option<&str>) -> Result<(), DbExportError> {
    let Some(text) = value else {
        return write_sql(writer, b"NULL");
    };
    write_sql(writer, b"'")?;
    let bytes = text.as_bytes();
    let mut start = 0;
    for (idx, _) in text.match_indices('\'') {
        write_sql(writer, &bytes[start..idx])?;
        write_sql(writer, b"''")?;
        start = idx + 1;
    }
    write_sql(writer, &bytes[start..])?;
    write_sql(writer, b"'")
}

fn write_sql_usize(writer: &mut dyn Write, value: Option<usize>) -> Result<(), DbExportError> {
    match value {
        Some(v) => write_sql_fmt(writer, format_args!("{v}")),
        None => write_sql(writer, b"NULL"),
    }
}

fn write_hex(writer: &mut dyn Write, bytes: &[u8]) -> Result<(), DbExportError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 4096];
    for chunk in bytes.chunks(out.len() / 2) {
        for (i, byte) in chunk.iter().enumerate() {
            out[i * 2] = HEX[(byte >> 4) as usize];
            out[i * 2 + 1] = HEX[(byte & 0x0f) as usize];
        }
        write_sql(writer, &out[..chunk.len() * 2])?;
    }
    Ok(())
}

fn write_sql_blob(
    writer: &mut dyn Write,
    bytes: &[u8],
    dialect: SqlDialect,
) -> Result<(), DbExportError> {
    match dialect {
        SqlDialect::Sqlite => {
            write_sql(writer, b"X'")?;
            write_hex(writer, bytes)?;
            write_sql(writer, b"'")
        }
        #[cfg(feature = "duckdb")]
        SqlDialect::Duckdb => {
            write_sql(writer, b"from_hex('")?;
            write_hex(writer, bytes)?;
            write_sql(writer, b"')")
        }
    }
}

fn write_insert_rows(
    graph: &Graph,
    dialect: SqlDialect,
    writer: &mut dyn Write,
) -> Result<(), DbExportError> {
    for (id, term) in graph.terms.iter().enumerate() {
        write_sql_fmt(
            writer,
            format_args!(
                "INSERT INTO terms VALUES ({id},{}",
                term_kind_int(term.kind)
            ),
        )?;
        write_sql(writer, b",")?;
        write_sql_text(writer, term.value.as_deref())?;
        write_sql(writer, b",")?;
        write_sql_usize(writer, term.datatype)?;
        write_sql(writer, b",")?;
        write_sql_text(writer, term.lang.as_deref())?;
        write_sql(writer, b",")?;
        write_sql_usize(writer, term.reifier)?;
        write_sql(writer, b");\n")?;
    }
    for (s, p, o, g) in &graph.quads {
        write_sql_fmt(
            writer,
            format_args!("INSERT INTO quads VALUES ({s},{p},{o},"),
        )?;
        write_sql_usize(writer, *g)?;
        write_sql(writer, b");\n")?;
    }
    for (r, (s, p, o)) in &graph.reifiers {
        write_sql_fmt(
            writer,
            format_args!("INSERT INTO reifiers VALUES ({r},{s},{p},{o});\n"),
        )?;
    }
    for (r, p, v) in &graph.annotations {
        write_sql_fmt(
            writer,
            format_args!("INSERT INTO annotations VALUES ({r},{p},{v});\n"),
        )?;
    }
    for (digest, entry) in &graph.blobs {
        let bytes = entry
            .decoded_bytes()
            .map_err(|err| DbExportError::new(format!("cannot decode blob {digest}: {err:?}")))?;
        write_sql(writer, b"INSERT INTO blobs VALUES (")?;
        write_sql_text(writer, Some(digest.as_str()))?;
        write_sql(writer, b",")?;
        write_sql_blob(writer, bytes.as_ref(), dialect)?;
        write_sql(writer, b");\n")?;
    }
    Ok(())
}

fn write_load_script(
    graph: &Graph,
    dialect: SqlDialect,
    writer: &mut dyn Write,
) -> Result<(), DbExportError> {
    for ddl in SCHEMA {
        write_sql_fmt(writer, format_args!("{ddl};\n"))?;
    }
    write_sql(writer, b"BEGIN TRANSACTION;\n")?;
    write_insert_rows(graph, dialect, writer)?;
    write_sql(writer, b"COMMIT;\n")?;
    for ddl in INDEXES {
        write_sql_fmt(writer, format_args!("{ddl};\n"))?;
    }
    Ok(())
}

fn run_sql_tool<F>(program: &str, args: &[&str], write_script: F) -> Result<(), DbExportError>
where
    F: FnOnce(&mut dyn Write) -> Result<(), DbExportError>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            DbExportError::new(format!(
                "{program} is required for this export and could not be started: {e}"
            ))
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| DbExportError::new(format!("{program}: stdin unavailable")))?;
    if let Err(err) = write_script(&mut stdin) {
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|e| DbExportError::new(format!("{program}: could not collect status: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(DbExportError::new(format!(
        "{program} failed with status {}: {}{}{}",
        output.status,
        stderr.trim(),
        if stderr.trim().is_empty() || stdout.trim().is_empty() {
            ""
        } else {
            "\n"
        },
        stdout.trim()
    )))
}

#[cfg(feature = "duckdb")]
fn table_count(graph: &Graph, table: &str) -> usize {
    match table {
        "terms" => graph.terms.len(),
        "quads" => graph.quads.len(),
        "reifiers" => graph.reifiers.len(),
        "annotations" => graph.annotations.len(),
        "blobs" => graph.blobs.len(),
        _ => 0,
    }
}

#[cfg(feature = "duckdb")]
fn write_sql_path(writer: &mut dyn Write, path: &Path) -> Result<(), DbExportError> {
    let path = path.to_string_lossy();
    let bytes = path.as_bytes();
    let mut start = 0;
    for (idx, _) in path.match_indices('\'') {
        write_sql(writer, &bytes[start..idx])?;
        write_sql(writer, b"''")?;
        start = idx + 1;
    }
    write_sql(writer, &bytes[start..])
}

#[cfg(feature = "duckdb")]
fn write_copy_stmt(writer: &mut dyn Write, table: &str, path: &Path) -> Result<(), DbExportError> {
    write_sql_fmt(writer, format_args!("COPY {table} TO '"))?;
    write_sql_path(writer, path)?;
    write_sql(writer, b"' (FORMAT parquet);\n")
}

fn temp_sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "export".into());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(
        ".{name}.{}.{}.{}",
        std::process::id(),
        nanos,
        suffix
    ))
}

fn replace_file(staged: &Path, out: &Path) -> Result<(), DbExportError> {
    if out.is_dir() {
        return Err(DbExportError::new(format!(
            "cannot replace directory {} with exported file",
            out.display()
        )));
    }
    if out.exists() {
        let backup = temp_sibling_path(out, "bak");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(out, &backup).map_err(|e| {
            DbExportError::new(format!(
                "cannot stage replacement for {}: {e}",
                out.display()
            ))
        })?;
        if let Err(err) = std::fs::rename(staged, out) {
            let _ = std::fs::rename(&backup, out);
            return Err(DbExportError::new(format!(
                "cannot replace {}: {err}",
                out.display()
            )));
        }
        let _ = std::fs::remove_file(&backup);
        return Ok(());
    }
    std::fs::rename(staged, out)
        .map_err(|e| DbExportError::new(format!("cannot write {}: {e}", out.display())))
}

fn export_database_file<F>(program: &str, out: &Path, write_script: F) -> Result<(), DbExportError>
where
    F: FnOnce(&mut dyn Write) -> Result<(), DbExportError>,
{
    let staged = temp_sibling_path(out, "tmp");
    let _ = std::fs::remove_file(&staged);
    let staged_arg = staged.to_string_lossy().into_owned();
    if let Err(err) = run_sql_tool(program, &[staged_arg.as_str()], write_script) {
        let _ = std::fs::remove_file(&staged);
        return Err(err);
    }
    replace_file(&staged, out)
}

/// Write a folded graph to a SQLite database.
pub fn to_sqlite(graph: &Graph, path: impl AsRef<Path>) -> Result<PathBuf, DbExportError> {
    let out = path.as_ref();
    export_database_file("sqlite3", out, |writer| {
        write_load_script(graph, SqlDialect::Sqlite, writer)
    })?;
    Ok(out.to_path_buf())
}

/// Write a folded graph to a DuckDB database.
#[cfg(feature = "duckdb")]
pub fn to_duckdb(graph: &Graph, path: impl AsRef<Path>) -> Result<PathBuf, DbExportError> {
    let out = path.as_ref();
    export_database_file("duckdb", out, |writer| {
        write_load_script(graph, SqlDialect::Duckdb, writer)
    })?;
    Ok(out.to_path_buf())
}

/// Write one Parquet file per non-empty relational table.
#[cfg(feature = "duckdb")]
pub fn to_parquet(graph: &Graph, out_dir: impl AsRef<Path>) -> Result<Vec<PathBuf>, DbExportError> {
    let target = out_dir.as_ref();
    std::fs::create_dir_all(target)
        .map_err(|e| DbExportError::new(format!("cannot create {}: {e}", target.display())))?;
    let staged_dir = temp_sibling_path(target, "tmpdir");
    let _ = std::fs::remove_dir_all(&staged_dir);
    std::fs::create_dir_all(&staged_dir).map_err(|e| {
        DbExportError::new(format!(
            "cannot create staging directory {}: {e}",
            staged_dir.display()
        ))
    })?;
    let mut written = Vec::new();
    for table in TABLES {
        if table_count(graph, table) == 0 {
            continue;
        }
        let staged = staged_dir.join(format!("{table}.parquet"));
        let out = target.join(format!("{table}.parquet"));
        written.push((*table, staged, out));
    }
    if let Err(err) = run_sql_tool("duckdb", &[], |writer| {
        write_load_script(graph, SqlDialect::Duckdb, writer)?;
        for (table, staged, _) in &written {
            write_copy_stmt(writer, table, staged)?;
        }
        Ok(())
    }) {
        let _ = std::fs::remove_dir_all(&staged_dir);
        return Err(err);
    }
    let mut final_paths = Vec::new();
    let mut replace_err = None;
    for (_, staged, out) in &written {
        if let Err(err) = replace_file(staged, out) {
            replace_err = Some(err);
            break;
        }
        final_paths.push(out.clone());
    }
    if replace_err.is_none() {
        for table in TABLES {
            if table_count(graph, table) == 0 {
                let _ = std::fs::remove_file(target.join(format!("{table}.parquet")));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&staged_dir);
    if let Some(err) = replace_err {
        return Err(err);
    }
    Ok(final_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Codec;
    use crate::wire::{digest_str, hex};

    fn zstd_bytes(data: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Uncompressed)
    }

    fn zstd_codec() -> Vec<Codec> {
        vec![Codec {
            name: "zstd".into(),
            cls: "compress".into(),
        }]
    }

    #[test]
    fn load_script_decodes_lazy_blob_without_caching_it() {
        let payload = b"blob-heavy relational fixture".repeat(128);
        let digest = digest_str(&payload);
        let mut graph = Graph::default();
        graph.set_lazy_blob(digest.clone(), zstd_bytes(&payload), zstd_codec());

        let mut script = Vec::new();
        write_load_script(&graph, SqlDialect::Sqlite, &mut script).unwrap();

        assert!(graph
            .blob_entry(&digest)
            .is_some_and(|entry| entry.is_lazy()));
        let script = String::from_utf8(script).unwrap();
        assert!(script.contains(&format!(
            "INSERT INTO blobs VALUES ('{}',X'{}');",
            digest,
            hex(&payload)
        )));
    }

    #[test]
    fn load_script_decode_failure_stops_before_commit() {
        let digest = format!("blake3:{}", "00".repeat(32));
        let mut graph = Graph::default();
        graph.set_lazy_blob(digest.clone(), b"not zstd".to_vec(), zstd_codec());

        let mut script = Vec::new();
        let err = write_load_script(&graph, SqlDialect::Sqlite, &mut script).unwrap_err();

        assert!(
            err.to_string().contains("cannot decode blob"),
            "unexpected error: {err}"
        );
        assert!(graph
            .blob_entry(&digest)
            .is_some_and(|entry| entry.is_lazy()));
        let script = String::from_utf8_lossy(&script);
        assert!(script.contains("BEGIN TRANSACTION;\n"));
        assert!(!script.contains("COMMIT;\n"));
    }
}

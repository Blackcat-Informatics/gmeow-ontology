// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Relational export CLI tests.

use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output};

use gmeow_gts::codec::Codec;
use gmeow_gts::model::Graph;

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors")
}

fn tmpdir() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("gts-db-test-{}-{n}", std::process::id()))
}

fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn gts(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gts"))
        .args(args)
        .output()
        .expect("gts binary runs")
}

fn gts_with_path(args: &[&str], path: OsString) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gts"))
        .args(args)
        .env("PATH", path)
        .output()
        .expect("gts binary runs")
}

fn prepend_path(dir: &Path) -> OsString {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(paths).unwrap()
}

#[cfg(unix)]
fn fake_failing_tool(dir: &Path, program: &str) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(program);
    std::fs::write(&path, "#!/bin/sh\nexit 7\n").unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[cfg(windows)]
fn fake_failing_tool(dir: &Path, program: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(format!("{program}.bat")),
        "@echo off\r\nexit /b 7\r\n",
    )
    .unwrap();
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn sqlite_query(db: &str, sql: &str) -> String {
    stdout(
        Command::new("sqlite3")
            .args(["-batch", "-noheader", db, sql])
            .output()
            .expect("sqlite3 runs"),
    )
}

#[cfg(feature = "duckdb")]
fn duckdb_query(db: &str, sql: &str) -> String {
    stdout(
        Command::new("duckdb")
            .args(["-csv", "-noheader", db, "-c", sql])
            .output()
            .expect("duckdb runs"),
    )
}

#[test]
fn to_sqlite_exports_schema_and_rows() {
    if !command_available("sqlite3") {
        eprintln!("skipping: sqlite3 is not installed");
        return;
    }
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db = tmp.join("minimal.sqlite");
    let out = gts(&[
        "to-sqlite",
        vectors().join("01-minimal.gts").to_str().unwrap(),
        db.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        sqlite_query(db.to_str().unwrap(), "SELECT count(*) FROM terms;").trim(),
        "3"
    );
    assert_eq!(
        sqlite_query(db.to_str().unwrap(), "SELECT count(*) FROM quads;").trim(),
        "1"
    );
    assert_eq!(
        sqlite_query(db.to_str().unwrap(), "SELECT lex FROM terms WHERE id = 0;").trim(),
        "https://example.org/Cat"
    );
}

#[test]
fn to_sqlite_refuses_damaged_input_before_writing() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db = tmp.join("damaged.sqlite");
    let out = gts(&[
        "to-sqlite",
        vectors().join("04-damaged-frame.gts").to_str().unwrap(),
        db.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("DamagedFrame"), "stderr lists diagnostics");
    assert!(err.contains("refusing export"), "stderr names refusal");
    assert!(!db.exists(), "no database should be written");
}

#[test]
fn to_sqlite_preserves_existing_output_when_tool_fails() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db = tmp.join("existing.sqlite");
    std::fs::write(&db, b"keep me").unwrap();
    let fake_bin = tmp.join("bin");
    fake_failing_tool(&fake_bin, "sqlite3");

    let out = gts_with_path(
        &[
            "to-sqlite",
            vectors().join("01-minimal.gts").to_str().unwrap(),
            db.to_str().unwrap(),
        ],
        prepend_path(&fake_bin),
    );

    assert_eq!(out.status.code(), Some(2));
    assert_eq!(std::fs::read(&db).unwrap(), b"keep me");
}

#[test]
fn to_sqlite_preserves_existing_output_when_blob_decode_fails() {
    if !command_available("sqlite3") {
        eprintln!("skipping: sqlite3 is not installed");
        return;
    }
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db = tmp.join("existing.sqlite");
    std::fs::write(&db, b"keep me").unwrap();

    let mut graph = Graph::default();
    graph.set_lazy_blob(
        format!("blake3:{}", "00".repeat(32)),
        b"not zstd".to_vec(),
        vec![Codec {
            name: "zstd".into(),
            cls: "compress".into(),
        }],
    );

    let err = gmeow_gts::db::to_sqlite(&graph, &db).unwrap_err();

    assert!(
        err.to_string().contains("cannot decode blob"),
        "unexpected error: {err}"
    );
    assert_eq!(std::fs::read(&db).unwrap(), b"keep me");
}

#[cfg(not(feature = "duckdb"))]
#[test]
fn duckdb_commands_require_feature() {
    let src = vectors().join("01-minimal.gts");
    for (command, out_path) in [
        ("to-duckdb", tmpdir().join("minimal.duckdb")),
        ("to-parquet", tmpdir().join("parquet")),
    ] {
        let out = gts(&[command, src.to_str().unwrap(), out_path.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(2), "{command}");
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(
            err.contains("optional DuckDB/Parquet exports are disabled"),
            "{command}: {err}"
        );
        assert!(err.contains("--features duckdb"), "{command}: {err}");
    }
}

#[cfg(feature = "duckdb")]
#[test]
fn to_duckdb_exports_schema_and_rows() {
    if !command_available("duckdb") {
        eprintln!("skipping: duckdb is not installed");
        return;
    }
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db = tmp.join("minimal.duckdb");
    let out = gts(&[
        "to-duckdb",
        vectors().join("01-minimal.gts").to_str().unwrap(),
        db.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        duckdb_query(db.to_str().unwrap(), "SELECT count(*) FROM terms;").trim(),
        "3"
    );
    assert_eq!(
        duckdb_query(db.to_str().unwrap(), "SELECT count(*) FROM quads;").trim(),
        "1"
    );
    assert_eq!(
        duckdb_query(db.to_str().unwrap(), "SELECT lex FROM terms WHERE id = 0;").trim(),
        "https://example.org/Cat"
    );
}

#[cfg(feature = "duckdb")]
#[test]
fn to_parquet_writes_non_empty_tables() {
    if !command_available("duckdb") {
        eprintln!("skipping: duckdb is not installed");
        return;
    }
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let out_dir = tmp.join("parquet");
    let out = gts(&[
        "to-parquet",
        vectors().join("22-inline-blob.gts").to_str().unwrap(),
        out_dir.to_str().unwrap(),
    ]);
    let text = stdout(out);

    for name in ["terms.parquet", "quads.parquet", "blobs.parquet"] {
        let path = out_dir.join(name);
        assert!(path.exists(), "{name} was written");
        assert!(
            text.contains(path.to_str().unwrap()),
            "{name} listed on stdout"
        );
    }
    assert!(!out_dir.join("reifiers.parquet").exists());
    assert!(!out_dir.join("annotations.parquet").exists());

    let blobs = out_dir.join("blobs.parquet");
    assert_eq!(
        duckdb_query(
            ":memory:",
            &format!(
                "SELECT count(*) FROM read_parquet('{}');",
                blobs.to_string_lossy().replace('\'', "''")
            ),
        )
        .trim(),
        "1"
    );
}

#[cfg(feature = "duckdb")]
#[test]
fn to_parquet_cleans_staging_dir_when_replace_fails() {
    if !command_available("duckdb") {
        eprintln!("skipping: duckdb is not installed");
        return;
    }
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let out_dir = tmp.join("parquet");
    std::fs::create_dir_all(out_dir.join("terms.parquet")).unwrap();

    let out = gts(&[
        "to-parquet",
        vectors().join("01-minimal.gts").to_str().unwrap(),
        out_dir.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("cannot replace directory"), "{err}");
    assert!(
        std::fs::read_dir(&tmp)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .all(|name| !name.starts_with(".parquet.") || !name.ends_with(".tmpdir")),
        "temporary staging directory was not removed"
    );
}

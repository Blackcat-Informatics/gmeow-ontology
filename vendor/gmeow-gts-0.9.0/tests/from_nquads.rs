// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `nquads -> gts` inverse-of-fold tests.

use std::path::PathBuf;
use std::process::Command;

use gmeow_gts::from_nquads::from_nquads;
use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::nquads::to_nquads;
use gmeow_gts::reader::read;
use gmeow_gts::writer::Writer;

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors")
}

fn sorted_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

fn roundtrip(data: &[u8]) -> bool {
    let nq1 = to_nquads(&read(data, true, None));
    let imported = from_nquads(&nq1).expect("fold output parses");
    let nq2 = to_nquads(&read(&imported, true, None));
    sorted_lines(&nq1) == sorted_lines(&nq2)
}

#[test]
fn fold_roundtrips_through_from_nquads_for_graph_vectors() {
    for entry in std::fs::read_dir(vectors()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("gts") {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let folded = read(&data, true, None);
        let nq = to_nquads(&folded);
        if nq.trim().is_empty() {
            continue;
        }
        assert!(roundtrip(&data), "round-trip failed for {}", path.display());
    }
}

#[test]
fn named_graph_reifier_and_annotation_roundtrip() {
    let mut w = Writer::new("dist");
    w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://ex/s".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://ex/p".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://ex/o".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://ex/g".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://ex/conf".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("0.9".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, Some(3))]);
    w.add_reifies(&[(0, (0, 1, 2))]);
    w.add_annot(&[(0, 4, 5)]);
    assert!(roundtrip(&w.to_bytes()));
}

#[test]
fn literals_lang_and_datatype_roundtrip() {
    let xsd_int = "http://www.w3.org/2001/XMLSchema#integer";
    let nq = format!(
        "<https://ex/s> <https://ex/label> \"Cat\"@en .\n\
         <https://ex/s> <https://ex/n> \"42\"^^<{xsd_int}> .\n\
         _:b0 <https://ex/p> <https://ex/s> .\n"
    );
    let imported = from_nquads(&nq).expect("N-Quads parses");
    let out = to_nquads(&read(&imported, true, None));
    assert_eq!(sorted_lines(&out), sorted_lines(&nq));
}

#[test]
fn rejects_malformed_nquads() {
    let err =
        from_nquads("<https://ex/s> <https://ex/p> .\n").expect_err("only two terms is malformed");
    assert!(err.to_string().contains("expected 3 or 4 terms"));
}

#[test]
fn rejects_malformed_unicode_escape_without_panicking() {
    let err = from_nquads("<https://ex/s> <https://ex/p> \"\\u000é\" .\n")
        .expect_err("escape ends inside a multibyte UTF-8 scalar");
    assert!(err.to_string().contains("unicode escape"));
}

#[test]
fn parses_backspace_and_formfeed_escapes() {
    let imported =
        from_nquads("<https://ex/s> <https://ex/p> \"\\b\\f\" .\n").expect("N-Quads parses");
    let graph = read(&imported, true, None);
    assert!(graph.terms.iter().any(|term| {
        term.kind == TermKind::Literal && term.value.as_deref() == Some("\u{0008}\u{000c}")
    }));
}

#[test]
fn rejects_unknown_literal_escape() {
    let err = from_nquads("<https://ex/s> <https://ex/p> \"\\x\" .\n")
        .expect_err("unknown escapes are invalid N-Quads");
    assert!(err.to_string().contains("unsupported escape"));
}

#[test]
fn rejects_invalid_rdf_term_positions() {
    let err = from_nquads("\"subject\" <https://ex/p> <https://ex/o> .\n")
        .expect_err("literal subjects are invalid");
    assert!(err.to_string().contains("invalid subject"));

    let err = from_nquads("<https://ex/s> \"predicate\" <https://ex/o> .\n")
        .expect_err("literal predicates are invalid");
    assert!(err.to_string().contains("predicate must be IRI"));

    let err = from_nquads("<https://ex/s> <https://ex/p> <https://ex/o> \"graph\" .\n")
        .expect_err("literal graph names are invalid");
    assert!(err.to_string().contains("invalid graph name"));
}

fn tmpdir() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("gts-from-nq-test-{}-{n}", std::process::id()))
}

fn gts(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gts"))
        .args(args)
        .output()
        .expect("gts binary runs")
}

#[test]
fn cli_from_nq_inverts_fold() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let src = vectors().join("11-datatype-defaulting.gts");
    let folded_src = gts(&["fold", src.to_str().unwrap()]);
    assert!(
        folded_src.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&folded_src.stderr)
    );
    let nq = String::from_utf8(folded_src.stdout).unwrap();
    let nq_path = tmp.join("in.nq");
    std::fs::write(&nq_path, &nq).unwrap();
    let out_path = tmp.join("out.gts");

    let out = gts(&[
        "from-nq",
        nq_path.to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let folded_out = gts(&["fold", out_path.to_str().unwrap()]);
    assert!(
        folded_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&folded_out.stderr)
    );
    let folded = String::from_utf8(folded_out.stdout).unwrap();
    assert_eq!(sorted_lines(&folded), sorted_lines(&nq));
}

#[test]
fn cli_from_nq_writes_stdout_when_no_out_path() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let nq_path = tmp.join("in.nq");
    std::fs::write(&nq_path, "<https://ex/s> <https://ex/p> \"value\" .\n").unwrap();
    let out = gts(&["from-nq", nq_path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let graph = read(&out.stdout, true, None);
    assert_eq!(
        sorted_lines(&to_nquads(&graph)),
        vec!["<https://ex/s> <https://ex/p> \"value\" .".to_string()]
    );
}

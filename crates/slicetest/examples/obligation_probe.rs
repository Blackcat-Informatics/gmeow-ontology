//! Obligation probe: answers "does obligation X hold over the AUTHORED slices?" through the
//! real parser + SPARQL, instead of pattern-matching Turtle text.
//!
//! Authoring a `logic:Constraint` means asserting that an obligation ALREADY holds over the
//! authored corpus — a constraint that reds the moment it lands is not a gate, it is a bug
//! report against the data. Answering "does it hold?" by grepping Turtle does not work: the
//! text scan counts prose inside `skos:definition` as triples and matches across multi-line
//! type lists, and it will report both false holds and false reds. Ask the parser instead.
//!
//! Run: `cargo run -p gmeow-slicetest --example obligation_probe`
//!
//! Scoped to `module.ttl` — the authored production surface. Widen it to every `.ttl` to see
//! what the conformance fixtures do, which is a DIFFERENT question: a fixture that omits a
//! field on purpose is not a defect, so an obligation may lawfully hold in production and
//! red across the examples.
use std::path::PathBuf;

fn main() -> gmeow_errors::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut files = Vec::new();
    for e in walk(&root.join("slices")) {
        if e.file_name().is_some_and(|n| n == "module.ttl") {
            files.push(e);
        }
    }
    files.sort();
    eprintln!("parsing {} ttl files", files.len());
    let ds = gmeow_slicetest::native_query::dataset_from_files(&files)?;

    // (class, required predicate)
    let probes = [
        ("logic/McpActionSchema", "logic/capability"),
        ("logic/McpActionSchema", "logic/precondition"),
        ("logic/McpActionSchema", "logic/effect"),
        ("logic/McpActionSchema", "logic/compensation"),
        ("logic/McpActionSchema", "logic/mcpToolName"),
        ("logic/ActionSchema", "logic/capability"),
        ("logic/ActionSchema", "logic/precondition"),
        ("gmeow/GTSSegment", "gmeow/gtsHeadId"),
        ("gmeow/GTSSegment", "gmeow/gtsProfile"),
        ("gmeow/GTSSegment", "gmeow/gtsSegmentIndex"),
        ("gmeow/GTSDocument", "gmeow/gtsSegment"),
        ("gmeow/FormalConcept", "gmeow/conceptExtent"),
        (
            "gmeow/DocumentationDistribution",
            "gmeow/distributionFamily",
        ),
        ("gmeow/Medium", "gmeow/mediumCodec"),
        ("gmeow/CompressionDictionary", "gmeow/dictionaryId"),
        ("gmeow/CompressionDictionary", "gmeow/trainsOverCorpus"),
    ];
    for (c, p) in probes {
        let cls = format!("https://blackcatinformatics.ca/{c}");
        let pred = format!("https://blackcatinformatics.ca/{p}");
        let total = count(&ds, &format!("SELECT DISTINCT ?s WHERE {{ ?s a <{cls}> }}"))?;
        let sat = count(
            &ds,
            &format!("SELECT DISTINCT ?s WHERE {{ ?s a <{cls}> ; <{pred}> ?v }}"),
        )?;
        let mark = if total > 0 && sat == total {
            "HOLDS "
        } else if total == 0 {
            "empty "
        } else {
            "REDS  "
        };
        println!(
            "{mark}{:>3}/{:<3} {}  requires  {}",
            sat,
            total,
            short(&cls),
            short(&pred)
        );
        if total > 0 && sat < total {
            let q = format!(
                "SELECT DISTINCT ?s WHERE {{ ?s a <{cls}> FILTER NOT EXISTS {{ ?s <{pred}> ?v }} }}"
            );
            for row in gmeow_slicetest::native_query::select(&ds, &q)?.rows {
                if let Some(Some(t)) = row.first() {
                    println!(
                        "        offender: {}",
                        gmeow_slicetest::native_query::render_term(t)
                    );
                }
            }
        }
    }
    Ok(())
}

fn count(ds: &std::sync::Arc<purrdf::RdfDataset>, q: &str) -> gmeow_errors::Result<usize> {
    Ok(gmeow_slicetest::native_query::select(ds, q)?.rows.len())
}
fn short(iri: &str) -> String {
    iri.rsplit('/').next().unwrap_or(iri).to_string()
}
fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

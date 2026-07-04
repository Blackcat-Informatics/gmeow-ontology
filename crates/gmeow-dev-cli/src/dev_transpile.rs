// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The projection / transpile / build commands: `build`, `project`, `transform`,
//! and `up-project`.
//!
//! Each reads the committed `gmeow.gts` snapshot from the working tree (the dev
//! razor — no embedded bundle), assembles the lawful up-projection + maximal
//! inputs from its folded blobs, and delegates to `gmeow_pipeline::projections`.

use std::path::Path;

use gmeow_pipeline::bundle_blobs;
use gmeow_pipeline::projections::{
    self, MaximalInputs, TagMap, UpProjectionInputs, GTS_VIEW_ALL, GTS_VIEW_GMEOW,
};

use crate::dev_common::{fail, project_root, snapshot_bytes};

/// The internal→BCP-47 retag map (used-tags only) built from a snapshot.
fn tag_map(bytes: &[u8]) -> TagMap {
    let Ok(dataset) = purrdf::gts::flattened_dataset_from_bytes(bytes) else {
        return TagMap::new();
    };
    let Ok(nt) = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    ) else {
        return TagMap::new();
    };
    gmeow_validate::language_tags::load_tag_map(&nt, "n-triples")
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

/// Serialize a flat default-graph quad stream to canonical N-Triples.
fn quads_to_nt(quads: &[purrdf::RdfQuad]) -> Result<String, String> {
    let flat = purrdf::flat_dataset_from_quads(quads)
        .map_err(|e| format!("N-Triples flatten failed: {e}"))?;
    let bytes = purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("N-Triples serialization failed: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("N-Triples output is not UTF-8: {e}"))
}

/// Re-serialize an N-Triples document as Turtle.
fn nt_to_turtle(nt: &str) -> Result<Vec<u8>, String> {
    let dataset = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| format!("projected N-Triples parse failed: {e}"))?;
    purrdf::serialize_dataset(
        &dataset,
        "text/turtle",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("Turtle serialization failed: {e}"))
}

// ── build ─────────────────────────────────────────────────────────────────────

/// `gmeow-dev build` — write derived serializations of the committed snapshot.
pub fn build() -> i32 {
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot fold snapshot: {e}")),
    };
    let out = root.join("dist");
    if let Err(e) = std::fs::create_dir_all(&out) {
        return fail(format!("cannot create {}: {e}", out.display()));
    }
    let writes: &[(&str, &str)] = &[
        ("gmeow.nq", "application/n-quads"),
        ("gmeow.ttl", "text/turtle"),
        ("gmeow.nt", "application/n-triples"),
    ];
    for (name, media) in writes {
        let selection = if *name == "gmeow.nt" {
            purrdf::SerializeGraph::DefaultGraph
        } else {
            purrdf::SerializeGraph::Dataset
        };
        match purrdf::serialize_dataset(&dataset, media, selection) {
            Ok(data) => {
                let target = out.join(name);
                if let Err(e) = std::fs::write(&target, data) {
                    return fail(format!("cannot write {}: {e}", target.display()));
                }
                println!("wrote {}", target.display());
            }
            Err(e) => return fail(format!("cannot serialize {name}: {e}")),
        }
    }
    match gmeow_pipeline::stages::yaml_ld::serialize_graph(&dataset) {
        Ok(text) => write_or_fail(&out.join("gmeow.jsonld"), text.as_bytes()),
        Err(e) => return fail(format!("cannot serialize gmeow.jsonld: {e}")),
    }
    .map_or_else(|c| c, |()| 0);
    match gmeow_pipeline::stages::yaml_ld::serialize_graph_yaml(&dataset, None) {
        Ok(text) => match write_or_fail(&out.join("gmeow.yamlld"), text.as_bytes()) {
            Ok(()) => 0,
            Err(c) => c,
        },
        Err(e) => fail(format!("cannot serialize gmeow.yamlld: {e}")),
    }
}

/// Write bytes to a path, printing a `wrote` line or returning a failure code.
fn write_or_fail(path: &Path, data: &[u8]) -> Result<(), i32> {
    std::fs::write(path, data)
        .map_err(|e| fail(format!("cannot write {}: {e}", path.display())))?;
    println!("wrote {}", path.display());
    Ok(())
}

// ── shared transpile input assembly ────────────────────────────────────────────

/// Assemble the lawful up-projection + maximal inputs from a folded snapshot: the
/// SSSOM lift maps, projection/EDOAL TTLs, ontology base graph, per-profile
/// CONSTRUCT queries, and the saturation refusal set.
fn assemble_inputs(bytes: &[u8]) -> Result<(UpProjectionInputs, MaximalInputs), String> {
    let sssom_texts: Vec<String> = bundle_blobs::bundled_sssom(bytes)
        .map_err(|e| format!("cannot read bundled SSSOM: {e}"))?
        .into_values()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .collect();
    let projection_ttls: Vec<String> = bundle_blobs::Bundle::from_snapshot(bytes)
        .map_err(|e| format!("cannot fold bundle: {e}"))?
        .archive(bundle_blobs::REP_MAPPINGS)
        .map_err(|e| format!("cannot read bundled mappings: {e}"))?
        .into_iter()
        .filter(|(k, _)| k.ends_with(".ttl"))
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .collect();
    let base = projections::gts_base_graph(bytes)?;
    let ontology_nt = quads_to_nt(&base)?;
    let projection_queries: Vec<(String, String)> = bundle_blobs::bundled_queries(bytes)
        .map_err(|e| format!("cannot read bundled queries: {e}"))?
        .into_iter()
        .filter(|(k, _)| k.ends_with(".rq"))
        .map(|(k, v)| {
            let stem = Path::new(&k)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or(k);
            (stem, String::from_utf8_lossy(&v).into_owned())
        })
        .collect();
    let denied = bundle_blobs::bundled_denied_cells(bytes)
        .map_err(|e| format!("cannot read denied cells: {e}"))?
        .unwrap_or_default();

    let up_inputs = UpProjectionInputs {
        sssom_texts,
        projection_ttls,
        ontology_nt: ontology_nt.clone(),
    };
    let maximal_inputs = MaximalInputs {
        ontology_nt,
        cells: Vec::new(),
        denied,
        projection_queries,
    };
    Ok((up_inputs, maximal_inputs))
}

/// Read a source RDF file (Turtle) or stdin (`-`) as `(source_nt, stem)`.
fn load_source_nt(source: &Path) -> Result<(String, String), String> {
    let is_stdin = source.as_os_str() == "-";
    let bytes = if is_stdin {
        use std::io::Read;
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        buf
    } else {
        std::fs::read(source).map_err(|e| format!("cannot read {}: {e}", source.display()))?
    };
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| format!("cannot parse Turtle source: {e}"))?;
    let nt = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("cannot project source to N-Triples: {e}"))?;
    let stem = if is_stdin {
        "stdin".to_owned()
    } else {
        source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "source".to_owned())
    };
    Ok((String::from_utf8_lossy(&nt).into_owned(), stem))
}

// ── project ───────────────────────────────────────────────────────────────────

/// `gmeow-dev project [SOURCE] --profile --data --lang` — a per-profile CONSTRUCT
/// over a data file, or a view filter over a `.gts` / the committed snapshot.
pub fn project(source: Option<&Path>, profile: &str, data: &str, _lang: Option<&str>) -> i32 {
    let root = project_root();
    let src: Option<std::path::PathBuf> = source.map(Path::to_path_buf).or_else(|| {
        if data.is_empty() {
            None
        } else {
            Some(data.into())
        }
    });

    let snapshot = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let tags = tag_map(&snapshot);
    let out = root.join("dist").join("project");
    if let Err(e) = std::fs::create_dir_all(&out) {
        return fail(format!("cannot create {}: {e}", out.display()));
    }

    let known = projections::profiles();
    match src {
        None => {
            // Default: project the committed snapshot base graph through the profile view.
            project_view(&snapshot, profile, &out, &tags)
        }
        Some(path)
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gts")) =>
        {
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
            };
            let valid = known.contains_key(profile)
                || profile == GTS_VIEW_GMEOW
                || GTS_VIEW_ALL.contains(&profile);
            if !valid {
                return fail(format!(
                    "unknown view: {profile} (vocab | gmeow | all | maximal)"
                ));
            }
            project_view(&bytes, profile, &out, &tags)
        }
        Some(path) => {
            // A GMEOW data file (.ttl): run the per-profile CONSTRUCT(s).
            let names: Vec<String> = if profile == "all" {
                known.keys().map(|k| (*k).to_owned()).collect()
            } else {
                vec![profile.to_owned()]
            };
            for name in &names {
                if !known.contains_key(name.as_str()) {
                    return fail(format!("unknown profile: {name}"));
                }
                let code = project_data_file(&snapshot, &path, name, &out, &tags);
                if code != 0 {
                    return code;
                }
            }
            0
        }
    }
}

/// A view filter over a `.gts` / snapshot: `project_gts_subset` → Turtle file.
fn project_view(bytes: &[u8], profile: &str, out: &Path, tags: &TagMap) -> i32 {
    match projections::project_gts_subset(bytes, profile, tags) {
        Ok(nt) => match nt_to_turtle(&nt) {
            Ok(ttl) => {
                write_or_fail(&out.join(format!("{profile}.ttl")), &ttl).map_or_else(|c| c, |()| 0)
            }
            Err(e) => fail(e),
        },
        Err(e) => fail(e),
    }
}

/// Run a profile's bundled CONSTRUCT over a user data file merged with the
/// snapshot ontology, writing the projected Turtle.
fn project_data_file(
    snapshot: &[u8],
    source: &Path,
    profile: &str,
    out: &Path,
    tags: &TagMap,
) -> i32 {
    let queries = match bundle_blobs::bundled_queries(snapshot) {
        Ok(q) => q,
        Err(e) => return fail(format!("cannot read bundled queries: {e}")),
    };
    let want = format!("{profile}.rq");
    let Some(query) = queries
        .iter()
        .find(|(k, _)| k.ends_with(&want))
        .map(|(_, v)| String::from_utf8_lossy(v).into_owned())
    else {
        return fail(format!("no bundled CONSTRUCT query for profile {profile}"));
    };
    let base = match projections::gts_base_graph(snapshot) {
        Ok(b) => b,
        Err(e) => return fail(format!("cannot read snapshot base graph: {e}")),
    };
    let ontology_nt = match quads_to_nt(&base) {
        Ok(nt) => nt,
        Err(e) => return fail(e),
    };
    let instance_bytes = match std::fs::read(source) {
        Ok(b) => b,
        Err(e) => return fail(format!("cannot read {}: {e}", source.display())),
    };
    let instance_ds = match purrdf::parse_dataset(&instance_bytes, "text/turtle", None) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot parse {}: {e}", source.display())),
    };
    let instance_nt = match purrdf::serialize_dataset(
        &instance_ds,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    ) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return fail(format!("cannot project instance to N-Triples: {e}")),
    };
    let source_nt = format!("{ontology_nt}\n{instance_nt}");
    match projections::project_graph(&source_nt, &query, tags) {
        Ok(nt) => match nt_to_turtle(&nt) {
            Ok(ttl) => {
                write_or_fail(&out.join(format!("{profile}.ttl")), &ttl).map_or_else(|c| c, |()| 0)
            }
            Err(e) => fail(e),
        },
        Err(e) => fail(e),
    }
}

// ── transform ──────────────────────────────────────────────────────────────────

/// `gmeow-dev transform ABOX -o --profiles --diff-target --report --lang`.
#[allow(clippy::too_many_arguments)]
pub fn transform(
    abox: &Path,
    out: Option<&Path>,
    profiles: &str,
    diff_target: Option<&Path>,
    report: Option<&Path>,
    _lang: Option<&str>,
) -> i32 {
    let root = project_root();
    let snapshot = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let tags = tag_map(&snapshot);

    let known = projections::profiles();
    if profiles != "all" {
        let unknown: Vec<&str> = profiles
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty() && !known.contains_key(*p))
            .collect();
        if !unknown.is_empty() {
            return fail(format!(
                "unknown projection profile(s): {}",
                unknown.join(", ")
            ));
        }
    }

    let (up_inputs, maximal_inputs) = match assemble_inputs(&snapshot) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let (source_nt, stem) = match load_source_nt(abox) {
        Ok(pair) => pair,
        Err(e) => return fail(e),
    };
    let report_native =
        match projections::transpile_graph(&source_nt, &stem, &up_inputs, &maximal_inputs, &tags) {
            Ok(r) => r,
            Err(e) => return fail(e),
        };
    eprintln!(
        "lifted {} facts · claimed {} inferred · gap {}",
        report_native.lifted, report_native.claimed, report_native.gap_terms
    );

    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("dist").join("transform").join(&stem));
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return fail(format!("cannot create {}: {e}", out_dir.display()));
    }
    let gts_path = out_dir.join(format!("{stem}.gts"));
    if let Err(e) = std::fs::write(&gts_path, &report_native.transform.gts_bytes) {
        return fail(format!("cannot write {}: {e}", gts_path.display()));
    }
    println!("wrote {}", gts_path.display());

    // Optional vocabulary-coverage diff against a parity target.
    if let Some(target) = diff_target {
        let table = match vocab_diff(&report_native.transform.gts_bytes, target) {
            Ok(t) => t,
            Err(e) => return fail(e),
        };
        match report {
            Some(path) => {
                if let Err(e) = std::fs::write(path, &table) {
                    return fail(format!("cannot write {}: {e}", path.display()));
                }
                println!("coverage report → {}", path.display());
            }
            None => println!("{table}"),
        }
    }
    0
}

/// A vocabulary-coverage diff between the maximal transform output and a target.
fn vocab_diff(maximal_gts: &[u8], target: &Path) -> Result<String, String> {
    let maximal_ds = purrdf::gts::flattened_dataset_from_bytes(maximal_gts)
        .map_err(|e| format!("cannot fold transform output: {e}"))?;
    let target_bytes =
        std::fs::read(target).map_err(|e| format!("cannot read {}: {e}", target.display()))?;
    let target_ds = purrdf::parse_dataset(&target_bytes, "text/turtle", None)
        .map_err(|e| format!("cannot parse {}: {e}", target.display()))?;
    let max_preds = predicate_set(&maximal_ds);
    let tgt_preds = predicate_set(&target_ds);
    let covered = tgt_preds.intersection(&max_preds).count();
    let missing: Vec<&String> = tgt_preds.difference(&max_preds).collect();
    let mut out = format!(
        "| predicate coverage | {covered}/{} |\n|---|---|\n",
        tgt_preds.len()
    );
    for p in missing {
        out.push_str(&format!("| missing | {p} |\n"));
    }
    Ok(out)
}

/// The set of predicate IRIs present in a dataset.
fn predicate_set(ds: &purrdf::RdfDataset) -> std::collections::BTreeSet<String> {
    use purrdf::dataset_view::{DatasetView, GraphMatch};
    let mut preds = std::collections::BTreeSet::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let purrdf::TermRef::Iri(p) = ds.resolve(q.p) {
            preds.insert(p.to_owned());
        }
    }
    preds
}

// ── up-project ─────────────────────────────────────────────────────────────────

/// `gmeow-dev up-project SOURCE -o` — lift a consumer-vocabulary RDF file UP into
/// pure GMEOW.
pub fn up_project(source: &Path, out: Option<&Path>) -> i32 {
    let root = project_root();
    let snapshot = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let tags = tag_map(&snapshot);
    let (up_inputs, _maximal) = match assemble_inputs(&snapshot) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let (source_nt, _stem) = match load_source_nt(source) {
        Ok(pair) => pair,
        Err(e) => return fail(e),
    };
    let result = match projections::up_project(&source_nt, &up_inputs, &tags) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let ttl = match nt_to_turtle(&result.graph_nt) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    match out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &ttl) {
                return fail(format!("cannot write {}: {e}", path.display()));
            }
            eprintln!("wrote {}", path.display());
        }
        None => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&ttl);
        }
    }
    eprintln!(
        "lifted {} facts · claimed {} inferred · gap {} terms",
        result.lifted,
        result.claimed,
        result.gap_terms.len()
    );
    0
}

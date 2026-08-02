// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The consumer down-projection profile inventory gate.
//!
//! A file under `dsl/mappings/projections/` is a SHIPPED CONSUMER SURFACE: its
//! `gmeow:ProjectionMapping` cells are what `gmeow project --profile <name>`
//! executes to lower GMEOW data onto schema.org, FOAF, DCAT, PROV, … Nothing
//! downstream records that a profile FILE exists: several profile files bind the
//! same `gmeow:profile` name (`schema-org.ttl`, `schema-org-images.ttl`,
//! `schema-org-procedures.ttl` all bind `"schema-org"`) and therefore fold into ONE
//! generated `.rq` and ONE generated `.edoal.ttl`. So the pipeline's
//! `gmeow:expectsGeneratedOutput` inventory — which pins generated PATHS — cannot
//! see a deleted profile at all: the paths it lists are still produced, merely with
//! fewer branches. Deleting `dsl/mappings/projections/schema-org-procedures.ttl`
//! removed eight consumer cells and reddened nothing.
//!
//! This gate closes that, in the same independent-oracle shape as the
//! expected-output inventory: the authored `gmeow:ProjectionProfile` rows in
//! `dsl/mappings/projection-profiles.ttl` are a SECOND source from the directory
//! listing, so
//!
//! * a profile file deleted without its row → declared ⊄ on-disk → HARD FAIL;
//! * a profile file added without a row → on-disk ⊄ declared → HARD FAIL;
//! * a profile hollowed out below its `gmeow:profileCellMinimum` → HARD FAIL.
//!
//! The third is what keeps the gate honest about the failure the second one alone
//! would miss: a profile that still parses but no longer projects anything is the
//! same loss in a new place.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Repo-relative path of the authored projection-profile inventory.
pub const PROJECTION_PROFILES_PATH: &str = "dsl/mappings/projection-profiles.ttl";
/// Repo-relative directory the inventory must exactly cover.
pub const PROJECTIONS_DIR: &str = "dsl/mappings/projections";

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// One declared projection-profile source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionProfile {
    /// IRI of the `gmeow:ProjectionProfile` individual (identity).
    pub iri: String,
    /// `gmeow:profileSource` — the repo-relative path of the authored document.
    pub source: String,
    /// `gmeow:profileCellMinimum` — the raise-only floor on its cell count.
    pub cell_minimum: usize,
}

fn inventory_err(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-mappings".to_string(),
        message: format!("projection-profile inventory ({PROJECTION_PROFILES_PATH}): {message}"),
    })
}

/// Load the authored `gmeow:ProjectionProfile` inventory from `root`.
///
/// Every binding is mandatory and single-valued; a missing source path, a missing
/// or non-integer floor, a duplicate path, or an empty inventory is a HARD FAIL
/// (no-optionality) — a row that cannot be joined to a file gates nothing.
pub fn load_projection_profiles(root: &Path) -> Result<Vec<ProjectionProfile>, gmeow_errors::Diag> {
    let path = root.join(PROJECTION_PROFILES_PATH);
    let bytes =
        std::fs::read(&path).map_err(|e| inventory_err(format!("read {}: {e}", path.display())))?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| inventory_err(format!("parse {}: {e}", path.display())))?;

    let profile_type = format!("{GMEOW}ProjectionProfile");
    let source_p = format!("{GMEOW}profileSource");
    let minimum_p = format!("{GMEOW}profileCellMinimum");

    let mut subjects: BTreeSet<String> = BTreeSet::new();
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut minimums: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for quad in dataset.owned_quads() {
        let purrdf::RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        if quad.predicate == RDF_TYPE {
            if matches!(&quad.object, purrdf::RdfTerm::Iri(o) if *o == profile_type) {
                subjects.insert(subject.clone());
            }
            continue;
        }
        let field = if quad.predicate == source_p {
            &mut sources
        } else if quad.predicate == minimum_p {
            &mut minimums
        } else {
            continue;
        };
        let purrdf::RdfTerm::Literal(lit) = &quad.object else {
            return Err(inventory_err(format!(
                "{subject}: {} must carry a literal",
                quad.predicate
            )));
        };
        field
            .entry(subject.clone())
            .or_default()
            .push(lit.lexical_form.clone());
    }

    if subjects.is_empty() {
        return Err(inventory_err(
            "no gmeow:ProjectionProfile rows — the consumer down-projection inventory is empty"
                .to_string(),
        ));
    }

    let mut profiles: Vec<ProjectionProfile> = Vec::new();
    let mut seen_sources: BTreeSet<String> = BTreeSet::new();
    for iri in &subjects {
        let source = match sources.get(iri).map(Vec::as_slice) {
            Some([one]) => one.clone(),
            Some(many) => {
                return Err(inventory_err(format!(
                    "{iri} has {} gmeow:profileSource values — a profile row names exactly one \
                     document",
                    many.len()
                )));
            }
            None => return Err(inventory_err(format!("{iri} has no gmeow:profileSource"))),
        };
        if !source.starts_with(PROJECTIONS_DIR) {
            return Err(inventory_err(format!(
                "{iri}: gmeow:profileSource {source:?} is not under {PROJECTIONS_DIR}/"
            )));
        }
        if !seen_sources.insert(source.clone()) {
            return Err(inventory_err(format!(
                "{iri}: gmeow:profileSource {source:?} is declared more than once (the inventory \
                 is a set, and a duplicate would let one row mask another's deletion)"
            )));
        }
        let cell_minimum = match minimums.get(iri).map(Vec::as_slice) {
            Some([one]) => one.parse::<usize>().map_err(|_| {
                inventory_err(format!(
                    "{iri}: gmeow:profileCellMinimum {one:?} is not a non-negative integer"
                ))
            })?,
            Some(many) => {
                return Err(inventory_err(format!(
                    "{iri} has {} gmeow:profileCellMinimum values — the floor is single-valued",
                    many.len()
                )));
            }
            None => {
                return Err(inventory_err(format!(
                    "{iri} ({source}) has no gmeow:profileCellMinimum"
                )));
            }
        };
        profiles.push(ProjectionProfile {
            iri: iri.clone(),
            source,
            cell_minimum,
        });
    }
    profiles.sort_by(|a, b| a.source.cmp(&b.source));
    Ok(profiles)
}

/// The `.ttl` documents actually present under `dsl/mappings/projections/`, as
/// repo-relative paths with `/` separators.
fn on_disk_profiles(root: &Path) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let dir = root.join(PROJECTIONS_DIR);
    let mut found: BTreeSet<String> = BTreeSet::new();
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| inventory_err(format!("read dir {}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| inventory_err(format!("read dir {}: {e}", dir.display())))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ttl") {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| {
            inventory_err(format!("non-UTF-8 profile file name in {}", dir.display()))
        })?;
        found.insert(format!("{PROJECTIONS_DIR}/{name}"));
    }
    Ok(found)
}

/// Count the `gmeow:ProjectionMapping` cells a profile document declares.
fn authored_cell_count(root: &Path, source: &str) -> Result<usize, gmeow_errors::Diag> {
    let path = root.join(source);
    let bytes =
        std::fs::read(&path).map_err(|e| inventory_err(format!("read {}: {e}", path.display())))?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| inventory_err(format!("parse {}: {e}", path.display())))?;
    let cell_type = format!("{GMEOW}ProjectionMapping");
    let mut cells: BTreeSet<String> = BTreeSet::new();
    for quad in dataset.owned_quads() {
        if quad.predicate != RDF_TYPE {
            continue;
        }
        let (purrdf::RdfTerm::Iri(subject), purrdf::RdfTerm::Iri(object)) =
            (&quad.subject, &quad.object)
        else {
            continue;
        };
        if *object == cell_type {
            cells.insert(subject.clone());
        }
    }
    Ok(cells.len())
}

/// Run the inventory gate: declared profile paths must EQUAL the on-disk set, and
/// every profile's authored cell count must hold at or above its floor.
///
/// Returns the measured per-profile cell counts on success.
pub fn check_projection_profile_inventory(
    root: &Path,
) -> Result<BTreeMap<String, usize>, gmeow_errors::Diag> {
    let declared = load_projection_profiles(root)?;
    let declared_paths: BTreeSet<String> = declared.iter().map(|p| p.source.clone()).collect();
    let on_disk = on_disk_profiles(root)?;

    let missing: Vec<&str> = declared_paths
        .difference(&on_disk)
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(inventory_err(format!(
            "{} declared consumer projection profile(s) are no longer on disk: {} — a consumer \
             down-projection surface was removed; restore it, or retire its gmeow:ProjectionProfile \
             row deliberately",
            missing.len(),
            missing.join(", ")
        )));
    }
    let unregistered: Vec<&str> = on_disk
        .difference(&declared_paths)
        .map(String::as_str)
        .collect();
    if !unregistered.is_empty() {
        return Err(inventory_err(format!(
            "{} projection profile document(s) are not declared in the inventory: {} — mint a \
             gmeow:ProjectionProfile row with its cell floor",
            unregistered.len(),
            unregistered.join(", ")
        )));
    }

    let mut measured: BTreeMap<String, usize> = BTreeMap::new();
    let mut below: Vec<String> = Vec::new();
    for profile in &declared {
        let count = authored_cell_count(root, &profile.source)?;
        measured.insert(profile.source.clone(), count);
        if count < profile.cell_minimum {
            below.push(format!(
                "{} declares {count} gmeow:ProjectionMapping cell(s) < \
                 gmeow:profileCellMinimum {}",
                profile.source, profile.cell_minimum
            ));
        }
    }
    if !below.is_empty() {
        return Err(inventory_err(format!(
            "{} projection profile cell-count ratchet(s) breached: {}",
            below.len(),
            below.join("; ")
        )));
    }
    Ok(measured)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repo root")
    }

    #[test]
    fn the_live_inventory_matches_the_live_projections_tree() {
        let measured =
            check_projection_profile_inventory(&repo_root()).expect("live inventory is exact");
        assert!(
            measured.len() >= 34,
            "the consumer projection-profile inventory shrank: {} profiles",
            measured.len()
        );
        assert_eq!(
            measured
                .get("dsl/mappings/projections/schema-org-procedures.ttl")
                .copied(),
            Some(8),
            "the schema.org HowTo/Recipe consumer profile must declare its eight cells"
        );
    }

    /// A deleted profile is caught by the declared ⊆ on-disk direction, and an
    /// unregistered new one by the reverse direction. Exercised over a temp copy of
    /// the live tree so the demonstration is the REAL gate over the REAL inventory,
    /// never a synthetic stand-in.
    #[test]
    fn deleting_a_profile_reds_and_adding_an_undeclared_one_reds() {
        let root = repo_root();
        let temp = tempfile::tempdir().expect("temp dir");
        let staged = temp.path().join("repo");
        std::fs::create_dir_all(staged.join(PROJECTIONS_DIR)).expect("staged tree");
        std::fs::copy(
            root.join(PROJECTION_PROFILES_PATH),
            staged.join(PROJECTION_PROFILES_PATH),
        )
        .expect("stage inventory");
        for path in on_disk_profiles(&root).expect("live profiles") {
            std::fs::copy(root.join(&path), staged.join(&path)).expect("stage profile");
        }
        check_projection_profile_inventory(&staged).expect("the staged copy is exact");

        let victim = staged
            .join(PROJECTIONS_DIR)
            .join("schema-org-procedures.ttl");
        std::fs::remove_file(&victim).expect("delete a profile");
        let error = check_projection_profile_inventory(&staged)
            .expect_err("deleting a consumer profile must red");
        assert!(
            error.to_string().contains("no longer on disk")
                && error.to_string().contains("schema-org-procedures.ttl"),
            "unexpected message: {error}"
        );

        std::fs::copy(
            root.join(PROJECTIONS_DIR).join("schema-org-procedures.ttl"),
            &victim,
        )
        .expect("restore the profile");
        check_projection_profile_inventory(&staged).expect("restored copy is exact again");

        std::fs::copy(&victim, staged.join(PROJECTIONS_DIR).join("undeclared.ttl"))
            .expect("add an undeclared profile");
        let error = check_projection_profile_inventory(&staged)
            .expect_err("an undeclared profile must red");
        assert!(
            error.to_string().contains("not declared in the inventory"),
            "unexpected message: {error}"
        );
    }

    /// Hollowing a profile out — the file survives, its cells do not — reds on the
    /// cell-count ratchet, so "parses but projects nothing" is not a silent pass.
    #[test]
    fn hollowing_a_profile_out_reds_on_the_cell_floor() {
        let root = repo_root();
        let temp = tempfile::tempdir().expect("temp dir");
        let staged = temp.path().join("repo");
        std::fs::create_dir_all(staged.join(PROJECTIONS_DIR)).expect("staged tree");
        std::fs::copy(
            root.join(PROJECTION_PROFILES_PATH),
            staged.join(PROJECTION_PROFILES_PATH),
        )
        .expect("stage inventory");
        for path in on_disk_profiles(&root).expect("live profiles") {
            std::fs::copy(root.join(&path), staged.join(&path)).expect("stage profile");
        }
        std::fs::write(
            staged
                .join(PROJECTIONS_DIR)
                .join("schema-org-procedures.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
        )
        .expect("hollow the profile out");
        let error =
            check_projection_profile_inventory(&staged).expect_err("an emptied profile must red");
        assert!(
            error.to_string().contains("cell-count ratchet"),
            "unexpected message: {error}"
        );
    }
}

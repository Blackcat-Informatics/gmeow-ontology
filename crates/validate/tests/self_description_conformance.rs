// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native citation & self-description conformance.
//!
//! Recreates the three checks that were *retained in Python and never migrated* when
//! `tests/test_citations.py` was purged — they are pure business-logic / cross-format
//! assertions over the authored self-model, not SHACL runs, so they have no conformance
//! twin in `conformance_citations.rs`. Reconstructed here natively in Rust, on the
//! `make check` gate, reading the authored files directly (no Python):
//!
//!   * `self_description_loader_pins_fields`         ← `test_self_description_loader`
//!   * `models_project_repository_and_brand_assets`  ← `test_self_description_models_project_repository_and_brand_assets`
//!   * `canonical_abstract_is_standardized`          ← `test_canonical_description_is_standardized`
//!
//! Every source read here is an **authored** file (`metadata/gmeow-self.ttl`,
//! `ontology/gmeow.ttl`, `CITATION.cff`) or an authored in-crate constant
//! (`deposit_config::ALIGNMENT_TARGETS`) — never a `generated/` projection — so the gate
//! is self-contained on a fresh checkout with no prior regeneration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_validate::self_desc::deposit_config::ALIGNMENT_TARGETS;
use gmeow_validate::self_desc::{default_self_desc_path, load_self_description};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

// ── Namespaces ────────────────────────────────────────────────────────────────
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const SELF: &str = "https://blackcatinformatics.ca/gmeow/self#";
/// The Work IRI (FRBR spine root; also the ontology subject in `ontology/gmeow.ttl`).
const WORK: &str = "https://blackcatinformatics.ca/gmeow";
const BII: &str = "https://blackcatinformatics.ca/#bii";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DCTERMS_DESCRIPTION: &str = "http://purl.org/dc/terms/description";

/// Repo root: `crates/validate/tests/…` → three ancestors up (mirrors the sibling tests).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn g(local: &str) -> String {
    format!("{GMEOW}{local}")
}
fn s(local: &str) -> String {
    format!("{SELF}{local}")
}

/// Collapse whitespace runs to single spaces and trim, so the three abstract copies
/// compare equal regardless of authored wrap style (the CFF `abstract` is a YAML folded
/// block `>-`). Faithful to the retired Python test's `_norm_ws`.
fn norm_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse(path: &Path) -> Arc<RdfDataset> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    purrdf::parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

// ── Triple-membership helpers over the authored dataset ─────────────────────────

/// Whether the IRI-object triple `<s> <p> <o>` is asserted. A missing term id means the
/// triple cannot exist (so the negative assertion sees an absent `gmeow:depicts` as absent).
fn has_iri_triple(ds: &RdfDataset, subj: &str, pred: &str, obj: &str) -> bool {
    let (Some(sid), Some(pid), Some(oid)) = (
        ds.term_id_by_value(&TermValue::iri(subj)),
        ds.term_id_by_value(&TermValue::iri(pred)),
        ds.term_id_by_value(&TermValue::iri(obj)),
    ) else {
        return false;
    };
    ds.quads_for_pattern(Some(sid), Some(pid), Some(oid), GraphMatch::Any)
        .next()
        .is_some()
}

/// Whether `<s> <p>` is asserted with *any* object. Used for predicate-absence: the logo
/// must carry no `gmeow:depicts` assertion at all, not merely none pointing at the Work.
fn has_predicate(ds: &RdfDataset, subj: &str, pred: &str) -> bool {
    let (Some(sid), Some(pid)) = (
        ds.term_id_by_value(&TermValue::iri(subj)),
        ds.term_id_by_value(&TermValue::iri(pred)),
    ) else {
        return false;
    };
    ds.quads_for_pattern(Some(sid), Some(pid), None, GraphMatch::Any)
        .next()
        .is_some()
}

/// Whether `<s> <p>` has a literal object whose lexical form equals `lex`.
fn has_literal_triple(ds: &RdfDataset, subj: &str, pred: &str, lex: &str) -> bool {
    let (Some(sid), Some(pid)) = (
        ds.term_id_by_value(&TermValue::iri(subj)),
        ds.term_id_by_value(&TermValue::iri(pred)),
    ) else {
        return false;
    };
    ds.quads_for_pattern(Some(sid), Some(pid), None, GraphMatch::Any)
        .any(|q| matches!(ds.resolve(q.o), TermRef::Literal { lexical, .. } if lexical == lex))
}

/// The first literal object of `<s> <p>` (its lexical form), if any.
fn first_literal(ds: &RdfDataset, subj: &str, pred: &str) -> Option<String> {
    let sid = ds.term_id_by_value(&TermValue::iri(subj))?;
    let pid = ds.term_id_by_value(&TermValue::iri(pred))?;
    ds.quads_for_pattern(Some(sid), Some(pid), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_string()),
            _ => None,
        })
}

// ── Test A: loader field assertions (← test_self_description_loader) ─────────────

#[test]
fn self_description_loader_pins_fields() {
    let sd = load_self_description(&default_self_desc_path(&repo_root()))
        .expect("metadata/gmeow-self.ttl loads as a SelfDescription");

    assert!(
        sd.title.starts_with("GMEOW"),
        "title must start with GMEOW, got {:?}",
        sd.title
    );
    assert_eq!(sd.version, "0.1.0");
    assert_eq!(sd.release_date, "2026-06-03");
    assert_eq!(sd.concept_doi, "10.67342/26w4o");
    assert_eq!(sd.version_doi, None);
    assert_eq!(sd.doi(), "10.67342/26w4o");
    assert_eq!(sd.version_iri, "https://blackcatinformatics.ca/gmeow/0.1.0");
    assert_eq!(sd.depositor_name, "Blackcat Informatics® Inc.");
    assert_eq!(sd.registrant, "Blackcat Informatics® Inc.");
    assert_eq!(sd.depositor_email, "root@blackcatinformatics.ca");
    assert_eq!(
        sd.license_uri,
        "https://creativecommons.org/licenses/by/4.0/"
    );
    assert_eq!(sd.homepage, "https://blackcatinformatics.ca/gmeow");
}

// ── Test B: project / repository / license / brand-asset triple membership ──────
//    (← test_self_description_models_project_repository_and_brand_assets)

#[test]
fn models_project_repository_and_brand_assets() {
    let ds = parse(&default_self_desc_path(&repo_root()));

    // SoftwareProject five-facet model.
    assert!(has_iri_triple(
        &ds,
        &s("project"),
        RDF_TYPE,
        &g("SoftwareProject")
    ));
    assert!(has_iri_triple(
        &ds,
        &s("project"),
        &g("hasRepository"),
        &s("repository")
    ));
    assert!(has_iri_triple(
        &ds,
        &s("project"),
        &g("maintenanceStatus"),
        &g("statusActive")
    ));
    assert!(has_iri_triple(
        &ds,
        &s("project"),
        &g("projectLicense"),
        &s("license-agpl-3")
    ));

    // License.
    assert!(has_iri_triple(
        &ds,
        &s("license-agpl-3"),
        RDF_TYPE,
        &g("License")
    ));
    assert!(has_iri_triple(
        &ds,
        &s("license-agpl-3"),
        &g("licensor"),
        BII
    ));

    // Repository.
    assert!(has_iri_triple(
        &ds,
        &s("repository"),
        RDF_TYPE,
        &g("Repository")
    ));
    assert!(has_iri_triple(
        &ds,
        &s("repository"),
        &g("repositoryType"),
        &g("repoTypeGit")
    ));

    // Brand assets: logo is a first-class MediaObject linked from BOTH the project and
    // the Work, is SVG, and is emblematic — NOT a depiction of the ontology.
    assert!(has_iri_triple(
        &ds,
        &s("project"),
        &g("hasLogo"),
        &s("logo-svg")
    ));
    assert!(has_iri_triple(&ds, WORK, &g("hasLogo"), &s("logo-svg")));
    assert!(has_iri_triple(
        &ds,
        &s("logo-svg"),
        RDF_TYPE,
        &g("MediaObject")
    ));
    assert!(has_literal_triple(
        &ds,
        &s("logo-svg"),
        &g("mediaType"),
        "image/svg+xml"
    ));
    // Negative: the logo is the emblem OF GMEOW, it does not gmeow:depicts anything —
    // predicate-absence, so a stray `depicts <other>` cannot slip past the invariant.
    assert!(
        !has_predicate(&ds, &s("logo-svg"), &g("depicts")),
        "logo-svg must carry no gmeow:depicts assertion (it is an emblem, not a depiction)"
    );

    // Social preview PNG derived from the SVG.
    assert!(has_iri_triple(
        &ds,
        &s("social-preview-png"),
        RDF_TYPE,
        &g("MediaObject")
    ));
    assert!(has_iri_triple(
        &ds,
        &s("social-preview-png"),
        &g("wasDerivedFrom"),
        &s("social-preview-svg")
    ));
}

// ── Test C: one canonical abstract, standardized across surfaces ────────────────
//    (← test_canonical_description_is_standardized)

/// True iff the text hard-codes a slice count, i.e. matches `\d+\s+self-contained slices`
/// (a digit run, then whitespace, then the phrase). Implemented without a regex dependency;
/// the slice count must live in the manifest tier, never in prose that would drift.
fn hard_codes_slice_count(text: &str) -> bool {
    let needle = "self-contained slices";
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find(needle) {
        let at = from + rel;
        let mut i = at;
        while i > 0 && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        // `i < at` ⇒ at least one whitespace char (\s+); preceding a digit ⇒ \d+ before it.
        if i < at && i > 0 && bytes[i - 1].is_ascii_digit() {
            return true;
        }
        from = at + needle.len();
    }
    false
}

#[test]
fn canonical_abstract_is_standardized() {
    let canonical = load_self_description(&default_self_desc_path(&repo_root()))
        .expect("self-description loads")
        .description;
    assert!(
        !canonical.is_empty(),
        "self-description carries no description"
    );

    // Ontology header dcterms:description == canonical (the serialization-facing copy).
    let onto = parse(&repo_root().join("ontology/gmeow.ttl"));
    let onto_desc = first_literal(&onto, WORK, DCTERMS_DESCRIPTION)
        .expect("ontology/gmeow.ttl carries dcterms:description on the ontology subject");
    assert_eq!(
        norm_ws(&onto_desc),
        norm_ws(&canonical),
        "ontology/gmeow.ttl dcterms:description drifted from the canonical abstract"
    );

    // CITATION.cff abstract == canonical (the human-citation copy).
    let cff_text =
        std::fs::read_to_string(repo_root().join("CITATION.cff")).expect("CITATION.cff readable");
    let cff: serde_yaml::Value =
        serde_yaml::from_str(&cff_text).expect("CITATION.cff parses as YAML");
    let cff_abstract = cff
        .get("abstract")
        .and_then(serde_yaml::Value::as_str)
        .expect("CITATION.cff carries an `abstract`");
    assert_eq!(
        norm_ws(cff_abstract),
        norm_ws(&canonical),
        "CITATION.cff abstract drifted from the canonical abstract"
    );

    // The stated external-vocabulary count must equal the real authored count. Faithful
    // to the retired Python check `f"{len(ALIGNMENT_TARGETS)} external vocabularies"`:
    // ALIGNMENT_TARGETS is the in-crate authored alignment list (the Python port), so the
    // advertised number can never silently drift from what GMEOW actually aligns to.
    let stated = format!("{} external vocabularies", ALIGNMENT_TARGETS.len());
    // Require a numeric left boundary: a bare `contains` would let a wrong larger count
    // pass (e.g. "186 external vocabularies" contains "86 external vocabularies"). The
    // matched digit run must not be the tail of a longer number.
    let states_exact_count = canonical.match_indices(&stated).any(|(at, _)| {
        canonical[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_digit())
    });
    assert!(
        states_exact_count,
        "canonical abstract must state {stated:?} (== ALIGNMENT_TARGETS.len())"
    );

    // The slice count is deliberately NOT stated in prose — it would drift as slices are
    // added; the manifest tier is the sole source of slice truth.
    assert!(
        canonical.contains("self-contained slices"),
        "canonical abstract must mention 'self-contained slices'"
    );
    assert!(
        !hard_codes_slice_count(&canonical),
        "slice count must not be hard-coded in the canonical description"
    );
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-corpus vendoring metadata + license audit (#753).
//!
//! Every vendored external corpus carries a `corpus.json` at
//! `conformance/logic/cases/external/<corpus>/corpus.json` declaring its SPDX
//! license, upstream source, pinned version/commit, refresh command, and lane. The
//! license is audited against the native [`crate::license`] policy BEFORE a corpus is
//! vendored: an IMPORT_OK license may be committed under `cases/external/`; a
//! REFERENCE_ONLY (or unknown) license is a hard error — such corpora may only be
//! fetched live in the Lane-B (`make maint-classic-cross-check`) lane.
//!
//! Parsing is manual + hard-fail (matching `profile.rs`): a missing required field,
//! a wrong type, an unknown key, or an unknown lane is an error — never a silent
//! default.

use serde_json::{Map, Value};

use crate::license::{policy_for_license, LicensePolicy};

/// Which run lane a corpus targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// The fast, required native gate (`make conformance`): small, sub-second,
    /// deterministic, decided natively, and AGREEING with the external source.
    A,
    /// The heavy, non-required oracle lane (`make maint-classic-cross-check`):
    /// full corpora, Docker-allowed, routed through the divergence ledger.
    B,
    /// The named honest-DlGap quarantine: cases the native engine cannot soundly
    /// decide (or decides while disagreeing with the source). The committed
    /// verdict is the FROZEN NATIVE verdict, NOT the source-declared one, so the
    /// `committed == declared` soundness check skips this lane; a dedicated
    /// divergence gate pins each case exactly instead. Fast + sub-second like
    /// Lane A (consistency checks), but deliberately divergent by construction.
    Divergence,
}

impl Lane {
    fn parse(s: &str) -> Result<Lane, String> {
        match s {
            "a" | "A" => Ok(Lane::A),
            "b" | "B" => Ok(Lane::B),
            "divergence" => Ok(Lane::Divergence),
            other => Err(format!(
                "corpus.json lane must be \"a\", \"b\", or \"divergence\", got {other:?}"
            )),
        }
    }
}

/// Parsed `corpus.json` metadata for one vendored external corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusMeta {
    /// The corpus name (matches the `cases/external/<corpus>/` directory).
    pub name: String,
    /// SPDX license identifier of the vendored artifacts (audited before vendoring).
    pub spdx_license: String,
    /// Upstream source URL.
    pub source_url: String,
    /// The pinned upstream version or commit the snapshot was taken from.
    pub version_or_commit: String,
    /// The reproducible command that refreshes the snapshot.
    pub refresh_command: String,
    /// The run lane this corpus targets.
    pub lane: Lane,
}

/// The keys a `corpus.json` may carry (closed surface; unknown keys hard-fail).
const ALLOWED_KEYS: [&str; 6] = [
    "name",
    "spdx_license",
    "source_url",
    "version_or_commit",
    "refresh_command",
    "lane",
];

/// Parse and validate a `corpus.json` value (hard-fail).
pub fn parse_corpus_meta(value: &Value) -> Result<CorpusMeta, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "corpus.json must be a JSON object".to_string())?;

    let mut unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| !ALLOWED_KEYS.contains(k))
        .collect();
    unknown.sort_unstable();
    if !unknown.is_empty() {
        return Err(format!(
            "corpus.json has unknown key(s) {unknown:?}; allowed keys are {ALLOWED_KEYS:?}"
        ));
    }

    let string_field = |key: &str| -> Result<String, String> { required_string(obj, key) };

    let lane = Lane::parse(&string_field("lane")?)?;
    Ok(CorpusMeta {
        name: string_field("name")?,
        spdx_license: string_field("spdx_license")?,
        source_url: string_field("source_url")?,
        version_or_commit: string_field("version_or_commit")?,
        refresh_command: string_field("refresh_command")?,
        lane,
    })
}

/// Read and parse a `corpus.json` file from disk.
pub fn load_corpus_meta(path: &std::path::Path) -> Result<CorpusMeta, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
    parse_corpus_meta(&value).map_err(|e| format!("{}: {e}", path.display()))
}

/// Audit a corpus's declared license: an IMPORT_OK license may be vendored; a
/// REFERENCE_ONLY (or unknown) license is a hard error.
///
/// This is the separately-testable precondition the lowering / ingest binary calls
/// BEFORE writing a vendored case (kept out of the pure `lower` transform).
pub fn audit_vendorable(meta: &CorpusMeta) -> Result<(), String> {
    match policy_for_license(&meta.spdx_license) {
        LicensePolicy::ImportOk => Ok(()),
        LicensePolicy::ReferenceOnly => Err(format!(
            "corpus {:?} declares license {:?}, which is REFERENCE_ONLY — it cannot be vendored \
             under cases/external/. Such corpora may only be fetched live in the Lane-B \
             (make maint-classic-cross-check) lane and never committed.",
            meta.name, meta.spdx_license
        )),
    }
}

/// Resolve the run [`Lane`] for a case directory, if it belongs to a vendored
/// external corpus.
///
/// A case under `cases/external/<corpus>/<case>/` carries its corpus metadata at the
/// parent `corpus.json`; this returns that corpus's declared lane. An endogenous case
/// (`cases/<category>/<case>/`, no parent `corpus.json`) returns `None` — it is always
/// run by the native gate. This keeps `crate::discover` category-agnostic: lane is
/// resolved by the runner, not baked into discovery.
///
/// # Errors
/// Hard-fails (no-optionality) when a parent `corpus.json` exists but is unreadable or
/// invalid — a malformed corpus metadata file is an error, never a silent skip.
pub fn lane_for_case(case_dir: &std::path::Path) -> Result<Option<Lane>, String> {
    let Some(parent) = case_dir.parent() else {
        return Ok(None);
    };
    let corpus_json = parent.join("corpus.json");
    if !corpus_json.is_file() {
        return Ok(None);
    }
    Ok(Some(load_corpus_meta(&corpus_json)?.lane))
}

fn required_string(obj: &Map<String, Value>, key: &str) -> Result<String, String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("corpus.json is missing the required string field {key:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta_value(license: &str, lane: &str) -> Value {
        json!({
            "name": "tiny",
            "spdx_license": license,
            "source_url": "https://example.org/tiny",
            "version_or_commit": "v1",
            "refresh_command": "cargo run -p gmeow-conformance --bin ingest-external -- ...",
            "lane": lane,
        })
    }

    #[test]
    fn parses_a_well_formed_corpus_json() {
        let m = parse_corpus_meta(&meta_value("CC-BY-4.0", "a")).unwrap();
        assert_eq!(m.name, "tiny");
        assert_eq!(m.spdx_license, "CC-BY-4.0");
        assert_eq!(m.lane, Lane::A);
    }

    #[test]
    fn import_ok_corpus_passes_the_audit() {
        let m = parse_corpus_meta(&meta_value("CC-BY-4.0", "a")).unwrap();
        assert!(audit_vendorable(&m).is_ok());
    }

    #[test]
    fn reference_only_corpus_fails_the_audit() {
        let m = parse_corpus_meta(&meta_value("CC-BY-NC-SA-4.0", "b")).unwrap();
        let err = audit_vendorable(&m).unwrap_err();
        assert!(err.contains("REFERENCE_ONLY"), "{err}");
    }

    #[test]
    fn unknown_license_fails_the_audit() {
        let m = parse_corpus_meta(&meta_value("WTFPL", "a")).unwrap();
        assert!(audit_vendorable(&m).is_err());
    }

    #[test]
    fn unknown_lane_hard_fails() {
        let err = parse_corpus_meta(&meta_value("CC-BY-4.0", "c")).unwrap_err();
        assert!(err.contains("lane must be"), "{err}");
    }

    #[test]
    fn divergence_lane_parses() {
        let m = parse_corpus_meta(&meta_value("W3C", "divergence")).unwrap();
        assert_eq!(m.lane, Lane::Divergence);
    }

    #[test]
    fn missing_field_hard_fails() {
        let err = parse_corpus_meta(&json!({ "name": "tiny" })).unwrap_err();
        assert!(err.contains("missing the required string field"), "{err}");
    }

    #[test]
    fn unknown_key_hard_fails() {
        let mut v = meta_value("CC-BY-4.0", "a");
        v.as_object_mut().unwrap().insert("nope".into(), json!(1));
        let err = parse_corpus_meta(&v).unwrap_err();
        assert!(err.contains("unknown key"), "{err}");
    }

    /// `lane_for_case` is the consumer that makes the `lane` field load-bearing: the
    /// Lane-A native runners skip a case iff this returns `Some(Lane::B)`. Exercise all
    /// three branches over a synthetic corpus tree (no new dev-dependency: plain
    /// `std::fs` under a pid-unique temp dir).
    #[test]
    fn lane_for_case_routes_external_corpora_and_ignores_endogenous() {
        use std::fs;

        fn corpus_json(name: &str, lane: &str) -> String {
            format!(
                "{{ \"name\": \"{name}\", \"spdx_license\": \"CC-BY-4.0\", \
                 \"source_url\": \"https://example.org/{name}\", \
                 \"version_or_commit\": \"v1\", \"refresh_command\": \"noop\", \
                 \"lane\": \"{lane}\" }}\n"
            )
        }

        let base = std::env::temp_dir().join(format!("gmeow-conf-lane-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        // External Lane-B corpus: a case here must be skipped by the native gate.
        let case_b = base.join("external/heavy-corpus/some-case");
        fs::create_dir_all(&case_b).unwrap();
        fs::write(
            base.join("external/heavy-corpus/corpus.json"),
            corpus_json("heavy-corpus", "b"),
        )
        .unwrap();

        // External Lane-A corpus: a case here runs in the native gate.
        let case_a = base.join("external/light-corpus/case-a");
        fs::create_dir_all(&case_a).unwrap();
        fs::write(
            base.join("external/light-corpus/corpus.json"),
            corpus_json("light-corpus", "a"),
        )
        .unwrap();

        // Endogenous case: no parent corpus.json → always native, never skipped.
        let endo = base.join("profiles/plain-case");
        fs::create_dir_all(&endo).unwrap();

        assert_eq!(lane_for_case(&case_b).unwrap(), Some(Lane::B));
        assert_eq!(lane_for_case(&case_a).unwrap(), Some(Lane::A));
        assert_eq!(lane_for_case(&endo).unwrap(), None);

        let _ = fs::remove_dir_all(&base);
    }
}

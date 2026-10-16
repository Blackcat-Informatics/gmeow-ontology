// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed view of `helpers-ledger.toml`.
//!
//! The file's header comment is the schema. Every key a row needs is required and
//! every key this reader does not know is refused: a misspelt `enforcd = true`
//! must fail loudly rather than leave a job silently unenforced.

use std::collections::BTreeMap;

use gmeow_errors::diag::Diag;

use crate::error::{Operation, admission, caused};
use crate::toml::{self, Value};

/// Why a second implementation of a job may exist.
pub(crate) const CRITERIA: [&str; 4] = ["a", "b", "c", "layering"];

/// One sanctioned second implementation. Its `criterion` and `reason`
/// are validated here and checked against the tree by the engine.
#[derive(Clone, Debug)]
pub(crate) struct Variant {
    /// The variant's own symbol, `crate::path::item`.
    pub(crate) symbol: String,
    /// The repo-relative file that defines it.
    pub(crate) file: String,
    /// Which detector it is exempt from: `isomorphic` and `forbidden` are this
    /// census's; any other id names a rule of the gate that owns it.
    pub(crate) detector: String,
    /// The documented item that states why the variant exists.
    pub(crate) anchor: String,
}

/// What a job forbids outside its home crate.
#[derive(Clone, Debug, Default)]
pub(crate) struct Forbidden {
    /// Integer literals, any spelling; compared by value.
    pub(crate) constants: Vec<u128>,
    /// `body:…`, `shim:…` or `table:…` fingerprint ids.
    pub(crate) fingerprints: Vec<String>,
    /// Regular expressions over function names.
    pub(crate) names: Vec<regex::Regex>,
}

/// One job: a piece of functionality with exactly one home.
#[derive(Clone, Debug)]
pub(crate) struct Job {
    /// The row id `--census` and `--count` take.
    pub(crate) id: String,
    /// The single implementation, `crate::path::item`.
    pub(crate) home: String,
    /// Further public entry points of the same implementation.
    pub(crate) entry_points: Vec<String>,
    /// Whether a forbidden match outside the home fails the gate today.
    pub(crate) enforced: bool,
    /// What is forbidden outside the home.
    pub(crate) forbidden: Forbidden,
    /// Sanctioned second implementations.
    pub(crate) variants: Vec<Variant>,
}

/// Which grouping a [`Distinct`] row names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GroupKind {
    /// An isomorphic-bodies group: one structural body fingerprint.
    Body,
    /// An isomorphic-shims group: one forwarder fingerprint.
    Shim,
}

impl GroupKind {
    /// The ledger spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Shim => "shim",
        }
    }
}

/// One set of items that share a body or forwarder shape and share no job.
///
/// It sanctions exactly one group: the group of its kind whose member set is
/// `members`, no more and no fewer. A new item of the same shape joins the group,
/// the sets differ, and the group is reported again.
#[derive(Clone, Debug)]
pub(crate) struct Distinct {
    /// The group's member symbols, `crate::path::item`, sorted. A symbol two
    /// members share (two trait impls on one type) is listed once per member.
    pub(crate) members: Vec<String>,
    /// Which grouping the set is a group of.
    pub(crate) kind: GroupKind,
    /// The documented item that states why the members are separate.
    pub(crate) anchor: String,
}

impl Distinct {
    /// The row's name in a finding: its kind and its members.
    pub(crate) fn label(&self) -> String {
        format!(
            "[[distinct]] {} [{}]",
            self.kind.as_str(),
            self.members.join(", ")
        )
    }
}

/// The whole ledger.
#[derive(Clone, Debug)]
pub(crate) struct Ledger {
    /// Rows in file order.
    pub(crate) jobs: Vec<Job>,
    /// `[[distinct]]` rows in file order.
    pub(crate) distinct: Vec<Distinct>,
    /// The parsed document, for the engine's cross-check.
    pub(crate) document: BTreeMap<String, Value>,
}

impl Ledger {
    /// The job with this id.
    pub(crate) fn job(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }
}

const JOB_KEYS: [&str; 12] = [
    "id",
    "summary",
    "home",
    "entry_points",
    "spec",
    "vectors",
    "bench",
    "sites",
    "replaces_external",
    "enforced",
    "forbidden",
    "variant",
];
const JOB_OPTIONAL: [&str; 1] = ["variant"];
const FORBIDDEN_KEYS: [&str; 3] = ["constants", "fingerprints", "names"];
const DISTINCT_KEYS: [&str; 5] = ["members", "kind", "criterion", "anchor", "reason"];
const VARIANT_KEYS: [&str; 6] = [
    "symbol",
    "file",
    "detector",
    "criterion",
    "anchor",
    "reason",
];

/// Parse the ledger text.
pub(crate) fn parse(text: &str) -> Result<Ledger, Diag> {
    let document = toml::parse(text).map_err(|error| {
        let line = error.line as u64;
        let mut diagnostic = caused(Operation::LedgerParse, error);
        diagnostic.inner_mut().source_ctx.location.line = Some(line);
        diagnostic
    })?;
    for key in document.keys() {
        if key != "job" && key != "distinct" {
            return Err(admission(
                Operation::LedgerSchema,
                format!("helpers-ledger.toml: unknown top-level key `{key}`"),
            ));
        }
    }
    let rows = match document.get("job") {
        None => Vec::new(),
        Some(Value::Array(rows)) => rows.clone(),
        Some(_) => {
            return Err(admission(
                Operation::LedgerSchema,
                "helpers-ledger.toml: `job` must be an array of tables".to_owned(),
            ));
        }
    };
    let mut jobs = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let Value::Table(row) = row else {
            return Err(admission(
                Operation::LedgerSchema,
                format!("job #{index} is not a table"),
            ));
        };
        let job = job(row).map_err(|error| {
            error.with_context(format!("helpers-ledger.toml job #{}", index + 1))
        })?;
        if jobs.iter().any(|seen: &Job| seen.id == job.id) {
            return Err(admission(
                Operation::LedgerSchema,
                format!("helpers-ledger.toml: job id `{}` is used twice", job.id),
            ));
        }
        jobs.push(job);
    }
    let distinct = match document.get("distinct") {
        None => Vec::new(),
        Some(Value::Array(rows)) => rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                match row {
                    Value::Table(row) => distinct(row),
                    _ => Err(admission(Operation::LedgerSchema, "not a table".to_owned())),
                }
                .map_err(|error| {
                    error.with_context(format!("helpers-ledger.toml distinct #{}", index + 1))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(admission(
                Operation::LedgerSchema,
                "helpers-ledger.toml: `distinct` must be an array of tables".to_owned(),
            ));
        }
    };
    for (index, row) in distinct.iter().enumerate() {
        if distinct[..index]
            .iter()
            .any(|seen| seen.kind == row.kind && seen.members == row.members)
        {
            return Err(admission(
                Operation::LedgerSchema,
                format!("helpers-ledger.toml: {} is listed twice", row.label()),
            ));
        }
    }
    Ok(Ledger {
        jobs,
        distinct,
        document,
    })
}

fn check_keys(
    table: &BTreeMap<String, Value>,
    known: &[&str],
    optional: &[&str],
    what: &str,
) -> Result<(), Diag> {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            return Err(admission(
                Operation::LedgerSchema,
                format!("unknown {what} key `{key}`"),
            ));
        }
    }
    for key in known {
        if !optional.contains(key) && !table.contains_key(*key) {
            return Err(admission(
                Operation::LedgerSchema,
                format!("{what} is missing required key `{key}`"),
            ));
        }
    }
    Ok(())
}

fn string(table: &BTreeMap<String, Value>, key: &str) -> Result<String, Diag> {
    match table.get(key) {
        Some(Value::Str(text)) if !text.trim().is_empty() => Ok(text.clone()),
        Some(Value::Str(_)) => Err(admission(
            Operation::LedgerSchema,
            format!("`{key}` must not be empty"),
        )),
        _ => Err(admission(
            Operation::LedgerSchema,
            format!("`{key}` must be a string"),
        )),
    }
}

fn strings(table: &BTreeMap<String, Value>, key: &str) -> Result<Vec<String>, Diag> {
    let Some(Value::Array(items)) = table.get(key) else {
        return Err(admission(
            Operation::LedgerSchema,
            format!("`{key}` must be an array of strings"),
        ));
    };
    items
        .iter()
        .map(|item| match item {
            Value::Str(text) if !text.trim().is_empty() => Ok(text.clone()),
            _ => Err(admission(
                Operation::LedgerSchema,
                format!("`{key}` must hold non-empty strings only"),
            )),
        })
        .collect()
}

fn job(row: &BTreeMap<String, Value>) -> Result<Job, Diag> {
    check_keys(row, &JOB_KEYS, &JOB_OPTIONAL, "job")?;
    let id = string(row, "id")?;
    for key in ["summary", "spec"] {
        string(row, key).map_err(|error| error.with_context(id.clone()))?;
    }
    for key in ["vectors", "bench", "sites", "replaces_external"] {
        strings(row, key).map_err(|error| error.with_context(id.clone()))?;
    }
    let Some(Value::Bool(enforced)) = row.get("enforced") else {
        return Err(admission(
            Operation::LedgerSchema,
            format!("{id}: `enforced` must be a boolean"),
        ));
    };
    let Some(Value::Table(forbidden)) = row.get("forbidden") else {
        return Err(admission(
            Operation::LedgerSchema,
            format!("{id}: `forbidden` must be a [job.forbidden] table"),
        ));
    };
    let variants = match row.get("variant") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Table(table) => variant(table),
                _ => Err(admission(
                    Operation::LedgerSchema,
                    "a variant must be a [[job.variant]] table".to_owned(),
                )),
            })
            .collect::<Result<_, _>>()
            .map_err(|error| error.with_context(id.clone()))?,
        Some(_) => {
            return Err(admission(
                Operation::LedgerSchema,
                format!("{id}: `variant` must be an array of tables"),
            ));
        }
    };
    Ok(Job {
        home: string(row, "home")?,
        entry_points: strings(row, "entry_points")?,
        enforced: *enforced,
        forbidden: forbidden_table(forbidden).map_err(|error| error.with_context(id.clone()))?,
        variants,
        id,
    })
}

fn forbidden_table(table: &BTreeMap<String, Value>) -> Result<Forbidden, Diag> {
    check_keys(table, &FORBIDDEN_KEYS, &[], "forbidden")?;
    let constants = strings(table, "constants")?
        .iter()
        .map(|spelling| {
            crate::normalize::integer_value(spelling).ok_or_else(|| {
                admission(
                    Operation::LedgerSchema,
                    format!("forbidden constant `{spelling}` is not an integer literal"),
                )
            })
        })
        .collect::<Result<_, _>>()?;
    let fingerprints = strings(table, "fingerprints")?;
    for id in &fingerprints {
        if !crate::normalize::is_fingerprint_id(id) {
            return Err(admission(
                Operation::LedgerSchema,
                format!(
                    "forbidden fingerprint `{id}` is not a `body:`, `shim:`, `table:` or `rule:` id"
                ),
            ));
        }
    }
    let names = strings(table, "names")?
        .iter()
        .map(|pattern| {
            regex::Regex::new(pattern).map_err(|error| {
                caused(Operation::LedgerSchema, error)
                    .with_context(format!("forbidden name `{pattern}`"))
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Forbidden {
        constants,
        fingerprints,
        names,
    })
}

fn criterion(table: &BTreeMap<String, Value>, what: &str) -> Result<(), Diag> {
    let criterion = string(table, "criterion")?;
    if CRITERIA.contains(&criterion.as_str()) {
        Ok(())
    } else {
        Err(admission(
            Operation::LedgerSchema,
            format!(
                "{what} criterion `{criterion}` is not one of {}",
                CRITERIA.join(", ")
            ),
        ))
    }
}

fn distinct(table: &BTreeMap<String, Value>) -> Result<Distinct, Diag> {
    check_keys(table, &DISTINCT_KEYS, &[], "distinct")?;
    criterion(table, "distinct")?;
    string(table, "reason")?;
    let kind = match string(table, "kind")?.as_str() {
        "body" => GroupKind::Body,
        "shim" => GroupKind::Shim,
        other => {
            return Err(admission(
                Operation::LedgerSchema,
                format!("distinct kind `{other}` is not `body` or `shim`"),
            ));
        }
    };
    let mut members = strings(table, "members")?;
    if members.len() < 2 {
        return Err(admission(
            Operation::LedgerSchema,
            "`members` must name the two or more members of one group".to_owned(),
        ));
    }
    members.sort();
    Ok(Distinct {
        members,
        kind,
        anchor: string(table, "anchor")?,
    })
}

fn variant(table: &BTreeMap<String, Value>) -> Result<Variant, Diag> {
    check_keys(table, &VARIANT_KEYS, &[], "variant")?;
    criterion(table, "variant")?;
    string(table, "reason")?;
    string(table, "file")?;
    Ok(Variant {
        symbol: string(table, "symbol")?,
        file: string(table, "file")?,
        detector: string(table, "detector")?,
        anchor: string(table, "anchor")?,
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn malformed_ledger_retains_typed_cause_line_and_failure_class() {
        use crate::error::{Admission, Operation};

        let diagnostic = parse("# header\n[job]\n[job]\n").unwrap_err();
        let kind = diagnostic
            .downcast_ref::<Admission>()
            .expect("admission kind");
        assert_eq!(kind.operation, Operation::LedgerParse);
        let cause = std::error::Error::source(kind)
            .and_then(|cause| cause.downcast_ref::<crate::toml::Error>())
            .expect("original TOML error");
        assert_eq!(cause.line, 3);
        assert_eq!(diagnostic.inner().source_ctx.location.line, Some(3));
        assert_eq!(
            diagnostic.inner().failure_class.as_deref(),
            Admission::FAILURE_CLASS
        );
    }

    #[test]
    fn schema_refusal_keeps_context_and_regex_cause_without_fabricated_causes() {
        use crate::error::{Admission, Operation};

        let diagnostic = parse(&ROW.replace("'^f$'", "'['")).unwrap_err();
        let kind = diagnostic
            .downcast_ref::<Admission>()
            .expect("admission kind");
        assert_eq!(kind.operation, Operation::LedgerSchema);
        assert!(
            std::error::Error::source(kind)
                .and_then(|cause| cause.downcast_ref::<regex::Error>())
                .is_some()
        );
        let rendered = format!("{diagnostic:#}");
        assert!(rendered.contains("forbidden name `[`"));
        assert!(rendered.contains("job #1"));
        let diagnostic = parse(&ROW.replace("enforced = true\n", "")).unwrap_err();
        let kind = diagnostic
            .downcast_ref::<Admission>()
            .expect("admission kind");
        assert!(std::error::Error::source(kind).is_none());
    }

    const ROW: &str = "[[job]]\nid = \"j\"\nsummary = \"s\"\nhome = \"c::f\"\nentry_points = []\nspec = \"x\"\nvectors = []\nbench = []\nsites = []\nreplaces_external = []\nenforced = true\n[job.forbidden]\nconstants = [\"0xcbf2_9ce4_8422_2325\"]\nfingerprints = [\"table:hex-lower\"]\nnames = ['^f$']\n";

    #[test]
    fn a_complete_row_is_read() {
        let ledger = parse(ROW).expect("a complete row");
        let job = ledger.job("j").expect("the row");
        assert!(job.enforced);
        assert_eq!(job.forbidden.constants, vec![14_695_981_039_346_656_037]);
    }

    #[test]
    fn a_missing_or_unknown_key_is_refused() {
        let missing = ROW.replace("enforced = true\n", "");
        assert!(
            parse(&missing)
                .unwrap_err()
                .to_string()
                .contains("enforced")
        );
        let unknown = ROW.replace("enforced = true\n", "enforced = true\nenforcd = true\n");
        assert!(parse(&unknown).unwrap_err().to_string().contains("enforcd"));
    }

    const DISTINCT: &str = "[[distinct]]\nmembers = [\"c::g\", \"c::f\"]\nkind = \"KIND\"\ncriterion = \"b\"\nanchor = \"c::f\"\nreason = \"r\"\n";

    #[test]
    fn a_distinct_row_is_read_with_its_members_sorted() {
        let ledger = parse(&format!("{ROW}{}", DISTINCT.replace("KIND", "shim"))).expect("a row");
        assert_eq!(ledger.distinct.len(), 1);
        assert_eq!(ledger.distinct[0].members, ["c::f", "c::g"]);
        assert_eq!(ledger.distinct[0].kind, super::GroupKind::Shim);
    }

    #[test]
    fn a_distinct_row_needs_a_known_kind_two_members_and_no_twin() {
        let refused = format!("{ROW}{}", DISTINCT.replace("KIND", "bodies"));
        assert!(parse(&refused).unwrap_err().to_string().contains("kind"));
        let accepted = format!("{ROW}{}", DISTINCT.replace("KIND", "body"));
        assert!(parse(&accepted).is_ok());
        let lone = DISTINCT
            .replace("KIND", "body")
            .replace("[\"c::g\", \"c::f\"]", "[\"c::g\"]");
        assert!(
            parse(&lone)
                .unwrap_err()
                .to_string()
                .contains("two or more")
        );
        let twice = format!("{0}{0}", DISTINCT.replace("KIND", "body"));
        assert!(
            parse(&twice)
                .unwrap_err()
                .to_string()
                .contains("listed twice")
        );
        let unknown = DISTINCT.replace("KIND", "body").replace("reason", "reasn");
        assert!(parse(&unknown).unwrap_err().to_string().contains("reasn"));
    }

    #[test]
    fn a_variant_needs_a_known_criterion() {
        let variant = "[[job.variant]]\nsymbol = \"c::g\"\nfile = \"f.rs\"\ndetector = \"isomorphic\"\ncriterion = \"CRITERION\"\nanchor = \"c::g\"\nreason = \"r\"\n";
        let refused = format!("{ROW}{}", variant.replace("CRITERION", "d"));
        assert!(
            parse(&refused)
                .unwrap_err()
                .to_string()
                .contains("criterion")
        );
        let accepted = format!("{ROW}{}", variant.replace("CRITERION", "layering"));
        assert_eq!(
            parse(&accepted).expect("layering").jobs[0].variants.len(),
            1
        );
    }
}

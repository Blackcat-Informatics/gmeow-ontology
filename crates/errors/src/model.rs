// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::diag::{Advice, Guidance, Remediation};

/// Serde skip-helper: `true` for `false` so a defaulted `bool` flag stays out of
/// the wire form (keeps existing JSON/SARIF goldens byte-unchanged).
fn is_false(value: &bool) -> bool {
    !*value
}

/// Normalized diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
    Info,
}

impl Severity {
    /// Parse a user/tool supplied severity label.
    pub fn parse(value: &str) -> crate::Result<Self> {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("error")
            || trimmed.eq_ignore_ascii_case("failure")
            || trimmed.eq_ignore_ascii_case("fatal")
            || trimmed.eq_ignore_ascii_case("violation")
        {
            Ok(Self::Error)
        } else if trimmed.eq_ignore_ascii_case("warning") || trimmed.eq_ignore_ascii_case("warn") {
            Ok(Self::Warning)
        } else if trimmed.eq_ignore_ascii_case("note") {
            Ok(Self::Note)
        } else if trimmed.eq_ignore_ascii_case("info")
            || trimmed.eq_ignore_ascii_case("information")
        {
            Ok(Self::Info)
        } else {
            Err(crate::diag::Diag::of_kind(
                crate::error::UnknownSeverityLabel {
                    value: trimmed.to_owned(),
                },
            ))
        }
    }

    /// Stable lowercase spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Info => "info",
        }
    }

    /// The inverse of the RDF projection's severity-individual local name — the
    /// right-inverse (section) of `render::severity_individual`, which maps
    /// `Error → "severityError"`, etc. Reads back the `gmeow:findingSeverity`
    /// object's local name from the `graph/diagnostics` projection. `None` for an
    /// unknown token so a corrupt projection is a caller-surfaced hard fail, never a
    /// silent default.
    pub fn from_individual_local(local: &str) -> Option<Self> {
        match local {
            "severityError" => Some(Self::Error),
            "severityWarning" => Some(Self::Warning),
            "severityNote" => Some(Self::Note),
            "severityInfo" => Some(Self::Info),
            _ => None,
        }
    }

    /// SARIF level spelling.
    pub fn sarif_level(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note | Self::Info => "note",
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Note => 2,
            Self::Info => 3,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_rank().cmp(&other.sort_rank())
    }
}

/// The KIND of a finding — an axis ORTHOGONAL to [`Severity`], because not all
/// findings are failures (the `logic:FindingCategory` value class).
///
/// Severity answers "how loud?"; category answers "what kind?". The load-bearing
/// case is [`FindingCategory::PermittedEpistemicConflict`]: a disclosed, witnessed
/// contradiction permitted by a glut-admitting reasoning contract is coherent, so
/// it is emitted at a NON-error severity and never fails the gate. The wire values
/// are the kebab spellings of the `logic:Finding*` individuals in
/// `slices/grounding/logic/module.ttl`.
///
/// This is a PAYLOAD axis, not an ORDERING axis: it is deliberately kept out of
/// [`Finding::sort_key`] and the SARIF fingerprint so adding a category to a
/// finding never perturbs report ordering or churns goldens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingCategory {
    /// A closed-world data-shape (SHACL) constraint breach. A failure.
    DataShapeViolation,
    /// An ontology modeling-discipline / structural breach, or an unsatisfiable
    /// class surfaced by reasoning. A failure.
    ModelingDisciplineViolation,
    /// A within-world contradiction the contract FORBIDS. A failure.
    ContradictionWitness,
    /// A disclosed, witnessed contradiction the contract PERMITS. NOT a failure —
    /// coherent, surfaced for transparency, never blocks a coherence certificate.
    PermittedEpistemicConflict,
    /// A construct the engine has no defined procedure for under the contract.
    UnsupportedSemanticFeature,
    /// A check that did not run to completion (budget-exhausted / incomplete).
    IncompleteCheck,
    /// A construct a lowering could not carry exactly (the loss ledger).
    ProjectionLoss,
    /// A trust / governance advisory (untrusted signer, soft policy note).
    PolicyWarning,
    /// A native↔published AGREEMENT — the native reasoner's verdict matched the
    /// community-decided ground truth. NOT a failure: it is positive corroborating
    /// evidence surfaced for the benchmark surface, coherent, and never blocks a
    /// coherence certificate (the opposite of an incomplete check).
    Corroboration,
    /// The closed CHATTER kind for general-purpose logging witnesses — the
    /// ordinary note/info stream a run emits to narrate its own progress. NOT a
    /// failure and takes no coherence stance: transient bookkeeping that never
    /// gates and carries no evidential weight.
    #[serde(rename = "transient-chatter")]
    Transient,
}

impl FindingCategory {
    /// Parse a kebab-case category label.
    pub fn parse(value: &str) -> crate::Result<Self> {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("data-shape-violation") {
            Ok(Self::DataShapeViolation)
        } else if trimmed.eq_ignore_ascii_case("modeling-discipline-violation") {
            Ok(Self::ModelingDisciplineViolation)
        } else if trimmed.eq_ignore_ascii_case("contradiction-witness") {
            Ok(Self::ContradictionWitness)
        } else if trimmed.eq_ignore_ascii_case("permitted-epistemic-conflict") {
            Ok(Self::PermittedEpistemicConflict)
        } else if trimmed.eq_ignore_ascii_case("unsupported-semantic-feature") {
            Ok(Self::UnsupportedSemanticFeature)
        } else if trimmed.eq_ignore_ascii_case("incomplete-check") {
            Ok(Self::IncompleteCheck)
        } else if trimmed.eq_ignore_ascii_case("projection-loss") {
            Ok(Self::ProjectionLoss)
        } else if trimmed.eq_ignore_ascii_case("policy-warning") {
            Ok(Self::PolicyWarning)
        } else if trimmed.eq_ignore_ascii_case("corroboration") {
            Ok(Self::Corroboration)
        } else if trimmed.eq_ignore_ascii_case("transient-chatter") {
            Ok(Self::Transient)
        } else {
            Err(crate::diag::Diag::of_kind(
                crate::error::UnknownFindingCategory {
                    value: trimmed.to_owned(),
                },
            ))
        }
    }

    /// The stable kebab-case wire spelling (matches the `serde` rename).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DataShapeViolation => "data-shape-violation",
            Self::ModelingDisciplineViolation => "modeling-discipline-violation",
            Self::ContradictionWitness => "contradiction-witness",
            Self::PermittedEpistemicConflict => "permitted-epistemic-conflict",
            Self::UnsupportedSemanticFeature => "unsupported-semantic-feature",
            Self::IncompleteCheck => "incomplete-check",
            Self::ProjectionLoss => "projection-loss",
            Self::PolicyWarning => "policy-warning",
            Self::Corroboration => "corroboration",
            Self::Transient => "transient-chatter",
        }
    }

    /// The inverse of [`iri_local`](FindingCategory::iri_local) — read the
    /// `logic:Finding*` individual's local name (`gmeow:findingCategory` object)
    /// back into the category. The section of the RDF projection's category leg.
    /// `None` for an unknown local name (hard fail, never a silent default).
    pub fn from_iri_local(local: &str) -> Option<Self> {
        match local {
            "FindingDataShapeViolation" => Some(Self::DataShapeViolation),
            "FindingModelingDisciplineViolation" => Some(Self::ModelingDisciplineViolation),
            "FindingContradictionWitness" => Some(Self::ContradictionWitness),
            "FindingPermittedEpistemicConflict" => Some(Self::PermittedEpistemicConflict),
            "FindingUnsupportedSemanticFeature" => Some(Self::UnsupportedSemanticFeature),
            "FindingIncompleteCheck" => Some(Self::IncompleteCheck),
            "FindingProjectionLoss" => Some(Self::ProjectionLoss),
            "FindingPolicyWarning" => Some(Self::PolicyWarning),
            "FindingCorroboration" => Some(Self::Corroboration),
            "FindingTransientChatter" => Some(Self::Transient),
            _ => None,
        }
    }

    /// The local name of the matching `logic:Finding*` ontology individual, so the
    /// RDF projection emits the same IRI the vocabulary mints.
    pub fn iri_local(self) -> &'static str {
        match self {
            Self::DataShapeViolation => "FindingDataShapeViolation",
            Self::ModelingDisciplineViolation => "FindingModelingDisciplineViolation",
            Self::ContradictionWitness => "FindingContradictionWitness",
            Self::PermittedEpistemicConflict => "FindingPermittedEpistemicConflict",
            Self::UnsupportedSemanticFeature => "FindingUnsupportedSemanticFeature",
            Self::IncompleteCheck => "FindingIncompleteCheck",
            Self::ProjectionLoss => "FindingProjectionLoss",
            Self::PolicyWarning => "FindingPolicyWarning",
            Self::Corroboration => "FindingCorroboration",
            Self::Transient => "FindingTransientChatter",
        }
    }
}

impl fmt::Display for FindingCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A concrete source or logical location for a diagnostic.
///
/// Beyond the file `path`/`line`/`column` and a free-form `logical` name, a
/// location can carry GTS wire coordinates (mirroring `purrdf::RdfLocation`):
/// the term-id, quad index, reifier-id, frame index, and segment index that
/// point a finding back into the exact position inside a GTS bundle. These flow
/// from the RDF/GTS adapter through the report into SARIF logical locations and
/// the `gmeow:` RDF projection so every consumer agrees on the same anchor.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_term_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_quad_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_reifier_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_frame_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_segment_index: Option<u64>,
}

impl Location {
    pub fn new(
        path: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
        logical: Option<String>,
    ) -> Self {
        Self {
            path,
            line,
            column,
            logical,
            ..Self::default()
        }
    }

    pub fn with_gts_term(mut self, term_id: u64) -> Self {
        self.gts_term_id = Some(term_id);
        self
    }

    pub fn with_gts_quad(mut self, quad_index: u64) -> Self {
        self.gts_quad_index = Some(quad_index);
        self
    }

    pub fn with_gts_reifier(mut self, reifier_id: u64) -> Self {
        self.gts_reifier_id = Some(reifier_id);
        self
    }

    pub fn with_gts_frame(mut self, frame_index: u64) -> Self {
        self.gts_frame_index = Some(frame_index);
        self
    }

    pub fn with_gts_segment(mut self, segment_index: u64) -> Self {
        self.gts_segment_index = Some(segment_index);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.path.is_none()
            && self.line.is_none()
            && self.column.is_none()
            && self.logical.is_none()
            && self.gts_term_id.is_none()
            && self.gts_quad_index.is_none()
            && self.gts_reifier_id.is_none()
            && self.gts_frame_index.is_none()
            && self.gts_segment_index.is_none()
    }

    pub fn display(&self) -> String {
        let mut out = self
            .path
            .as_deref()
            .or(self.logical.as_deref())
            .unwrap_or("<unknown>")
            .to_owned();
        if let Some(line) = self.line {
            out.push(':');
            out.push_str(&line.to_string());
            if let Some(column) = self.column {
                out.push(':');
                out.push_str(&column.to_string());
            }
        }
        if let Some(term_id) = self.gts_term_id {
            out.push_str(&format!(" term#{term_id}"));
        }
        if let Some(quad_index) = self.gts_quad_index {
            out.push_str(&format!(" quad#{quad_index}"));
        }
        if let Some(reifier_id) = self.gts_reifier_id {
            out.push_str(&format!(" reifier#{reifier_id}"));
        }
        if let Some(frame_index) = self.gts_frame_index {
            out.push_str(&format!(" frame#{frame_index}"));
        }
        if let Some(segment_index) = self.gts_segment_index {
            out.push_str(&format!(" segment#{segment_index}"));
        }
        out
    }
}

/// Optional rule metadata for a stable diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<String>,
    /// The standing, registry-authored `gmeow:ruleRemediation` fix guidance for this
    /// rule — the "how to fix a violation" prose the doc/catalog graph carries per
    /// code, joined onto the report's rule registry by the producer so the
    /// renderer can surface it once per finding code. `None` when the rule authors
    /// no remediation (never fabricated at render time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// The `gmeow:howToUse` guidance prose for the governing term of this rule —
    /// the deep/verify-surface usage guidance the doc graph carries. `None` when
    /// the governing term authors none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub how_to_use: Option<String>,
    pub default_severity: Severity,
}

impl Rule {
    pub fn new(id: impl Into<String>, default_severity: Severity) -> Self {
        Self {
            id: id.into(),
            title: None,
            description: None,
            help_uri: None,
            remediation: None,
            how_to_use: None,
            default_severity,
        }
    }

    /// Attach the registry-authored `gmeow:ruleRemediation` fix guidance.
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    /// Attach the governing term's `gmeow:howToUse` guidance prose.
    pub fn with_how_to_use(mut self, how_to_use: impl Into<String>) -> Self {
        self.how_to_use = Some(how_to_use.into());
        self
    }
}

/// The role of a compilation unit in a diagnostic attribution (§9 / S5).
///
/// This mirrors `purrdf::provenance::AttributionRole` but uses owned strings
/// so `gmeow-errors` remains dep-free of `gmeow-rdf` (the layering rule:
/// diagnostics must not import the RDF kernel). The canonical string form is
/// identical to `AttributionRole::as_str()`.
///
/// The `UnitId`→slice-IRI resolution happens in `gmeow-validate` (which has
/// access to both the provenance interners and the diagnostics model); by the
/// time an `Attribution` reaches `DiagnosticAttribution` the IRI is already
/// resolved and the numeric id is discarded (S0.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticAttribution {
    /// The public slice IRI — never a numeric unit id (S0.5).
    pub slice_iri: String,
    /// The role this slice played (mirrors `AttributionRole::as_str()`).
    ///
    /// Known values: `"assertion-origin"`, `"definition-owner"`,
    /// `"shape-owner"`, `"rule-owner"`, `"focus-origin"`, `"value-origin"`,
    /// `"derivation-support"`, `"evaluation-scope"`.
    pub role: String,
    /// Optional provenance note (human-readable; does NOT enter fingerprints).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// A secondary, TEXT-bearing labelled span projected from a witness node's
/// [`Label`](crate::diag::Label) — the human MESSAGE plus the [`Location`] it
/// anchors to (Rust-compiler-style "defined here" / SHACL result-path / offending
/// value).
///
/// Distinct from [`related_locations`](Finding::related_locations): that field
/// carries the *bare* antecedent-IRI provenance edges (a location with no message);
/// a related label additionally carries the label prose the LSP renders as a
/// `DiagnosticRelatedInformation` entry. The two are kept separate so a labelled
/// secondary span never loses its message when it rides through the flat model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedLabel {
    pub location: Location,
    pub message: String,
}

/// One normalized diagnostic emitted by any GMEOW tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_locations: Vec<Location>,
    /// Secondary TEXT-bearing labelled spans — the faithful projection of the
    /// witness node's [`Label`](crate::diag::Label)s that keeps each secondary
    /// span's MESSAGE beside its location (the flat
    /// [`related_locations`](Finding::related_locations) twin carries the bare
    /// span with no message). Empty for findings that carry no labelled spans;
    /// `skip_serializing_if` keeps them out of the wire form so existing
    /// JSON/SARIF/RDF goldens are byte-unchanged when absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_labels: Vec<RelatedLabel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// Structured, standpoint-bearing advice — the faithful projection of the
    /// witness node's [`Advice`] that keeps each suggestion's standpoint and
    /// outward help URI (the flat [`suggestions`](Finding::suggestions) strings are
    /// the lossy text-only twin kept for the existing renderers). Empty for
    /// hand-built findings that carry no structured advice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advice: Vec<Advice>,
    /// registry-authored remediations projected from the witness node — the "how to fix"
    /// payload rendered into SARIF `fixes` and the CLI/HTML remediation line. Empty
    /// when the finding's rule authors no remediation (never fabricated).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remediation: Vec<Remediation>,
    /// The canonical `blake3` fingerprint IRI when this finding is a ledger
    /// witness — the SAME IRI the antecedent edges of downstream findings point at,
    /// so the projected diagnostic graph's subject/antecedent-object IRIs close.
    /// `None` for hand-built findings that were never ledger witnesses (they fall
    /// back to the content-hash subject IRI in the RDF projection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_iri: Option<String>,
    /// The code-blind source-anchor IRI (`gmeow:findingAnchor`) — the cross-node
    /// join key two different-code findings at one source position share. `None`
    /// for hand-built findings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_iri: Option<String>,
    /// Whether [`anchor_iri`](Finding::anchor_iri) names a genuine source position
    /// (a `gmeow:NonTrivialAnchor`) as opposed to the shared empty default anchor —
    /// the guard the cross-node-glut join reads.
    #[serde(default, skip_serializing_if = "is_false")]
    pub anchor_non_trivial: bool,
    /// The finding IRIs this finding derives FROM (`gmeow:findingAntecedent`) — the
    /// provenance-DAG edges, keyed on the antecedents' canonical fingerprint IRIs.
    /// Empty for a root finding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub antecedents: Vec<String>,
    /// The reasoner-derived shared root antecedent (`gmeow:findingRootCause`) —
    /// present only after the diagnostic meta-reasoning fold has run over the
    /// projected graph and been read back. `None` on a freshly-projected finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_cause: Option<String>,
    /// The reasoner-derived cluster node (`gmeow:findingCluster`) this finding
    /// belongs to — the "N findings share root R" grouping. `None` until the fold
    /// has run and been read back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<String>,
    /// The reasoner-derived cross-node glut edges (`gmeow:crossNodeGlutWith`) — the
    /// other findings at one non-trivial anchor whose coherence polarity opposes
    /// this one's, a conflict the same-fingerprint merge cannot see across codes.
    /// Empty until the fold has run and been read back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_node_glut_with: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The KIND of this finding, orthogonal to its severity (the 8-way
    /// `logic:FindingCategory`). `None` for findings that predate or fall outside
    /// the taxonomy; `skip_serializing_if` keeps absent categories out of the wire
    /// form so existing JSON/SARIF/RDF goldens are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<FindingCategory>,
    /// The gating STANDPOINT this finding is asserted from (advisory ⊑ perspectival
    /// ⊑ binding). It is the third truth-axis of the grade — the leg the gate
    /// morphism reads alongside severity and category — carried onto the projected
    /// finding so the RDF `gmeow:findingStandpoint` twin (and the SHACL up-set
    /// shape that reads it) is not vacuous. `None` for hand-built findings that
    /// predate the grading substrate; `skip_serializing_if` keeps it out of the
    /// wire form when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standpoint: Option<crate::grade::Standpoint>,
    /// Structured slice attributions for this finding (§9 / S5).
    ///
    /// Records which slices (by public IRI, never numeric id) played which roles
    /// in producing this finding. Empty when no attribution context is available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributions: Vec<DiagnosticAttribution>,
    /// The DOCUMENTED ontology terms this finding structurally concerns — distinct
    /// from the ABox focus node(s) in [`locations`](Finding::locations). A SHACL
    /// violation's honest documented attribution is its CONSTRAINED PROPERTY (the
    /// `sh:path`), a documented `gmeow:` term, NOT the data individual that tripped
    /// the shape; carrying it here (rather than overloading the focus location) lets
    /// the docs "Diagnostics you might hit" per-term join light up the property's
    /// page. Empty for findings that concern no single documented term (an honest
    /// absence, never fabricated); `skip_serializing_if` keeps it out of the wire
    /// form when absent so existing JSON/SARIF/RDF goldens are byte-unchanged, and
    /// the SARIF/RDF/HTML projections never read it so those bytes never change even
    /// when it IS present (it rides only the full-fidelity JSON `Report`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documented_terms: Vec<String>,
    /// Per-term usage guidance (howToUse/useWhen/avoidWhen) joined from the bundle
    /// documentation graph. Empty when the finding's rule/documented terms author
    /// none (never fabricated).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance: Vec<Guidance>,
    /// The logic-world quad-reifier IRIs this finding's verdict derives FROM
    /// (`gmeow:findingDerivedFromQuad`) — the explain-skeleton cited IRIs of the
    /// reasoned quads that fired. A SEPARATE edge from `antecedents`/`root_cause`
    /// (which are finding-fingerprint IRIs); never conflated with them. Empty for
    /// non-reasoned findings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from_quads: Vec<String>,
}

impl Finding {
    pub fn new(severity: Severity, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            tool: None,
            locations: Vec::new(),
            related_locations: Vec::new(),
            related_labels: Vec::new(),
            suggestions: Vec::new(),
            advice: Vec::new(),
            remediation: Vec::new(),
            finding_iri: None,
            anchor_iri: None,
            anchor_non_trivial: false,
            antecedents: Vec::new(),
            root_cause: None,
            cluster: None,
            cross_node_glut_with: Vec::new(),
            tags: Vec::new(),
            detail: None,
            category: None,
            standpoint: None,
            attributions: Vec::new(),
            documented_terms: Vec::new(),
            guidance: Vec::new(),
            derived_from_quads: Vec::new(),
        }
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    /// Tag this finding with its [`FindingCategory`] (the orthogonal KIND axis).
    pub fn with_category(mut self, category: FindingCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Tag this finding with the [`Standpoint`](crate::grade::Standpoint) it is
    /// asserted from (the gating-strength truth-axis).
    pub fn with_standpoint(mut self, standpoint: crate::grade::Standpoint) -> Self {
        self.standpoint = Some(standpoint);
        self
    }

    /// Attribute this finding to a DOCUMENTED ontology term it structurally concerns
    /// (e.g. a SHACL violation's constrained `sh:path` property) — distinct from the
    /// ABox focus node in [`locations`](Finding::locations). Additive: the docs
    /// per-term diagnostics join reads it; the SARIF/RDF/HTML projections do not.
    pub fn with_documented_term(mut self, term_iri: impl Into<String>) -> Self {
        self.documented_terms.push(term_iri.into());
        self
    }

    /// Attach a per-term [`Guidance`] claim (howToUse/useWhen/avoidWhen), joined
    /// from the bundle documentation graph. Never fabricated: only ever called
    /// with a claim projected verbatim from the graph.
    pub fn with_guidance(mut self, guidance: Guidance) -> Self {
        self.guidance.push(guidance);
        self
    }

    /// Attach a per-term [`Guidance`] claim in place (the non-chainable twin of
    /// [`with_guidance`](Finding::with_guidance), mirroring
    /// [`add_location`](Finding::add_location)'s in-place style).
    pub fn push_guidance(&mut self, guidance: Guidance) {
        self.guidance.push(guidance);
    }

    /// Attach the quad-reifier IRIs (`gmeow:findingDerivedFromQuad`) this
    /// finding's verdict derives from — the explain-skeleton citations of the
    /// reasoned quads that fired.
    pub fn with_derived_from_quads(
        mut self,
        quads: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.derived_from_quads
            .extend(quads.into_iter().map(Into::into));
        self
    }

    pub fn add_location(&mut self, location: Location) {
        if !location.is_empty() {
            self.locations.push(location);
        }
    }

    /// Attach a secondary TEXT-bearing labelled span. A label with neither a
    /// message nor a locating span carries nothing, so it is dropped (mirrors the
    /// empty-guard on [`add_location`](Finding::add_location)).
    pub fn add_related_label(&mut self, label: RelatedLabel) {
        if !label.message.is_empty() || !label.location.is_empty() {
            self.related_labels.push(label);
        }
    }

    pub fn primary_location(&self) -> Option<&Location> {
        self.locations.first()
    }

    pub fn sort_key(&self) -> (Severity, &str, String, &str) {
        (
            self.severity,
            self.code.as_str(),
            self.primary_location()
                .map(Location::display)
                .unwrap_or_default(),
            self.message.as_str(),
        )
    }

    pub fn normalize(&mut self) {
        self.tags.sort();
        self.tags.dedup();
        self.suggestions.sort();
        self.suggestions.dedup();
        self.locations.sort_by_key(Location::display);
        self.related_locations.sort_by_key(Location::display);
        // Deterministic order for the text-bearing secondary spans, keyed on the
        // rendered location then the message so a multi-label finding projects the
        // same byte sequence regardless of attach order.
        self.related_labels.sort_by(|a, b| {
            (a.location.display(), &a.message).cmp(&(b.location.display(), &b.message))
        });
        // The provenance-DAG edges and cross-node glut edges are content-addressed
        // IRIs; sort+dedup them so the projected graph is deterministic regardless
        // of the ledger's antecedent-union order.
        self.antecedents.sort();
        self.antecedents.dedup();
        self.cross_node_glut_with.sort();
        self.cross_node_glut_with.dedup();
        // Deterministic order for the documented-term attributions so a finding
        // projects the same byte sequence regardless of attach order.
        self.documented_terms.sort();
        self.documented_terms.dedup();
        // The explain-skeleton derivation edges are content-addressed IRIs; sort+dedup
        // them so the projected graph is deterministic regardless of attach order.
        self.derived_from_quads.sort();
        self.derived_from_quads.dedup();
        // Per-term guidance claims can reach a finding via the ledger merge path,
        // which appends them in diagnostic arrival order — so sort+dedup them on the
        // same `(modality, term_iri, text)` identity the enrich join uses, keeping the
        // projected SARIF/RDF byte sequence deterministic regardless of attach order.
        self.guidance.sort_by(|a, b| {
            (a.modality as u8, &a.term_iri, &a.text).cmp(&(b.modality as u8, &b.term_iri, &b.text))
        });
        self.guidance.dedup_by(|a, b| {
            a.modality == b.modality && a.term_iri == b.term_iri && a.text == b.text
        });
    }
}

/// A complete diagnostics report for one developer tool run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl Report {
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            findings: Vec::new(),
            rules: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn from_legacy(
        tool: impl Into<String>,
        errors: impl IntoIterator<Item = String>,
        warnings: impl IntoIterator<Item = String>,
    ) -> Self {
        let tool = tool.into();
        let mut report = Self::new(tool.clone());
        for message in errors {
            report.add_finding(Finding::new(
                Severity::Error,
                format!("{tool}.error"),
                message,
            ));
        }
        for message in warnings {
            report.add_finding(Finding::new(
                Severity::Warning,
                format!("{tool}.warning"),
                message,
            ));
        }
        report
    }

    pub fn add_finding(&mut self, mut finding: Finding) {
        if finding.tool.is_none() {
            finding.tool = Some(self.tool.clone());
        }
        self.findings.push(finding);
    }

    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    pub fn ok(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count()
    }

    /// The number of findings carrying each finding code, keyed by code (sorted).
    ///
    /// The deterministic per-code tally behind the summarized text render and the
    /// recorded coverage-ratchet baselines — a stable, low-cardinality projection
    /// of a report that may hold thousands of per-term findings. The keys borrow
    /// the codes already owned by the report, so tallying allocates no strings.
    pub fn counts_by_code(&self) -> BTreeMap<&str, usize> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for finding in &self.findings {
            *counts.entry(finding.code.as_str()).or_default() += 1;
        }
        counts
    }

    pub fn normalize(&mut self) {
        for finding in &mut self.findings {
            finding.normalize();
        }
        self.findings
            .sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        self.rules.sort_by(|a, b| a.id.cmp(&b.id));
        self.rules.dedup_by(|a, b| a.id == b.id);
    }

    pub fn normalized(&self) -> Self {
        let mut clone = self.clone();
        clone.normalize();
        clone
    }

    pub fn legacy_errors(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| f.message.clone())
            .collect()
    }

    pub fn legacy_warnings(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .map(|f| f.message.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_report_preserves_error_warning_strings() {
        let report = Report::from_legacy(
            "validate",
            ["missing skos:definition".to_owned()],
            ["docs.md has no anchors".to_owned()],
        );

        assert!(!report.ok());
        assert_eq!(report.legacy_errors(), ["missing skos:definition"]);
        assert_eq!(report.legacy_warnings(), ["docs.md has no anchors"]);
    }

    #[test]
    fn report_normalization_is_deterministic() {
        let mut report = Report::new("test");
        report.add_finding(Finding::new(Severity::Warning, "z", "later"));
        report.add_finding(Finding::new(Severity::Error, "a", "first"));

        let normalized = report.normalized();

        assert_eq!(normalized.findings[0].severity, Severity::Error);
        assert_eq!(normalized.findings[0].code, "a");
    }

    #[test]
    fn location_without_wire_coords_serializes_compactly() {
        let location = Location::new(Some("a.ttl".to_owned()), Some(3), None, None);
        let json = serde_json::to_string(&location).expect("serialize");
        // skip_serializing_if keeps absent wire coords out of the wire form.
        assert!(!json.contains("gts_"), "unexpected wire keys: {json}");
        let round: Location = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round, location);
    }

    #[test]
    fn location_wire_coords_round_trip_and_display() {
        let location = Location::default()
            .with_gts_segment(2)
            .with_gts_quad(42)
            .with_gts_term(7);

        let json = serde_json::to_string(&location).expect("serialize");
        let round: Location = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round, location);
        assert_eq!(round.gts_quad_index, Some(42));

        // display() participates in Finding::sort_key, so the coords must render
        // deterministically in declared order (term, quad, reifier, frame, segment).
        assert_eq!(location.display(), "<unknown> term#7 quad#42 segment#2");
        assert!(!location.is_empty());
    }

    #[test]
    fn empty_location_stays_empty_with_wire_fields() {
        assert!(Location::default().is_empty());
    }

    #[test]
    fn finding_category_wire_values_round_trip() {
        // Iterate the closed `ALL` set so a newly-minted category cannot escape the
        // serde-rename == as_str == parse() round-trip invariant.
        for category in FindingCategory::ALL {
            // serde rename == as_str == the kebab wire value parse() accepts.
            let json = serde_json::to_string(&category).expect("serialize");
            assert_eq!(json, format!("\"{}\"", category.as_str()));
            assert_eq!(FindingCategory::parse(category.as_str()).unwrap(), category);
            let round: FindingCategory = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(round, category);
        }
        assert!(FindingCategory::parse("not-a-category").is_err());
    }

    #[test]
    fn finding_without_category_serializes_compactly() {
        // skip_serializing_if keeps an absent category out of the wire form, so
        // existing JSON/SARIF/RDF goldens are byte-unchanged.
        let finding = Finding::new(Severity::Error, "x.code", "msg");
        assert_eq!(finding.category, None);
        let json = serde_json::to_string(&finding).expect("serialize");
        assert!(
            !json.contains("category"),
            "unexpected category key: {json}"
        );
    }

    #[test]
    fn with_category_attaches_and_round_trips() {
        let finding = Finding::new(
            Severity::Warning,
            "validate.deep.permitted-conflict",
            "glut",
        )
        .with_category(FindingCategory::PermittedEpistemicConflict);
        let json = serde_json::to_string(&finding).expect("serialize");
        assert!(json.contains("\"category\":\"permitted-epistemic-conflict\""));
        let round: Finding = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            round.category,
            Some(FindingCategory::PermittedEpistemicConflict)
        );
    }

    #[test]
    fn category_is_not_an_ordering_axis() {
        // Two findings identical but for category must compare equal under
        // sort_key — the taxonomy never perturbs report ordering.
        let bare = Finding::new(Severity::Error, "c", "m");
        let tagged = Finding::new(Severity::Error, "c", "m")
            .with_category(FindingCategory::ContradictionWitness);
        assert_eq!(bare.sort_key(), tagged.sort_key());
    }

    #[test]
    fn normalize_orders_and_dedups_guidance() {
        use crate::diag::{GuidanceModality, GuidanceSource};
        use crate::grade::Standpoint;

        let claim = |modality, term: &str, text: &str, source| Guidance {
            modality,
            source,
            term_iri: term.to_owned(),
            text: text.to_owned(),
            standpoint: Standpoint::Advisory,
            help_uri: None,
        };

        // Guidance can reach a finding via the ledger merge in arrival order, so
        // build it deliberately unsorted, with one duplicate that differs only in the
        // non-identity fields (`source`/`help_uri`) — it must collapse to one.
        let mut finding = Finding::new(Severity::Warning, "c", "m")
            .with_guidance(claim(
                GuidanceModality::AvoidWhen,
                "gmeow:B",
                "avoid B",
                GuidanceSource::DocumentedTerm,
            ))
            .with_guidance(claim(
                GuidanceModality::HowToUse,
                "gmeow:A",
                "use A",
                GuidanceSource::DocumentedTerm,
            ));
        // A second surfacing of the how-to-use claim from the rule-governing key,
        // carrying a help URI — same `(modality, term_iri, text)`, so a duplicate.
        finding.push_guidance(Guidance {
            help_uri: Some("https://example/anchor".to_owned()),
            source: GuidanceSource::RuleGoverningTerm,
            ..claim(
                GuidanceModality::HowToUse,
                "gmeow:A",
                "use A",
                GuidanceSource::DocumentedTerm,
            )
        });

        finding.normalize();

        // Sorted on `(modality as u8, term_iri, text)`: HowToUse(0) before
        // AvoidWhen(2), and the duplicate how-to-use claim collapsed to one.
        assert_eq!(finding.guidance.len(), 2);
        assert_eq!(finding.guidance[0].modality, GuidanceModality::HowToUse);
        assert_eq!(finding.guidance[0].term_iri, "gmeow:A");
        assert_eq!(finding.guidance[1].modality, GuidanceModality::AvoidWhen);
        assert_eq!(finding.guidance[1].term_iri, "gmeow:B");

        // Idempotent: a second normalize is a no-op — the canonical form is stable.
        let before = finding.guidance.clone();
        finding.normalize();
        assert_eq!(finding.guidance, before);
    }
}

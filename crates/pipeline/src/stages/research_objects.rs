// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `research-objects` export leaf (P4): Croissant / RO-Crate / DataCite
//! Frictionless / DCAT research-object projections.
//!
//! The flagship Lillith GraphRAG worked example is rendered into
//! `generated/research-objects/lillith/` — the no-drift gate. Each artifact is a
//! GENERATED lossy projection of canonical GMEOW instance data, declaring its drops in
//! the format's native slot; the purrdf codecs additionally carry a soundness-checked
//! structural loss ledger, surfaced via `report_projection_losses`.
//!
//! The Croissant / DataCite / Frictionless / RO-Crate projections are cut onto the
//! purrdf research-object codecs (`project_croissant` / `project_datacite` /
//! `project_frictionless` / `project_ro_crate_with_assets`): a single caller-vocabulary
//! source A-Box per codec, projected to the format's canonical bytes. The RO-Crate
//! export uses the Attached codec — its `ro-crate-metadata.json` + `ro-crate-preview.html`
//! are engine-emitted, and the six worked-example A-Box `.ttl` files plus the Croissant
//! copy ride as caller-supplied `RoCrateAssets` payloads (each `.ttl` retagged
//! `x-gmeow`→BCP-47 and re-serialized through the canonical Turtle path). The `dcat.ttl`
//! runs the generated `dcat.rq` CONSTRUCT over the WHOLE composed ontology (every slice
//! source) plus the worked-example A-Box, canonicalized through the same purrdf Turtle
//! fold, so it drifts with the ontology. The git-ignored crate `.zip` is NOT produced.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use purrdf::{RdfDataset, RdfLiteral, RdfTerm, SparqlResult};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::native_query;
use crate::stages::source_load::module_files;

/// The native instance graph: a frozen dataset paired with its flat default-graph quad
/// stream (collected once for the many linear-scan reads the projection performs).
struct Store {
    quads: Vec<purrdf::RdfQuad>,
}

impl Store {
    fn from_dataset(dataset: &RdfDataset) -> Self {
        // The research-object inputs are Turtle (default graph only); keep the default-
        // graph quads in source-faithful form (statement layer re-materialized so a
        // `gmeow:contentDigest` etc. is visible exactly as authored).
        let quads = purrdf::native_quads::flat_rdf_quads_from_dataset(dataset)
            .into_iter()
            .filter(|q| q.graph_name.is_none())
            .collect();
        Self { quads }
    }

    /// Iterate `(subject, predicate, object)` of every default-graph quad.
    fn triples(&self) -> impl Iterator<Item = &purrdf::RdfQuad> {
        self.quads.iter()
    }
}

/// Logical-path prefix of the committed research-object artifacts.
pub const RESEARCH_OBJECTS_DIR: &str = "generated/research-objects/lillith";

const NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

const CROISSANT_CONFORMS_TO: &str = "http://mlcommons.org/croissant/1.1";
/// The `@context` value emitted by the RO-Crate 1.3 codec (an opaque profile IRI; the
/// caller supplies the full offline expansion table alongside it).
const RO_CRATE_CONTEXT: &str = "https://w3id.org/ro/crate/1.3/context";
/// The absolute RO-Crate 1.3 profile IRI the metadata descriptor `conformsTo`.
const RO_CRATE_PROFILE: &str = "https://w3id.org/ro/crate/1.3";
const DATACITE_NS: &str = "http://datacite.org/schema/kernel-4";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// The worked example's AUTHORED instance Turtle inputs, in generator order.
/// `(repo-relative path, crate file name)`. These are pure authored-source reads
/// (`slices/…`, `evals/…`); the sixth worked-example input — `scores.ttl` — is NOT
/// authored: it is the `stage-export-evals` product (see [`SCORES_INPUT_LABEL`]),
/// threaded in from the consumed evals product rather than read off the git-ignored
/// `generated/` tree (the stale-disk-fold class).
const AUTHORED_EXAMPLE_INPUTS: [(&str, &str); 5] = [
    (
        "slices/extensions/graphrag/examples/lillith-dataset.ttl",
        "lillith-dataset.ttl",
    ),
    (
        "slices/extensions/graphrag/examples/lillith-pipeline.ttl",
        "lillith-pipeline.ttl",
    ),
    (
        "slices/core/ai/examples/grounded-claim.ttl",
        "grounded-claim.ttl",
    ),
    ("evals/corpus.ttl", "corpus.ttl"),
    ("evals/rubric.ttl", "rubric.ttl"),
];

/// The logical label of the sixth worked-example input, `generated/evals/scores.ttl`.
/// It is the `stage-export-evals` product ([`crate::stages::evals::SCORES_PATH`]); the
/// research-objects stage sources its bytes from that consumed product, never a disk read
/// of the git-ignored file. Kept identical to the producer's path so the parsed A-Box is
/// byte-identical regardless of whether the bytes came from disk or the carrier.
const SCORES_INPUT_LABEL: &str = crate::stages::evals::SCORES_PATH;
/// The crate file name of the scores input (its RO-Crate member basename).
const SCORES_INPUT_NAME: &str = "scores.ttl";

/// One worked-example A-Box input in generator order: `(logical-label, crate-name, bytes)`.
type ExampleInput = (&'static str, &'static str, Vec<u8>);

/// The six worked-example A-Box inputs in generator order: the five authored Turtle files
/// read off disk plus `scores.ttl`, whose bytes are threaded in via `scores_ttl` (the
/// consumed `stage-export-evals` product) — never re-read off the git-ignored `generated/`
/// tree. `scores.ttl` stays LAST, preserving the union order the artifacts were generated under.
fn example_inputs(root: &Path, scores_ttl: &[u8]) -> Result<Vec<ExampleInput>, gmeow_errors::Diag> {
    let mut out: Vec<ExampleInput> = Vec::with_capacity(AUTHORED_EXAMPLE_INPUTS.len() + 1);
    for (rel, name) in AUTHORED_EXAMPLE_INPUTS {
        out.push((rel, name, std::fs::read(root.join(rel))?));
    }
    out.push((SCORES_INPUT_LABEL, SCORES_INPUT_NAME, scores_ttl.to_vec()));
    Ok(out)
}

fn g(local: &str) -> String {
    format!("{NS}{local}")
}

// ── helpers: load instance graph ──────────────────────────────────────────────

/// Parse `bytes` into a frozen native dataset (the canonical native codec).
fn parse_into(bytes: &[u8], path: &str) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    native_query::dataset_from_turtle(bytes, path)
}

/// Parse the six worked-example Turtle files into one native A-Box `Store` (each parsed
/// through the native codec then unioned, blanks standardized apart per source). The five
/// authored inputs are read off disk; `scores.ttl` rides in via `scores_ttl` (the consumed
/// `stage-export-evals` product), never a disk read of the git-ignored generated tree.
fn load_instance_graph(root: &Path, scores_ttl: &[u8]) -> Result<Store, gmeow_errors::Diag> {
    let inputs = example_inputs(root, scores_ttl)?;
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::with_capacity(inputs.len());
    for (label, _name, bytes) in &inputs {
        parsed.push(parse_into(bytes, label)?);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    Ok(Store::from_dataset(&RdfDataset::union(&refs)))
}

// ── instance-graph reads (mirror the Python `_text`/`_label` helpers) ──────────

/// First object literal lexical value (rdflib `g.value` picks an arbitrary one;
/// these subjects carry at most one of each text predicate).
fn text(store: &Store, subject: &str, predicate: &str) -> String {
    let mut best: Option<String> = None;
    for q in store.triples() {
        if !iri_is(&q.subject, subject) || q.predicate != predicate {
            continue;
        }
        let v = match &q.object {
            RdfTerm::Literal(l) => canonical_lexical(l),
            RdfTerm::Iri(n) => n.clone(),
            RdfTerm::BlankNode(b) => b.clone(),
            RdfTerm::Triple(_) => String::new(),
        };
        // rdflib `value()` returns a deterministic single value; for these
        // single-valued predicates any is fine, but keep the smallest for stability.
        best = Some(match best {
            Some(prev) if prev <= v => prev,
            _ => v,
        });
    }
    best.unwrap_or_default()
}

fn value_node(store: &Store, subject: &str, predicate: &str) -> Option<String> {
    let mut hits: Vec<String> = store
        .triples()
        .filter(|q| iri_is(&q.subject, subject) && q.predicate == predicate)
        .filter_map(|q| match &q.object {
            RdfTerm::Iri(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// True if `term` is the IRI `iri`.
fn iri_is(term: &RdfTerm, iri: &str) -> bool {
    matches!(term, RdfTerm::Iri(n) if n == iri)
}

fn label(store: &Store, subject: &str) -> String {
    let l = text(store, subject, RDFS_LABEL);
    if !l.is_empty() {
        return l;
    }
    let t = text(store, subject, &g("title"));
    if !t.is_empty() {
        return t;
    }
    subject.to_string()
}

/// Subjects of `rdf:type type_iri`, sorted by IRI.
fn subjects_of_type(store: &Store, type_iri: &str) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store.triples() {
        if q.predicate == RDF_TYPE
            && iri_is(&q.object, type_iri)
            && let RdfTerm::Iri(n) = &q.subject
        {
            set.insert(n.clone());
        }
    }
    set.into_iter().collect()
}

/// All object lexical/IRI values for `(subject, predicate)`, sorted unique.
fn objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store.triples() {
        if !iri_is(&q.subject, subject) || q.predicate != predicate {
            continue;
        }
        match &q.object {
            RdfTerm::Literal(l) => {
                set.insert(canonical_lexical(l));
            }
            RdfTerm::Iri(n) => {
                set.insert(n.clone());
            }
            _ => {}
        }
    }
    set.into_iter().collect()
}

fn slug(iri: &str) -> String {
    let trimmed = iri.trim_end_matches('/');
    let tail = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let tail = tail.rsplit('#').next().unwrap_or(tail);
    if tail.is_empty() {
        "resource".to_string()
    } else {
        tail.to_string()
    }
}

// ── literal canonicalization (instance-read lexical forms) ─────────────────────

/// Lexical value used by the instance-graph reads (`text`/`objects`): an xsd:dateTime
/// with a trailing `Z` offset re-isoformats to `+00:00`; everything else keeps its
/// lexical form.
fn canonical_lexical(l: &RdfLiteral) -> String {
    let lex = l.lexical_form.clone();
    if l.datatype.as_deref() == Some(&format!("{XSD}dateTime")[..]) {
        return canonical_datetime(&lex);
    }
    lex
}

/// rdflib canonicalizes xsd:dateTime via `datetime.fromisoformat(...).isoformat()`;
/// in this corpus the only transform is a trailing `Z` → `+00:00`.
fn canonical_datetime(lex: &str) -> String {
    if let Some(stripped) = lex.strip_suffix('Z') {
        format!("{stripped}+00:00")
    } else {
        lex.to_string()
    }
}

/// The JSON / JSON-LD / XML / HTML canonical form of an xsd:dateTime: a trailing
/// `+00:00` UTC offset collapses to `Z`. The CI-canonical artifacts emit `Z`;
/// a locally-regenerated fold may carry `+00:00` (Python isoformat). Normalizing
/// here makes the text outputs byte-identical to the committed artifacts whether
/// the input `gmeow.gts` carries `…+00:00` or `…Z`.
fn json_datetime(lex: &str) -> String {
    if let Some(stripped) = lex.strip_suffix("+00:00") {
        format!("{stripped}Z")
    } else {
        lex.to_string()
    }
}

// ── DatasetMeta ────────────────────────────────────────────────────────────────

struct DatasetMeta {
    iri: String,
    title: String,
    description: String,
    date_published: String,
    landing_page: String,
    version: Option<String>,
    cite_as: Option<String>,
}

fn dataset_meta(store: &Store) -> Result<DatasetMeta, gmeow_errors::Diag> {
    let mut candidates: Vec<String> = subjects_of_type(store, &g("Dataset"))
        .into_iter()
        .filter(|ds| value_node(store, ds, &g("hasLicense")).is_some())
        .collect();
    candidates.sort();
    let ds = candidates.into_iter().next().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: "no licensed gmeow:Dataset node found".into(),
        })
    })?;
    // The licensed dataset must carry a resolvable SPDX license id (a hard precondition
    // the codecs rely on); validate it here even though the codec sources the license IRI
    // straight off the A-Box rather than this descriptor.
    let license_node = value_node(store, &ds, &g("hasLicense")).unwrap();
    if text(store, &license_node, &g("spdxLicenseId")).is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "dataset descriptor {ds} has a gmeow:License without a gmeow:spdxLicenseId"
            ),
        }));
    }
    // Canonicalize the UTC offset to `Z` for the JSON / JSON-LD / XML emitters
    // (datapackage `created`, croissant/ro-crate `datePublished`, datacite `<date>`).
    // The fold may carry the lexical dateTime as either `…+00:00` or `…Z`; collapsing
    // `+00:00` → `Z` here keeps these text outputs stable regardless of which form the
    // input `gmeow.gts` happens to use. The `.ttl` payloads are serialized through the
    // canonical Turtle fold (raw lexical form) and are unaffected by this field.
    let date_published = json_datetime(&text(store, &ds, &g("datePublished")));
    let year_ok =
        date_published.len() >= 4 && date_published.chars().take(4).all(|c| c.is_ascii_digit());
    if !year_ok {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("dataset descriptor {ds} needs a valid gmeow:datePublished"),
        }));
    }
    let version = {
        let v = text(store, &ds, &g("version"));
        if v.is_empty() { None } else { Some(v) }
    };
    let cite_as = {
        let v = text(store, &ds, &g("citeAs"));
        if v.is_empty() { None } else { Some(v) }
    };
    let title = {
        let t = text(store, &ds, &g("title"));
        if t.is_empty() { label(store, &ds) } else { t }
    };
    let landing = {
        let l = text(store, &ds, &g("sourceLocation"));
        if l.is_empty() { ds.clone() } else { l }
    };
    Ok(DatasetMeta {
        iri: ds.clone(),
        title,
        // P5: every projection is lossy and declares its drops in the format's own native
        // description slot (Croissant/Frictionless/RO-Crate `description`, DataCite abstract).
        // The purrdf codecs additionally carry a soundness-checked structural loss ledger,
        // surfaced via `report_projection_losses`; this caller-authored note states the
        // gmeow-domain reductions the flat research-object formats cannot themselves ledger.
        description: {
            let base = text(store, &ds, &g("description"));
            let drops = "Declared drops (P5): reified relators (copyright, roles, memberships) \
                flatten; RDF 1.2 statement annotations (confidence, accordingTo, the four clocks) \
                are dropped; standpoint indexing is dropped — contested claims appear without \
                their vantage; blake3 remains the internal canonical content digest while \
                sha256/md5 are projected where supplied and the format allows.";
            if base.is_empty() {
                drops.to_string()
            } else {
                format!("{base} {drops}")
            }
        },
        date_published,
        landing_page: landing,
        version,
        cite_as,
    })
}

// ── digest maps ────────────────────────────────────────────────────────────────

/// Collect `gmeow:contentDigest` values keyed by `algorithm` (unprefixed → "digest").
fn digest_map(store: &Store, doc: &str) -> BTreeMap<String, String> {
    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    for raw in objects(store, doc, &g("contentDigest")) {
        let (key, hex) = match raw.split_once(':') {
            Some((algo, hex)) => (algo.to_string(), hex.to_string()),
            None => ("digest".to_string(), raw.clone()),
        };
        digests.entry(key).or_insert(hex);
    }
    digests
}

struct DocInfo {
    iri: String,
    name: String,
    content_url: String,
    digests: BTreeMap<String, String>,
}

fn documents(store: &Store) -> Vec<DocInfo> {
    subjects_of_type(store, &g("Document"))
        .into_iter()
        .map(|doc| DocInfo {
            name: label(store, &doc),
            content_url: text(store, &doc, &g("sourceLocation")),
            digests: digest_map(store, &doc),
            iri: doc,
        })
        .collect()
}

// ── research-object codec configuration (purrdf project_croissant / _datacite / _frictionless) ──

/// Wrap a projection failure as a pipeline parse diagnostic.
fn ro_err(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Parse { message })
}

/// Right-sized [`purrdf::ProjectionLimits`] for the tiny Lillith worked example — a
/// handful of small artifacts and shallow JSON, NOT the 128 MB SKOS/OBO bounds. The
/// 12-artifact ceiling covers the attached RO-Crate package's widest member set
/// (`ro-crate-metadata.json` + `ro-crate-preview.html` + the seven payload assets).
fn research_limits() -> Result<purrdf::ProjectionLimits, gmeow_errors::Diag> {
    purrdf::ProjectionLimits::new(12, 4_000_000, 8_000_000, 16_000_000, 12)
        .map_err(|e| ro_err(format!("research-object ProjectionLimits: {e}")))
}

/// The complete caller-owned RDF vocabulary binding: how the source research-object
/// A-Box built by [`build_research_source`] expresses each semantic role. gmeow
/// predicates/classes for the concepts the worked example carries; the real rdf:/xsd:
/// datatype IRIs the pivot compares literal datatypes against; a distinct absolute
/// gmeow IRI for every remaining role (purrdf rejects any missing, relative, or
/// duplicate binding). Because [`build_research_source`] emits triples keyed off THIS
/// same map, source and reader can never drift.
fn research_roles() -> Result<purrdf::ResearchObjectRoles, gmeow_errors::Diag> {
    use purrdf::ResearchRole as RR;
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let iri = |role: purrdf::ResearchRole| -> String {
        match role {
            RR::RdfType => RDF_TYPE.to_string(),
            RR::DatasetClass => g("Dataset"),
            RR::Title => g("title"),
            RR::Description => g("description"),
            RR::Identifier => g("citeAs"),
            RR::Version => g("version"),
            RR::Issued => g("datePublished"),
            RR::Modified => g("dateModified"),
            RR::LandingPage => g("sourceLocation"),
            RR::Keyword => g("keyword"),
            RR::License => g("hasLicense"),
            RR::Creator => g("wasAttributedTo"),
            RR::Publisher => g("researchPublisher"),
            RR::HasResource => g("hasResource"),
            RR::HasActivity => g("hasActivity"),
            RR::HasRecordSet => g("hasRecordSet"),
            RR::AgentClass => g("Organization"),
            RR::AgentName => RDFS_LABEL.to_string(),
            RR::ResourceClass => g("Document"),
            RR::ResourceName => g("resourceName"),
            RR::ResourceDescription => g("resourceDescription"),
            RR::ResourcePath => g("resourcePath"),
            RR::ResourceUrl => g("contentUrl"),
            RR::MediaType => g("mediaType"),
            RR::Format => g("resourceFormat"),
            RR::ByteSize => g("byteSize"),
            RR::Checksum => g("hasChecksum"),
            RR::ChecksumClass => g("Checksum"),
            RR::ChecksumAlgorithm => g("checksumAlgorithm"),
            RR::ChecksumValue => g("checksumValue"),
            RR::ActivityClass => g("Activity"),
            RR::ActivityName => g("activityName"),
            RR::Instrument => g("instrument"),
            RR::Actor => g("actor"),
            RR::Object => g("activityObject"),
            RR::Result => g("activityResult"),
            RR::EndTime => g("endTime"),
            RR::Workflow => g("workflow"),
            RR::RecordSetClass => g("RecordSet"),
            RR::RecordSetName => g("recordSetName"),
            RR::RecordSetDescription => g("recordSetDescription"),
            RR::HasField => g("hasField"),
            RR::HasRow => g("hasRow"),
            RR::FieldClass => g("Field"),
            RR::FieldName => g("fieldName"),
            RR::FieldDataType => g("fieldDataType"),
            RR::JsonDatatype => format!("{RDF}JSON"),
            RR::RdfLangString => format!("{RDF}langString"),
            RR::RdfDirLangString => format!("{RDF}dirLangString"),
            RR::XsdString => format!("{XSD}string"),
            RR::XsdNonNegativeInteger => format!("{XSD}nonNegativeInteger"),
            RR::XsdDateTime => format!("{XSD}dateTime"),
        }
    };
    let map: BTreeMap<purrdf::ResearchRole, String> = purrdf::RESEARCH_ROLES
        .iter()
        .copied()
        .map(|role| (role, iri(role)))
        .collect();
    purrdf::ResearchObjectRoles::new(map)
        .map_err(|e| ro_err(format!("research-object ResearchObjectRoles: {e}")))
}

/// The shared research-object config (roles + identity + policy) every codec consumes.
/// The dataset identity is the canonical `gmeow:Dataset` IRI; the entity base is the
/// gmeow namespace (ends in `/`, so minted resource/checksum/record-set IRIs resolve).
fn research_common_config(
    dataset_iri: &str,
) -> Result<purrdf::ResearchObjectConfig, gmeow_errors::Diag> {
    let roles = research_roles()?;
    let identity = purrdf::ResearchObjectIdentity::new(dataset_iri, NS)
        .map_err(|e| ro_err(format!("research-object ResearchObjectIdentity: {e}")))?;
    let policy =
        purrdf::ResearchObjectPolicy::new(research_limits()?, 100_000, 100_000, 100_000, 12)
            .map_err(|e| ro_err(format!("research-object ResearchObjectPolicy: {e}")))?;
    Ok(purrdf::ResearchObjectConfig::new(roles, identity, policy))
}

/// The gmeow-owned [`purrdf::CroissantConfig`]: a complete compact-term vocabulary,
/// its offline JSON-LD expansion table (one distinct absolute IRI per term), and the
/// Croissant conformance profile emitted through `conformsTo`.
fn croissant_config(
    common: purrdf::ResearchObjectConfig,
) -> Result<purrdf::CroissantConfig, gmeow_errors::Diag> {
    use purrdf::CroissantRole as CR;
    let term = |role: purrdf::CroissantRole| -> &'static str {
        match role {
            CR::DatasetClass => "sc:Dataset",
            CR::FileObjectClass => "cr:FileObject",
            CR::RecordSetClass => "cr:RecordSet",
            CR::FieldClass => "cr:Field",
            CR::AgentClass => "sc:Organization",
            CR::ActivityClass => "sc:CreateAction",
            CR::Name => "name",
            CR::Description => "description",
            CR::Identifier => "identifier",
            CR::Version => "version",
            CR::DatePublished => "datePublished",
            CR::DateModified => "dateModified",
            CR::Url => "url",
            CR::Keywords => "keywords",
            CR::License => "license",
            CR::Creator => "creator",
            CR::Publisher => "publisher",
            CR::Distribution => "distribution",
            CR::Activity => "recordActivity",
            CR::RecordSet => "recordSet",
            CR::ConformsTo => "conformsTo",
            CR::Path => "contentPath",
            CR::ContentUrl => "contentUrl",
            CR::EncodingFormat => "encodingFormat",
            CR::Format => "fileFormat",
            CR::ContentSize => "contentSize",
            CR::Sha256 => "sha256",
            CR::InlineContent => "inlineData",
            CR::Field => "field",
            CR::DataType => "dataType",
            CR::Records => "data",
            CR::Instrument => "instrument",
            CR::Agent => "actionAgent",
            CR::Object => "object",
            CR::Result => "result",
            CR::EndTime => "endTime",
            CR::Workflow => "workflow",
        }
    };
    let expand = |t: &str| -> String {
        format!(
            "https://blackcatinformatics.ca/gmeow/croissant#{}",
            t.replace(':', "_")
        )
    };
    let vocabulary_map: BTreeMap<purrdf::CroissantRole, String> = purrdf::CROISSANT_ROLES
        .iter()
        .copied()
        .map(|role| (role, term(role).to_string()))
        .collect();
    let definitions: BTreeMap<String, String> = purrdf::CROISSANT_ROLES
        .iter()
        .copied()
        .map(|role| {
            let t = term(role);
            (t.to_string(), expand(t))
        })
        .collect();
    let vocabulary = purrdf::CroissantVocabulary::new(vocabulary_map)
        .map_err(|e| ro_err(format!("CroissantVocabulary: {e}")))?;
    let context = purrdf::OfflineJsonLdContext::new(
        serde_json::Value::String(CROISSANT_CONFORMS_TO.to_string()),
        definitions,
    )
    .map_err(|e| ro_err(format!("Croissant OfflineJsonLdContext: {e}")))?;
    purrdf::CroissantConfig::new(common, context, vocabulary, CROISSANT_CONFORMS_TO)
        .map_err(|e| ro_err(format!("CroissantConfig: {e}")))
}

/// The gmeow-owned [`purrdf::DataCiteConfig`]: the DataCite 4.6 element namespace,
/// XML-Schema-instance namespace, schema location, and the selected controlled values.
fn datacite_config(
    common: purrdf::ResearchObjectConfig,
) -> Result<purrdf::DataCiteConfig, gmeow_errors::Diag> {
    let controlled = purrdf::DataCiteControlledValues::new(
        "DOI",
        "Dataset",
        "Organizational",
        "gmeow-agent",
        g("agentIdentifierScheme"),
        "URL",
        "IsDescribedBy",
        "HasPart",
        "IsProducedBy",
        "References",
        "Issued",
        "Updated",
        "Abstract",
    )
    .map_err(|e| ro_err(format!("DataCiteControlledValues: {e}")))?;
    purrdf::DataCiteConfig::new(
        common,
        DATACITE_NS,
        XSI_NS,
        "https://schema.datacite.org/meta/kernel-4.5/metadata.xsd",
        controlled,
    )
    .map_err(|e| ro_err(format!("DataCiteConfig: {e}")))
}

/// The gmeow-owned [`purrdf::FrictionlessConfig`]: the Data Package v1 profile and the
/// caller-selected package name.
fn frictionless_config(
    common: purrdf::ResearchObjectConfig,
    package_name: &str,
) -> Result<purrdf::FrictionlessConfig, gmeow_errors::Diag> {
    purrdf::FrictionlessConfig::new(common, purrdf::FRICTIONLESS_PROFILE, package_name)
        .map_err(|e| ro_err(format!("FrictionlessConfig: {e}")))
}

/// The gmeow-owned attached [`purrdf::RoCrateConfig`]: a complete RO-Crate 1.3
/// compact-term vocabulary, its offline JSON-LD expansion table (one distinct absolute
/// IRI per term), the absolute 1.3 profile IRI, and the reserved attached-crate
/// identities (`ro-crate-metadata.json` descriptor, `./` root). The `Attached` packaging
/// makes the codec emit `ro-crate-metadata.json` + `ro-crate-preview.html` and carry the
/// caller-supplied [`purrdf::RoCrateAssets`] payloads alongside them.
fn ro_crate_config(
    common: purrdf::ResearchObjectConfig,
) -> Result<purrdf::RoCrateConfig, gmeow_errors::Diag> {
    use purrdf::RoCrateRole as CR;
    let term = |role: purrdf::RoCrateRole| -> &'static str {
        match role {
            CR::RootDatasetClass => "Dataset",
            CR::MetadataDescriptorClass => "CreativeWork",
            CR::FileClass => "File",
            CR::AgentClass => "Organization",
            CR::ActivityClass => "CreateAction",
            CR::RecordSetClass => "RecordSet",
            CR::FieldClass => "Field",
            CR::Name => "name",
            CR::Description => "description",
            CR::Identifier => "identifier",
            CR::Version => "version",
            CR::DatePublished => "datePublished",
            CR::DateModified => "dateModified",
            CR::Url => "url",
            CR::Keywords => "keywords",
            CR::License => "license",
            CR::Creator => "creator",
            CR::Publisher => "publisher",
            CR::HasPart => "hasPart",
            CR::Mentions => "mentions",
            CR::ConformsTo => "conformsTo",
            CR::About => "about",
            CR::Path => "contentPath",
            CR::ContentUrl => "contentUrl",
            CR::EncodingFormat => "encodingFormat",
            CR::Format => "fileFormat",
            CR::ContentSize => "contentSize",
            CR::Checksum => "checksum",
            CR::ChecksumAlgorithm => "checksumAlgorithm",
            CR::ChecksumValue => "checksumValue",
            CR::InlineContent => "inlineData",
            CR::Field => "field",
            CR::DataType => "dataType",
            CR::Records => "data",
            CR::Instrument => "instrument",
            CR::Agent => "actionAgent",
            CR::Object => "object",
            CR::Result => "result",
            CR::EndTime => "endTime",
            CR::Workflow => "workflow",
        }
    };
    let expand =
        |t: &str| -> String { format!("https://blackcatinformatics.ca/gmeow/ro-crate#{t}") };
    let vocabulary_map: BTreeMap<purrdf::RoCrateRole, String> = purrdf::RO_CRATE_ROLES
        .iter()
        .copied()
        .map(|role| (role, term(role).to_string()))
        .collect();
    let definitions: BTreeMap<String, String> = purrdf::RO_CRATE_ROLES
        .iter()
        .copied()
        .map(|role| {
            let t = term(role);
            (t.to_string(), expand(t))
        })
        .collect();
    let vocabulary = purrdf::RoCrateVocabulary::new(vocabulary_map)
        .map_err(|e| ro_err(format!("RoCrateVocabulary: {e}")))?;
    let context = purrdf::OfflineJsonLdContext::new(
        serde_json::Value::String(RO_CRATE_CONTEXT.to_string()),
        definitions,
    )
    .map_err(|e| ro_err(format!("RO-Crate OfflineJsonLdContext: {e}")))?;
    purrdf::RoCrateConfig::new(
        common,
        context,
        vocabulary,
        RO_CRATE_PROFILE,
        purrdf::RO_CRATE_ARTIFACT,
        "./",
        purrdf::RoCratePackaging::Attached,
    )
    .map_err(|e| ro_err(format!("RoCrateConfig: {e}")))
}

/// Intern `(subject, predicate, object)` IRIs and push the default-graph relation.
fn push_rel(builder: &mut purrdf::RdfDatasetBuilder, subject: &str, predicate: &str, object: &str) {
    let subject = builder.intern_iri(subject);
    let predicate = builder.intern_iri(predicate);
    let object = builder.intern_iri(object);
    builder.push_quad(subject, predicate, object, None);
}

/// Push a typed-literal `(subject, predicate, value^^datatype)` default-graph statement.
fn push_lit(
    builder: &mut purrdf::RdfDatasetBuilder,
    subject: &str,
    predicate: &str,
    value: &str,
    datatype: &str,
) {
    let subject = builder.intern_iri(subject);
    let predicate = builder.intern_iri(predicate);
    let object = builder.intern_literal(purrdf::RdfLiteral {
        lexical_form: value.to_string(),
        datatype: Some(datatype.to_string()),
        language: None,
        direction: None,
    });
    builder.push_quad(subject, predicate, object, None);
}

/// True when `url` is an absolute HTTP(S) IRI (safe to emit as an RDF IRI object).
fn is_http_iri(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Build the single research-object source [`RdfDataset`] every codec projects from,
/// mirroring the reads of the retired hand-rolled builders but re-expressed in the
/// caller role vocabulary of [`research_roles`]: the licensed `gmeow:Dataset` and its
/// catalog metadata, the attributed organisation as both creator and publisher, each
/// `gmeow:Document` as a resource with its content-address checksums, and the
/// chunk/claim/eval-score record sets with typed fields and canonical JSON rows.
fn build_research_source(
    common: &purrdf::ResearchObjectConfig,
    store: &Store,
    ds: &DatasetMeta,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    use purrdf::ResearchRole as RR;
    let roles = common.roles();
    let xsd_string = roles.iri(RR::XsdString).to_string();
    let json_dt = roles.iri(RR::JsonDatatype).to_string();
    let dsi = ds.iri.as_str();
    let mut b = purrdf::RdfDatasetBuilder::new();

    // ── the dataset descriptor ──────────────────────────────────────────────────
    push_rel(
        &mut b,
        dsi,
        roles.iri(RR::RdfType),
        roles.iri(RR::DatasetClass),
    );
    push_lit(&mut b, dsi, roles.iri(RR::Title), &ds.title, &xsd_string);
    if !ds.description.is_empty() {
        push_lit(
            &mut b,
            dsi,
            roles.iri(RR::Description),
            &ds.description,
            &xsd_string,
        );
    }
    if !ds.date_published.is_empty() {
        push_lit(
            &mut b,
            dsi,
            roles.iri(RR::Issued),
            &ds.date_published,
            &xsd_string,
        );
    }
    if let Some(v) = &ds.version {
        push_lit(&mut b, dsi, roles.iri(RR::Version), v, &xsd_string);
    }
    if let Some(c) = &ds.cite_as {
        push_lit(&mut b, dsi, roles.iri(RR::Identifier), c, &xsd_string);
    }
    if is_http_iri(&ds.landing_page) {
        push_rel(&mut b, dsi, roles.iri(RR::LandingPage), &ds.landing_page);
    } else if !ds.landing_page.is_empty() {
        push_lit(
            &mut b,
            dsi,
            roles.iri(RR::LandingPage),
            &ds.landing_page,
            &xsd_string,
        );
    }
    if let Some(license) = value_node(store, dsi, &g("hasLicense")) {
        push_rel(&mut b, dsi, roles.iri(RR::License), &license);
    }
    // The attributed organisation is projected as BOTH creator and publisher (the
    // catalog projections carried the same org into each slot); DataCite/Frictionless
    // require a named agent in both roles.
    if let Some(org) = value_node(store, dsi, &g("wasAttributedTo")) {
        push_rel(&mut b, dsi, roles.iri(RR::Creator), &org);
        push_rel(&mut b, dsi, roles.iri(RR::Publisher), &org);
        push_rel(
            &mut b,
            &org,
            roles.iri(RR::RdfType),
            roles.iri(RR::AgentClass),
        );
        let name = label(store, &org);
        if !name.is_empty() {
            push_lit(&mut b, &org, roles.iri(RR::AgentName), &name, &xsd_string);
        }
    }

    // ── resources: each gmeow:Document, minted under the entity base ────────────
    let mut seen_resource: BTreeSet<String> = BTreeSet::new();
    for doc in documents(store) {
        let base = slug(&doc.iri).to_lowercase().replace('_', "-");
        let mut name = base.clone();
        let mut disambiguate = 2;
        while !seen_resource.insert(name.clone()) {
            name = format!("{base}-{disambiguate}");
            disambiguate += 1;
        }
        let rid = format!("{NS}{name}");
        push_rel(&mut b, dsi, roles.iri(RR::HasResource), &rid);
        push_rel(
            &mut b,
            &rid,
            roles.iri(RR::RdfType),
            roles.iri(RR::ResourceClass),
        );
        if !doc.name.is_empty() {
            push_lit(
                &mut b,
                &rid,
                roles.iri(RR::ResourceName),
                &doc.name,
                &xsd_string,
            );
        }
        if is_http_iri(&doc.content_url) {
            push_rel(&mut b, &rid, roles.iri(RR::ResourceUrl), &doc.content_url);
        } else if !doc.content_url.is_empty() {
            push_lit(
                &mut b,
                &rid,
                roles.iri(RR::ResourcePath),
                &doc.content_url,
                &xsd_string,
            );
        }
        for (algo, hex) in &doc.digests {
            let cid = format!("{NS}checksum/{name}/{algo}");
            push_rel(&mut b, &rid, roles.iri(RR::Checksum), &cid);
            push_rel(
                &mut b,
                &cid,
                roles.iri(RR::RdfType),
                roles.iri(RR::ChecksumClass),
            );
            push_lit(
                &mut b,
                &cid,
                roles.iri(RR::ChecksumAlgorithm),
                algo,
                &xsd_string,
            );
            push_lit(&mut b, &cid, roles.iri(RR::ChecksumValue), hex, &xsd_string);
        }
    }

    // ── record sets: chunks, claims, eval scores ────────────────────────────────
    let emit_record_set = |b: &mut purrdf::RdfDatasetBuilder,
                           id: &str,
                           name: &str,
                           description: &str,
                           fields: &[(&str, &str)],
                           rows: &[String]| {
        push_rel(b, dsi, roles.iri(RR::HasRecordSet), id);
        push_rel(b, id, roles.iri(RR::RdfType), roles.iri(RR::RecordSetClass));
        push_lit(b, id, roles.iri(RR::RecordSetName), name, &xsd_string);
        push_lit(
            b,
            id,
            roles.iri(RR::RecordSetDescription),
            description,
            &xsd_string,
        );
        for (fname, dtype) in fields {
            let fid = format!("{NS}field/{name}/{fname}");
            push_rel(b, id, roles.iri(RR::HasField), &fid);
            push_rel(b, &fid, roles.iri(RR::RdfType), roles.iri(RR::FieldClass));
            push_lit(b, &fid, roles.iri(RR::FieldName), fname, &xsd_string);
            push_lit(b, &fid, roles.iri(RR::FieldDataType), dtype, &xsd_string);
        }
        for row in rows {
            push_lit(b, id, roles.iri(RR::HasRow), row, &json_dt);
        }
    };
    let row_json = |value: &serde_json::Value| -> Result<String, gmeow_errors::Diag> {
        serde_json::to_string(value).map_err(|e| ro_err(format!("record-set row JSON: {e}")))
    };

    let chunks = subjects_of_type(store, &g("Chunk"));
    if !chunks.is_empty() {
        let mut rows = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            rows.push(row_json(&serde_json::json!({
                "chunks/id": chunk,
                "chunks/source": text(store, chunk, &g("chunkOf")),
                "chunks/spanStart": text(store, chunk, &g("spanStart")).parse::<i64>().unwrap_or(0),
                "chunks/spanEnd": text(store, chunk, &g("spanEnd")).parse::<i64>().unwrap_or(0),
                "chunks/digest": text(store, chunk, &g("contentDigest")),
            }))?);
        }
        emit_record_set(
            &mut b,
            &format!("{NS}recordset/chunks"),
            "chunks",
            "Content-addressed retrieval segments with typed offsets into their source documents.",
            &[
                ("id", "sc:Text"),
                ("source", "sc:Text"),
                ("spanStart", "sc:Integer"),
                ("spanEnd", "sc:Integer"),
                ("digest", "sc:Text"),
            ],
            &rows,
        );
    }

    let claims = subjects_of_type(store, &g("StandpointClaim"));
    if !claims.is_empty() {
        let mut rows = Vec::with_capacity(claims.len());
        for claim in &claims {
            rows.push(row_json(&serde_json::json!({
                "claims/id": claim,
                "claims/vantage": text(store, claim, &g("vantage")),
                "claims/modality": slug(&text(store, claim, &g("claimModality"))),
                "claims/grounded": value_node(store, claim, &g("groundedIn")).is_some(),
            }))?);
        }
        emit_record_set(
            &mut b,
            &format!("{NS}recordset/claims"),
            "claims",
            "Model-extracted claims: vantage-attributed, modality-tagged, grounded flag from evidence spans. (Standpoint nuance beyond the flag is a declared drop.)",
            &[
                ("id", "sc:Text"),
                ("vantage", "sc:Text"),
                ("modality", "sc:Text"),
                ("grounded", "sc:Boolean"),
            ],
            &rows,
        );
    }

    let mut score_rows: Vec<String> = Vec::new();
    for assessment in subjects_of_type(store, &g("Assessment")) {
        let lexical = text(store, &assessment, &g("assessmentScoreValue"));
        if lexical.is_empty() {
            continue;
        }
        let parsed: f64 = lexical.trim().parse().map_err(|e| {
            ro_err(format!(
                "assessmentScoreValue {lexical:?} is not a valid float: {e}"
            ))
        })?;
        let number = serde_json::Number::from_f64(parsed).ok_or_else(|| {
            ro_err(format!(
                "assessmentScoreValue {lexical:?} is not a finite JSON number"
            ))
        })?;
        score_rows.push(row_json(&serde_json::json!({
            "evalScores/model": text(store, &assessment, &g("assessmentTarget")),
            "evalScores/criterion": slug(&text(store, &assessment, &g("assessmentCriterion"))),
            "evalScores/score": number,
        }))?);
    }
    if !score_rows.is_empty() {
        emit_record_set(
            &mut b,
            &format!("{NS}recordset/evalScores"),
            "evalScores",
            "Vantage-indexed rubric assessments from the gmeow-evals harness.",
            &[
                ("model", "sc:Text"),
                ("criterion", "sc:Text"),
                ("score", "sc:Float"),
            ],
            &score_rows,
        );
    }

    b.freeze()
        .map_err(|e| ro_err(format!("research-object source dataset freeze: {e}")))
}

/// Build the RO-Crate source [`RdfDataset`] the attached codec projects from. Unlike
/// [`build_research_source`] (whose resources are the worked example's `gmeow:Document`
/// content-addressed sources), the RO-Crate source's RESOURCES ARE THE PACKAGED FILES:
/// the six retagged A-Box `.ttl` payloads plus the Croissant copy. Each packaged file
/// becomes a `ResourceClass` node minted under the entity base (so its native id strips
/// to the bare `<filename>`), linked into the root dataset via `HasResource`, declaring
/// its `ResourcePath = <filename>` and `ByteSize = <exact payload byte length>`. This is
/// exactly what `validate_attached_assets` cross-checks against the supplied
/// [`purrdf::RoCrateAssets`]: every local File resource is in the root `hasPart`, its
/// single path equals its native id, an asset exists at that path, and the declared byte
/// size matches the asset body one-to-one. `files` carries `(<filename>, byte-len)` for
/// all seven payloads in the SAME order/identity the assets are built from.
fn build_ro_crate_source(
    common: &purrdf::ResearchObjectConfig,
    store: &Store,
    ds: &DatasetMeta,
    files: &[(String, usize)],
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    use purrdf::ResearchRole as RR;
    let roles = common.roles();
    let xsd_string = roles.iri(RR::XsdString).to_string();
    let xsd_nn = roles.iri(RR::XsdNonNegativeInteger).to_string();
    let dsi = ds.iri.as_str();
    let mut b = purrdf::RdfDatasetBuilder::new();

    // ── the root dataset descriptor (mirrors the catalog metadata) ──────────────
    push_rel(
        &mut b,
        dsi,
        roles.iri(RR::RdfType),
        roles.iri(RR::DatasetClass),
    );
    push_lit(&mut b, dsi, roles.iri(RR::Title), &ds.title, &xsd_string);
    if !ds.description.is_empty() {
        push_lit(
            &mut b,
            dsi,
            roles.iri(RR::Description),
            &ds.description,
            &xsd_string,
        );
    }
    if !ds.date_published.is_empty() {
        push_lit(
            &mut b,
            dsi,
            roles.iri(RR::Issued),
            &ds.date_published,
            &xsd_string,
        );
    }
    if let Some(v) = &ds.version {
        push_lit(&mut b, dsi, roles.iri(RR::Version), v, &xsd_string);
    }
    if let Some(c) = &ds.cite_as {
        push_lit(&mut b, dsi, roles.iri(RR::Identifier), c, &xsd_string);
    }
    if is_http_iri(&ds.landing_page) {
        push_rel(&mut b, dsi, roles.iri(RR::LandingPage), &ds.landing_page);
    } else if !ds.landing_page.is_empty() {
        push_lit(
            &mut b,
            dsi,
            roles.iri(RR::LandingPage),
            &ds.landing_page,
            &xsd_string,
        );
    }
    if let Some(license) = value_node(store, dsi, &g("hasLicense")) {
        push_rel(&mut b, dsi, roles.iri(RR::License), &license);
    }
    if let Some(org) = value_node(store, dsi, &g("wasAttributedTo")) {
        push_rel(&mut b, dsi, roles.iri(RR::Creator), &org);
        push_rel(&mut b, dsi, roles.iri(RR::Publisher), &org);
        push_rel(
            &mut b,
            &org,
            roles.iri(RR::RdfType),
            roles.iri(RR::AgentClass),
        );
        let name = label(store, &org);
        if !name.is_empty() {
            push_lit(&mut b, &org, roles.iri(RR::AgentName), &name, &xsd_string);
        }
    }

    // ── resources: the packaged crate files, one File per payload ───────────────
    for (name, len) in files {
        // Mint under the entity base so `config.native_id` strips the prefix back to
        // the bare `<filename>` — the crate-relative path the asset is keyed by.
        let rid = format!("{NS}{name}");
        push_rel(&mut b, dsi, roles.iri(RR::HasResource), &rid);
        push_rel(
            &mut b,
            &rid,
            roles.iri(RR::RdfType),
            roles.iri(RR::ResourceClass),
        );
        push_lit(&mut b, &rid, roles.iri(RR::ResourceName), name, &xsd_string);
        push_lit(&mut b, &rid, roles.iri(RR::ResourcePath), name, &xsd_string);
        push_lit(
            &mut b,
            &rid,
            roles.iri(RR::ByteSize),
            &len.to_string(),
            &xsd_nn,
        );
    }

    b.freeze()
        .map_err(|e| ro_err(format!("RO-Crate source dataset freeze: {e}")))
}

/// Group a purrdf [`purrdf::LossLedger`] by `(code, note)` and trace it — no RDF →
/// (Croissant | DataCite | Frictionless) lowering loss is silently dropped. Mirrors
/// `export.rs`'s `report_projection_losses`.
fn report_projection_losses(surface: &str, ledger: &purrdf::LossLedger) {
    let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for loss in ledger.entries() {
        let subject = loss
            .location
            .as_deref()
            .and_then(|location| location.subject.as_deref())
            .unwrap_or("<unlocated>");
        grouped
            .entry((loss.code.as_ref(), loss.note.as_ref()))
            .or_default()
            .push(subject);
    }
    for ((construct, reason), mut subjects) in grouped {
        subjects.sort_unstable();
        subjects.dedup();
        let examples = subjects
            .iter()
            .take(5)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if subjects.len() > 5 {
            format!(" (+{} more)", subjects.len() - 5)
        } else {
            String::new()
        };
        tracing::info!(
            target: "export_projection_loss",
            surface = surface,
            construct = construct,
            subjects = subjects.len(),
            reason = reason,
            examples = %format!("{examples}{suffix}"),
            "lossy drop projecting the research-object source A-Box",
        );
    }
}

// ── source-Turtle payload → canonical Turtle (with x-gmeow language retag) ─────

/// Load the internal→BCP-47 language-tag map from the carrier varieties in the
/// module surfaces. The internal `x-gmeow-*` tag rides `lang:carrierTag` on a
/// carrier variety since the lang: graft; its public BCP-47 code is DERIVED over
/// the model (never authored per language) — the variety's `lang:varietyOf`
/// parent sign system carries the ISO 639 primary subtag as `skos:notation`
/// (script suppressed for the carriers), matching the tag the `bcp47` projection
/// folds.
fn load_tag_map(root: &Path) -> Result<BTreeMap<String, String>, gmeow_errors::Diag> {
    const P_CARRIER: &str = "https://blackcatinformatics.ca/lang/carrierTag";
    const P_VARIETY_OF: &str = "https://blackcatinformatics.ca/lang/varietyOf";
    const P_NOTATION: &str = "http://www.w3.org/2004/02/skos/core#notation";

    let mut parsed: Vec<Arc<RdfDataset>> = Vec::new();
    for module in module_files(root)? {
        let bytes = std::fs::read(&module)?;
        parsed.push(parse_into(&bytes, &module.display().to_string())?);
    }
    let onto = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&onto)?;
    parsed.push(parse_into(&bytes, "ontology/gmeow.ttl")?);
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    let store = Store::from_dataset(&RdfDataset::union(&refs));

    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for q in store.triples() {
        if q.predicate != P_CARRIER {
            continue;
        }
        let RdfTerm::Iri(subj) = &q.subject else {
            continue;
        };
        let RdfTerm::Literal(internal_lit) = &q.object else {
            continue;
        };
        let internal = internal_lit.lexical_form.clone();
        // The carrier variety's parent sign system (lang:varietyOf).
        let Some(parent) = store.triples().find_map(|qq| {
            (iri_is(&qq.subject, subj) && qq.predicate == P_VARIETY_OF)
                .then(|| match &qq.object {
                    RdfTerm::Iri(p) => Some(p.clone()),
                    _ => None,
                })
                .flatten()
        }) else {
            continue;
        };
        // The parent's ISO 639 primary subtag (skos:notation) is the derived BCP-47 tag.
        if let Some(ext) = store.triples().find_map(|qq| {
            (iri_is(&qq.subject, &parent) && qq.predicate == P_NOTATION)
                .then(|| match &qq.object {
                    RdfTerm::Literal(l) => Some(l.lexical_form.clone()),
                    _ => None,
                })
                .flatten()
        }) {
            map.insert(internal, ext.trim().to_ascii_lowercase());
        }
    }
    Ok(map)
}

/// Render one worked-example A-Box `.ttl` as an RO-Crate payload body: parse the source
/// Turtle, retag its `@x-gmeow-*` literal language tags to their public BCP-47 form at the
/// term level, canonicalize every typed literal to the W3C XSD mapping, and re-serialize
/// through the canonical Turtle fold (`serialize_dataset` → N-Triples →
/// `canonical_turtle`, the exact path `render_dcat` uses). The returned bytes are the
/// caller-supplied [`purrdf::RoCrateAssets`] payload AND the source of the resource's
/// declared `ByteSize`, so the two can never drift.
fn render_source_turtle_payload(
    bytes: &[u8],
    path: &str,
    tag_map: &BTreeMap<String, String>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let dataset = parse_into(bytes, path)?;

    // Re-emit each triple, retagging `@x-gmeow-*` literal language tags to their public
    // BCP-47 form on the way through; the flat quad stream re-materializes the RDF 1.2
    // statement layer so the source A-Box round-trips through the canonical serializer.
    let mut retagged: Vec<purrdf::RdfQuad> =
        purrdf::native_quads::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            .map(|mut quad| {
                if let RdfTerm::Literal(lit) = &quad.object {
                    quad.object = RdfTerm::Literal(retag_native_literal(lit, tag_map));
                }
                quad
            })
            .collect();
    // Canonicalize every typed-literal lexical form to the W3C XSD canonical mapping
    // (the native codecs preserve raw lexical forms, so without this the round-trip
    // would drift) — the SAME normalization the snapshot carrier applies.
    for quad in &mut retagged {
        canonicalize_term_xsd(&mut quad.object)?;
    }
    let flat = purrdf::native_quads::flat_dataset_from_quads(&retagged).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("{path}: re-freeze retagged quads: {e}"),
        })
    })?;
    let nt = purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("{path}: serialize N-Triples: {e}"),
        })
    })?;

    // Emit EXACTLY the canonical fold (shared prefix authority, no trailing fixup):
    // the file IS `render(graph)`, the same bytes the superset gate reconstructs.
    // `nt` is already bytes from the native serializer, so pass it by reference.
    purrdf::turtle_normalize::canonical_turtle(&nt, &crate::stages::superset::rdf_prefixes())
        .map(String::into_bytes)
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))
}

/// Retag a native literal's `@x-gmeow-*` language tag to its public BCP-47 form.
fn retag_native_literal(lit: &RdfLiteral, tag_map: &BTreeMap<String, String>) -> RdfLiteral {
    if let Some(lang) = &lit.language
        && let Some(ext) = tag_map.get(lang)
    {
        let mut out = lit.clone();
        out.language = Some(ext.clone());
        return out;
    }
    lit.clone()
}

/// Canonicalize a single owned [`RdfTerm`] in place to the W3C XSD canonical mapping
/// (the native twin of the literal value-space the transient oxigraph store used to
/// apply on parse): a typed literal with a recognized XSD datatype is rewritten to its
/// canonical lexical form, a quoted-triple term recurses, and every other term is left
/// VERBATIM. A malformed lexical for a RECOGNIZED XSD datatype HARD-fails. Mirrors the
/// snapshot carrier's `canonicalize_term_xsd` exactly so the two paths cannot drift.
fn canonicalize_term_xsd(term: &mut RdfTerm) -> Result<(), gmeow_errors::Diag> {
    match term {
        RdfTerm::Literal(literal) => {
            if literal.language.is_some() {
                return Ok(());
            }
            if let Some(datatype_iri) = literal.datatype.as_deref() {
                match purrdf::xsd::parse_by_iri(&literal.lexical_form, datatype_iri) {
                    Ok(Some(value)) => literal.lexical_form = value.canonical_lexical(),
                    Ok(None) => {}
                    Err(e) => {
                        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                            message: format!(
                                "malformed typed literal {:?}^^<{datatype_iri}>: {e:?}",
                                literal.lexical_form
                            ),
                        }));
                    }
                }
            }
            Ok(())
        }
        RdfTerm::Triple(triple) => {
            canonicalize_term_xsd(&mut triple.subject)?;
            canonicalize_term_xsd(&mut triple.object)?;
            Ok(())
        }
        RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => Ok(()),
    }
}

// ── render: the committed artifact map ─────────────────────────────────────────

/// Render every committed research-object artifact under `root`, keyed by its
/// logical (repo-relative) path.
pub fn render_research_objects(
    root: &Path,
    dcat_rq: &str,
    scores_ttl: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let store = load_instance_graph(root, scores_ttl)?;
    let ds = dataset_meta(&store)?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let p = |rel: &str| format!("{RESEARCH_OBJECTS_DIR}/{rel}");

    // The shared research-object config + the single caller-vocabulary source A-Box
    // that Croissant, DataCite, and Frictionless all project from.
    let common = research_common_config(&ds.iri)?;
    let source = build_research_source(&common, &store, &ds)?;

    // Croissant (top-level) — purrdf project_croissant.
    let croissant = purrdf::project_croissant(source.as_ref(), &croissant_config(common.clone())?)
        .map_err(|e| ro_err(format!("project_croissant: {e}")))?;
    report_projection_losses("croissant", &croissant.loss_ledger);
    let croissant_bytes = croissant
        .package
        .get(purrdf::CROISSANT_ARTIFACT)
        .ok_or_else(|| ro_err("Croissant package is missing its artifact".into()))?
        .to_vec();
    out.insert(p("lillith.croissant.jsonld"), croissant_bytes.clone());

    // RO-Crate (Attached codec): render the seven payload bodies (six retagged A-Box
    // `.ttl` files + the Croissant copy), declare them as sized File resources in a
    // dedicated source A-Box, and project through `project_ro_crate_with_assets`. The
    // codec emits `ro-crate-metadata.json` + `ro-crate-preview.html` and carries the seven
    // payloads; `validate_attached_assets` cross-checks every File resource against its
    // asset (path == native id, declared byte size == asset length, one-to-one).
    let tag_map = load_tag_map(root)?;
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();
    for (label, name, bytes) in example_inputs(root, scores_ttl)? {
        let payload = render_source_turtle_payload(&bytes, label, &tag_map)?;
        assets.push((name.to_string(), payload));
    }
    assets.push((
        "lillith.croissant.jsonld".to_string(),
        croissant_bytes.clone(),
    ));
    // Deterministic member order: the ProjectionPackage stores paths in lexical order, so
    // sort the caller list to match (and to keep the source resource order stable).
    assets.sort();
    let files: Vec<(String, usize)> = assets
        .iter()
        .map(|(name, body)| (name.clone(), body.len()))
        .collect();
    let ro_source = build_ro_crate_source(&common, &store, &ds, &files)?;
    let ro_assets = purrdf::RoCrateAssets::from_artifacts(
        research_limits()?,
        assets
            .iter()
            .map(|(name, body)| (name.clone(), body.clone())),
    )
    .map_err(|e| ro_err(format!("RoCrateAssets: {e}")))?;
    let ro_crate = purrdf::project_ro_crate_with_assets(
        ro_source.as_ref(),
        &ro_crate_config(common.clone())?,
        &ro_assets,
    )
    .map_err(|e| ro_err(format!("project_ro_crate_with_assets: {e}")))?;
    report_projection_losses("ro-crate", &ro_crate.loss_ledger);
    for (path, body) in ro_crate.package.artifacts() {
        out.insert(p(&format!("ro-crate/{path}")), body.to_vec());
    }

    // DCAT: CONSTRUCT over the whole composed ontology + the worked-example A-Box.
    let dcat = render_dcat(root, dcat_rq, scores_ttl)?;
    out.insert(p("lillith.dcat.ttl"), dcat.into_bytes());

    // DataCite XML — purrdf project_datacite.
    let datacite = purrdf::project_datacite(source.as_ref(), &datacite_config(common.clone())?)
        .map_err(|e| ro_err(format!("project_datacite: {e}")))?;
    report_projection_losses("datacite", &datacite.loss_ledger);
    let datacite_bytes = datacite
        .package
        .get(purrdf::DATACITE_ARTIFACT)
        .ok_or_else(|| ro_err("DataCite package is missing its artifact".into()))?
        .to_vec();
    out.insert(p("lillith.datacite.xml"), datacite_bytes);

    // Frictionless datapackage.json — purrdf project_frictionless.
    let package_name = slug(&ds.iri).to_lowercase().replace('_', "-");
    let frictionless = purrdf::project_frictionless(
        source.as_ref(),
        &frictionless_config(common, &package_name)?,
    )
    .map_err(|e| ro_err(format!("project_frictionless: {e}")))?;
    report_projection_losses("frictionless", &frictionless.loss_ledger);
    let frictionless_bytes = frictionless
        .package
        .get(purrdf::FRICTIONLESS_ARTIFACT)
        .ok_or_else(|| ro_err("Frictionless package is missing its artifact".into()))?
        .to_vec();
    out.insert(p("datapackage.json"), frictionless_bytes);

    Ok(out)
}

/// Build the DCAT store (whole ontology + example A-Box), run `dcat.rq`, serialize.
/// `dcat_rq` is the CONSTRUCT query text, threaded in from the consumed stage-mappings
/// product (`generated/queries/dcat.rq`) — never re-read off disk (the stale-disk-fold class).
fn render_dcat(
    root: &Path,
    dcat_rq: &str,
    scores_ttl: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::new();
    // The whole authored ontology: ontology/gmeow.ttl + every slice module.ttl.
    let onto = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&onto)?;
    parsed.push(parse_into(&bytes, "ontology/gmeow.ttl")?);
    for module in module_files(root)? {
        let bytes = std::fs::read(&module)?;
        parsed.push(parse_into(&bytes, &module.display().to_string())?);
    }
    // The worked-example A-Box (scores.ttl rides in from the consumed evals product).
    for (label, _name, bytes) in example_inputs(root, scores_ttl)? {
        parsed.push(parse_into(&bytes, label)?);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    let dataset = Arc::new(RdfDataset::union(&refs));

    let graph = match native_query::query(&dataset, dcat_rq)? {
        SparqlResult::Graph(graph) => graph,
        _ => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: "dcat.rq did not return a CONSTRUCT graph".into(),
            }));
        }
    };
    // The CONSTRUCT result is a native dataset; canonicalize its typed-literal lexical
    // forms to the W3C XSD mapping (the native codecs preserve raw lexical forms),
    // serialize to N-Triples (NO gts round-trip), then canonicalize to Turtle.
    let mut quads = purrdf::native_quads::flat_rdf_quads_from_dataset(&graph);
    for quad in &mut quads {
        canonicalize_term_xsd(&mut quad.object)?;
    }
    let canon = purrdf::native_quads::flat_dataset_from_quads(&quads).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("dcat.rq re-freeze: {e}"),
        })
    })?;
    let nt = purrdf::serialize_dataset(
        &canon,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("dcat.rq serialize N-Triples: {e}"),
        })
    })?;
    // Emit EXACTLY the canonical fold (shared prefix authority, no banner): the file
    // IS `render(graph)`, the bytes the superset gate reconstructs from the bundle.
    // `nt` is already bytes from the native serializer, so pass it by reference.
    purrdf::turtle_normalize::canonical_turtle(&nt, &crate::stages::superset::rdf_prefixes())
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The committed path of the DCAT CONSTRUCT query — a `stage-mappings` product artifact
/// (a generated projection), consumed from that product, never re-read off disk.
const DCAT_QUERY_PATH: &str = "generated/queries/dcat.rq";

/// The `research-objects` export-leaf stage.
pub struct ResearchObjectsStage {
    consumes: Vec<String>,
}

impl ResearchObjectsStage {
    /// Construct the stage. It consumes:
    ///
    /// * `stage-export-evals` — to obtain `generated/evals/scores.ttl` (a product of the
    ///   SAME run, written to the git-ignored generated tree only by the post-pipeline
    ///   fanout) from that stage's in-memory product, and
    /// * `stage-mappings` — to obtain the generated DCAT CONSTRUCT query
    ///   (`generated/queries/dcat.rq`) from that stage's in-memory product.
    ///
    /// Both are sourced from the consumed product rather than re-reading the stale/absent
    /// committed files off disk (the stale-disk-fold class): a scores or `dcat.rq` edit then
    /// reaches the research objects in a single regenerate, and a cold clone (no materialized
    /// generated tree) still builds.
    /// Kept in sorted order to match the registry `consumes()` and the module.ttl
    /// `dataflowConsumes`.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-export-evals".to_string(),
                "stage-mappings".to_string(),
            ],
        }
    }
}

impl Default for ResearchObjectsStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ResearchObjectsStage {
    fn id(&self) -> &str {
        "stage-export-research-objects"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v4: croissant/datacite/frictionless/ro-crate are projected by the purrdf 0.12.0
        // research-object codecs (RO-Crate is Attached, payloads carried as RoCrateAssets); the
        // rdflib-parity serializers are gone and the goldens are re-blessed. DCAT stays on its
        // whole-ontology `dcat.rq` CONSTRUCT. `scores.ttl`/`dcat.rq` still ride in from the
        // consumed stage-export-evals / stage-mappings products (never a git-ignored disk read).
        "research_objects.v4"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // Pure authored-source reads: the FIVE authored worked-example A-Box inputs and the
        // language-tag map (root ontology + slice modules). NONE are in the composed fold, so
        // declare them so any edit busts the cache. Two inputs are NOT declared here — they are
        // generated projections consumed from upstream products (whose digests cover their
        // edits), never read off disk: `generated/evals/scores.ttl` (stage-export-evals) and
        // `generated/queries/dcat.rq` (stage-mappings). A generated/ path in input_files would
        // itself be the stale-disk-fold class.
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for (rel, _) in AUTHORED_EXAMPLE_INPUTS {
            files.push(root.join(rel));
        }
        files.push(root.join("ontology").join("gmeow.ttl"));
        files.extend(module_files(root)?);
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // The generated DCAT CONSTRUCT query, sourced from THIS run's stage-mappings
        // product (fail-closed: a missing artifact is a hard error, never a disk fallback).
        let dcat_rq = input
            .upstream
            .get("stage-mappings")
            .and_then(|p| p.artifact(DCAT_QUERY_PATH))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!(
                        "missing {DCAT_QUERY_PATH} in the stage-mappings product; refusing to \
                         re-read the stale committed query off disk (fail-closed)"
                    ),
                })
            })?;
        let dcat_rq = std::str::from_utf8(dcat_rq).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("{DCAT_QUERY_PATH} is not utf-8: {e}"),
            })
        })?;
        // `generated/evals/scores.ttl` from THIS run's stage-export-evals product
        // (fail-closed: a missing artifact is a hard error, never a disk fallback of the
        // git-ignored generated tree).
        let scores_ttl = input
            .upstream
            .get("stage-export-evals")
            .and_then(|p| p.artifact(SCORES_INPUT_LABEL))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!(
                        "missing {SCORES_INPUT_LABEL} in the stage-export-evals product; refusing \
                         to re-read the git-ignored generated file off disk (fail-closed)"
                    ),
                })
            })?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_research_objects(input.root, dcat_rq, scores_ttl)?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn scores_ttl_rides_the_evals_product_not_disk() {
        // The scores bytes are threaded from the (consumed) evals product, never read off
        // the git-ignored generated/evals/scores.ttl: a sentinel passed as the scores bytes
        // appears verbatim as the LAST example input, labelled by the producer's SCORES_PATH.
        let root = repo_root();
        let sentinel = b"# sentinel scores\n".as_slice();
        let inputs = example_inputs(&root, sentinel).expect("example inputs");
        assert_eq!(inputs.len(), 6, "five authored inputs + scores.ttl");
        let (label, name, bytes) = inputs.last().expect("scores input present");
        assert_eq!(*label, crate::stages::evals::SCORES_PATH);
        assert_eq!(*label, "generated/evals/scores.ttl");
        assert_eq!(*name, "scores.ttl");
        assert_eq!(bytes.as_slice(), sentinel);
    }

    #[test]
    fn input_files_omit_generated_scores_and_the_dag_edge_binds() {
        let root = repo_root();
        let stage = ResearchObjectsStage::default();
        // The generated evals product no longer rides input_files() (the stale-disk-fold
        // class): its freshness rides the consumed stage-export-evals product digest.
        let files = stage.input_files(&root).expect("input files");
        assert!(
            files
                .iter()
                .all(|f| !f.ends_with("generated/evals/scores.ttl")),
            "generated/evals/scores.ttl must not be an input_files() disk read"
        );
        // The DAG edge binds: the stage consumes both producers, in sorted order.
        assert_eq!(
            stage.consumes(),
            &[
                "stage-export-evals".to_string(),
                "stage-mappings".to_string()
            ]
        );
    }

    #[test]
    fn research_objects_are_byte_identical_to_committed() {
        let root = repo_root();
        // The DCAT query is a stage-mappings product artifact; in production the stage
        // reads it off that product. This byte-parity test drives the pure renderer
        // directly, so it supplies the committed query text (the same bytes the mappings
        // stage would emit) — asserting the rendered bundle is byte-identical to committed.
        let dcat_rq = std::fs::read_to_string(root.join(DCAT_QUERY_PATH))
            .expect("committed generated/queries/dcat.rq");
        // scores.ttl is a stage-export-evals product; produce it FRESH from the evals
        // renderer (the same bytes the evals leaf emits and the fanout writes to disk)
        // rather than reading the git-ignored generated/evals/scores.ttl — the production
        // product-sourcing path, not a stale disk read.
        let evals = crate::stages::evals::render_evals(&root).expect("render evals");
        let scores_ttl = evals
            .get(crate::stages::evals::SCORES_PATH)
            .expect("evals product carries scores.ttl");
        let arts = render_research_objects(&root, &dcat_rq, scores_ttl).expect("render");

        // Pin the family by its member-name SET, not a bare count: a count of 13 cannot catch a
        // silent membership swap (a top-level artifact migrating under `ro-crate/` while a new
        // member appears leaves the count unchanged). The four purrdf codecs + the untouched DCAT
        // CONSTRUCT project exactly these 13 logical paths.
        let base = RESEARCH_OBJECTS_DIR;
        let expected: BTreeSet<String> = [
            "lillith.croissant.jsonld",
            "lillith.datacite.xml",
            "datapackage.json",
            "lillith.dcat.ttl",
            "ro-crate/ro-crate-metadata.json",
            "ro-crate/ro-crate-preview.html",
            "ro-crate/corpus.ttl",
            "ro-crate/grounded-claim.ttl",
            "ro-crate/lillith-dataset.ttl",
            "ro-crate/lillith-pipeline.ttl",
            "ro-crate/rubric.ttl",
            "ro-crate/scores.ttl",
            "ro-crate/lillith.croissant.jsonld",
        ]
        .into_iter()
        .map(|member| format!("{base}/{member}"))
        .collect();
        let actual: BTreeSet<String> = arts.keys().cloned().collect();
        assert_eq!(
            actual, expected,
            "research-objects family membership drifted"
        );

        let mut failures: Vec<String> = Vec::new();
        for (path, bytes) in &arts {
            let committed = std::fs::read(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            if bytes != &committed {
                // First differing line, for fast iteration.
                let got = String::from_utf8_lossy(bytes);
                let want = String::from_utf8_lossy(&committed);
                let mut detail = String::new();
                for (i, (a, b)) in got.lines().zip(want.lines()).enumerate() {
                    if a != b {
                        detail = format!("line {}: got {a:?} want {b:?}", i + 1);
                        break;
                    }
                }
                if detail.is_empty() {
                    detail = format!("len got {} want {}", bytes.len(), committed.len());
                }
                failures.push(format!("{path}: {detail}"));
            }
        }
        assert!(
            failures.is_empty(),
            "research-objects byte-parity drift:\n{}",
            failures.join("\n")
        );
    }

    /// One four-codec projection outcome (DCAT excluded): the four purrdf projections
    /// plus the sorted RO-Crate asset payloads and the shared config, enough to re-drive
    /// the reader (round-trip) and asset-recovery paths.
    struct FourCodecProjection {
        croissant: purrdf::ResearchObjectPackageProjection,
        datacite: purrdf::ResearchObjectPackageProjection,
        frictionless: purrdf::ResearchObjectPackageProjection,
        ro_crate: purrdf::ResearchObjectPackageProjection,
        /// The seven RO-Crate payloads: six retagged A-Box `.ttl` files + the Croissant copy.
        assets: Vec<(String, Vec<u8>)>,
        common: purrdf::ResearchObjectConfig,
        package_name: String,
    }

    /// Reproduce the FOUR-codec portion of [`render_research_objects`] directly (DCAT is
    /// deliberately excluded — it needs the generated `dcat.rq` product). This is the exact
    /// codec-invocation sequence `render_research_objects` uses, minus the logical-path
    /// bookkeeping, so it exercises the codecs with no materialized `generated/` disk read.
    fn project_four_codecs(root: &Path, scores_ttl: &[u8]) -> FourCodecProjection {
        let store = load_instance_graph(root, scores_ttl).expect("instance graph");
        let ds = dataset_meta(&store).expect("dataset meta");
        let common = research_common_config(&ds.iri).expect("common config");
        let source = build_research_source(&common, &store, &ds).expect("research source");

        // Assertion 1: each project_* returns Ok (its internal `ensure_sound` passed).
        let croissant_cfg = croissant_config(common.clone()).expect("croissant config");
        let croissant = purrdf::project_croissant(source.as_ref(), &croissant_cfg);
        assert!(
            croissant.is_ok(),
            "project_croissant: {:?}",
            croissant.err()
        );
        let croissant = croissant.unwrap();

        let datacite_cfg = datacite_config(common.clone()).expect("datacite config");
        let datacite = purrdf::project_datacite(source.as_ref(), &datacite_cfg);
        assert!(datacite.is_ok(), "project_datacite: {:?}", datacite.err());
        let datacite = datacite.unwrap();

        let package_name = slug(&ds.iri).to_lowercase().replace('_', "-");
        let frictionless_cfg =
            frictionless_config(common.clone(), &package_name).expect("frictionless config");
        let frictionless = purrdf::project_frictionless(source.as_ref(), &frictionless_cfg);
        assert!(
            frictionless.is_ok(),
            "project_frictionless: {:?}",
            frictionless.err()
        );
        let frictionless = frictionless.unwrap();

        // RO-Crate assets: six retagged A-Box `.ttl` payloads + the Croissant copy, sorted to
        // match the ProjectionPackage's lexical member order (same as render).
        let tag_map = load_tag_map(root).expect("tag map");
        let mut assets: Vec<(String, Vec<u8>)> = Vec::new();
        for (label, name, bytes) in example_inputs(root, scores_ttl).expect("example inputs") {
            let payload = render_source_turtle_payload(&bytes, label, &tag_map).expect("payload");
            assets.push((name.to_string(), payload));
        }
        let croissant_bytes = croissant
            .package
            .get(purrdf::CROISSANT_ARTIFACT)
            .expect("croissant artifact")
            .to_vec();
        assets.push(("lillith.croissant.jsonld".to_string(), croissant_bytes));
        assets.sort();
        let files: Vec<(String, usize)> = assets
            .iter()
            .map(|(name, body)| (name.clone(), body.len()))
            .collect();
        let ro_source =
            build_ro_crate_source(&common, &store, &ds, &files).expect("ro-crate source");
        let ro_assets = purrdf::RoCrateAssets::from_artifacts(
            research_limits().expect("limits"),
            assets
                .iter()
                .map(|(name, body)| (name.clone(), body.clone())),
        )
        .expect("ro-crate assets");
        let ro_crate_cfg = ro_crate_config(common.clone()).expect("ro-crate config");
        let ro_crate =
            purrdf::project_ro_crate_with_assets(ro_source.as_ref(), &ro_crate_cfg, &ro_assets);
        assert!(
            ro_crate.is_ok(),
            "project_ro_crate_with_assets: {:?}",
            ro_crate.err()
        );
        let ro_crate = ro_crate.unwrap();

        FourCodecProjection {
            croissant,
            datacite,
            frictionless,
            ro_crate,
            assets,
            common,
            package_name,
        }
    }

    /// Stage-1-runnable proof that the FOUR purrdf research-object codecs (Croissant,
    /// DataCite, Frictionless, RO-Crate) are correct WITHOUT a materialized `generated/`
    /// tree — it drives them directly, sourcing `scores.ttl` from the evals product rather
    /// than a disk read. DCAT is excluded (it needs the generated `dcat.rq` product; the
    /// byte-parity gate above covers it once Stage 2/3 has materialized the tree).
    #[test]
    fn research_object_codecs_project_soundly_stage_one() {
        let root = repo_root();
        // scores.ttl rides the (consumed) evals product, not the git-ignored generated file.
        let evals = crate::stages::evals::render_evals(&root).expect("render evals");
        let scores_ttl = evals
            .get(crate::stages::evals::SCORES_PATH)
            .expect("evals product carries scores.ttl")
            .clone();

        let out = project_four_codecs(&root, &scores_ttl);

        let croissant_bytes = out
            .croissant
            .package
            .get(purrdf::CROISSANT_ARTIFACT)
            .expect("croissant.json present")
            .to_vec();
        let datacite_bytes = out
            .datacite
            .package
            .get(purrdf::DATACITE_ARTIFACT)
            .expect("datacite.xml present")
            .to_vec();
        let frictionless_bytes = out
            .frictionless
            .package
            .get(purrdf::FRICTIONLESS_ARTIFACT)
            .expect("datapackage.json present")
            .to_vec();
        let ro_meta_bytes = out
            .ro_crate
            .package
            .get(purrdf::RO_CRATE_ARTIFACT)
            .expect("ro-crate-metadata.json present")
            .to_vec();

        // ── Assertion 2: package member sets ────────────────────────────────────────
        assert_eq!(purrdf::CROISSANT_ARTIFACT, "croissant.json");
        assert_eq!(purrdf::FRICTIONLESS_ARTIFACT, "datapackage.json");
        assert!(out.frictionless.package.get("datapackage.json").is_some());
        // datacite exposes purrdf::DATACITE_ARTIFACT (asserted present above).
        let ro_members: BTreeSet<String> = out
            .ro_crate
            .package
            .artifacts()
            .map(|(path, _)| path.to_string())
            .collect();
        let expected_ro: BTreeSet<String> = [
            "ro-crate-metadata.json",
            "ro-crate-preview.html",
            "corpus.ttl",
            "grounded-claim.ttl",
            "lillith-dataset.ttl",
            "lillith-pipeline.ttl",
            "rubric.ttl",
            "scores.ttl",
            "lillith.croissant.jsonld",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(ro_members, expected_ro, "ro-crate package membership");
        assert_eq!(ro_members.len(), 9, "ro-crate has 9 members");

        // ── Assertion 3: R2 identity presence in emitted bytes ──────────────────────
        let croissant_text = String::from_utf8(croissant_bytes.clone()).expect("croissant utf8");
        assert!(
            croissant_text.contains(CROISSANT_CONFORMS_TO),
            "croissant conformsTo identity"
        );
        let datacite_text = String::from_utf8(datacite_bytes.clone()).expect("datacite utf8");
        assert!(
            datacite_text.contains(DATACITE_NS),
            "datacite kernel-4 namespace"
        );
        let frictionless_text =
            String::from_utf8(frictionless_bytes.clone()).expect("frictionless utf8");
        assert!(
            frictionless_text.contains(purrdf::FRICTIONLESS_PROFILE),
            "frictionless profile identity"
        );
        assert_eq!(purrdf::FRICTIONLESS_PROFILE, "frictionless-data-package-1");
        let ro_meta_text = String::from_utf8(ro_meta_bytes.clone()).expect("ro-crate utf8");
        assert!(
            ro_meta_text.contains("w3id.org/ro/crate/1.3"),
            "ro-crate 1.3 profile identity"
        );

        // ── Assertion 4: P5 native-slot declared-drop note ──────────────────────────
        // The dataset description carries the caller-authored P5 note ("Declared drops
        // (P5): …") into each codec's native description slot (Croissant/Frictionless/
        // RO-Crate `description`, DataCite abstract). "Declared drops" is the literal that
        // actually appears verbatim in all four (ASCII, unescaped by JSON/XML canonicalization).
        const DROP_MARKER: &str = "Declared drops";
        assert!(croissant_text.contains(DROP_MARKER), "croissant P5 note");
        assert!(datacite_text.contains(DROP_MARKER), "datacite P5 note");
        assert!(
            frictionless_text.contains(DROP_MARKER),
            "frictionless P5 note"
        );
        assert!(ro_meta_text.contains(DROP_MARKER), "ro-crate P5 note");

        // ── Assertion 5: R2 reserved-prefix rule ────────────────────────────────────
        assert!(
            ro_members
                .iter()
                .all(|path| !path.starts_with("ro-crate-preview_files/")),
            "no ro-crate member may claim the reserved preview-files prefix"
        );

        // ── Assertion 7: impl_version ───────────────────────────────────────────────
        assert_eq!(
            ResearchObjectsStage::new().impl_version(),
            "research_objects.v4"
        );

        // ── Assertion 6: determinism — a second full run is byte-identical ───────────
        let again = project_four_codecs(&root, &scores_ttl);
        let members = |package: &purrdf::ProjectionPackage| -> BTreeMap<String, Vec<u8>> {
            package
                .artifacts()
                .map(|(path, body)| (path.to_string(), body.to_vec()))
                .collect()
        };
        assert_eq!(
            members(&out.croissant.package),
            members(&again.croissant.package),
            "croissant determinism"
        );
        assert_eq!(
            members(&out.datacite.package),
            members(&again.datacite.package),
            "datacite determinism"
        );
        assert_eq!(
            members(&out.frictionless.package),
            members(&again.frictionless.package),
            "frictionless determinism"
        );
        assert_eq!(
            members(&out.ro_crate.package),
            members(&again.ro_crate.package),
            "ro-crate determinism"
        );
        assert_eq!(out.assets, again.assets, "ro-crate asset determinism");

        // ── Assertion 8: round-trip lift-invariance (T1) ────────────────────────────
        // Every codec has an inverse reader; each `read_*` succeeds on the codec's own
        // canonical bytes (its full re-parse + `ensure_sound` strictness passes), lifting
        // caller-vocabulary RDF 1.2 back out. The invariant asserted is byte-level canonical
        // idempotence: re-projecting the reader's lifted dataset reproduces the identical
        // canonical bytes — the codec's output is a fixed point of `project ∘ lift ∘ read`.
        //
        // NOTE — deviation from a strict `projected.model == reread.model` equality: that
        // does NOT hold for the real gmeow worked-example data because these projections are
        // genuinely lossy over it. Each resource carries three `gmeow:contentDigest`s
        // (blake3, md5, sha256); Croissant/RO-Crate keep only the format's single `sha256`
        // slot, so blake3+md5 are a declared P5 drop and the reread model has fewer checksums
        // than the projected model. (The purrdf unit tests assert model equality only against
        // single-checksum fixtures.) Byte-level idempotence is the honest, stronger T1
        // statement for canonical output and is what is asserted here. See the task report.
        let croissant_cfg = croissant_config(out.common.clone()).expect("croissant read config");
        let croissant_read =
            purrdf::read_croissant(&out.croissant.package, &croissant_cfg).expect("read_croissant");
        let croissant_reproj =
            purrdf::project_croissant(croissant_read.dataset.as_ref(), &croissant_cfg)
                .expect("re-project croissant from lifted dataset");
        assert_eq!(
            croissant_reproj.package.get(purrdf::CROISSANT_ARTIFACT),
            out.croissant.package.get(purrdf::CROISSANT_ARTIFACT),
            "croissant canonical idempotence (project ∘ lift ∘ read)"
        );

        let datacite_cfg = datacite_config(out.common.clone()).expect("datacite read config");
        let datacite_read =
            purrdf::read_datacite(&out.datacite.package, &datacite_cfg).expect("read_datacite");
        let datacite_reproj =
            purrdf::project_datacite(datacite_read.dataset.as_ref(), &datacite_cfg)
                .expect("re-project datacite from lifted dataset");
        assert_eq!(
            datacite_reproj.package.get(purrdf::DATACITE_ARTIFACT),
            out.datacite.package.get(purrdf::DATACITE_ARTIFACT),
            "datacite canonical idempotence (project ∘ lift ∘ read)"
        );

        let frictionless_cfg = frictionless_config(out.common.clone(), &out.package_name)
            .expect("frictionless read config");
        let frictionless_read =
            purrdf::read_frictionless(&out.frictionless.package, &frictionless_cfg)
                .expect("read_frictionless");
        let frictionless_reproj =
            purrdf::project_frictionless(frictionless_read.dataset.as_ref(), &frictionless_cfg)
                .expect("re-project frictionless from lifted dataset");
        assert_eq!(
            frictionless_reproj
                .package
                .get(purrdf::FRICTIONLESS_ARTIFACT),
            out.frictionless.package.get(purrdf::FRICTIONLESS_ARTIFACT),
            "frictionless canonical idempotence (project ∘ lift ∘ read)"
        );

        let ro_crate_cfg = ro_crate_config(out.common.clone()).expect("ro-crate read config");
        let ro_crate_read =
            purrdf::read_ro_crate(&out.ro_crate.package, &ro_crate_cfg).expect("read_ro_crate");
        let ro_assets_again = purrdf::RoCrateAssets::from_artifacts(
            research_limits().expect("limits"),
            out.assets
                .iter()
                .map(|(name, body)| (name.clone(), body.clone())),
        )
        .expect("ro-crate assets");
        let ro_crate_reproj = purrdf::project_ro_crate_with_assets(
            ro_crate_read.dataset.as_ref(),
            &ro_crate_cfg,
            &ro_assets_again,
        )
        .expect("re-project ro-crate from lifted dataset");
        assert_eq!(
            ro_crate_reproj.package.get(purrdf::RO_CRATE_ARTIFACT),
            out.ro_crate.package.get(purrdf::RO_CRATE_ARTIFACT),
            "ro-crate canonical idempotence (project ∘ lift ∘ read)"
        );

        // ── Assertion 9: RoCrateAssets payload round-trip (U2) ───────────────────────
        // The attached RO-Crate package's seven payloads recover byte-for-byte.
        let recovered = purrdf::RoCrateAssets::from_attached_package(&out.ro_crate.package)
            .expect("from_attached_package");
        let recovered_map: BTreeMap<String, Vec<u8>> = recovered
            .artifacts()
            .map(|(path, body)| (path.to_string(), body.to_vec()))
            .collect();
        let expected_map: BTreeMap<String, Vec<u8>> = out.assets.iter().cloned().collect();
        assert_eq!(recovered.len(), 7, "ro-crate carries seven payloads");
        assert_eq!(
            recovered_map, expected_map,
            "ro-crate asset payloads round-trip byte-for-byte"
        );
    }
}

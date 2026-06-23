// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native SSSOM emission — the whole of GMEOW's `*.sssom.tsv` emitter, sourced
//! entirely from Rust (#848).
//!
//! This is the SUBSUME/ENHANCE move that pulls the SSSOM *emission* orchestrator
//! out of Python (`gmeow_tools.mapping_compile.emit_sssom`) and into the slice
//! framework. The Python side now passes nothing but the repo-root path; every
//! input is discovered natively here:
//!
//! * **Slice equivalence cells** ← [`SliceCatalog::discover`] →
//!   [`ArtifactRole::Mapping`] artifacts (their Turtle `content` bytes).
//! * **Shared DSL equivalence cells + mapping-set metadata** ← the
//!   `dsl/mappings/` tree (globbed `*.ttl` recursively). That tree is not a slice
//!   yet, so reading it directly in Rust is correct for now.
//! * **The prefix / CURIE map** ← the curated [`PREFIX_REGISTRY`] (a static
//!   mirror of `config.PREFIXES`). The per-file `@prefix` declarations are
//!   deliberately NOT used: they name the same namespace differently
//!   (e.g. `@prefix bf:` where the registry says `bibframe`), so byte-parity with
//!   the Python `curie()` shortener — which always resolves through
//!   `config.PREFIXES` — demands the curated registry. See [`PREFIX_REGISTRY`].
//! * **Self-metadata (version + release date)** ← `metadata/gmeow-self.ttl`
//!   (the Manifestation node's `gmeow:versionFingerprint` and
//!   `gmeow:datePublished`).
//!
//! The emitted TSV is **byte-identical** to the historical Python emitter — that
//! is the gate this module is held to (`gmeow-dev regenerate mappings` must show
//! zero drift on the 66 committed files). The emission rules (column order,
//! CURIE-shortening, confidence formatting, the YAML-ish `#` header, the JSON
//! `ensure_ascii` comment quoting, the `# #` trailer) are reproduced exactly; see
//! the per-function docs for the Python counterpart each one mirrors.
//!
//! ## Why this lives in `gmeow-slice`
//!
//! `gmeow-slice` is the one crate that depends on `gmeow-rdf` (the SSSOM IR +
//! codec) *and* owns [`SliceCatalog`], so it is the only place that can both
//! discover the slice mapping sources and reuse the codec. The TSV text is
//! written directly here (rather than through [`gmeow_rdf::sssom::serialize_tsv`])
//! because GMEOW's curated header — provenance scalars, the *used*-prefix
//! curie_map, the trailer block — is richer than the round-trip serializer's, and
//! byte-parity is the contract.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, NamedNode, Term};
use oxigraph::store::Store;

use crate::artifact::ArtifactRole;
use crate::catalog::SliceCatalog;
use crate::error::SliceError;

// ── Namespace constants ───────────────────────────────────────────────────────

/// The GMEOW namespace base, used by the inline tests to build synthetic IRIs.
#[cfg(test)]
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const GM_TERM_EQUIVALENCE: &str = "https://blackcatinformatics.ca/gmeow/TermEquivalence";
const GM_ALIGN_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignSubject";
const GM_ALIGN_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/alignPredicate";
const GM_ALIGN_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignObject";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const GM_JUSTIFICATION: &str = "https://blackcatinformatics.ca/gmeow/justification";
const GM_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/comment";
const GM_SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";
const GM_SUBJECT_LABEL: &str = "https://blackcatinformatics.ca/gmeow/subjectLabel";
const GM_OBJECT_LABEL: &str = "https://blackcatinformatics.ca/gmeow/objectLabel";

const GM_MAPPING_SET: &str = "https://blackcatinformatics.ca/gmeow/MappingSet";
const GM_SET_ID: &str = "https://blackcatinformatics.ca/gmeow/setId";
const GM_LICENSE: &str = "https://blackcatinformatics.ca/gmeow/license";
const GM_SET_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/setComment";
const GM_SET_TRAILER: &str = "https://blackcatinformatics.ca/gmeow/setTrailer";

const GM_VERSION_FINGERPRINT: &str = "https://blackcatinformatics.ca/gmeow/versionFingerprint";
const GM_DATE_PUBLISHED: &str = "https://blackcatinformatics.ca/gmeow/datePublished";

/// The default `mapping_justification` IRI when a cell carries none
/// (`semapv:ManualMappingCuration`). Mirrors `_DEFAULT_JUSTIFICATION`.
const DEFAULT_JUSTIFICATION: &str = "https://w3id.org/semapv/vocab/ManualMappingCuration";

/// The canonical SSSOM column order GMEOW writes (`_SSSOM_ORDER`). A label column
/// only appears when at least one row populates it; the [`SSSOM_ALWAYS`] columns
/// are always present.
const SSSOM_ORDER: &[&str] = &[
    "subject_id",
    "subject_label",
    "predicate_id",
    "object_id",
    "object_label",
    "mapping_justification",
    "confidence",
    "comment",
];

/// The columns GMEOW always emits, even when blank for every row
/// (`_SSSOM_ALWAYS`).
const SSSOM_ALWAYS: &[&str] = &[
    "subject_id",
    "predicate_id",
    "object_id",
    "mapping_justification",
    "confidence",
    "comment",
];

/// The canonical GMEOW prefix registry, in `config.PREFIXES` **insertion order**.
///
/// This is the single authority for both CURIE-shortening (`_sssom_id` ∘ `curie`)
/// and the emitted `# curie_map:` block. It must mirror
/// `src/gmeow_tools/config.py::PREFIXES`, NOT the per-file `@prefix`
/// declarations — those use different prefix *names* for the same namespace
/// (e.g. a source declares `@prefix bf:` where the registry names it `bibframe`),
/// some registry prefixes are never declared in any source they shorten
/// (e.g. `mf` in the places set), and some source prefixes are absent from the
/// registry (e.g. `earl`, which is therefore left as a bare absolute URI). Using
/// `@prefix` declarations instead of this registry produces drift on the
/// committed corpus, so byte-parity demands the curated registry.
///
/// Insertion order is load-bearing: CURIE-shortening sorts by descending
/// namespace length with the *registry order* as the tie-break (mirroring
/// `mapping_dsl._NS_TO_PREFIX`, a Python stable sort keyed on `-len(ns)` over the
/// dict's insertion order). The block-comment groupings below follow `config.py`.
pub(crate) const PREFIX_REGISTRY: &[(&str, &str)] = &[
    ("gmeow", "https://blackcatinformatics.ca/gmeow/"),
    ("logic", "https://blackcatinformatics.ca/logic/"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcmitype", "http://purl.org/dc/dcmitype/"),
    ("vann", "http://purl.org/vocab/vann/"),
    ("void", "http://rdfs.org/ns/void#"),
    ("dcat", "http://www.w3.org/ns/dcat#"),
    ("dqv", "http://www.w3.org/ns/dqv#"),
    ("sssom", "https://w3id.org/sssom/"),
    ("semapv", "https://w3id.org/semapv/vocab/"),
    ("fno", "https://w3id.org/function/ontology#"),
    ("fnom", "https://w3id.org/function/vocabulary/mapping#"),
    ("edoal", "http://ns.inria.org/edoal/1.0/#"),
    ("align", "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#"),
    ("gufo", "http://purl.org/nemo/gufo#"),
    ("umbel", "http://umbel.org/umbel#"),
    ("umbelrc", "http://umbel.org/umbel/rc/"),
    ("dul", "http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#"),
    ("bfo", "http://purl.obolibrary.org/obo/"),
    ("sumo", "https://www.ontologyportal.org/SUMO.owl#"),
    ("cyc", "http://sw.opencyc.org/2012/05/10/concept/en/"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("rel", "http://purl.org/vocab/relationship/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("prof", "http://www.w3.org/ns/dx/prof/"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ssn", "http://www.w3.org/ns/ssn/"),
    ("sweet", "http://sweetontology.net/"),
    ("om", "http://www.wurvoc.org/vocabularies/om-1.8/"),
    ("qb", "http://purl.org/linked-data/cube#"),
    ("mf", "http://www.opengis.net/ont/movingfeatures#"),
    ("sta", "http://www.opengis.net/def/ont/sensorthings/1.1/"),
    ("iso19156", "http://www.isotc211.org/iso19156/"),
    ("oboe", "http://ecoinformatics.org/oboe/oboe.1.2/oboe-core.owl#"),
    ("obi", "http://purl.obolibrary.org/obo/OBI_"),
    ("iao", "http://purl.obolibrary.org/obo/IAO_"),
    ("pato", "http://purl.obolibrary.org/obo/PATO_"),
    ("crmarc", "http://www.cidoc-crm.org/crmarchaeo/"),
    ("iptc", "http://iptc.org/std/NewsML-G2/"),
    ("bbc", "http://www.bbc.co.uk/ontologies/news/"),
    ("obscore", "http://www.ivoa.net/rdf/ObsCore#"),
    ("ppsr", "https://purl.org/ppsr/core#"),
    ("loinc", "http://loinc.org/rdf/"),
    ("snomed", "http://snomed.info/id/"),
    ("np", "http://www.nanopub.org/nschema#"),
    ("crm", "http://www.cidoc-crm.org/cidoc-crm/"),
    ("crmsci", "http://www.cidoc-crm.org/extensions/crmsci/"),
    ("crminf", "http://www.ics.forth.gr/isl/CRMinf/"),
    ("crmdig", "http://www.ics.forth.gr/isl/CRMdig/"),
    ("oa", "http://www.w3.org/ns/oa#"),
    ("exif", "http://www.w3.org/2003/12/exif/ns#"),
    ("iiif", "http://iiif.io/api/presentation/3#"),
    ("cito", "http://purl.org/spar/cito/"),
    ("credit", "https://credit.niso.org/contributor-roles/"),
    ("pav", "http://purl.org/pav/"),
    ("org", "http://www.w3.org/ns/org#"),
    ("moat", "http://moat-project.org/ns#"),
    ("tags", "http://www.holygoat.co.uk/owl/redwood/0.1/tags/"),
    ("time", "http://www.w3.org/2006/time#"),
    ("teo", "https://sbmi.uth.edu/bsdi/TEO_1.0.0.owl#"),
    ("pos", "http://purl.org/ieee1872-owl/pos#"),
    ("cora", "http://purl.org/ieee1872-owl/cora#"),
    ("knowrob", "http://knowrob.org/kb/knowrob.owl#"),
    ("soma", "http://www.ease-crc.org/ont/SOMA.owl#"),
    ("qudt", "http://qudt.org/schema/qudt/"),
    ("unit", "http://qudt.org/vocab/unit/"),
    ("edtf", "http://id.loc.gov/datatypes/edtf/"),
    ("periodo", "http://n2t.net/ark:/99152/"),
    ("gts", "http://resource.geosciml.org/ontology/timescale/gts#"),
    ("ivoa", "http://www.ivoa.net/rdf/"),
    ("crmgeo", "http://www.ics.forth.gr/isl/CRMgeo/"),
    ("lode", "http://linkedevents.org/ontology/"),
    ("sem", "http://semanticweb.cs.vu.nl/2009/11/sem/"),
    ("ical", "http://www.w3.org/2002/12/cal/icaltzd#"),
    ("schema", "https://schema.org/"),
    ("gedcom", "http://www.w3.org/2000/10/swap/pim/gedcom#"),
    ("vcard", "http://www.w3.org/2006/vcard/ns#"),
    ("mo", "http://purl.org/ontology/mo/"),
    ("mbz", "https://musicbrainz.org/"),
    ("discogs", "https://www.discogs.com/"),
    ("afo", "https://w3id.org/afo/onto/1.1#"),
    ("afv", "https://w3id.org/afo/vocab/1.1#"),
    ("jams", "http://w3id.org/polifonia/ontology/jams/"),
    ("pon", "https://w3id.org/polifonia/ontology/"),
    ("chord", "http://purl.org/ontology/chord/"),
    ("mimo", "http://www.mimo-db.eu/InstrumentsKeywords/"),
    ("pplan", "http://purl.org/net/p-plan#"),
    ("opmw", "https://www.opmw.org/ontology/"),
    ("bpmn", "http://www.omg.org/spec/BPMN/20100524/MODEL#"),
    ("ro_crate", "https://w3id.org/ro/crate/#"),
    ("brick", "https://brickschema.org/schema/Brick#"),
    ("bot", "https://w3id.org/bot#"),
    ("ifc", "http://www.buildingsmart-tech.org/ifcOWL/IFC4#"),
    ("vcardx", "https://blackcatinformatics.ca/vcard-ext/"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("sf", "http://www.opengis.net/ont/sf#"),
    ("wgs84", "http://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("gtfs", "http://vocab.gtfs.org/terms#"),
    ("tgn", "http://vocab.getty.edu/tgn/"),
    ("lgdo", "http://linkedgeodata.org/ontology/"),
    ("pleiades", "http://pleiades.stoa.org/places/vocab#"),
    ("whg", "https://whgazetteer.org/"),
    ("gvp", "http://vocab.getty.edu/ontology#"),
    ("mrg", "http://marineregions.org/ns/ontology#"),
    ("bibo", "http://purl.org/ontology/bibo/"),
    ("bibframe", "http://id.loc.gov/ontologies/bibframe/"),
    ("dpv", "https://w3id.org/dpv#"),
    ("frbr", "http://purl.org/vocab/frbr/core#"),
    ("fabio", "http://purl.org/spar/fabio/"),
    ("lrmoo", "http://iflastandards.info/ns/lrm/lrmoo/"),
    ("sioc", "http://rdfs.org/sioc/ns#"),
    ("as", "https://www.w3.org/ns/activitystreams#"),
    ("mads", "http://www.loc.gov/mads/rdf/v1#"),
    ("esco", "http://data.europa.eu/esco/model#"),
    ("esco-base", "http://data.europa.eu/esco/"),
    ("ceterms", "https://purl.org/ctdl/terms/"),
    ("ctdlasn", "https://credreg.net/ctdlasn/terms/"),
    ("onet", "https://www.onetcenter.org/"),
    ("nmo", "http://www.semanticdesktop.org/ontologies/2007/03/22/nmo#"),
    ("wot", "http://xmlns.com/wot/0.1/"),
    ("vc", "https://www.w3.org/2018/credentials#"),
    ("did", "https://www.w3.org/ns/did#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("cc", "http://creativecommons.org/ns#"),
    ("premis", "http://www.loc.gov/premis/rdf/v3/"),
    ("rstmt", "https://rightsstatements.org/vocab/"),
    ("ccpd", "https://creativecommons.org/publicdomain/"),
    ("spdx", "http://spdx.org/rdf/terms#"),
    ("spdxlic", "http://spdx.org/licenses/"),
    ("codemeta", "https://codemeta.github.io/terms/#"),
    ("forgefed", "https://forgefed.org/ns#"),
    ("swh", "https://www.softwareheritage.org/data-model/"),
    ("ma", "http://www.w3.org/ns/ma-ont#"),
    ("gsso", "http://purl.obolibrary.org/obo/GSSO_"),
    ("homosaurus", "https://homosaurus.org/v4/"),
    ("fhir", "http://hl7.org/fhir/"),
    ("bio", "http://purl.org/vocab/bio/0.1/"),
    ("gx", "http://gedcomx.org/"),
    ("gxv", "http://gedcomx.org/v1/"),
    ("gn", "http://www.geonames.org/ontology#"),
    ("wd", "http://www.wikidata.org/entity/"),
    ("wdt", "http://www.wikidata.org/prop/direct/"),
    ("wikibase", "http://wikiba.se/ontology#"),
    ("p", "http://www.wikidata.org/prop/"),
    ("ps", "http://www.wikidata.org/prop/statement/"),
    ("wds", "http://www.wikidata.org/entity/statement/"),
    ("lexvo", "http://lexvo.org/id/"),
    ("lvont", "http://lexvo.org/ontology#"),
    ("glottolog", "https://glottolog.org/resource/languoid/id/"),
    ("ontolex", "http://www.w3.org/ns/lemon/ontolex#"),
    ("lime", "http://www.w3.org/ns/lemon/lime#"),
    ("fibo-fnd-acc-cur", "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/CurrencyAmount/"),
    ("fibo-iso4217", "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/ISO4217-CurrencyCodes/"),
    ("fibo-fnd-acc-ae", "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/AccountingEquity/"),
    ("fibo-fnd-pas-ps", "https://spec.edmcouncil.org/fibo/ontology/FND/ProductsAndServices/ProductsAndServices/"),
    ("fibo-fbc-fi-fi", "https://spec.edmcouncil.org/fibo/ontology/FBC/FinancialInstruments/FinancialInstruments/"),
    ("fibo-fbc-pas-fpas", "https://spec.edmcouncil.org/fibo/ontology/FBC/ProductsAndServices/FinancialProductsAndServices/"),
    ("mls", "http://www.w3.org/ns/mls#"),
    ("faldo", "http://biohackathon.org/resource/faldo#"),
    ("so", "http://purl.obolibrary.org/obo/SO_"),
    ("ladm", "http://www.opengis.net/ont/ladm#"),
    ("cp", "http://inspire.ec.europa.eu/ont/cp#"),
];

// ── Native DSL model ───────────────────────────────────────────────────────────

/// One `gmeow:TermEquivalence` cell — compiles to exactly one SSSOM row.
///
/// Mirrors the Python `EquivalenceCell` dataclass (`mapping_dsl.EquivalenceCell`).
/// IRIs are kept as full absolute strings; CURIE-shortening happens at emit time.
#[derive(Debug, Clone)]
struct EquivalenceCell {
    subject: String,
    predicate: String,
    obj: String,
    /// `gm:confidence` (a float), or `None` when the cell omits it.
    confidence: Option<f64>,
    /// `gm:justification` IRI, or `None` (defaults to `semapv:ManualMappingCuration`).
    justification: Option<String>,
    comment: String,
    sssom_file: String,
    subject_label: String,
    object_label: String,
}

/// Per-file SSSOM header metadata (`gmeow:MappingSet`). Mirrors the Python
/// `MappingSet` dataclass.
#[derive(Debug, Clone, Default)]
struct MappingSet {
    set_id: String,
    license: String,
    comment: String,
    trailer: String,
}

/// The fully-discovered SSSOM source model: every equivalence cell and the
/// per-file mapping-set metadata. The prefix map is the static [`PREFIX_REGISTRY`]
/// (the curated `config.PREFIXES` authority), not derived from the sources.
struct SssomSources {
    equivalences: Vec<EquivalenceCell>,
    mapping_sets: BTreeMap<String, MappingSet>,
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Emit every SSSOM TSV from the repo at `root`, returning `{sssom_file → tsv}`.
///
/// `sssom_file` is the bare file name (e.g. `gmeow-accessibility.sssom.tsv`), the
/// same key the Python `emit_sssom` returned. The text is byte-identical to the
/// historical Python emitter (the parity gate).
///
/// All inputs are sourced natively from `root`:
///
/// * slice `mappings/*.ttl` artifacts via [`SliceCatalog::discover`];
/// * the shared `dsl/mappings/**/*.ttl` tree;
/// * the prefix map from the parsed Turtle `@prefix` declarations;
/// * `metadata/gmeow-self.ttl` for the version + release date.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source — no
/// degraded fallback (CONSTITUTION / no-compromises): a malformed Turtle source,
/// a self-description without the Manifestation version/date, or an I/O failure
/// is a hard error.
pub fn emit_sssom_sets(root: &Path) -> Result<BTreeMap<String, String>, SliceError> {
    let (version, release_date) = read_self_metadata(root)?;
    let sources = collect_sources(root)?;
    render_sets(&sources, &version, &release_date)
}

// ── Source collection ──────────────────────────────────────────────────────────

/// Discover and parse every SSSOM source (the shared DSL tree + slice mapping
/// artifacts) into ONE merged oxigraph store, then extract the equivalence cells
/// and mapping-set metadata from it.
///
/// The single-store merge is load-bearing for parity: when two `gmeow:MappingSet`
/// nodes target the same file (e.g. `gmeow-music.sssom.tsv`), the Python compiler
/// resolves the collision by last-write-wins over its merged-graph iteration —
/// which is a stable function of insertion order in the shared oxigraph backend.
/// Inserting the sources here in the SAME order Python uses (the sorted
/// `dsl/mappings/**/*.ttl` tree first, then the sorted slice
/// `*/*/mappings/*.ttl` artifacts) and iterating the one store reproduces that
/// resolution exactly.
fn collect_sources(root: &Path) -> Result<SssomSources, SliceError> {
    let store =
        Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))?;

    // 1. The shared DSL tree (dsl/mappings/**/*.ttl), sorted — Python parses these
    //    first (`sorted(MAPPING_DSL_DIR.rglob("*.ttl"))`).
    let dsl_dir = root.join("dsl").join("mappings");
    let mut dsl_files: Vec<std::path::PathBuf> = Vec::new();
    collect_ttl_files(&dsl_dir, &mut dsl_files)?;
    dsl_files.sort();
    for path in &dsl_files {
        let bytes = std::fs::read(path).map_err(SliceError::Io)?;
        load_into_store(&store, &bytes, path)?;
    }

    // 2. Slice mapping artifacts (slices/*/*/mappings/*.ttl), sorted by their
    //    on-disk path — Python appends these via `iter_slice_mapping_files()` =
    //    `sorted(SLICES_DIR.glob("*/*/mappings/*.ttl"))`. The catalog discovers
    //    the same Mapping-role artifacts; sorting their full paths matches.
    let slices_dir = root.join("slices");
    if slices_dir.is_dir() {
        let catalog = SliceCatalog::discover(&slices_dir)?;
        let mut slice_mappings: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role == ArtifactRole::Mapping {
                    let path = record.slice_dir.join(&artifact.logical_path);
                    slice_mappings.push((path, artifact.content.clone()));
                }
            }
        }
        slice_mappings.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, bytes) in &slice_mappings {
            load_into_store(&store, bytes, path)?;
        }
    }

    let mut equivalences: Vec<EquivalenceCell> = Vec::new();
    let mut mapping_sets: BTreeMap<String, MappingSet> = BTreeMap::new();
    extract_equivalences(&store, &mut equivalences)?;
    extract_mapping_sets(&store, &mut mapping_sets)?;

    Ok(SssomSources {
        equivalences,
        mapping_sets,
    })
}

/// Recursively collect every `*.ttl` file under `dir` (no-op if `dir` is absent).
fn collect_ttl_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), SliceError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(SliceError::Io)? {
        let entry = entry.map_err(SliceError::Io)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(SliceError::Io)?;
        if file_type.is_dir() {
            collect_ttl_files(&path, out)?;
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
    Ok(())
}

/// Parse one Turtle source into the shared store (lenient, mirroring
/// `catalog::parse_turtle_to_store` so GMEOW's `@x-gmeow-*` language tags parse).
fn load_into_store(store: &Store, bytes: &[u8], path: &Path) -> Result<(), SliceError> {
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes)
    {
        let quad = quad
            .map_err(|e| SliceError::Parse(format!("syntax error in {}: {e}", path.display())))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// Extract `gmeow:TermEquivalence` cells from a store (mirrors `_equivalences`).
fn extract_equivalences(store: &Store, out: &mut Vec<EquivalenceCell>) -> Result<(), SliceError> {
    for subject in subjects_of_type(store, GM_TERM_EQUIVALENCE)? {
        let cell = NamedNode::new(&subject)
            .map_err(|e| SliceError::Parse(format!("invalid cell IRI {subject}: {e}")))?;

        let get_iri =
            |pred: &str| -> Result<Option<String>, SliceError> { object_iri(store, &cell, pred) };
        let get_lit = |pred: &str| -> Result<Option<String>, SliceError> {
            object_literal(store, &cell, pred)
        };

        let subject_iri = get_iri(GM_ALIGN_SUBJECT)?;
        let predicate_iri = get_iri(GM_ALIGN_PREDICATE)?;
        let object_iri_v = get_iri(GM_ALIGN_OBJECT)?;
        let sssom_file = get_lit(GM_SSSOM_FILE)?;

        let (Some(subject_iri), Some(predicate_iri), Some(object_iri_v)) =
            (subject_iri, predicate_iri, object_iri_v)
        else {
            return Err(SliceError::Parse(format!(
                "term equivalence {subject} missing subject/predicate/object"
            )));
        };
        let Some(sssom_file) = sssom_file else {
            return Err(SliceError::Parse(format!(
                "term equivalence {subject} missing sssomFile"
            )));
        };

        let confidence = match get_lit(GM_CONFIDENCE)? {
            Some(text) => Some(text.parse::<f64>().map_err(|_| {
                SliceError::Parse(format!(
                    "term equivalence {subject} has non-numeric confidence"
                ))
            })?),
            None => None,
        };

        out.push(EquivalenceCell {
            subject: subject_iri,
            predicate: predicate_iri,
            obj: object_iri_v,
            confidence,
            justification: get_iri(GM_JUSTIFICATION)?,
            comment: get_lit(GM_COMMENT)?.unwrap_or_default(),
            sssom_file,
            subject_label: get_lit(GM_SUBJECT_LABEL)?.unwrap_or_default(),
            object_label: get_lit(GM_OBJECT_LABEL)?.unwrap_or_default(),
        });
    }
    Ok(())
}

/// Extract `gmeow:MappingSet` metadata from a store (mirrors `_mapping_sets`).
fn extract_mapping_sets(
    store: &Store,
    out: &mut BTreeMap<String, MappingSet>,
) -> Result<(), SliceError> {
    for subject in subjects_of_type(store, GM_MAPPING_SET)? {
        let node = NamedNode::new(&subject)
            .map_err(|e| SliceError::Parse(format!("invalid mapping set IRI {subject}: {e}")))?;
        let Some(file) = object_literal(store, &node, GM_SSSOM_FILE)? else {
            return Err(SliceError::Parse(format!(
                "mapping set {subject} missing sssomFile"
            )));
        };
        out.insert(
            file,
            MappingSet {
                set_id: object_literal(store, &node, GM_SET_ID)?.unwrap_or_default(),
                license: object_literal(store, &node, GM_LICENSE)?.unwrap_or_default(),
                comment: object_literal(store, &node, GM_SET_COMMENT)?.unwrap_or_default(),
                trailer: object_literal(store, &node, GM_SET_TRAILER)?.unwrap_or_default(),
            },
        );
    }
    Ok(())
}

// ── Self-metadata ──────────────────────────────────────────────────────────────

/// Read `(version, release_date)` from `metadata/gmeow-self.ttl`.
///
/// The Manifestation node is the unique subject carrying
/// `gmeow:versionFingerprint`; its `gmeow:datePublished` literal is the release
/// date (mirrors `self_desc.load_self_description_from_graph`). Hard-fails if the
/// file is absent or carries no such node.
fn read_self_metadata(root: &Path) -> Result<(String, String), SliceError> {
    let path = root.join("metadata").join("gmeow-self.ttl");
    let bytes = std::fs::read(&path).map_err(SliceError::Io)?;
    let store =
        Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))?;
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(&bytes[..])
    {
        let quad = quad
            .map_err(|e| SliceError::Parse(format!("syntax error in {}: {e}", path.display())))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }

    let fingerprint = NamedNode::new(GM_VERSION_FINGERPRINT)
        .map_err(|e| SliceError::Parse(format!("invalid versionFingerprint IRI: {e}")))?;

    // The Manifestation: the (single) subject of gmeow:versionFingerprint.
    let manifestation = store
        .quads_for_pattern(
            None,
            Some(fingerprint.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .transpose()
        .map_err(|e| SliceError::Parse(e.to_string()))?
        .map(|q| q.subject)
        .ok_or_else(|| {
            SliceError::InvalidManifest(
                "no manifestation with gmeow:versionFingerprint in gmeow-self.ttl".to_owned(),
            )
        })?;

    let oxigraph::model::NamedOrBlankNode::NamedNode(manifestation) = manifestation else {
        return Err(SliceError::InvalidManifest(
            "gmeow:versionFingerprint subject is not a named node".to_owned(),
        ));
    };

    let version =
        object_literal(&store, &manifestation, GM_VERSION_FINGERPRINT)?.ok_or_else(|| {
            SliceError::InvalidManifest("manifestation missing versionFingerprint".to_owned())
        })?;
    let release_date =
        object_literal(&store, &manifestation, GM_DATE_PUBLISHED)?.ok_or_else(|| {
            SliceError::InvalidManifest("manifestation missing datePublished".to_owned())
        })?;
    Ok((version, release_date))
}

// ── Store helpers ──────────────────────────────────────────────────────────────

/// Every named-node subject of `?s a <type_iri>`.
fn subjects_of_type(store: &Store, type_iri: &str) -> Result<Vec<String>, SliceError> {
    let rdf_type = NamedNode::new(RDF_TYPE)
        .map_err(|e| SliceError::Parse(format!("invalid rdf:type IRI: {e}")))?;
    let class = NamedNode::new(type_iri)
        .map_err(|e| SliceError::Parse(format!("invalid type IRI {type_iri}: {e}")))?;
    let mut subjects = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(class.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let oxigraph::model::NamedOrBlankNode::NamedNode(nn) = &quad.subject {
            subjects.push(nn.as_str().to_owned());
        }
    }
    Ok(subjects)
}

/// The first IRI object of `subject pred ?o`, or `None`.
fn object_iri(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object(store, subject, pred)? {
        Some(Term::NamedNode(nn)) => Ok(Some(nn.as_str().to_owned())),
        _ => Ok(None),
    }
}

/// The lexical form of the first literal object of `subject pred ?o`, or `None`.
fn object_literal(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object(store, subject, pred)? {
        Some(Term::Literal(lit)) => Ok(Some(lit.value().to_owned())),
        _ => Ok(None),
    }
}

/// The first object term of `subject pred ?o` in the default graph.
///
/// "First" follows the store's pattern-iteration order. Each of the predicates
/// used here is single-valued per cell in the corpus, matching the Python
/// `graph.value` semantics.
fn first_object(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<Term>, SliceError> {
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    match store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
    {
        Some(quad) => Ok(Some(
            quad.map_err(|e| SliceError::Parse(e.to_string()))?.object,
        )),
        None => Ok(None),
    }
}

// ── Rendering ──────────────────────────────────────────────────────────────────

/// One materialized SSSOM row: the eight named column cells (`_SSSOM_ORDER`).
struct Row {
    subject_id: String,
    subject_label: String,
    predicate_id: String,
    object_id: String,
    object_label: String,
    mapping_justification: String,
    confidence: String,
    comment: String,
}

impl Row {
    /// The cell value for a named column.
    fn cell(&self, column: &str) -> &str {
        match column {
            "subject_id" => &self.subject_id,
            "subject_label" => &self.subject_label,
            "predicate_id" => &self.predicate_id,
            "object_id" => &self.object_id,
            "object_label" => &self.object_label,
            "mapping_justification" => &self.mapping_justification,
            "confidence" => &self.confidence,
            "comment" => &self.comment,
            _ => "",
        }
    }
}

/// Render every mapping set to its TSV text (mirrors `emit_sssom`).
fn render_sets(
    sources: &SssomSources,
    version: &str,
    release_date: &str,
) -> Result<BTreeMap<String, String>, SliceError> {
    // Longest-IRI-prefix table for CURIE shortening (`mapping_dsl._NS_TO_PREFIX`),
    // built from the curated registry (the prefix authority).
    let ns_to_prefix = build_ns_to_prefix();

    // Group rows by sssom file, preserving cell order (rows are sorted at emit).
    let mut by_file: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for eq in &sources.equivalences {
        let justification = eq
            .justification
            .clone()
            .unwrap_or_else(|| DEFAULT_JUSTIFICATION.to_owned());
        let row = Row {
            subject_id: sssom_id(&eq.subject, &ns_to_prefix),
            subject_label: eq.subject_label.clone(),
            predicate_id: sssom_id(&eq.predicate, &ns_to_prefix),
            object_id: sssom_id(&eq.obj, &ns_to_prefix),
            object_label: eq.object_label.clone(),
            mapping_justification: sssom_id(&justification, &ns_to_prefix),
            confidence: conf(eq.confidence),
            comment: eq.comment.clone(),
        };
        by_file.entry(eq.sssom_file.clone()).or_default().push(row);
    }

    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (file, rows) in &by_file {
        let meta = sources.mapping_sets.get(file);
        let text = render_one(rows, meta, version, release_date);
        out.insert(file.clone(), text);
    }
    Ok(out)
}

/// Render one SSSOM file's text from its rows + metadata.
fn render_one(
    rows: &[Row],
    meta: Option<&MappingSet>,
    version: &str,
    release_date: &str,
) -> String {
    // Columns: always-on, plus any optional column some row populates.
    let columns: Vec<&str> = SSSOM_ORDER
        .iter()
        .copied()
        .filter(|c| SSSOM_ALWAYS.contains(c) || rows.iter().any(|r| !r.cell(c).is_empty()))
        .collect();

    // Used prefixes: the sorted set of prefix tokens appearing in the entity /
    // justification columns of this file's rows that are in the registry.
    let mut used: BTreeSet<String> = BTreeSet::new();
    for r in rows {
        for tok in [
            &r.subject_id,
            &r.predicate_id,
            &r.object_id,
            &r.mapping_justification,
        ] {
            if let Some((prefix, _)) = tok.split_once(':') {
                if registry_iri(prefix).is_some() {
                    used.insert(prefix.to_owned());
                }
            }
        }
    }

    let mut lines = sssom_header(meta, &used, version, release_date);

    if let Some(meta) = meta {
        if !meta.trailer.is_empty() {
            // Refused/deferred mappings kept IN the artifact: a second '#' makes
            // each trailer line a YAML-invisible comment (`emit_sssom`).
            for line in meta.trailer.lines() {
                lines.push(format!("# #{}", line.strip_prefix('#').unwrap_or(line)));
            }
        }
    }

    lines.push(columns.join("\t"));

    // Rows sorted by (subject_id, predicate_id, object_id).
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        (&a.subject_id, &a.predicate_id, &a.object_id).cmp(&(
            &b.subject_id,
            &b.predicate_id,
            &b.object_id,
        ))
    });
    for r in sorted {
        let cells: Vec<&str> = columns.iter().map(|c| r.cell(c)).collect();
        lines.push(cells.join("\t"));
    }

    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Build the SSSOM YAML-ish `#` metadata header (mirrors `_sssom_header`).
fn sssom_header(
    meta: Option<&MappingSet>,
    used: &BTreeSet<String>,
    version: &str,
    release_date: &str,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(meta) = meta {
        if !meta.set_id.is_empty() {
            lines.push(format!("# mapping_set_id: {}", meta.set_id));
            lines.push(format!("# mapping_set_version: {version}"));
            lines.push(format!("# license: {}", meta.license));
        }
    }
    lines.push("# mapping_tool: gmeow regenerate (mappings)".to_owned());
    lines.push(format!("# mapping_tool_version: {version}"));
    lines.push(format!("# mapping_date: {release_date}"));
    if let Some(meta) = meta {
        if !meta.comment.is_empty() {
            // Collapse any whitespace run (incl. multi-line """...""" newlines) to
            // single spaces, then JSON-quote (ensure_ascii — non-ASCII as \uXXXX).
            let collapsed = collapse_whitespace(&meta.comment);
            lines.push(format!("# comment: {}", json_quote_ascii(&collapsed)));
        }
    }
    lines.push("# curie_map:".to_owned());
    for prefix in used {
        // `used` only contains registry prefixes (checked at build).
        if let Some(iri) = registry_iri(prefix) {
            lines.push(format!("#   {prefix}: {iri}"));
        }
    }
    lines
}

// ── CURIE shortening ───────────────────────────────────────────────────────────

/// The registry namespace IRI for a prefix, or `None` (mirrors `prefix in PREFIXES`
/// / `PREFIXES[prefix]`).
fn registry_iri(prefix: &str) -> Option<&'static str> {
    PREFIX_REGISTRY
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, ns)| *ns)
}

/// Build the longest-IRI-first `(namespace, prefix)` table used to shorten an IRI
/// to a CURIE, from the curated [`PREFIX_REGISTRY`].
///
/// Mirrors `mapping_dsl._NS_TO_PREFIX`: a stable sort keyed on descending
/// namespace length. The tiebreak among equal-length namespaces is the registry's
/// own insertion order (Python sorts `((ns, p) for p, ns in PREFIXES.items())`
/// stably by `-len(ns)`, so equal-length namespaces keep `config.PREFIXES` order).
fn build_ns_to_prefix() -> Vec<(&'static str, &'static str)> {
    let mut pairs: Vec<(&'static str, &'static str)> =
        PREFIX_REGISTRY.iter().map(|(p, ns)| (*ns, *p)).collect();
    // Stable sort: longest namespace first, registry order preserved for ties.
    pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.len()));
    pairs
}

/// Return a SSSOM-safe identifier: a CURIE when a namespace prefixes the IRI,
/// otherwise the bare absolute URI (mirrors `_sssom_id` ∘ `curie`).
///
/// Python's `curie()` returns `<iri>` for an unmatched namespace and `_sssom_id`
/// strips the angle brackets, leaving the bare URI — so an unmatched IRI is
/// emitted verbatim here.
fn sssom_id(iri: &str, ns_to_prefix: &[(&str, &str)]) -> String {
    for (ns, prefix) in ns_to_prefix {
        if let Some(local) = iri.strip_prefix(*ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_owned()
}

// ── Confidence formatting ──────────────────────────────────────────────────────

/// Format a confidence exactly as Python's `_conf`:
///
/// * `None` → `""`;
/// * an integer-valued float → `"{v:.1f}"` (e.g. `1.0` → `"1.0"`);
/// * otherwise → `"{v:g}"` (Python `%g`: 6 significant figures, trailing zeros
///   and a trailing point stripped — e.g. `0.8` → `"0.8"`, `0.75` → `"0.75"`).
fn conf(value: Option<f64>) -> String {
    let Some(v) = value else {
        return String::new();
    };
    if v == v.trunc() {
        format!("{v:.1}")
    } else {
        format_g(v)
    }
}

/// Render a float the way Python's `"{:g}"` does: shortest of fixed/scientific at
/// 6 significant digits, with trailing zeros (and a dangling decimal point)
/// removed. The committed corpus only exercises short fixed-point confidences
/// (0.3 … 0.98), all of which round-trip through this exactly; the full `%g`
/// algorithm is implemented so any future confidence stays parity-safe.
fn format_g(v: f64) -> String {
    // Python's default `%g` precision is 6 significant figures.
    const SIG: usize = 6;
    if v == 0.0 {
        return "0".to_owned();
    }
    let exponent = v.abs().log10().floor() as i32;
    // %g uses scientific notation when exp < -4 or exp >= precision.
    if exponent < -4 || exponent >= SIG as i32 {
        // Scientific form: d.ddddde±XX with trailing zeros trimmed.
        let mantissa_prec = SIG.saturating_sub(1);
        let s = format!("{v:.*e}", mantissa_prec);
        return trim_scientific(&s);
    }
    // Fixed form: precision = sig figs minus the integer-part digits.
    let decimals = (SIG as i32 - 1 - exponent).max(0) as usize;
    let s = format!("{v:.*}", decimals);
    trim_fixed(&s)
}

/// Strip trailing zeros (and a dangling `.`) from a fixed-point decimal string.
fn trim_fixed(s: &str) -> String {
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0');
        let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
        trimmed.to_owned()
    } else {
        s.to_owned()
    }
}

/// Trim the mantissa of a `{:e}` string and normalize its exponent to Python's
/// `%g` shape (`e±NN`, at least two exponent digits, no `+` dropped).
fn trim_scientific(s: &str) -> String {
    let (mantissa, exp) = match s.split_once('e') {
        Some((m, e)) => (m, e),
        None => return s.to_owned(),
    };
    let mantissa = trim_fixed(mantissa);
    // Normalize exponent: sign + at least two digits (Python uses e.g. `e-05`).
    let (sign, digits) = match exp.strip_prefix('-') {
        Some(d) => ('-', d),
        None => ('+', exp.strip_prefix('+').unwrap_or(exp)),
    };
    let digits = if digits.len() < 2 {
        format!("{digits:0>2}")
    } else {
        digits.to_owned()
    };
    format!("{mantissa}e{sign}{digits}")
}

// ── Comment quoting ────────────────────────────────────────────────────────────

/// Collapse every whitespace run to a single space and strip leading/trailing
/// whitespace — Python's `" ".join(text.split())`.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// JSON-quote a string the way Python's `json.dumps(s)` does with the default
/// `ensure_ascii=True`: wrap in `"`, escape the JSON control set, and emit every
/// non-ASCII scalar as a `\uXXXX` escape (surrogate pair for astral planes).
///
/// `serde_json::to_string` would NOT do this — it emits raw UTF-8 for non-ASCII —
/// so the committed corpus (which carries em-dashes as `—`) requires this
/// faithful re-implementation rather than serde.
fn json_quote_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if c.is_ascii() => out.push(c),
            c => {
                // Non-ASCII: emit \uXXXX (with a surrogate pair beyond the BMP),
                // exactly as Python's json.dumps(ensure_ascii=True).
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_formats_match_python() {
        assert_eq!(conf(None), "");
        assert_eq!(conf(Some(1.0)), "1.0"); // integer-valued → {:.1f}
        assert_eq!(conf(Some(0.0)), "0.0");
        assert_eq!(conf(Some(2.0)), "2.0");
        assert_eq!(conf(Some(0.8)), "0.8"); // {:g}
        assert_eq!(conf(Some(0.75)), "0.75");
        assert_eq!(conf(Some(0.98)), "0.98");
        assert_eq!(conf(Some(0.3)), "0.3");
        assert_eq!(conf(Some(0.35)), "0.35");
        assert_eq!(conf(Some(0.55)), "0.55");
    }

    #[test]
    fn format_g_handles_six_sig_figs() {
        // Python: f"{0.123456789:g}" == "0.123457" (6 sig figs, rounded).
        assert_eq!(format_g(0.123456789), "0.123457");
        // f"{1234567.0:g}" == "1.23457e+06"
        assert_eq!(format_g(1234567.0), "1.23457e+06");
        // f"{0.00001234:g}" == "1.234e-05"
        assert_eq!(format_g(0.00001234), "1.234e-05");
    }

    #[test]
    fn sssom_id_shortens_longest_prefix_then_falls_back() {
        // The curated registry already carries the colliding obo/ namespaces
        // (`obi`=obo/OBI_, `gsso`=obo/GSSO_, `bfo`=obo/), so longest-match is
        // exercised against the real table.
        let table = build_ns_to_prefix();

        assert_eq!(
            sssom_id("https://blackcatinformatics.ca/gmeow/ToolCall", &table),
            "gmeow:ToolCall"
        );
        assert_eq!(
            sssom_id("http://www.w3.org/2004/02/skos/core#closeMatch", &table),
            "skos:closeMatch"
        );
        // Longest namespace (OBI_) beats the shorter obo/ namespace.
        assert_eq!(
            sssom_id("http://purl.obolibrary.org/obo/OBI_0000070", &table),
            "obi:0000070"
        );
        assert_eq!(
            sssom_id("http://purl.obolibrary.org/obo/BFO_0000001", &table),
            "bfo:BFO_0000001"
        );
        // No namespace match → bare absolute URI.
        assert_eq!(
            sssom_id("https://example.org/unregistered/Thing", &table),
            "https://example.org/unregistered/Thing"
        );
    }

    #[test]
    fn json_quote_escapes_non_ascii_as_unicode() {
        // The em-dash (U+2014) must render as the ASCII escape —
        // (ensure_ascii), NOT raw UTF-8 — the corpus-critical behaviour.
        assert_eq!(json_quote_ascii("a \u{2014} b"), "\"a \\u2014 b\"");
        // The JSON control escapes.
        assert_eq!(json_quote_ascii("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(json_quote_ascii("x\ty"), "\"x\\ty\"");
        // An astral-plane scalar becomes a UTF-16 surrogate pair, exactly as
        // Python's json.dumps does: U+1F408 (🐈) → 🐈.
        assert_eq!(json_quote_ascii("\u{1f408}"), "\"\\ud83d\\udc08\"");
    }

    #[test]
    fn collapse_whitespace_joins_runs() {
        assert_eq!(collapse_whitespace("a  b\n  c\t d"), "a b c d");
        assert_eq!(collapse_whitespace("  trimmed  "), "trimmed");
    }

    /// End-to-end: a synthetic source set renders the canonical TSV shape — the
    /// column selection (label only when populated), the used-prefix curie_map,
    /// row sorting, the trailing newline, and the header provenance.
    #[test]
    fn render_one_emits_canonical_tsv() {
        let table = build_ns_to_prefix();

        let make = |subj: &str, pred: &str, obj: &str, c: Option<f64>| Row {
            subject_id: sssom_id(subj, &table),
            subject_label: String::new(),
            predicate_id: sssom_id(pred, &table),
            object_id: sssom_id(obj, &table),
            object_label: String::new(),
            mapping_justification: sssom_id(DEFAULT_JUSTIFICATION, &table),
            confidence: conf(c),
            comment: String::new(),
        };
        // Two rows, deliberately out of (subject, predicate, object) order.
        let rows = vec![
            make(
                &format!("{GMEOW}Zeta"),
                "http://www.w3.org/2004/02/skos/core#closeMatch",
                &format!("{GMEOW}Bar"),
                Some(0.8),
            ),
            make(
                &format!("{GMEOW}Alpha"),
                "http://www.w3.org/2004/02/skos/core#exactMatch",
                &format!("{GMEOW}Foo"),
                Some(1.0),
            ),
        ];
        let meta = MappingSet {
            set_id: "https://blackcatinformatics.ca/gmeow/mappings/demo".to_owned(),
            license: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
            comment: "Demo  set\nwith   wrap".to_owned(),
            trailer: "# REFUSED nothing here".to_owned(),
        };

        let text = render_one(&rows, Some(&meta), "0.1.0", "2026-06-03");
        let expected = "\
# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/demo
# mapping_set_version: 0.1.0
# license: https://creativecommons.org/licenses/by/4.0/
# mapping_tool: gmeow regenerate (mappings)
# mapping_tool_version: 0.1.0
# mapping_date: 2026-06-03
# comment: \"Demo set with wrap\"
# curie_map:
#   gmeow: https://blackcatinformatics.ca/gmeow/
#   semapv: https://w3id.org/semapv/vocab/
#   skos: http://www.w3.org/2004/02/skos/core#
# # REFUSED nothing here
subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment
gmeow:Alpha\tskos:exactMatch\tgmeow:Foo\tsemapv:ManualMappingCuration\t1.0\t
gmeow:Zeta\tskos:closeMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t0.8\t
";
        assert_eq!(text, expected);
    }

    /// A populated label column appears; a justification-less cell defaults.
    #[test]
    fn render_one_includes_label_column_when_populated() {
        let table = build_ns_to_prefix();
        let row = Row {
            subject_id: sssom_id(&format!("{GMEOW}Foo"), &table),
            subject_label: "Foo label".to_owned(),
            predicate_id: sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", &table),
            object_id: sssom_id(&format!("{GMEOW}Bar"), &table),
            object_label: String::new(),
            mapping_justification: sssom_id(DEFAULT_JUSTIFICATION, &table),
            confidence: conf(None),
            comment: String::new(),
        };
        let text = render_one(&[row], None, "0.1.0", "2026-06-03");
        // subject_label present, object_label absent (no row populates it).
        let header_row = text
            .lines()
            .find(|l| l.starts_with("subject_id"))
            .expect("column header");
        assert_eq!(
            header_row,
            "subject_id\tsubject_label\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment"
        );
        // No set metadata → no mapping_set_id line.
        assert!(!text.contains("mapping_set_id"));
    }
}

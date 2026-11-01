// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-scoped native admission, separate from broad vocabulary residue counting.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::grounding_catalog::{GroundingCatalog, OwnedGroundingSource};
use gmeow_logic_compile::projections::sssom;
use purrdf::{RdfDataset, RdfTerm, TermRef};
use std::sync::{Arc, OnceLock};

use super::{CatalogFamily, CatalogRegistry, GMEOW, RDF_TYPE, families_for_target, registry_err};

const OWNER: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const GROUNDING: &str = "https://blackcatinformatics.ca/logic/GroundingCorrespondence";
const SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";
const PROJECTION: &str = "https://blackcatinformatics.ca/gmeow/ProjectionMapping";

/// Original physical occurrence; unrelated domain documents carry no grounding authority.
pub(crate) struct OriginalMapping {
    owner: String,
    path: String,
    bytes: Arc<[u8]>,
    dataset: Arc<RdfDataset>,
    physical: OnceLock<OwnedGroundingSource>,
}

impl OriginalMapping {
    pub(crate) fn new(owner: &str, path: &str, bytes: &[u8], dataset: Arc<RdfDataset>) -> Self {
        Self {
            owner: owner.to_owned(),
            path: path.to_owned(),
            bytes: Arc::from(bytes),
            dataset,
            physical: OnceLock::new(),
        }
    }

    fn physical(&self) -> gmeow_errors::Result<&OwnedGroundingSource> {
        self.physical
            .get_or_try_init(|| OwnedGroundingSource::new(&self.owner, &self.path, &self.bytes))
    }
}

/// Producer-verified ownership and admitted identity of one kernel correspondence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NativeKernelEvidence {
    /// Registered target-family identity.
    pub family: String,
    /// Exact correspondence identity produced by the shared frontend.
    pub correspondence: String,
    /// Complete non-law semantic key from the admitted correspondence.
    pub semantic_key: String,
    /// Canonical source term.
    pub source: String,
    /// Actual external target term.
    pub target: String,
    /// Source term's canonical Module-role owner.
    pub source_owner: String,
    /// Original declaring Module artifact.
    pub source_module_path: String,
    /// SHA-256 identity of that original Module artifact.
    pub source_module_digest: String,
    /// Actual owner of this defining Mapping artifact.
    pub artifact_owner: String,
    /// Normalized repository-relative Mapping artifact path.
    pub artifact_path: String,
    /// SHA-256 identity of the original Mapping bytes.
    pub artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ModuleDeclaration {
    owner: String,
    path: String,
    digest: String,
}

/// Canonical declarations from original Module-role artifacts, never mapping annotations.
#[derive(Default)]
pub(crate) struct CanonicalOwnership {
    claims: BTreeMap<String, BTreeSet<Option<String>>>,
    declarations: BTreeMap<String, BTreeSet<ModuleDeclaration>>,
}

impl CanonicalOwnership {
    pub(crate) fn record_module(
        &mut self,
        owner: &str,
        path: &str,
        bytes: &[u8],
        dataset: &RdfDataset,
    ) {
        use sha2::{Digest, Sha256};
        let digest = purrdf_hash::hex::encode(&Sha256::digest(bytes));
        for quad in purrdf::native_quads::flat_rdf_quads(dataset) {
            if quad.predicate != OWNER || quad.graph_name.is_some() {
                continue;
            }
            let RdfTerm::Iri(source) = quad.subject else {
                continue;
            };
            let declared = match quad.object {
                RdfTerm::Iri(owner) => Some(owner),
                _ => None,
            };
            self.claims
                .entry(source.clone())
                .or_default()
                .insert(declared.clone());
            let kernel_home = match owner {
                "https://blackcatinformatics.ca/gmeow/slices/logic" => {
                    Some("slices/grounding/logic/")
                }
                "https://blackcatinformatics.ca/gmeow/slices/math" => {
                    Some("slices/grounding/math/")
                }
                "https://blackcatinformatics.ca/gmeow/slices/lang" => {
                    Some("slices/grounding/lang/")
                }
                _ => None,
            };
            let physical_home = !kernel_owner(owner)
                || kernel_home.is_some_and(|stem| {
                    path.starts_with(stem)
                        && path.ends_with(".ttl")
                        && !path.contains('\\')
                        && path
                            .split('/')
                            .all(|part| !part.is_empty() && part != "." && part != "..")
                });
            if declared.as_deref() == Some(owner) && physical_home {
                self.declarations
                    .entry(source)
                    .or_default()
                    .insert(ModuleDeclaration {
                        owner: owner.to_owned(),
                        path: path.to_owned(),
                        digest: digest.clone(),
                    });
            }
        }
    }

    fn source(&self, source: &str) -> gmeow_errors::Result<Option<&ModuleDeclaration>> {
        let claims = self.claims.get(source);
        if claims.is_none()
            && ![
                GMEOW,
                "https://blackcatinformatics.ca/logic/",
                "https://blackcatinformatics.ca/math/",
                "https://blackcatinformatics.ca/lang/",
            ]
            .iter()
            .any(|namespace| source.starts_with(namespace))
        {
            return Ok(None);
        }
        let declaration = self.declarations.get(source);
        if claims.is_none_or(|claims| claims.len() != 1 || claims.contains(&None))
            || declaration.is_none_or(|declarations| declarations.len() != 1)
        {
            return Err(registry_err(format!(
                "native kernel candidate <{source}> requires one unambiguous canonical declaration in its actual owner's Module-role artifact"
            )));
        }
        Ok(declaration.and_then(|declarations| declarations.first()))
    }
}

pub(super) fn families_from_dataset(
    dataset: &RdfDataset,
) -> gmeow_errors::Result<BTreeSet<String>> {
    let class = format!("{GMEOW}NativeKernelCatalogGuard");
    let mut families = BTreeSet::new();
    for quad in purrdf::native_quads::flat_rdf_quads(dataset) {
        if quad.predicate == RDF_TYPE && quad.object == RdfTerm::Iri(class.clone()) {
            let RdfTerm::Iri(family) = quad.subject else {
                return Err(registry_err(
                    "native kernel guard requires a named catalog family".into(),
                ));
            };
            families.insert(family);
        }
    }
    Ok(families)
}

pub(super) fn check_regimes(
    registry: &CatalogRegistry,
    guarded: &BTreeSet<String>,
) -> gmeow_errors::Result<()> {
    for family in &registry.native_kernel_families {
        if !registry.families.iter().any(|item| &item.iri == family) {
            return Err(registry_err(format!(
                "native kernel guard names unregistered family {family}"
            )));
        }
    }
    for family in &registry.families {
        let broad = family.namespaces.iter().any(|stem| {
            guarded
                .iter()
                .any(|namespace| stem.starts_with(namespace) || namespace.starts_with(stem))
        });
        let exempt = registry
            .exemptions
            .iter()
            .filter(|item| item.family_iri == family.iri)
            .count();
        let native = usize::from(registry.native_kernel_families.contains(&family.iri));
        if usize::from(broad) + exempt + native != 1 {
            return Err(registry_err(format!(
                "{} requires exactly one governance regime: broad guard, bounded exemption or native kernel guard",
                family.name
            )));
        }
    }
    Ok(())
}

fn kernel_owner(owner: &str) -> bool {
    matches!(
        owner,
        "https://blackcatinformatics.ca/gmeow/slices/logic"
            | "https://blackcatinformatics.ca/gmeow/slices/math"
            | "https://blackcatinformatics.ca/gmeow/slices/lang"
    )
}

fn scoped_family<'a, 'b>(
    registry: &'a CatalogRegistry,
    ownership: &'b CanonicalOwnership,
    source: &str,
    target: &str,
) -> gmeow_errors::Result<Option<(&'a CatalogFamily, &'b ModuleDeclaration)>> {
    let families = families_for_target(&registry.families, target);
    let Some(family) = families
        .into_iter()
        .find(|family| registry.native_kernel_families.contains(&family.iri))
    else {
        return Ok(None);
    };
    let declaration = ownership.source(source)?;
    Ok(declaration
        .filter(|declaration| kernel_owner(&declaration.owner))
        .map(|declaration| (family, declaration)))
}

fn require_kernel_source(
    registry: &CatalogRegistry,
    ownership: &CanonicalOwnership,
    source: &str,
    target: &str,
) -> gmeow_errors::Result<()> {
    let selected = families_for_target(&registry.families, target)
        .iter()
        .any(|family| registry.native_kernel_families.contains(&family.iri));
    if selected && scoped_family(registry, ownership, source, target)?.is_none() {
        return Err(registry_err(format!(
            "grounding correspondence from <{source}> to <{target}> requires a canonical kernel-owned source"
        )));
    }
    Ok(())
}

/// Admit one original Mapping-role document before any cross-artifact union.
pub(crate) fn admit_source(
    registry: &CatalogRegistry,
    ontology: &DslView<'_>,
    ownership: &CanonicalOwnership,
    original: &OriginalMapping,
) -> gmeow_errors::Result<Vec<NativeKernelEvidence>> {
    let dataset = original.dataset.as_ref();
    let path = &original.path;
    let view = DslView::new(dataset);
    let mut candidates = BTreeSet::new();
    for quad in purrdf::native_quads::flat_rdf_quads(dataset) {
        if quad.graph_name.is_none()
            && sssom::is_alignment_predicate(&quad.predicate)
            && let (RdfTerm::Iri(source), RdfTerm::Iri(target)) = (quad.subject, quad.object)
            && scoped_family(registry, ownership, &source, &target)?.is_some()
        {
            candidates.insert((source, quad.predicate, target));
        }
    }
    for statement in view.reified_statements() {
        let (source, predicate, target) = statement.triple()?;
        if statement.annotation_has_type(GROUNDING)
            && let TermRef::Iri(target_iri) = &target
            && families_for_target(&registry.families, target_iri)
                .iter()
                .any(|family| registry.native_kernel_families.contains(&family.iri))
        {
            let TermRef::Iri(source_iri) = &source else {
                return Err(registry_err(format!(
                    "{path}: native grounding correspondence to <{target_iri}> requires an IRI source"
                )));
            };
            require_kernel_source(registry, ownership, source_iri, target_iri)?;
        }
        if let (TermRef::Iri(source), TermRef::Iri(predicate), TermRef::Iri(target)) =
            (source, predicate, target)
            && (sssom::is_alignment_predicate(predicate)
                || statement.annotation_has_type(GROUNDING)
                || statement.annotation_literal(SSSOM_FILE)?.is_some())
            && scoped_family(registry, ownership, source, target)?.is_some()
        {
            if !sssom::is_alignment_predicate(predicate)
                || !statement.annotation_has_type(GROUNDING)
                || statement.annotation_literal(SSSOM_FILE)?.is_none()
            {
                return Err(registry_err(format!(
                    "{path}: kernel alignment <{source}> <{predicate}> <{target}> requires a complete native grounding envelope and sssomFile discriminator"
                )));
            }
            candidates.insert((source.to_owned(), predicate.to_owned(), target.to_owned()));
        }
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let physical = original.physical()?;
    let admitted = GroundingCatalog::admit(&view, ontology)?;
    let cells = sssom::equivalence_cells(&view)?;
    for (source, predicate, target) in &candidates {
        if !cells.iter().any(|cell| {
            cell.subject == *source
                && cell.predicate == *predicate
                && cell.obj == *target
                && cell.grounding
        }) {
            return Err(registry_err(format!(
                "{path}: kernel alignment <{source}> <{predicate}> <{target}> has no admitted native grounding witness"
            )));
        }
    }
    let identity = physical.identity();
    let mut evidence = Vec::new();
    for correspondence in &admitted.program().correspondences {
        let (Some(source), Some(target)) = (
            &correspondence.source_endpoint,
            &correspondence.target_endpoint,
        ) else {
            continue;
        };
        let Some((family, declaration)) = scoped_family(registry, ownership, source, target)?
        else {
            continue;
        };
        if !correspondence.grounding || !family.owners.contains(&identity.owner) {
            return Err(registry_err(format!(
                "{path}: kernel correspondence {} lacks the family's physically owned native grounding authority",
                correspondence.iri
            )));
        }
        evidence.push(NativeKernelEvidence {
            family: family.iri.clone(),
            correspondence: correspondence.iri.clone(),
            semantic_key: native_semantic_key(correspondence),
            source: source.clone(),
            target: target.clone(),
            source_owner: declaration.owner.clone(),
            source_module_path: declaration.path.clone(),
            source_module_digest: declaration.digest.clone(),
            artifact_owner: identity.owner.clone(),
            artifact_path: identity.path.clone(),
            artifact_digest: identity.digest.clone(),
        });
    }
    Ok(evidence)
}

/// Use the actual merged analysis, preserving cross-artifact pattern references.
pub(crate) fn check_projection_alternatives(
    registry: &CatalogRegistry,
    ownership: &CanonicalOwnership,
    evidence: &mut Vec<NativeKernelEvidence>,
    analysis: &gmeow_logic_compile::projections::correspondence_frontend::CorrespondenceAnalysis,
    originals: &[OriginalMapping],
) -> gmeow_errors::Result<()> {
    let documents: Vec<Vec<_>> = originals
        .iter()
        .map(|original| purrdf::native_quads::flat_rdf_quads(&original.dataset).collect())
        .collect();
    let mut named = BTreeMap::<String, BTreeSet<usize>>::new();
    for (index, quads) in documents.iter().enumerate() {
        for quad in quads {
            if let RdfTerm::Iri(subject) = &quad.subject {
                named.entry(subject.clone()).or_default().insert(index);
            }
        }
    }
    let admitted_subjects: BTreeSet<_> = analysis
        .projection_cells()
        .iter()
        .map(|cell| cell.iri.as_str())
        .collect();
    let structural_subjects: BTreeSet<_> = documents
        .iter()
        .flatten()
        .filter_map(|quad| {
            let RdfTerm::Iri(subject) = &quad.subject else {
                return None;
            };
            (!admitted_subjects.contains(subject.as_str())
                && matches!(
                    quad.predicate.as_str(),
                    "https://blackcatinformatics.ca/gmeow/hasMappingPattern"
                        | "https://blackcatinformatics.ca/gmeow/hasBinding"
                ))
            .then(|| subject.clone())
        })
        .collect();
    let untyped = if structural_subjects.is_empty() {
        Vec::new()
    } else {
        let source_views: Vec<_> = originals
            .iter()
            .map(|original| original.dataset.as_ref())
            .collect();
        let merged = RdfDataset::union(&source_views)?;
        analysis.inspect_projection_subjects(&DslView::new(&merged), &structural_subjects)?
    };
    for cell in analysis.projection_cells().iter().chain(&untyped) {
        for binding in &cell.bindings {
            let mut pairs = BTreeSet::new();
            if binding.emit_sssom {
                pairs.extend(sssom::projection_sssom_pairs(cell, binding)?);
            } else {
                let sources: BTreeSet<_> = cell
                    .pattern
                    .edoal_source
                    .as_deref()
                    .into_iter()
                    .chain(
                        cell.grounding
                            .as_ref()
                            .and_then(|grounding| grounding.source_endpoint.as_deref()),
                    )
                    .collect();
                let targets: BTreeSet<_> = binding
                    .to_predicate
                    .as_deref()
                    .into_iter()
                    .chain(binding.to_class.as_deref())
                    .chain(binding.edoal_target.as_deref())
                    .chain(
                        binding
                            .template_atoms
                            .iter()
                            .filter_map(|atom| atom.predicate.as_deref()),
                    )
                    .chain(
                        binding
                            .template_atoms
                            .iter()
                            .filter_map(|atom| atom.object_value.as_deref()),
                    )
                    .collect();
                for source in sources {
                    for target in &targets {
                        pairs.insert((source.to_owned(), (*target).to_owned()));
                    }
                }
            }
            for (source, target) in pairs {
                if let Some((family, declaration)) =
                    scoped_family(registry, ownership, &source, &target)?
                {
                    let supported = binding.selected_templates.iter().any(|selected| {
                        selected.support.source_property == source
                            && selected.support.target_property == target
                            && evidence.iter().any(|proof| {
                                proof.correspondence == selected.support.correspondence
                            })
                    });
                    if supported
                        && cell.grounding.is_none()
                        && admitted_subjects.contains(cell.iri.as_str())
                    {
                        continue;
                    }
                    let origins = projection_origins(&cell.iri, &documents, &named, ownership);
                    if origins.is_empty() {
                        return Err(registry_err(format!(
                            "kernel projection {} has no original physical defining occurrence",
                            cell.iri
                        )));
                    }
                    let mut admitted = None;
                    for original in originals {
                        let view = DslView::new(&original.dataset);
                        let defining = view.objects_of(&cell.iri, RDF_TYPE).iter().any(|term| {
                            term.as_iri() == Some(PROJECTION) || term.as_iri() == Some(GROUNDING)
                        }) || ["justification", "hasMappingPattern", "hasBinding"]
                            .iter()
                            .any(|field| {
                                !view
                                    .objects_of(&cell.iri, &format!("{GMEOW}{field}"))
                                    .is_empty()
                            })
                            || [
                                "sourceEndpoint",
                                "targetEndpoint",
                                "morphismClass",
                                "morphismKind",
                                "preservationKind",
                            ]
                            .iter()
                            .any(|field| {
                                !view
                                    .objects_of(
                                        &cell.iri,
                                        &format!("https://blackcatinformatics.ca/logic/{field}"),
                                    )
                                    .is_empty()
                            });
                        if !defining {
                            continue;
                        }
                        let physical = original.physical()?;
                        if !family.owners.contains(&physical.identity().owner) {
                            return Err(registry_err(format!(
                                "{}: kernel projection {} is outside its catalog owner",
                                original.path, cell.iri
                            )));
                        }
                        if !view.subjects_of_type(PROJECTION).contains(&cell.iri)
                            || !view.subjects_of_type(GROUNDING).contains(&cell.iri)
                        {
                            return Err(registry_err(format!(
                                "{}: kernel projection {} requires both native frontend and grounding markers",
                                original.path, cell.iri
                            )));
                        }
                        let mut local = cell.clone();
                        local.grounding = Some(
                            gmeow_logic_compile::projections::get_leg::GroundingAuthoring {
                                justification: Some(singular_iri(
                                    &view,
                                    &cell.iri,
                                    &format!("{GMEOW}justification"),
                                )?),
                                morphism_class: Some(singular_iri(
                                    &view,
                                    &cell.iri,
                                    "https://blackcatinformatics.ca/logic/morphismClass",
                                )?),
                                morphism_kind: Some(singular_iri(
                                    &view,
                                    &cell.iri,
                                    "https://blackcatinformatics.ca/logic/morphismKind",
                                )?),
                                preservation: Some(singular_iri(
                                    &view,
                                    &cell.iri,
                                    "https://blackcatinformatics.ca/logic/preservationKind",
                                )?),
                                source_endpoint: Some(singular_iri(
                                    &view,
                                    &cell.iri,
                                    "https://blackcatinformatics.ca/logic/sourceEndpoint",
                                )?),
                                target_endpoint: Some(singular_iri(
                                    &view,
                                    &cell.iri,
                                    "https://blackcatinformatics.ca/logic/targetEndpoint",
                                )?),
                            },
                        );
                        if local
                            .grounding
                            .as_ref()
                            .and_then(|grounding| grounding.source_endpoint.as_deref())
                            != Some(source.as_str())
                            || (!binding.emit_sssom
                                && cell.pattern.edoal_source.as_deref() != Some(source.as_str()))
                        {
                            return Err(registry_err(format!(
                                "{}: kernel projection {} sourceEndpoint differs from its actual parsed source",
                                original.path, cell.iri
                            )));
                        }
                        let (correspondence, _) = gmeow_logic_compile::projections::correspondence_frontend::correspondence_for_binding(&local, binding)?;
                        if let Some(previous) = &admitted
                            && native_semantic_key(previous) != native_semantic_key(&correspondence)
                        {
                            return Err(registry_err(format!(
                                "kernel projection {} has conflicting physical envelopes",
                                cell.iri
                            )));
                        }
                        admitted = Some(correspondence);
                    }
                    let correspondence = admitted.ok_or_else(|| {
                        registry_err(format!(
                            "kernel projection {} lacks its complete original grounding envelope",
                            cell.iri
                        ))
                    })?;
                    for index in origins {
                        let physical = originals[index].physical()?;
                        let identity = physical.identity();
                        if !family.owners.contains(&identity.owner) {
                            return Err(registry_err(format!(
                                "{}: kernel projection {} consumes off-owner defining metadata",
                                identity.path, cell.iri
                            )));
                        }
                        evidence.push(NativeKernelEvidence {
                            family: family.iri.clone(),
                            correspondence: correspondence.iri.clone(),
                            semantic_key: native_semantic_key(&correspondence),
                            source: source.clone(),
                            target: target.clone(),
                            source_owner: declaration.owner.clone(),
                            source_module_path: declaration.path.clone(),
                            source_module_digest: declaration.digest.clone(),
                            artifact_owner: identity.owner.clone(),
                            artifact_path: identity.path.clone(),
                            artifact_digest: identity.digest.clone(),
                        });
                    }
                }
            }
        }
    }
    // A stripped frontend marker cannot make a previously explicit endpoint pair disappear.
    for original in originals {
        let view = DslView::new(&original.dataset);
        let native_aliases = sssom::equivalence_cells(&view)?
            .into_iter()
            .filter_map(|cell| cell.source_alias)
            .collect::<BTreeSet<_>>();
        for (subject, source) in
            view.quads_with_predicate("https://blackcatinformatics.ca/logic/sourceEndpoint")
        {
            let (Some(subject), Some(source)) = (subject.as_iri(), source.as_iri()) else {
                continue;
            };
            if native_aliases.contains(subject) {
                continue;
            }
            for target in view.object_iris(
                subject,
                "https://blackcatinformatics.ca/logic/targetEndpoint",
            ) {
                if scoped_family(registry, ownership, source, &target)?.is_some()
                    && !analysis
                        .projection_cells()
                        .iter()
                        .any(|cell| cell.iri == subject)
                {
                    return Err(registry_err(format!(
                        "{}: kernel endpoint declaration <{subject}> has no native frontend",
                        original.path
                    )));
                }
            }
        }
    }
    evidence.sort();
    Ok(())
}

fn singular_iri(
    view: &DslView<'_>,
    subject: &str,
    predicate: &str,
) -> gmeow_errors::Result<String> {
    let objects = view.objects_of(subject, predicate);
    match objects.as_slice() {
        [value] if value.as_iri().is_some() => Ok(value.as_iri().expect("matched IRI").to_owned()),
        _ => Err(registry_err(format!(
            "original projection <{subject}> requires one IRI <{predicate}>"
        ))),
    }
}

/// Follow owned definition nodes while leaving canonical term references by reference.
fn projection_origins(
    root: &str,
    documents: &[Vec<purrdf::RdfQuad>],
    named: &BTreeMap<String, BTreeSet<usize>>,
    ownership: &CanonicalOwnership,
) -> BTreeSet<usize> {
    let mut work: Vec<_> = named
        .get(root)
        .into_iter()
        .flatten()
        .map(|index| (*index, RdfTerm::Iri(root.to_owned())))
        .collect();
    let mut visited = Vec::new();
    let mut origins = BTreeSet::new();
    while let Some((index, node)) = work.pop() {
        if visited.contains(&(index, node.clone())) {
            continue;
        }
        visited.push((index, node.clone()));
        origins.insert(index);
        for quad in documents[index].iter().filter(|quad| quad.subject == node) {
            match &quad.object {
                RdfTerm::BlankNode(_) => work.push((index, quad.object.clone())),
                RdfTerm::Iri(iri)
                    if !ownership.claims.contains_key(iri)
                        && !matches!(
                            quad.predicate.as_str(),
                            RDF_TYPE
                                | OWNER
                                | "https://blackcatinformatics.ca/logic/sourceEndpoint"
                                | "https://blackcatinformatics.ca/logic/targetEndpoint"
                                | "https://blackcatinformatics.ca/gmeow/edoalSource"
                                | "https://blackcatinformatics.ca/gmeow/edoalTarget"
                                | "https://blackcatinformatics.ca/gmeow/toClass"
                                | "https://blackcatinformatics.ca/gmeow/toPredicate"
                                | "https://blackcatinformatics.ca/gmeow/whenValue"
                                | "https://blackcatinformatics.ca/gmeow/objectValue"
                                | "https://blackcatinformatics.ca/gmeow/tObjValue"
                                | "https://blackcatinformatics.ca/gmeow/predicate"
                                | "https://blackcatinformatics.ca/gmeow/tPred"
                        ) =>
                {
                    if let Some(indices) = named.get(iri) {
                        work.extend(indices.iter().map(|index| (*index, quad.object.clone())));
                    }
                }
                _ => {}
            }
        }
    }
    origins
}

pub(super) fn validate_evidence(
    registry: &CatalogRegistry,
    evidence: &[NativeKernelEvidence],
) -> gmeow_errors::Result<()> {
    let mut admitted: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut semantics = BTreeMap::new();
    for proof in evidence {
        let Some(family) = registry
            .families
            .iter()
            .find(|family| family.iri == proof.family)
        else {
            return Err(registry_err(format!(
                "native proof names unregistered family {}",
                proof.family
            )));
        };
        let expected_path = match proof.artifact_owner.as_str() {
            "https://blackcatinformatics.ca/gmeow/slices/logic" => {
                "slices/grounding/logic/mappings/"
            }
            "https://blackcatinformatics.ca/gmeow/slices/math" => "slices/grounding/math/mappings/",
            "https://blackcatinformatics.ca/gmeow/slices/lang" => "slices/grounding/lang/mappings/",
            _ => "",
        };
        let module_stem = match proof.source_owner.as_str() {
            "https://blackcatinformatics.ca/gmeow/slices/logic" => "slices/grounding/logic/",
            "https://blackcatinformatics.ca/gmeow/slices/math" => "slices/grounding/math/",
            "https://blackcatinformatics.ca/gmeow/slices/lang" => "slices/grounding/lang/",
            _ => "",
        };
        if !registry.native_kernel_families.contains(&family.iri)
            || !kernel_owner(&proof.source_owner)
            || module_stem.is_empty()
            || !proof.source_module_path.starts_with(module_stem)
            || !proof.source_module_path.ends_with(".ttl")
            || proof
                .source_module_path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || proof.source_module_path.contains('\\')
            || proof.source_module_digest.len() != 64
            || !proof
                .source_module_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !family.owners.contains(&proof.artifact_owner)
            || expected_path.is_empty()
            || !proof.artifact_path.starts_with(expected_path)
            || !proof.artifact_path.ends_with(".ttl")
            || proof
                .artifact_path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || proof.artifact_path.contains('\\')
            || proof.artifact_digest.len() != 64
            || !proof
                .artifact_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || proof.correspondence.is_empty()
            || proof.semantic_key.is_empty()
            || proof.source.is_empty()
            || !family
                .namespaces
                .iter()
                .any(|namespace| proof.target.starts_with(namespace))
        {
            return Err(registry_err(format!(
                "invalid native kernel proof for {}",
                proof.correspondence
            )));
        }
        let signature = (
            &proof.semantic_key,
            &proof.source,
            &proof.target,
            &proof.source_owner,
        );
        if let Some(previous) = semantics.insert(&proof.correspondence, signature)
            && previous != signature
        {
            return Err(registry_err(format!(
                "conflicting native kernel proofs for {}",
                proof.correspondence
            )));
        }
        admitted
            .entry(&family.iri)
            .or_default()
            .insert(&proof.correspondence);
    }
    for family in registry
        .families
        .iter()
        .filter(|family| registry.native_kernel_families.contains(&family.iri))
    {
        let count = admitted.get(family.iri.as_str()).map_or(0, BTreeSet::len);
        if count < family.minimum {
            return Err(registry_err(format!(
                "{} native kernel evidence has {count} admitted correspondences below minimum {}",
                family.name, family.minimum
            )));
        }
    }
    Ok(())
}

/// Reconcile the source proof with every typed kernel row, including retagged rows.
pub(crate) fn reconcile(
    registry: &CatalogRegistry,
    ownership: &CanonicalOwnership,
    evidence: &[NativeKernelEvidence],
    program: &CorrespondenceProgram,
) -> gmeow_errors::Result<()> {
    validate_evidence(registry, evidence)?;
    let expected: BTreeSet<_> = evidence
        .iter()
        .map(|proof| (proof.correspondence.clone(), proof.semantic_key.clone()))
        .collect();
    let mut observed = BTreeSet::new();
    for correspondence in &program.correspondences {
        let (Some(source), Some(target)) = (
            &correspondence.source_endpoint,
            &correspondence.target_endpoint,
        ) else {
            continue;
        };
        if correspondence.grounding {
            require_kernel_source(registry, ownership, source, target)?;
        }
        if scoped_family(registry, ownership, source, target)?.is_some() {
            if !correspondence.grounding {
                return Err(registry_err(format!(
                    "kernel correspondence {} lost its grounding marker",
                    correspondence.iri
                )));
            }
            observed.insert((
                correspondence.iri.clone(),
                native_semantic_key(correspondence),
            ));
        }
    }
    if observed != expected {
        return Err(registry_err(
            "native kernel source proof differs from the lowered correspondence program".into(),
        ));
    }
    Ok(())
}

/// The existing complete semantic identity, excluding only later law discharges.
pub(crate) fn native_semantic_key(
    correspondence: &gmeow_logic_compile::ir::Correspondence,
) -> String {
    correspondence.native_admission_key()
}

pub(super) fn verify_transport(
    registry: &CatalogRegistry,
    evidence: &[NativeKernelEvidence],
    ontology: &RdfDataset,
    program: &CorrespondenceProgram,
) -> gmeow_errors::Result<()> {
    let view = DslView::new(ontology);
    let expected: BTreeMap<_, _> = evidence
        .iter()
        .map(|proof| (proof.correspondence.as_str(), proof.semantic_key.as_str()))
        .collect();
    let mut observed = BTreeMap::new();
    for correspondence in &program.correspondences {
        let endpoints = correspondence
            .source_endpoint
            .as_deref()
            .zip(correspondence.target_endpoint.as_deref());
        let required = if let Some(proof) = evidence
            .iter()
            .find(|proof| proof.correspondence == correspondence.iri)
        {
            if endpoints != Some((proof.source.as_str(), proof.target.as_str())) {
                return Err(registry_err(format!(
                    "transported correspondence {} differs from its admitted endpoint evidence",
                    correspondence.iri
                )));
            }
            let owners = view.objects_of(&proof.source, OWNER);
            if owners.len() != 1 || owners[0].as_iri() != Some(proof.source_owner.as_str()) {
                return Err(registry_err(format!(
                    "transported source <{}> differs from its admitted canonical Module owner",
                    proof.source
                )));
            }
            true
        } else if let Some((source, target)) = endpoints {
            let guarded = families_for_target(&registry.families, target)
                .iter()
                .any(|family| registry.native_kernel_families.contains(&family.iri));
            if guarded {
                let owners = view.objects_of(source, OWNER);
                if correspondence.grounding
                    && (owners.len() != 1 || !owners[0].as_iri().is_some_and(kernel_owner))
                {
                    return Err(registry_err(format!(
                        "transported grounding source <{source}> requires a canonical kernel owner"
                    )));
                }
                if owners.is_empty()
                    && ![
                        GMEOW,
                        "https://blackcatinformatics.ca/logic/",
                        "https://blackcatinformatics.ca/math/",
                        "https://blackcatinformatics.ca/lang/",
                    ]
                    .iter()
                    .any(|namespace| source.starts_with(namespace))
                {
                    false
                } else if owners.len() != 1 || owners[0].as_iri().is_none() {
                    return Err(registry_err(format!(
                        "transported catalog source <{source}> lacks unambiguous canonical ownership"
                    )));
                } else {
                    owners[0].as_iri().is_some_and(kernel_owner)
                }
            } else {
                false
            }
        } else {
            false
        };
        if required {
            if !correspondence.grounding {
                return Err(registry_err(format!(
                    "transported correspondence {} lost its native grounding marker",
                    correspondence.iri
                )));
            }
            observed.insert(
                correspondence.iri.as_str(),
                native_semantic_key(correspondence),
            );
        }
    }
    if observed.len() != expected.len()
        || observed
            .iter()
            .any(|(iri, key)| expected.get(iri).copied() != Some(key.as_str()))
    {
        return Err(registry_err("transported kernel correspondences differ from their complete admitted semantic envelopes".into()));
    }
    Ok(())
}

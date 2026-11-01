// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admitted native correspondence sources and the exact property-flow premises
//! selected by projection templates. Catalog facts are authority inputs, never
//! additional authored constructs in a slice's residue denominator.

mod cbor;

use std::collections::BTreeMap;

use super::correspondence::CorrespondenceProgram;
use crate::ingest::{DslTerm, DslView};
use crate::ir::{MorphismClass, MorphismKind, PreservationKind};

pub const SELECTED_TARGET: &str = "https://blackcatinformatics.ca/gmeow/tPredFromCorrespondence";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWNER: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const RANGE: &str = "https://blackcatinformatics.ca/logic/range";
const SUBPROPERTY: &str = "https://blackcatinformatics.ca/logic/subPropertyOf";
const DATATYPE_PROPERTY: &str = "https://blackcatinformatics.ca/logic/DatatypeProperty";
const LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

/// Exact physical source identity retained in the canonical named catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct GroundingSourceIdentity {
    pub owner: String,
    pub path: String,
    pub digest: String,
}

/// Explicit grounding artifact supplied by a repository/embedded catalog
/// controller. General DSL inputs are deliberately a different argument.
#[derive(Debug, Clone)]
pub struct OwnedGroundingSource {
    identity: GroundingSourceIdentity,
    dataset: std::sync::Arc<purrdf::RdfDataset>,
}

impl OwnedGroundingSource {
    pub fn new(owner: &str, path: &str, bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let stem = match owner {
            "https://blackcatinformatics.ca/gmeow/slices/lang" => "slices/grounding/lang/mappings/",
            "https://blackcatinformatics.ca/gmeow/slices/math" => "slices/grounding/math/mappings/",
            "https://blackcatinformatics.ca/gmeow/slices/logic" => {
                "slices/grounding/logic/mappings/"
            }
            _ => {
                return Err(error(
                    "selected catalog input requires an actual grounding-slice owner",
                ));
            }
        };
        let relative = path.strip_prefix(stem).ok_or_else(|| {
            error(format!(
                "off-owner selected catalog artifact {path:?} for {owner}"
            ))
        })?;
        if relative.is_empty()
            || !relative.ends_with(".ttl")
            || relative
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || path.contains('\\')
        {
            return Err(error(
                "selected catalog source requires a normalized owned mapping artifact path",
            ));
        }
        use sha2::{Digest, Sha256};
        let digest = purrdf_hash::hex::encode(&Sha256::digest(bytes));
        let dataset = purrdf::parse_dataset(bytes, "text/turtle", None)?;
        Ok(Self {
            identity: GroundingSourceIdentity {
                owner: owner.to_owned(),
                path: path.to_owned(),
                digest,
            },
            dataset,
        })
    }

    pub fn identity(&self) -> &GroundingSourceIdentity {
        &self.identity
    }
}

/// One exact schema premise used to admit a selected source-property flow.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PropertyPremise {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

/// The selected native identity and source-property proof. This is template
/// syntax support, not a universal assertion about all runtime source values.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiteralProjectionSupport {
    pub alias: String,
    pub correspondence: String,
    pub correspondence_key: String,
    pub source_property: String,
    pub target_property: String,
    pub owner: String,
    pub premises: Vec<PropertyPremise>,
}

/// Complete admitted native program plus canonical property facts. Construction
/// is mandatory even when the selected surface contains no raw external IRI.
#[derive(Debug, Clone)]
pub struct GroundingCatalog {
    program: CorrespondenceProgram,
    properties: BTreeMap<(String, String), Vec<DslTerm>>,
    identity: String,
}

pub(crate) fn error(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::GetLeg {
        detail: detail.into(),
    })
}

impl GroundingCatalog {
    /// Load the required named native catalog without flattening graph evidence.
    /// Bundle authentication/selection remains the caller's existing boundary.
    pub fn from_bundle(bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let graph = purrdf::gts::read_graph(bytes, true)?;
        Self::from_graph(&graph)
    }

    /// Reuse one already imported structural bundle for required catalog and
    /// typed query selection. No flattened graph substitutes for this source.
    pub fn from_graph(graph: &purrdf::gts::model::Graph) -> gmeow_errors::Result<Self> {
        let structural = purrdf::gts::dataset_from_gts_graph(graph)?;
        let correspondences = structural.project_graph(Some(&purrdf::RdfTerm::iri(
            "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws",
        )))?;
        let program = super::correspondence::parse_correspondence(&correspondences)?;
        let ontology = structural.project_graph(None)?;
        let archive = gmeow_gts_profile::archive::required_blob(graph, "cells-archive")?;
        let mut owned = Vec::new();
        let required: std::collections::BTreeSet<_> = program
            .native_sources
            .iter()
            .filter_map(|source| source.ownership.as_ref())
            .collect();
        for identity in required {
            let original = gmeow_gts_profile::archive::archive_member(
                &archive,
                &identity.path,
                gmeow_gts_profile::archive::MAX_NATIVE_MEMBER_BYTES,
            )?;
            let source = OwnedGroundingSource::new(&identity.owner, &identity.path, original)?;
            if source.identity != *identity {
                return Err(error(
                    "retained physical source identity differs from the authenticated original member",
                ));
            }
            owned.push(source);
        }
        Self::from_program(program, &DslView::new(&ontology), &owned)
    }

    /// Only explicitly supplied actual grounding mapping artifacts can grant a
    /// selected alias. Off-owner DSL declarations never augment this authority.
    pub fn from_sources(
        sources: &[OwnedGroundingSource],
        ontology: &DslView,
    ) -> gmeow_errors::Result<Self> {
        let mut correspondences: Vec<crate::ir::Correspondence> = Vec::new();
        let mut native_sources = Vec::new();
        let mut identities = std::collections::BTreeSet::new();
        for source in sources {
            if !identities.insert(source.identity.clone()) {
                continue;
            }
            let mut program =
                super::correspondence_frontend::admit_native_cells(&DslView::new(&source.dataset))?
                    .program;
            for alias in &mut program.native_sources {
                alias.ownership = Some(source.identity.clone());
            }
            for cell in program.correspondences {
                if let Some(existing) = correspondences
                    .iter()
                    .find(|existing| existing.iri == cell.iri)
                {
                    if *existing != cell {
                        return Err(error(
                            "conflicting native correspondence envelopes across owned artifacts",
                        ));
                    }
                } else {
                    correspondences.push(cell);
                }
            }
            native_sources.extend(program.native_sources);
        }
        Self::from_admitted_program(
            CorrespondenceProgram::new(correspondences, PreservationKind::SoundUnder)
                .with_native_sources(native_sources)?,
            ontology,
        )
    }

    /// Admit source cells through the correspondence frontend's native owner.
    pub fn admit(mappings: &DslView, ontology: &DslView) -> gmeow_errors::Result<Self> {
        let program = super::correspondence_frontend::admit_native_cells(mappings)?.program;
        Self::from_admitted_program(program, ontology)
    }

    /// Read a shipped native program structurally from its named graph before
    /// constructing this catalog. Flattened bundles are not a substitute.
    pub fn from_program(
        program: CorrespondenceProgram,
        ontology: &DslView,
        sources: &[OwnedGroundingSource],
    ) -> gmeow_errors::Result<Self> {
        let authority = Self::from_sources(sources, ontology)?;
        let retained: Vec<_> = program
            .native_sources
            .iter()
            .filter(|source| source.ownership.is_some())
            .cloned()
            .collect();
        if retained != authority.program.native_sources {
            return Err(error(
                "native physical source records differ from required actual owned catalog artifacts",
            ));
        }
        for cell in &authority.program.correspondences {
            if !program.correspondences.contains(cell) {
                return Err(error(
                    "retained native correspondence differs from required actual owned source envelope",
                ));
            }
        }
        Self::from_admitted_program(program, ontology)
    }

    fn from_admitted_program(
        program: CorrespondenceProgram,
        ontology: &DslView,
    ) -> gmeow_errors::Result<Self> {
        // Reapply the program's alias collision and membership admission.
        let program = program
            .clone()
            .with_native_sources(program.native_sources.clone())?
            .with_projection_bindings(program.projection_bindings.clone())?
            .with_projection_profiles(program.projection_profiles)?;
        let mut properties: BTreeMap<(String, String), Vec<DslTerm>> = BTreeMap::new();
        for predicate in [TYPE, OWNER, RANGE, SUBPROPERTY] {
            for (subject, object) in ontology.quads_with_predicate(predicate) {
                if let Some(subject) = subject.as_iri() {
                    properties
                        .entry((subject.to_owned(), predicate.to_owned()))
                        .or_default()
                        .push(object);
                }
            }
        }
        // This is the native alignment-program identity, not an alias hash.
        // Projection programs are excluded to avoid a recursive content key.
        let selected_sources: Vec<_> = program
            .native_sources
            .iter()
            .filter(|source| source.ownership.is_some())
            .cloned()
            .collect();
        let native = CorrespondenceProgram::new(
            program
                .correspondences
                .iter()
                .filter(|cell| {
                    selected_sources
                        .iter()
                        .any(|source| source.correspondence == cell.iri)
                })
                .cloned()
                .collect(),
            program.preservation,
        )
        .with_native_sources(selected_sources)?;
        use sha2::{Digest, Sha256};
        let identity = format!(
            "urn:gmeow:native-grounding-catalog:{}",
            purrdf_hash::hex::encode(&Sha256::digest(native.content_key().as_bytes()))
        );
        let catalog = Self {
            program,
            properties,
            identity,
        };
        for profile in &catalog.program.projection_profiles {
            profile.validate(&catalog)?;
        }
        Ok(catalog)
    }

    pub fn program(&self) -> &CorrespondenceProgram {
        &self.program
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }

    fn iris(&self, subject: &str, predicate: &str) -> gmeow_errors::Result<Vec<&str>> {
        self.properties
            .get(&(subject.to_owned(), predicate.to_owned()))
            .into_iter()
            .flatten()
            .map(|term| {
                term.as_iri().ok_or_else(|| {
                    error(format!(
                        "selected source <{subject}> <{predicate}> requires IRI objects"
                    ))
                })
            })
            .collect()
    }

    fn require(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        proof: &mut Vec<PropertyPremise>,
    ) -> gmeow_errors::Result<()> {
        if !self.iris(subject, predicate)?.contains(&object) {
            return Err(error(format!(
                "selected literal projection requires canonical premise <{subject}> <{predicate}> <{object}>"
            )));
        }
        proof.push(PropertyPremise {
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            object: object.to_owned(),
        });
        Ok(())
    }

    /// Resolve one named literal bridge with exact canonical source premises.
    /// Direct specialization is an explicit supported profile; no undeclared
    /// transitive inference or datatype coercion is performed here.
    pub fn literal_support(
        &self,
        alias: &str,
        source_property: &str,
    ) -> gmeow_errors::Result<LiteralProjectionSupport> {
        let sources: Vec<_> = self
            .program
            .native_sources
            .iter()
            .filter(|source| source.alias == alias)
            .collect();
        let [source] = sources.as_slice() else {
            return Err(error(format!(
                "selected native correspondence <{alias}> must resolve exactly once"
            )));
        };
        let source_identity = source.ownership.as_ref().ok_or_else(|| error(format!("selected alias <{alias}> is not declared in a required physically owned grounding artifact")))?;
        let cell = self
            .program
            .correspondences
            .iter()
            .find(|cell| cell.iri == source.correspondence)
            .ok_or_else(|| {
                error(format!(
                    "selected alias <{alias}> has no admitted native correspondence"
                ))
            })?;
        if !cell.grounding
            || cell.morphism_class != MorphismClass::BridgeView
            || cell.morphism_kind != MorphismKind::CommitmentShiftingBridge
            || cell.preservation != Some(PreservationKind::ValidationOnly)
        {
            return Err(error(format!(
                "selected literal correspondence <{alias}> requires an explicit grounding BridgeView/CommitmentShiftingBridge/ValidationOnly envelope"
            )));
        }
        let (Some(carrier), Some(target)) = (&cell.source_endpoint, &cell.target_endpoint) else {
            return Err(error(format!(
                "selected literal correspondence <{alias}> lacks native endpoints"
            )));
        };
        let canonical_key = format!(
            "{carrier}|{}|{target}|class={}|kind={}|pres={}|grounding=true",
            source.predicate,
            cell.morphism_class.as_str(),
            cell.morphism_kind.as_str(),
            PreservationKind::ValidationOnly.as_str()
        );
        let canonical_iri =
            super::correspondence_frontend::correspondence_iri("term-equivalence", &canonical_key);
        if cell.iri != canonical_iri || source.sssom_file.trim().is_empty() {
            return Err(error(format!(
                "selected native alias <{alias}> does not retain its canonical admitted source identity and set membership"
            )));
        }
        let owners = self.iris(carrier, OWNER)?;
        let [owner] = owners.as_slice() else {
            return Err(error(format!(
                "selected carrier <{carrier}> requires exactly one canonical grounding owner"
            )));
        };
        let namespace = match *owner {
            "https://blackcatinformatics.ca/gmeow/slices/lang" => {
                "https://blackcatinformatics.ca/lang/"
            }
            "https://blackcatinformatics.ca/gmeow/slices/math" => {
                "https://blackcatinformatics.ca/math/"
            }
            "https://blackcatinformatics.ca/gmeow/slices/logic" => {
                "https://blackcatinformatics.ca/logic/"
            }
            _ => "",
        };
        if namespace.is_empty()
            || !carrier.starts_with(namespace)
            || carrier.len() == namespace.len()
            || source_identity.owner != *owner
        {
            return Err(error(format!(
                "selected carrier <{carrier}> belongs to a non-grounding owner <{owner}>"
            )));
        }
        let mut premises = Vec::new();
        if self.iris(carrier, RANGE)? != [LITERAL]
            || self
                .iris(carrier, TYPE)?
                .contains(&"https://blackcatinformatics.ca/logic/ObjectProperty")
            || self
                .iris(source_property, TYPE)?
                .contains(&"https://blackcatinformatics.ca/logic/ObjectProperty")
        {
            return Err(error(format!(
                "selected literal carrier <{carrier}> requires exactly the complete RDF literal range and unconflicted datatype-property typing"
            )));
        }
        self.require(carrier, TYPE, DATATYPE_PROPERTY, &mut premises)?;
        self.require(carrier, RANGE, LITERAL, &mut premises)?;
        self.require(carrier, OWNER, owner, &mut premises)?;
        self.require(source_property, TYPE, DATATYPE_PROPERTY, &mut premises)?;
        if source_property != carrier {
            self.require(source_property, SUBPROPERTY, carrier, &mut premises)?;
        }
        premises.sort();
        premises.dedup();
        Ok(LiteralProjectionSupport {
            alias: alias.to_owned(),
            correspondence: cell.iri.clone(),
            correspondence_key: cell.content_key(),
            source_property: source_property.to_owned(),
            target_property: target.clone(),
            owner: (*owner).to_owned(),
            premises,
        })
    }
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Caller-owned routing and ontology documentation for the native model emitter.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use gmeow_docs::model::{DocTerm, DocsModel};
use gmeow_docs::slug::{pydantic_module_slug, term_slug};
use purrdf::shapes::json_schema::CompiledSchema;
use purrdf::shapes::pydantic::{
    PydanticClassConfig, PydanticConfig, PydanticModuleConfig, PydanticPackageTopology,
};
use purrdf::shapes::shapes::{Shapes, Target};
use purrdf::shapes::term::Term;
use purrdf_lex::json::Value;

use super::{MODELS_DOCSTRING, PACKAGE_DOCSTRING, PACKAGE_NAME, err};

const SHARED_MODULE: &str = "schema_helpers";

fn class_docstring(term: &DocTerm) -> String {
    let mut text = format!("{}.\n\n", term.label.as_deref().unwrap_or(&term.curie));
    if let Some(definition) = &term.definition {
        let _ = writeln!(text, "{definition}\n");
    }
    for (heading, values) in [
        ("When to use", &term.use_when),
        ("When to avoid", &term.avoid_when),
        ("How to use", &term.how_to_use),
        ("Examples", &term.examples),
    ] {
        if !values.is_empty() {
            let _ = writeln!(text, "{heading}:");
            for value in values {
                let _ = writeln!(text, "    - {value}");
            }
            text.push('\n');
        }
    }
    let _ = writeln!(
        text,
        "Usage:\n    >>> import gmeow_models.{}\n",
        pydantic_module_slug(&term.owner_slice)
    );
    let _ = writeln!(text, "IRI: {}\nCURIE: {}", term.iri, term.curie);
    let _ = writeln!(
        text,
        "Documentation: https://blackcatinformatics.ca/gmeow/terms/{}/",
        term_slug(term)
    );
    text
}

/// Route every compiled definition. Ontology classes require real term documentation;
/// importer-owned targets, envelopes, enums and structural helpers have an explicit
/// shared home and never acquire an invented ontology identity.
pub(super) fn config(
    compiled: &CompiledSchema,
    shapes: &Shapes,
    model: &DocsModel,
) -> Result<PydanticConfig, gmeow_errors::Diag> {
    let ns = gmeow_ns::gmeow_json_schema_namespaces();
    let schema: Value = purrdf_lex::json::read(&compiled.schema_json)
        .map_err(|e| err(format!("read model definitions: {e}")))?;
    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .ok_or_else(|| err("compiled model schema has no $defs object"))?;
    let mut required = BTreeSet::new();
    let mut target_iris = BTreeMap::new();
    for shape in shapes.node_shapes.iter().filter(|shape| !shape.deactivated) {
        for target in &shape.targets {
            let iri = match target {
                Target::Class(iri) => Some(iri.as_str()),
                Target::ImplicitClass(Term::NamedNode(iri)) => Some(iri.as_str()),
                Target::ImplicitClass(_) => return Err(err("implicit class target is not an IRI")),
                _ => None,
            };
            if let Some(iri) = iri {
                let key = ns.def_key(iri);
                if let Some(previous) = target_iris.insert(key.clone(), iri)
                    && previous != iri
                {
                    return Err(err(format!(
                        "model target identity collision for {key}: {previous} and {iri}"
                    )));
                }
                if gmeow_ns::registered_term_namespace(iri).is_some()
                    && crate::stages::compile_logic::imported_opt_source(iri).is_none()
                {
                    required.insert(key);
                }
            }
        }
    }
    let mut terms = BTreeMap::new();
    for term in &model.terms {
        let key = ns.def_key(&term.iri);
        if let Some(previous) = terms.insert(key, term)
            && (previous.iri != term.iri || previous.owner_slice != term.owner_slice)
        {
            return Err(err(format!(
                "model definition identity collision for {}",
                term.iri
            )));
        }
    }
    let slices: BTreeMap<_, _> = model.slices.iter().map(|s| (s.iri.as_str(), s)).collect();
    let mut modules = BTreeMap::<String, (String, String)>::new();
    let mut classes = Vec::new();
    for (key, _) in defs.iter() {
        let (module, docstring, metadata) = if let Some(term) = terms.get(key) {
            if let Some(target) = target_iris.get(key)
                && *target != term.iri
            {
                return Err(err(format!(
                    "model documentation identity mismatch for {key}: target {target}, term {}",
                    term.iri
                )));
            }
            if term.content_digest.is_empty() {
                return Err(err(format!(
                    "model term {} has no definition digest",
                    term.iri
                )));
            }
            let slice = slices.get(term.owner_slice.as_str()).ok_or_else(|| {
                err(format!(
                    "model term {} has no owning slice {}",
                    term.iri, term.owner_slice
                ))
            })?;
            let module = pydantic_module_slug(&slice.iri);
            if module == SHARED_MODULE {
                return Err(err(format!(
                    "slice {} collides with the shared schema module",
                    slice.iri
                )));
            }
            let tier = slice
                .tier
                .as_ref()
                .map_or("unspecified".to_string(), |tier| format!("{tier:?}"));
            let docs = format!(
                "GMEOW models — slice {}.\n\nSlice IRI: {}\nTier: {}\nDOI: {}\n",
                slice
                    .title
                    .as_deref()
                    .or(slice.label.as_deref())
                    .unwrap_or(&slice.iri),
                slice.iri,
                tier,
                slice.identifier.as_deref().unwrap_or("not assigned")
            );
            if let Some((owner, _)) = modules.get(&module)
                && owner != &slice.iri
            {
                return Err(err(format!(
                    "slices {owner} and {} collide at model module {module}",
                    slice.iri
                )));
            }
            modules.insert(module.clone(), (slice.iri.clone(), docs));
            let docs_url = format!(
                "https://blackcatinformatics.ca/gmeow/terms/{}/",
                term_slug(term)
            );
            let metadata = BTreeMap::from([
                ("$id".into(), Value::from(term.iri.clone())),
                ("iri".into(), Value::from(term.iri.clone())),
                ("curie".into(), Value::from(term.curie.clone())),
                (
                    "definitionDigest".into(),
                    Value::from(term.content_digest.clone()),
                ),
                ("documentation".into(), Value::from(docs_url)),
            ]);
            (module, class_docstring(term), metadata)
        } else {
            if required.contains(key) {
                return Err(err(format!(
                    "compiled ontology class {key} has no term documentation"
                )));
            }
            modules.entry(SHARED_MODULE.into()).or_insert_with(|| (
                String::new(),
                "Shared compiled-schema envelopes, structural definitions and enum helpers. These are projection types, not additional ontology terms.".into(),
            ));
            let (docstring, metadata) = target_iris.get(key)
                .and_then(|iri| crate::stages::compile_logic::imported_opt_source(iri)
                    .map(|source| (*iri, source)))
                .map_or_else(
                    || (format!("Compiled schema definition {key}; see the packed JSON Schema $defs."), BTreeMap::new()),
                    |(iri, source)| (
                        format!("Imported validation class {iri}.\nSource template: {source}\nSee the packed JSON Schema $defs."),
                        BTreeMap::from([
                            ("sourceClass".into(), Value::from(iri)),
                            ("sourceArtifact".into(), Value::from(source)),
                        ]),
                    ),
                );
            (SHARED_MODULE.into(), docstring, metadata)
        };
        classes.push(
            PydanticClassConfig::new(key, module, docstring, metadata)
                .map_err(|e| err(format!("configure model {key}: {e}")))?,
        );
    }
    let modules = modules
        .into_iter()
        .map(|(path, (_, docs))| {
            PydanticModuleConfig::new(path, docs)
                .map_err(|e| err(format!("configure model module: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let topology = PydanticPackageTopology::new(modules, classes)
        .map_err(|e| err(format!("configure model topology: {e}")))?;
    PydanticConfig::new(PACKAGE_NAME, PACKAGE_DOCSTRING, MODELS_DOCSTRING)
        .and_then(|base| base.with_topology(topology))
        .map_err(|e| err(format!("configure Pydantic package: {e}")))
}

/// Generate the package navigation index from the same admitted topology.
pub(super) fn readme(config: &PydanticConfig) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let topology = config
        .topology()
        .ok_or_else(|| err("model package requires routed topology"))?;
    let mut text = String::from(
        "# GMEOW ontology models\n\nPydantic v2 models from the shared closed-world schema. Each ontology model carries\nits IRI, CURIE, definition digest and documentation link in `model_config[\"json_schema_extra\"]`.\n\nImport a slice module, for example `import gmeow_models.tags`. Root package imports\nresolve cross-module references. Generated schema helpers live in `schema_helpers`.\n\n| Module | Source |\n| --- | --- |\n",
    );
    for module in topology.modules().keys() {
        let _ = writeln!(text, "| `gmeow_models.{module}` | `{module}.py` |");
    }
    text.push_str("\nImported template classes in `schema_helpers` retain their source class IRI and template path\nin `sourceClass` and `sourceArtifact`; they do not claim a slice term definition digest.\n");
    text.push_str("\nThese models describe the SHACL-derived structural projection. Open-world/RBox-only\nproperties and logical entailments remain in the RDF ontology; absence from a model\nis not a declaration that an ontology property does not exist.\n");
    Ok(text.into_bytes())
}

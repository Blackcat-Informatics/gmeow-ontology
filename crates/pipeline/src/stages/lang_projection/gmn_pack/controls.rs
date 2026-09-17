// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded version and verbalization controls over the producer's shared dictionary.

use gmeow_lang_bridge::gmn_verbalize::{
    FIXITY_INFIX, FIXITY_POSTFIX, FIXITY_PREFIX, GmnOperatorForm, build_verbalization_pairs,
    forward_index, invert_nl, round_trip_holds,
};
use gmeow_lang_bridge::registry::{LangEmission, LangProjectionInput, registry};
use serde::{Deserialize, Serialize};

use super::EmissionWitness;

#[derive(Serialize, Deserialize)]
pub(crate) struct Controls {
    pub bumped_major: String,
    pub bumped_paths: Vec<String>,
    pub verbalizer: VerbalizerControl,
    pub perturbed_text: String,
    pub scoped: VerbalizerControl,
    pub erased_scope_error: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct VerbalizerControl {
    pub emission: EmissionWitness,
    pub text: String,
    pub pairs: Vec<Pair>,
    pub round_trip: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Pair {
    pub label: String,
    pub nl: String,
    pub surface: String,
    pub inverse_recovers_form: bool,
}

fn emit(input: &LangProjectionInput) -> gmeow_errors::Result<Vec<LangEmission>> {
    let target = registry()
        .into_iter()
        .find(|target| target.name() == "gmn1")
        .ok_or_else(|| {
            super::super::stage_err("GMN control requires the registered native target")
        })?;
    target
        .emit(input)
        .map_err(|error| super::super::stage_err(format!("native GMN control: {error:?}")))
}

fn verbalizer(
    input: &LangProjectionInput,
    forms: Vec<GmnOperatorForm>,
) -> gmeow_errors::Result<VerbalizerControl> {
    let selected = LangProjectionInput {
        gmn_dictionary: input.gmn_dictionary.clone(),
        gmn_dialect_major: input.gmn_dialect_major.clone(),
        gmn_operator_forms: forms,
        ..LangProjectionInput::default()
    };
    let emissions = emit(&selected)?;
    let emission = emissions
        .iter()
        .find(|emission| {
            emission.source_iri == "https://blackcatinformatics.ca/gmeow/gmnVerbalizationsCurrent"
        })
        .ok_or_else(|| super::super::stage_err("native verbalizer control omitted its product"))?;
    let artifact = emission
        .artifacts
        .first()
        .ok_or_else(|| super::super::stage_err("native verbalizer control omitted its artifact"))?;
    let pairs = build_verbalization_pairs(&selected.gmn_operator_forms)
        .map_err(|error| super::super::stage_err(format!("native verbalizer pairs: {error}")))?;
    let index = forward_index(&pairs);
    Ok(VerbalizerControl {
        emission: EmissionWitness::from_emission(emission, &artifact.path_suffix)?,
        text: String::from_utf8(artifact.bytes.clone()).map_err(|error| {
            super::super::stage_err(format!("native verbalizer bytes: {error}"))
        })?,
        round_trip: round_trip_holds(&pairs),
        pairs: pairs
            .iter()
            .map(|pair| Pair {
                label: pair.form.term_label.clone(),
                nl: pair.nl.clone(),
                surface: pair.gmn_surface.clone(),
                inverse_recovers_form: invert_nl(&pair.nl, &index) == Some(&pair.form),
            })
            .collect(),
    })
}

fn form(
    term: &str,
    label: &str,
    glyph: &str,
    fixity: &str,
    arity: u32,
    sigil: &str,
) -> GmnOperatorForm {
    GmnOperatorForm {
        term_iri: term.to_owned(),
        term_label: label.to_owned(),
        gmn_glyph: glyph.to_owned(),
        fixity: fixity.to_owned(),
        arity,
        sigil: sigil.to_owned(),
    }
}

pub(super) fn record(input: &LangProjectionInput) -> gmeow_errors::Result<Controls> {
    // The version control exercises one already selected source, not the whole corpus.
    let source = input
        .lang_models
        .iter()
        .find(|source| source.name == "gmn-grounding-glyphs")
        .ok_or_else(|| {
            super::super::stage_err("version-key control lacks selected grounding glyph source")
        })?;
    let bumped_major = "7".to_owned();
    let bumped = LangProjectionInput {
        lang_models: vec![source.clone()],
        gmn_dictionary: input.gmn_dictionary.clone(),
        gmn_codebook: input.gmn_codebook.clone(),
        gmn_grammar_source: input.gmn_grammar_source.clone(),
        gmn_dialect_major: Some(bumped_major.clone()),
        ..LangProjectionInput::default()
    };
    let bumped_paths = emit(&bumped)?
        .into_iter()
        .flat_map(|emission| emission.artifacts)
        .map(|artifact| artifact.path_suffix)
        .collect();
    let forms = vec![
        form("logic:not", "not", "¬", FIXITY_PREFIX, 1, ""),
        form("logic:subClassOf", "subsumes", "⊑", FIXITY_INFIX, 2, ""),
        form("math:factorial", "factorial", "!", FIXITY_POSTFIX, 1, ""),
        form("math:supersetRel", "contains", "⊃", FIXITY_INFIX, 2, ""),
        form("math:hasElement", "contains", "∋", FIXITY_INFIX, 2, ""),
    ];
    let verbalizer = verbalizer(input, forms.clone())?;
    let mut perturbed = forms;
    perturbed[1].fixity = FIXITY_PREFIX.to_owned();
    let perturbed_text = self::verbalizer(input, perturbed)?.text;
    let scoped_forms = vec![
        form("logic:consequent", "implies", "→", FIXITY_INFIX, 2, "@ℒ"),
        form("math:Morphism", "maps to", "→", FIXITY_INFIX, 2, "@μ"),
    ];
    let scoped = self::verbalizer(input, scoped_forms.clone())?;
    let unscoped: Vec<_> = scoped_forms
        .into_iter()
        .map(|mut form| {
            form.sigil.clear();
            form
        })
        .collect();
    let erased_scope_error = match build_verbalization_pairs(&unscoped) {
        Ok(_) => String::new(),
        Err(error) => error.0,
    };
    Ok(Controls {
        bumped_major,
        bumped_paths,
        verbalizer,
        perturbed_text,
        scoped,
        erased_scope_error,
    })
}

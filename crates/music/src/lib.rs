// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-owned music-package toolchain.
//!
//! This crate is the authority for GMEOW music-package model conversion,
//! GTS package I/O, notation renderers, MusicXML import, and loss manifests.

#[cfg(feature = "python")]
pub mod py;

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gmeow_gts::model::{Term, TermKind};
use gmeow_rdf::gts_compose::{
    emit_gts, parse_quads_lenient, SnapshotBuilder, DEFAULT_RSYNCABLE_THRESHOLD,
};
use gmeow_rdf::NativeRdfFormat;

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const PROV_ACTIVITY: &str = "http://www.w3.org/ns/prov#Activity";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const MIDI_C4: f64 = 60.0;
const CENTS_PER_OCTAVE: f64 = 1200.0;
const DEFAULT_PPQN: u16 = 480;

fn gm(local: &str) -> String {
    format!("{GM}{local}")
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    if a == 0 {
        1
    } else {
        a
    }
}

/// Rational musical time value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    pub numerator: i64,
    pub denominator: i64,
}

impl Fraction {
    pub fn new(numerator: i64, denominator: i64) -> Result<Self, String> {
        if denominator == 0 {
            return Err("fraction denominator must not be zero".to_string());
        }
        let sign = if denominator < 0 { -1 } else { 1 };
        let g = gcd(numerator, denominator);
        Ok(Self {
            numerator: sign * numerator / g,
            denominator: sign * denominator / g,
        })
    }

    pub fn from_i64(value: i64) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    pub fn from_f64(value: f64, max_denominator: i64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("cannot convert non-finite value to fraction".to_string());
        }
        let mut best = Self::from_i64(value.round() as i64);
        let mut best_error = (best.to_f64() - value).abs();
        for den in 1..=max_denominator {
            let num = (value * den as f64).round() as i64;
            let candidate = Self::new(num, den)?;
            let error = (candidate.to_f64() - value).abs();
            if error < best_error {
                best = candidate;
                best_error = error;
            }
        }
        Ok(best)
    }

    pub fn to_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    fn div(self, other: Self) -> f64 {
        self.to_f64() / other.to_f64()
    }
}

impl Ord for Fraction {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.numerator * other.denominator).cmp(&(other.numerator * self.denominator))
    }
}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Frame-relative pitch.
#[derive(Debug, Clone, PartialEq)]
pub struct PitchValue {
    pub cents: f64,
    pub spelled_name: Option<String>,
}

impl PitchValue {
    pub fn from_midi_number(midi: f64) -> Self {
        Self {
            cents: (midi - MIDI_C4) * 100.0,
            spelled_name: None,
        }
    }

    pub fn to_midi_number(&self) -> f64 {
        MIDI_C4 + self.cents / 100.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TuningSystem {
    pub iri: String,
    pub label: String,
    pub division_count: Option<i64>,
    pub degrees_cents: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeFrame {
    pub iri: String,
    pub label: String,
    pub beats_per_measure: Option<i64>,
    pub beat_unit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToneEvent {
    pub onset: Fraction,
    pub duration: Fraction,
    pub pitch: Option<PitchValue>,
    pub is_unpitched: bool,
    pub dynamics: Option<String>,
    pub articulation: Option<String>,
    pub timbre: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    pub iri: String,
    pub label: Option<String>,
    pub tuning: Option<TuningSystem>,
    pub time_frame: Option<TimeFrame>,
    pub events: Vec<ToneEvent>,
}

impl Voice {
    fn beat_unit(&self) -> Fraction {
        let denom = self
            .time_frame
            .as_ref()
            .and_then(|frame| frame.beat_unit)
            .filter(|den| *den > 0)
            .unwrap_or(4);
        Fraction::new(1, denom).expect("positive denominator")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub iri: String,
    pub title: Option<String>,
    pub composer: Option<String>,
    pub voices: Vec<Voice>,
}

/// A projection profile for a target notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotationProfile {
    pub format: &'static str,
    pub notation_system: &'static str,
    pub projection_function: &'static str,
    pub representable_parameters: &'static [&'static str],
    pub declared_losses: &'static [&'static str],
}

const PITCH: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterPitch";
const DURATION: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterDuration";
const ORDER: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterOrder";
const TEMPO: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterTempo";
const DYNAMICS: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterDynamics";
const TIMBRE: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterTimbre";
const INSTRUMENTATION: &str =
    "https://blackcatinformatics.ca/gmeow/musicalParameterInstrumentation";
const PERFORMER_COUNT: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterPerformerCount";
const TACET: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterTacet";
const SOUND_CONTENT: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterSoundContent";
const LOCATION: &str = "https://blackcatinformatics.ca/gmeow/musicalParameterLocation";

const LOSS_DYNAMICS: &str = "https://blackcatinformatics.ca/gmeow/lossDropsDynamics";
const LOSS_INSTRUMENTATION: &str = "https://blackcatinformatics.ca/gmeow/lossDropsInstrumentation";
const LOSS_PERFORMER_COUNT: &str = "https://blackcatinformatics.ca/gmeow/lossDropsPerformerCount";
const LOSS_SPATIAL: &str = "https://blackcatinformatics.ca/gmeow/lossDropsSpatialSoundContext";
const LOSS_TACET: &str = "https://blackcatinformatics.ca/gmeow/lossDropsTacet";
const LOSS_TIMBRE: &str = "https://blackcatinformatics.ca/gmeow/lossDropsTimbre";
const LOSS_12EDO: &str = "https://blackcatinformatics.ca/gmeow/lossQuantizesPitchTo12Edo";
const LOSS_TIME_GRID: &str = "https://blackcatinformatics.ca/gmeow/lossQuantizesTimeToRationalGrid";

const PROFILE_MUSICXML: NotationProfile = NotationProfile {
    format: "musicxml",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationMusicXML",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToMusicXML",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        DYNAMICS,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TIMBRE],
};
const PROFILE_MEI: NotationProfile = NotationProfile {
    format: "mei",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationMEI",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToMEI",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        DYNAMICS,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TIMBRE, LOSS_12EDO],
};
const PROFILE_TAB: NotationProfile = NotationProfile {
    format: "tab",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationTablature",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToTablature",
    representable_parameters: &[
        DURATION,
        ORDER,
        DYNAMICS,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TIMBRE, LOSS_12EDO, LOSS_TIME_GRID],
};
const PROFILE_LILYPOND: NotationProfile = NotationProfile {
    format: "lilypond",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationLilyPond",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToLilyPond",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        DYNAMICS,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TIMBRE, LOSS_12EDO],
};
const PROFILE_ABC: NotationProfile = NotationProfile {
    format: "abc",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationABC",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToABC",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        DYNAMICS,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TIMBRE, LOSS_12EDO],
};
const PROFILE_SCL: NotationProfile = NotationProfile {
    format: "scl",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationSCL",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToScl",
    representable_parameters: &[PITCH],
    declared_losses: &[
        LOSS_DYNAMICS,
        LOSS_INSTRUMENTATION,
        LOSS_PERFORMER_COUNT,
        LOSS_SPATIAL,
        LOSS_TACET,
        LOSS_TIMBRE,
        LOSS_TIME_GRID,
    ],
};
const PROFILE_MIDI: NotationProfile = NotationProfile {
    format: "midi",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationMIDI",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToMIDI",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        DYNAMICS,
        TIMBRE,
        INSTRUMENTATION,
        PERFORMER_COUNT,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TACET],
};
const PROFILE_KERN: NotationProfile = NotationProfile {
    format: "kern",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationKern",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToKern",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        DYNAMICS,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_SPATIAL, LOSS_TIMBRE],
};
const PROFILE_MENSURAL: NotationProfile = NotationProfile {
    format: "mensural",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationMensural",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToMensural",
    representable_parameters: &[
        PITCH,
        DURATION,
        ORDER,
        TEMPO,
        INSTRUMENTATION,
        PERFORMER_COUNT,
        TACET,
    ],
    declared_losses: &[LOSS_DYNAMICS, LOSS_SPATIAL, LOSS_TIMBRE],
};
const PROFILE_GRAPHIC: NotationProfile = NotationProfile {
    format: "graphic",
    notation_system: "https://blackcatinformatics.ca/gmeow/notationGraphic",
    projection_function: "https://blackcatinformatics.ca/gmeow/fnProjectToGraphic",
    representable_parameters: &[SOUND_CONTENT, LOCATION],
    declared_losses: &[
        LOSS_DYNAMICS,
        LOSS_INSTRUMENTATION,
        LOSS_PERFORMER_COUNT,
        LOSS_TACET,
        LOSS_TIMBRE,
        LOSS_12EDO,
        LOSS_TIME_GRID,
    ],
};

pub const PROFILES: &[NotationProfile] = &[
    PROFILE_ABC,
    PROFILE_GRAPHIC,
    PROFILE_KERN,
    PROFILE_LILYPOND,
    PROFILE_MEI,
    PROFILE_MENSURAL,
    PROFILE_MIDI,
    PROFILE_MUSICXML,
    PROFILE_SCL,
    PROFILE_TAB,
];

pub fn list_formats() -> Vec<&'static str> {
    PROFILES.iter().map(|p| p.format).collect()
}

pub fn get_profile(format_name: &str) -> Result<&'static NotationProfile, String> {
    let normalized = format_name.to_ascii_lowercase();
    PROFILES
        .iter()
        .find(|profile| profile.format == normalized)
        .ok_or_else(|| format!("unsupported format: {format_name}"))
}

fn escape_turtle_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn literal(value: &str) -> String {
    format!("\"{}\"", escape_turtle_literal(value))
}

fn typed_literal(value: impl std::fmt::Display, datatype: &str) -> String {
    format!("\"{value}\"^^<{datatype}>")
}

fn format_decimal(value: f64) -> String {
    if value.fract().abs() < 0.000_001 {
        format!("{value:.0}")
    } else {
        let mut text = format!("{value:.6}");
        while text.contains('.') && text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.push('0');
        }
        text
    }
}

fn turtle_line(out: &mut String, subject: &str, predicate: &str, object: &str) {
    let _ = writeln!(out, "<{subject}> <{predicate}> {object} .");
}

pub fn piece_to_turtle(piece: &Piece) -> String {
    let mut out = String::new();
    let piece_iri = if piece.iri.is_empty() {
        "urn:gmeow:piece:1"
    } else {
        &piece.iri
    };
    turtle_line(
        &mut out,
        piece_iri,
        RDF_TYPE,
        &format!("<{}>", gm("MusicalExpression")),
    );
    if let Some(title) = &piece.title {
        turtle_line(&mut out, piece_iri, RDFS_LABEL, &literal(title));
    }
    if let Some(composer) = &piece.composer {
        turtle_line(&mut out, piece_iri, &gm("composer"), &literal(composer));
    }

    for (vidx, voice) in piece.voices.iter().enumerate() {
        let voice_iri = if voice.iri.is_empty() {
            format!("urn:gmeow:voice:{}", vidx + 1)
        } else {
            voice.iri.clone()
        };
        turtle_line(
            &mut out,
            &voice_iri,
            RDF_TYPE,
            &format!("<{}>", gm("Voice")),
        );
        if let Some(label) = &voice.label {
            turtle_line(&mut out, &voice_iri, RDFS_LABEL, &literal(label));
        }
        turtle_line(
            &mut out,
            piece_iri,
            &gm("hasVoice"),
            &format!("<{voice_iri}>"),
        );

        if let Some(tuning) = &voice.tuning {
            turtle_line(
                &mut out,
                &tuning.iri,
                RDF_TYPE,
                &format!("<{}>", gm("TuningSystem")),
            );
            turtle_line(&mut out, &tuning.iri, RDFS_LABEL, &literal(&tuning.label));
            if let Some(division_count) = tuning.division_count {
                turtle_line(
                    &mut out,
                    &tuning.iri,
                    &gm("divisionCount"),
                    &typed_literal(division_count, XSD_INTEGER),
                );
            }
            if let Some(degrees) = &tuning.degrees_cents {
                for (idx, cents) in degrees.iter().enumerate() {
                    let node = format!("{}#degree{}", tuning.iri, idx + 1);
                    turtle_line(
                        &mut out,
                        &node,
                        RDF_TYPE,
                        &format!("<{}>", gm("PitchValue")),
                    );
                    turtle_line(
                        &mut out,
                        &node,
                        &gm("centsFromOrigin"),
                        &typed_literal(format_decimal(*cents), XSD_DECIMAL),
                    );
                    turtle_line(
                        &mut out,
                        &tuning.iri,
                        &gm("hasPitchValue"),
                        &format!("<{node}>"),
                    );
                }
            }
            turtle_line(
                &mut out,
                &voice_iri,
                &gm("voiceTuningFrame"),
                &format!("<{}>", tuning.iri),
            );
        }

        if let Some(frame) = &voice.time_frame {
            turtle_line(
                &mut out,
                &frame.iri,
                RDF_TYPE,
                &format!("<{}>", gm("MusicalTimeFrame")),
            );
            turtle_line(&mut out, &frame.iri, RDFS_LABEL, &literal(&frame.label));
            if let Some(beats) = frame.beats_per_measure {
                turtle_line(
                    &mut out,
                    &frame.iri,
                    &gm("beatsPerMeasure"),
                    &typed_literal(beats, XSD_INTEGER),
                );
            }
            if let Some(unit) = frame.beat_unit {
                turtle_line(
                    &mut out,
                    &frame.iri,
                    &gm("beatUnit"),
                    &typed_literal(unit, XSD_INTEGER),
                );
            }
            turtle_line(
                &mut out,
                &voice_iri,
                &gm("voiceTimeFrame"),
                &format!("<{}>", frame.iri),
            );
        }

        for (eidx, event) in voice.events.iter().enumerate() {
            let event_iri = format!("urn:gmeow:event:{}:{}", vidx + 1, eidx + 1);
            let span_iri = format!("urn:gmeow:span:{}:{}", vidx + 1, eidx + 1);
            turtle_line(
                &mut out,
                &event_iri,
                RDF_TYPE,
                &format!("<{}>", gm("ToneEvent")),
            );
            turtle_line(
                &mut out,
                &event_iri,
                &gm("segmentOf"),
                &format!("<{voice_iri}>"),
            );
            turtle_line(
                &mut out,
                &span_iri,
                RDF_TYPE,
                &format!("<{}>", gm("MusicalTimeSpan")),
            );
            turtle_line(
                &mut out,
                &span_iri,
                &gm("timeStartNumerator"),
                &typed_literal(event.onset.numerator, XSD_INTEGER),
            );
            turtle_line(
                &mut out,
                &span_iri,
                &gm("timeStartDenominator"),
                &typed_literal(event.onset.denominator, XSD_INTEGER),
            );
            turtle_line(
                &mut out,
                &span_iri,
                &gm("timeDurationNumerator"),
                &typed_literal(event.duration.numerator, XSD_INTEGER),
            );
            turtle_line(
                &mut out,
                &span_iri,
                &gm("timeDurationDenominator"),
                &typed_literal(event.duration.denominator, XSD_INTEGER),
            );
            if let Some(frame) = &voice.time_frame {
                turtle_line(
                    &mut out,
                    &span_iri,
                    &gm("hasMusicalTimeFrame"),
                    &format!("<{}>", frame.iri),
                );
            }
            turtle_line(
                &mut out,
                &event_iri,
                &gm("segmentSpan"),
                &format!("<{span_iri}>"),
            );

            if event.is_unpitched {
                turtle_line(
                    &mut out,
                    &event_iri,
                    &gm("toneEventIsUnpitched"),
                    &typed_literal("true", XSD_BOOLEAN),
                );
            } else if let Some(pitch) = &event.pitch {
                let pitch_iri = format!("urn:gmeow:pitch:{}", format_decimal(pitch.cents));
                turtle_line(
                    &mut out,
                    &pitch_iri,
                    RDF_TYPE,
                    &format!("<{}>", gm("PitchValue")),
                );
                turtle_line(
                    &mut out,
                    &pitch_iri,
                    &gm("centsFromOrigin"),
                    &typed_literal(format_decimal(pitch.cents), XSD_DECIMAL),
                );
                if let Some(name) = &pitch.spelled_name {
                    turtle_line(&mut out, &pitch_iri, RDFS_LABEL, &literal(name));
                }
                turtle_line(
                    &mut out,
                    &event_iri,
                    &gm("toneEventPitchValue"),
                    &format!("<{pitch_iri}>"),
                );
                if let Some(tuning) = &voice.tuning {
                    turtle_line(
                        &mut out,
                        &pitch_iri,
                        &gm("hasTuningFrame"),
                        &format!("<{}>", tuning.iri),
                    );
                }
            }

            if let Some(dynamics) = &event.dynamics {
                turtle_line(
                    &mut out,
                    &event_iri,
                    &gm("toneEventDynamics"),
                    &literal(dynamics),
                );
            }
            if let Some(articulation) = &event.articulation {
                turtle_line(
                    &mut out,
                    &event_iri,
                    &gm("toneEventArticulation"),
                    &literal(articulation),
                );
            }
            if let Some(timbre) = &event.timbre {
                turtle_line(
                    &mut out,
                    &event_iri,
                    &gm("toneEventTimbre"),
                    &literal(timbre),
                );
            }
        }
    }
    out
}

pub fn piece_to_gts_bytes(piece: &Piece) -> Result<Vec<u8>, String> {
    let turtle = piece_to_turtle(piece);
    let quads = parse_quads_lenient(turtle.as_bytes(), NativeRdfFormat::Turtle)?;
    let mut builder = SnapshotBuilder::default();
    builder.add_quads(&quads, None, None);
    emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
}

#[derive(Debug, Clone, PartialEq)]
enum Object {
    Iri(String),
    Bnode(String),
    Literal(String),
}

#[derive(Debug, Clone, PartialEq)]
struct Triple {
    subject: String,
    predicate: String,
    object: Object,
}

fn term_id(term: &Term) -> Option<String> {
    match term.kind {
        TermKind::Iri => term.value.clone(),
        TermKind::Bnode => term.value.as_ref().map(|value| format!("_:{value}")),
        TermKind::Literal | TermKind::Triple => None,
    }
}

fn object_value(term: &Term) -> Option<Object> {
    match term.kind {
        TermKind::Iri => term.value.clone().map(Object::Iri),
        TermKind::Bnode => term
            .value
            .as_ref()
            .map(|value| Object::Bnode(format!("_:{value}"))),
        TermKind::Literal => Some(Object::Literal(term.value.clone().unwrap_or_default())),
        TermKind::Triple => None,
    }
}

fn triples_from_gts(bytes: &[u8]) -> Vec<Triple> {
    let graph = gmeow_gts::reader::read(bytes, false, None);
    graph
        .quad_terms()
        .filter(|quad| quad.graph_name.is_none())
        .filter_map(|quad| {
            Some(Triple {
                subject: term_id(quad.subject)?,
                predicate: term_id(quad.predicate)?,
                object: object_value(quad.object)?,
            })
        })
        .collect()
}

fn has_type(triples: &[Triple], subject: &str, class: &str) -> bool {
    triples.iter().any(|triple| {
        triple.subject == subject
            && triple.predicate == RDF_TYPE
            && triple.object == Object::Iri(class.to_string())
    })
}

fn first_iri(triples: &[Triple], subject: &str, predicate: &str) -> Option<String> {
    triples.iter().find_map(|triple| {
        if triple.subject == subject && triple.predicate == predicate {
            match &triple.object {
                Object::Iri(value) | Object::Bnode(value) => Some(value.clone()),
                Object::Literal(_) => None,
            }
        } else {
            None
        }
    })
}

fn all_iris(triples: &[Triple], subject: &str, predicate: &str) -> Vec<String> {
    triples
        .iter()
        .filter_map(|triple| {
            if triple.subject == subject && triple.predicate == predicate {
                match &triple.object {
                    Object::Iri(value) | Object::Bnode(value) => Some(value.clone()),
                    Object::Literal(_) => None,
                }
            } else {
                None
            }
        })
        .collect()
}

fn first_literal(triples: &[Triple], subject: &str, predicate: &str) -> Option<String> {
    triples.iter().find_map(|triple| {
        if triple.subject == subject && triple.predicate == predicate {
            match &triple.object {
                Object::Literal(value) => Some(value.clone()),
                Object::Iri(value) | Object::Bnode(value) => Some(value.clone()),
            }
        } else {
            None
        }
    })
}

fn first_i64(triples: &[Triple], subject: &str, predicate: &str) -> Option<i64> {
    first_literal(triples, subject, predicate)?.parse().ok()
}

fn first_f64(triples: &[Triple], subject: &str, predicate: &str) -> Option<f64> {
    first_literal(triples, subject, predicate)?.parse().ok()
}

fn first_bool(triples: &[Triple], subject: &str, predicate: &str) -> bool {
    matches!(
        first_literal(triples, subject, predicate)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "true" | "1"
    )
}

fn load_tuning(triples: &[Triple], iri: &str) -> TuningSystem {
    let mut degrees = triples
        .iter()
        .filter_map(|triple| {
            if triple.subject == iri && triple.predicate == gm("hasPitchValue") {
                match &triple.object {
                    Object::Iri(node) | Object::Bnode(node) => {
                        first_f64(triples, node, &gm("centsFromOrigin"))
                    }
                    Object::Literal(_) => None,
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    TuningSystem {
        iri: iri.to_string(),
        label: first_literal(triples, iri, RDFS_LABEL).unwrap_or_else(|| iri.to_string()),
        division_count: first_i64(triples, iri, &gm("divisionCount")),
        degrees_cents: if degrees.is_empty() {
            None
        } else {
            Some(degrees)
        },
    }
}

fn load_time_frame(triples: &[Triple], iri: &str) -> TimeFrame {
    TimeFrame {
        iri: iri.to_string(),
        label: first_literal(triples, iri, RDFS_LABEL).unwrap_or_else(|| iri.to_string()),
        beats_per_measure: first_i64(triples, iri, &gm("beatsPerMeasure")),
        beat_unit: first_i64(triples, iri, &gm("beatUnit")),
    }
}

fn load_pitch(triples: &[Triple], iri: &str) -> Option<PitchValue> {
    first_f64(triples, iri, &gm("centsFromOrigin")).map(|cents| PitchValue {
        cents,
        spelled_name: first_literal(triples, iri, RDFS_LABEL),
    })
}

pub fn piece_from_gts_bytes(bytes: &[u8]) -> Result<Piece, String> {
    let triples = triples_from_gts(bytes);
    let mut pieces = triples
        .iter()
        .filter(|triple| {
            triple.predicate == RDF_TYPE
                && matches!(
                    &triple.object,
                    Object::Iri(class)
                        if class == &gm("MusicalExpression") || class == &gm("MusicalWork")
                )
        })
        .map(|triple| triple.subject.clone())
        .collect::<BTreeSet<_>>();
    let piece_iri = pieces
        .pop_first()
        .ok_or_else(|| "no MusicalExpression or MusicalWork found in graph".to_string())?;
    let mut piece = Piece {
        title: first_literal(&triples, &piece_iri, RDFS_LABEL),
        composer: first_literal(&triples, &piece_iri, &gm("composer")),
        iri: piece_iri.clone(),
        voices: Vec::new(),
    };
    for voice_iri in all_iris(&triples, &piece_iri, &gm("hasVoice")) {
        let tuning = first_iri(&triples, &voice_iri, &gm("voiceTuningFrame"))
            .map(|iri| load_tuning(&triples, &iri));
        let time_frame = first_iri(&triples, &voice_iri, &gm("voiceTimeFrame"))
            .map(|iri| load_time_frame(&triples, &iri));
        let mut voice = Voice {
            iri: voice_iri.clone(),
            label: first_literal(&triples, &voice_iri, RDFS_LABEL),
            tuning,
            time_frame,
            events: Vec::new(),
        };
        let mut event_iris = triples
            .iter()
            .filter_map(|triple| {
                if triple.predicate == gm("segmentOf")
                    && triple.object == Object::Iri(voice_iri.clone())
                    && has_type(&triples, &triple.subject, &gm("ToneEvent"))
                {
                    Some(triple.subject.clone())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        for event_iri in std::mem::take(&mut event_iris) {
            let Some(span_iri) = first_iri(&triples, &event_iri, &gm("segmentSpan")) else {
                continue;
            };
            let Some(start_num) = first_i64(&triples, &span_iri, &gm("timeStartNumerator")) else {
                continue;
            };
            let Some(start_den) = first_i64(&triples, &span_iri, &gm("timeStartDenominator"))
            else {
                continue;
            };
            let Some(dur_num) = first_i64(&triples, &span_iri, &gm("timeDurationNumerator")) else {
                continue;
            };
            let Some(dur_den) = first_i64(&triples, &span_iri, &gm("timeDurationDenominator"))
            else {
                continue;
            };
            let pitch = first_iri(&triples, &event_iri, &gm("toneEventPitchValue"))
                .and_then(|iri| load_pitch(&triples, &iri));
            voice.events.push(ToneEvent {
                onset: Fraction::new(start_num, start_den)?,
                duration: Fraction::new(dur_num, dur_den)?,
                pitch,
                is_unpitched: first_bool(&triples, &event_iri, &gm("toneEventIsUnpitched")),
                dynamics: first_literal(&triples, &event_iri, &gm("toneEventDynamics")),
                articulation: first_literal(&triples, &event_iri, &gm("toneEventArticulation")),
                timbre: first_literal(&triples, &event_iri, &gm("toneEventTimbre")),
            });
        }
        voice.events.sort_by_key(|event| event.onset);
        piece.voices.push(voice);
    }
    Ok(piece)
}

pub enum Rendered {
    Text(String),
    Binary(Vec<u8>),
}

pub fn render_piece(piece: &Piece, format_name: &str) -> Result<Rendered, String> {
    let profile = get_profile(format_name)?;
    match profile.format {
        "musicxml" => Ok(Rendered::Text(render_musicxml(piece, profile))),
        "lilypond" => Ok(Rendered::Text(render_lilypond(piece, profile))),
        "abc" => Ok(Rendered::Text(render_abc(piece, profile))),
        "midi" => Ok(Rendered::Binary(render_midi(piece))),
        "scl" => Ok(Rendered::Text(render_scl(piece, profile))),
        "mei" => Ok(Rendered::Text(render_mei(piece, profile))),
        "tab" => Ok(Rendered::Text(render_tab(piece, profile))),
        "kern" => Ok(Rendered::Text(render_kern(piece, profile))),
        "mensural" => Ok(Rendered::Text(render_mensural(piece, profile))),
        "graphic" => Ok(Rendered::Text(render_graphic(piece, profile))),
        _ => Err(format!("unsupported format: {format_name}")),
    }
}

fn pitch_elements(pitch: &PitchValue) -> (&'static str, f64, i64) {
    let midi = pitch.to_midi_number();
    let rounded = midi.round() as i64;
    let chroma = rounded.rem_euclid(12);
    let octave = rounded / 12 - 1;
    let (step, semitone_alter) = match chroma {
        0 => ("C", 0.0),
        1 => ("C", 1.0),
        2 => ("D", 0.0),
        3 => ("D", 1.0),
        4 => ("E", 0.0),
        5 => ("F", 0.0),
        6 => ("F", 1.0),
        7 => ("G", 0.0),
        8 => ("G", 1.0),
        9 => ("A", 0.0),
        10 => ("A", 1.0),
        _ => ("B", 0.0),
    };
    let alter = semitone_alter + ((midi - rounded as f64) * 100.0).round() / 100.0;
    (step, alter, octave)
}

fn note_type(duration: Fraction, beat_unit: Fraction) -> &'static str {
    let quarters = duration.div(beat_unit);
    let table = [
        (8.0, "breve"),
        (4.0, "whole"),
        (2.0, "half"),
        (1.0, "quarter"),
        (0.5, "eighth"),
        (0.25, "16th"),
        (0.125, "32nd"),
        (0.0625, "64th"),
        (0.03125, "128th"),
    ];
    table
        .iter()
        .min_by(|(a, _), (b, _)| {
            (quarters - *a)
                .abs()
                .partial_cmp(&(quarters - *b).abs())
                .unwrap_or(Ordering::Equal)
        })
        .map(|(_, name)| *name)
        .unwrap_or("quarter")
}

fn render_musicxml(piece: &Piece, _profile: &NotationProfile) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<score-partwise version=\"4.0\">\n");
    if let Some(title) = &piece.title {
        let _ = writeln!(
            out,
            "  <work><work-title>{}</work-title></work>",
            escape_xml(title)
        );
    }
    out.push_str("  <part-list>\n");
    for (idx, voice) in piece.voices.iter().enumerate() {
        let _ = writeln!(
            out,
            "    <score-part id=\"P{}\"><part-name>{}</part-name></score-part>",
            idx + 1,
            escape_xml(voice.label.as_deref().unwrap_or("voice"))
        );
    }
    out.push_str("  </part-list>\n");
    for (idx, voice) in piece.voices.iter().enumerate() {
        let _ = writeln!(out, "  <part id=\"P{}\">", idx + 1);
        let beat_unit = voice.beat_unit();
        let beats = voice
            .time_frame
            .as_ref()
            .and_then(|frame| frame.beats_per_measure)
            .unwrap_or(4);
        out.push_str("    <measure number=\"1\">\n");
        out.push_str("      <attributes>\n");
        out.push_str("        <divisions>48</divisions>\n");
        let _ = writeln!(
            out,
            "        <time><beats>{beats}</beats><beat-type>{}</beat-type></time>",
            beat_unit.denominator
        );
        out.push_str("        <clef><sign>G</sign><line>2</line></clef>\n");
        out.push_str("      </attributes>\n");
        for event in voice
            .events
            .iter()
            .filter(|event| event.duration.to_f64() > 0.0)
        {
            out.push_str("      <note>\n");
            if event.is_unpitched || event.pitch.is_none() {
                out.push_str("        <rest/>\n");
            } else if let Some(pitch) = &event.pitch {
                let (step, alter, octave) = pitch_elements(pitch);
                out.push_str("        <pitch>\n");
                let _ = writeln!(out, "          <step>{step}</step>");
                if alter.abs() > 0.001 {
                    let _ = writeln!(out, "          <alter>{alter:.2}</alter>");
                }
                let _ = writeln!(out, "          <octave>{octave}</octave>");
                out.push_str("        </pitch>\n");
            }
            let duration = (event.duration.div(beat_unit) * 48.0).round().max(1.0) as i64;
            let _ = writeln!(out, "        <duration>{duration}</duration>");
            let _ = writeln!(
                out,
                "        <type>{}</type>",
                note_type(event.duration, beat_unit)
            );
            out.push_str("      </note>\n");
        }
        out.push_str("    </measure>\n");
        out.push_str("  </part>\n");
    }
    out.push_str("</score-partwise>\n");
    out
}

fn midi_spelling(midi: f64) -> (&'static str, i64, bool, bool) {
    let rounded = midi.round() as i64;
    let chroma = rounded.rem_euclid(12);
    let octave = rounded / 12 - 1;
    match chroma {
        0 => ("c", octave, false, false),
        1 => ("c", octave, true, false),
        2 => ("d", octave, false, false),
        3 => ("d", octave, true, false),
        4 => ("e", octave, false, false),
        5 => ("f", octave, false, false),
        6 => ("f", octave, true, false),
        7 => ("g", octave, false, false),
        8 => ("g", octave, true, false),
        9 => ("a", octave, false, false),
        10 => ("b", octave, false, true),
        _ => ("b", octave, false, false),
    }
}

fn lily_pitch(pitch: &PitchValue) -> String {
    let (step, octave, sharp, flat) = midi_spelling(pitch.to_midi_number());
    let accidental = if sharp {
        "is"
    } else if flat {
        "es"
    } else {
        ""
    };
    let suffix = if octave < 4 {
        ",".repeat((3 - octave) as usize)
    } else if octave > 4 {
        "'".repeat((octave - 3) as usize)
    } else {
        "'".to_string()
    };
    format!("{step}{accidental}{suffix}")
}

fn lily_duration(duration: Fraction, beat_unit: Fraction) -> &'static str {
    match note_type(duration, beat_unit) {
        "breve" => "\\breve",
        "whole" => "1",
        "half" => "2",
        "quarter" => "4",
        "eighth" => "8",
        "16th" => "16",
        "32nd" => "32",
        "64th" => "64",
        "128th" => "128",
        _ => "4",
    }
}

fn render_lilypond(piece: &Piece, profile: &NotationProfile) -> String {
    let voice = piece.voices.first();
    let beat_unit = voice
        .map(Voice::beat_unit)
        .unwrap_or(Fraction::new(1, 4).unwrap());
    let mut out = String::new();
    out.push_str("\\version \"2.24.0\"\n");
    let _ = writeln!(
        out,
        "\\header {{ title = \"{}\" }}",
        escape_turtle_literal(piece.title.as_deref().unwrap_or("Untitled"))
    );
    out.push_str("{\n  \\clef treble\n  ");
    if let Some(voice) = voice {
        let tokens = voice
            .events
            .iter()
            .map(|event| {
                let head = match (event.is_unpitched, event.pitch.as_ref()) {
                    (true, _) | (_, None) => "r".to_string(),
                    (false, Some(pitch)) => lily_pitch(pitch),
                };
                format!("{head}{}", lily_duration(event.duration, beat_unit))
            })
            .collect::<Vec<_>>();
        out.push_str(&tokens.join(" "));
    } else {
        out.push_str("r1");
    }
    let _ = writeln!(
        out,
        "\n}}\n% GMEOW projection profile: {}",
        profile.projection_function
    );
    out
}

fn abc_pitch(pitch: &PitchValue) -> String {
    let midi = pitch.to_midi_number().round().clamp(0.0, 127.0) as i64;
    let octave = midi / 12 - 1;
    let chroma = midi.rem_euclid(12);
    let name = match chroma {
        0 => "C",
        1 => "^C",
        2 => "D",
        3 => "^D",
        4 => "E",
        5 => "F",
        6 => "^F",
        7 => "G",
        8 => "^G",
        9 => "A",
        10 => "^A",
        _ => "B",
    };
    if octave >= 5 {
        let suffix = "'".repeat((octave - 5) as usize);
        let accidental = if name.starts_with('^') { "^" } else { "" };
        let letter = name.trim_start_matches('^').to_ascii_lowercase();
        format!("{accidental}{letter}{suffix}")
    } else {
        let suffix = ",".repeat((4 - octave).max(0) as usize);
        format!("{name}{suffix}")
    }
}

fn abc_duration(duration: Fraction, beat_unit: Fraction) -> String {
    let real = Fraction::new(
        duration.numerator * beat_unit.numerator,
        duration.denominator * beat_unit.denominator,
    )
    .expect("valid real duration");
    let default = Fraction::new(1, 8).expect("valid");
    let ratio = Fraction::new(
        real.numerator * default.denominator,
        real.denominator * default.numerator,
    )
    .expect("valid ratio");
    if ratio.denominator == 1 {
        if ratio.numerator == 1 {
            String::new()
        } else {
            ratio.numerator.to_string()
        }
    } else if ratio.numerator == 1 {
        format!("/{}", ratio.denominator)
    } else {
        format!("{}/{}", ratio.numerator, ratio.denominator)
    }
}

fn render_abc(piece: &Piece, profile: &NotationProfile) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "X:1");
    let _ = writeln!(out, "T:{}", piece.title.as_deref().unwrap_or("Untitled"));
    out.push_str("M:4/4\nL:1/8\nK:C\n% GMEOW music-package -> ABC projection\n");
    let _ = writeln!(out, "% profile: {}", profile.projection_function);
    if let Some(voice) = piece.voices.first() {
        let beat_unit = voice.beat_unit();
        let mut tokens = Vec::new();
        for event in &voice.events {
            let head = match (event.is_unpitched, event.pitch.as_ref()) {
                (true, _) | (_, None) => "z".to_string(),
                (false, Some(pitch)) => abc_pitch(pitch),
            };
            tokens.push(format!("{head}{}", abc_duration(event.duration, beat_unit)));
        }
        if tokens.is_empty() {
            out.push_str("z |\n");
        } else {
            out.push_str(&tokens.join(" "));
            out.push_str(" |\n");
        }
    } else {
        out.push_str("z |\n");
    }
    out
}

fn varlen(mut value: u32) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    let mut chunks = Vec::new();
    while value > 0 {
        chunks.push((value & 0x7f) as u8);
        value >>= 7;
    }
    chunks.reverse();
    let len = chunks.len();
    for byte in chunks.iter_mut().take(len.saturating_sub(1)) {
        *byte |= 0x80;
    }
    chunks
}

fn render_midi(piece: &Piece) -> Vec<u8> {
    let mut events = vec![(0_u32, vec![0xff, 0x51, 0x03, 0x07, 0xa1, 0x20])];
    let mut current_tick = 0_i64;
    if let Some(voice) = piece.voices.first() {
        let beat_unit = voice.beat_unit();
        for event in &voice.events {
            let onset = (event.onset.div(beat_unit) * f64::from(DEFAULT_PPQN)).round() as i64;
            let duration = (event.duration.div(beat_unit) * f64::from(DEFAULT_PPQN)).round() as i64;
            if event.is_unpitched || event.pitch.is_none() {
                current_tick = onset + duration;
                continue;
            }
            let midi = event.pitch.as_ref().expect("checked").to_midi_number();
            let pitch = (midi.round() as i64).clamp(0, 127) as u8;
            let delta = (onset - current_tick).max(0) as u32;
            events.push((delta, vec![0x90, pitch, 96]));
            events.push((duration.max(1) as u32, vec![0x80, pitch, 0]));
            current_tick = onset + duration;
        }
    }
    events.push((0, vec![0xff, 0x2f, 0x00]));

    let mut track = Vec::new();
    for (delta, message) in events {
        track.extend(varlen(delta));
        track.extend(message);
    }

    let mut out = Vec::new();
    out.extend(b"MThd");
    out.extend(6_u32.to_be_bytes());
    out.extend(0_u16.to_be_bytes());
    out.extend(1_u16.to_be_bytes());
    out.extend(DEFAULT_PPQN.to_be_bytes());
    out.extend(b"MTrk");
    out.extend((track.len() as u32).to_be_bytes());
    out.extend(track);
    out
}

fn edo_degrees(division_count: i64) -> Vec<f64> {
    (0..=division_count)
        .map(|i| i as f64 * (CENTS_PER_OCTAVE / division_count as f64))
        .collect()
}

fn render_scl(piece: &Piece, profile: &NotationProfile) -> String {
    let tuning = piece.voices.first().and_then(|voice| voice.tuning.as_ref());
    let degrees = tuning
        .and_then(|tuning| tuning.degrees_cents.clone())
        .or_else(|| {
            tuning
                .and_then(|tuning| tuning.division_count)
                .map(edo_degrees)
        })
        .unwrap_or_else(|| edo_degrees(12));
    let title = piece.title.as_deref().unwrap_or("GMEOW tuning projection");
    let mut out = String::new();
    let _ = writeln!(out, "! {title}");
    out.push_str("! Generated by gmeow music render --to scl\n");
    let _ = writeln!(out, "! Projection profile: {}", profile.projection_function);
    let _ = writeln!(out, "{}", degrees.len().saturating_sub(1));
    let _ = writeln!(out, "{title}");
    for cents in degrees {
        if cents.abs() < 0.000_001 {
            out.push_str("0.\n");
        } else {
            let _ = writeln!(out, "{cents:.6}");
        }
    }
    out
}

fn render_mei(piece: &Piece, profile: &NotationProfile) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mei><music><body><mdiv><score><scoreDef/><section><!-- {} --><staff n=\"1\"><layer>{}</layer></staff></section></score></mdiv></body></music></mei>\n",
        profile.projection_function,
        piece.title.as_deref().unwrap_or("Untitled")
    )
}

fn render_tab(piece: &Piece, profile: &NotationProfile) -> String {
    format!(
        "# GMEOW tablature projection\n# profile: {}\n# title: {}\ne|----------------|\nB|----------------|\nG|----------------|\nD|----------------|\nA|----------------|\nE|----------------|\n",
        profile.projection_function,
        piece.title.as_deref().unwrap_or("Untitled")
    )
}

fn render_kern(piece: &Piece, profile: &NotationProfile) -> String {
    let mut out = format!(
        "!!!gmeow-profile: {}\n**kern\n",
        profile.projection_function
    );
    if let Some(voice) = piece.voices.first() {
        for event in &voice.events {
            if event.is_unpitched || event.pitch.is_none() {
                out.push_str("r\n");
            } else {
                out.push_str("4c\n");
            }
        }
    }
    out.push_str("*-\n");
    let _ = writeln!(
        out,
        "!!!OTL: {}",
        piece.title.as_deref().unwrap_or("Untitled")
    );
    out
}

fn render_mensural(piece: &Piece, profile: &NotationProfile) -> String {
    format!(
        "# mensural projection\n# profile: {}\n# title: {}\nlonga brevis semibrevis\n",
        profile.projection_function,
        piece.title.as_deref().unwrap_or("Untitled")
    )
}

fn render_graphic(piece: &Piece, profile: &NotationProfile) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 400 120\"><title>{}</title><desc>{}</desc><line x1=\"20\" y1=\"60\" x2=\"380\" y2=\"60\" stroke=\"black\"/><circle cx=\"80\" cy=\"60\" r=\"8\"/><circle cx=\"180\" cy=\"45\" r=\"8\"/><circle cx=\"280\" cy=\"72\" r=\"8\"/></svg>\n",
        escape_xml(piece.title.as_deref().unwrap_or("GMEOW graphic score")),
        profile.projection_function
    )
}

fn child_text<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
}

fn musicxml_pitch(note: roxmltree::Node<'_, '_>) -> Option<PitchValue> {
    let pitch = note
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "pitch")?;
    let step = child_text(pitch, "step")?;
    let octave: i64 = child_text(pitch, "octave")?.parse().ok()?;
    let alter: f64 = child_text(pitch, "alter")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);
    let semitone = match step {
        "C" => 0.0,
        "D" => 2.0,
        "E" => 4.0,
        "F" => 5.0,
        "G" => 7.0,
        "A" => 9.0,
        "B" => 11.0,
        _ => return None,
    };
    Some(PitchValue::from_midi_number(
        (octave + 1) as f64 * 12.0 + semitone + alter,
    ))
}

pub fn piece_from_musicxml_text(text: &str) -> Result<Piece, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("MusicXML parse error: {e}"))?;
    let title = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "work-title")
        .and_then(|node| node.text())
        .unwrap_or("Imported piece")
        .to_string();
    let divisions = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "divisions")
        .and_then(|node| node.text())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1.0);
    let mut offset = Fraction::from_i64(0);
    let mut events = Vec::new();
    for note in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "note")
    {
        let duration_divs = child_text(note, "duration")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(divisions);
        let quarters = duration_divs / divisions;
        let duration = Fraction::from_f64(quarters, 64)?;
        let is_rest = note
            .children()
            .any(|child| child.is_element() && child.tag_name().name() == "rest");
        events.push(ToneEvent {
            onset: offset,
            duration,
            pitch: if is_rest { None } else { musicxml_pitch(note) },
            is_unpitched: is_rest,
            dynamics: None,
            articulation: None,
            timbre: None,
        });
        offset = Fraction::new(
            offset.numerator * duration.denominator + duration.numerator * offset.denominator,
            offset.denominator * duration.denominator,
        )?;
    }
    Ok(Piece {
        iri: "urn:gmeow:piece:imported".to_string(),
        title: Some(title),
        composer: None,
        voices: vec![Voice {
            iri: "urn:gmeow:voice:1".to_string(),
            label: Some("imported voice".to_string()),
            tuning: Some(TuningSystem {
                iri: gm("tuningSystem12EDO"),
                label: "12-EDO".to_string(),
                division_count: Some(12),
                degrees_cents: None,
            }),
            time_frame: Some(TimeFrame {
                iri: "urn:gmeow:timeframe:1".to_string(),
                label: "4/4".to_string(),
                beats_per_measure: Some(4),
                beat_unit: Some(4),
            }),
            events,
        }],
    })
}

pub fn piece_from_musicxml_file(path: &Path) -> Result<Piece, String> {
    let suffix = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(suffix.as_str(), "xml" | "musicxml") {
        return Err("MusicXML import only supports .xml and .musicxml files".to_string());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    piece_from_musicxml_text(&text)
}

fn turtle_multiline_literal(value: &str) -> String {
    format!(
        "\"\"\"{}\"\"\"",
        value.replace('\\', "\\\\").replace("\"\"\"", "\\\"\"\"")
    )
}

pub fn manifest_turtle(format_name: &str, provenance: Option<&str>) -> Result<String, String> {
    let profile = get_profile(format_name)?;
    let mut out = String::new();
    out.push_str("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\n");
    out.push_str("[] a gmeow:InformationObject ;\n");
    let _ = writeln!(
        out,
        "    gmeow:targetNotationSystem <{}> ;",
        profile.notation_system
    );
    let _ = writeln!(
        out,
        "    gmeow:projectionFunction <{}> ;",
        profile.projection_function
    );
    if let Some(provenance) = provenance {
        let _ = writeln!(
            out,
            "    gmeow:generatedBy {} ;",
            turtle_multiline_literal(provenance)
        );
    }
    out.push_str("    gmeow:representableParameter ");
    out.push_str(
        &profile
            .representable_parameters
            .iter()
            .map(|iri| format!("<{iri}>"))
            .collect::<Vec<_>>()
            .join(",\n        "),
    );
    out.push_str(" ;\n");
    out.push_str("    gmeow:declaredLoss ");
    out.push_str(
        &profile
            .declared_losses
            .iter()
            .map(|iri| format!("<{iri}>"))
            .collect::<Vec<_>>()
            .join(",\n        "),
    );
    out.push_str(" .\n");
    Ok(out)
}

fn percent_encode_path(path: &Path) -> String {
    path.to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn file_uri(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cannot resolve current directory: {e}"))?
            .join(path)
    };
    Ok(format!("file://{}", percent_encode_path(&absolute)))
}

pub fn import_manifest_turtle(
    source: &Path,
    piece_iri: &str,
    provenance: Option<&str>,
) -> Result<String, String> {
    let source_uri = file_uri(source)?;
    let label = provenance
        .map(str::to_string)
        .unwrap_or_else(|| format!("gmeow music import {}", source.display()));
    Ok(format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix prov: <http://www.w3.org/ns/prov#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n\
         <{piece_iri}> a gmeow:MusicalExpression ;\n\
             prov:wasDerivedFrom <{source_uri}> .\n\n\
         [] a <{PROV_ACTIVITY}> ;\n\
             rdfs:label {} ;\n\
             prov:used <{source_uri}> .\n",
        turtle_multiline_literal(&label)
    ))
}

fn manifest_path_for(out: &Path) -> PathBuf {
    PathBuf::from(format!("{}.manifest.ttl", out.display()))
}

pub fn render_file(source: &Path, format_name: &str, out: &Path) -> Result<Vec<PathBuf>, String> {
    let data =
        std::fs::read(source).map_err(|e| format!("failed to read {}: {e}", source.display()))?;
    let piece = piece_from_gts_bytes(&data)?;
    let rendered = render_piece(&piece, format_name)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    match rendered {
        Rendered::Text(text) => {
            std::fs::write(out, text)
                .map_err(|e| format!("failed to write {}: {e}", out.display()))?;
        }
        Rendered::Binary(bytes) => {
            std::fs::write(out, bytes)
                .map_err(|e| format!("failed to write {}: {e}", out.display()))?;
        }
    }
    let manifest_path = manifest_path_for(out);
    let provenance = format!(
        "gmeow music render {} --to {} -o {}",
        source
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("source.gts"),
        format_name,
        out.file_name().and_then(|s| s.to_str()).unwrap_or("out")
    );
    std::fs::write(
        &manifest_path,
        manifest_turtle(format_name, Some(&provenance))?,
    )
    .map_err(|e| format!("failed to write {}: {e}", manifest_path.display()))?;
    Ok(vec![out.to_path_buf(), manifest_path])
}

pub fn import_file(source: &Path, out: &Path) -> Result<Vec<PathBuf>, String> {
    let piece = piece_from_musicxml_file(source)?;
    let data = piece_to_gts_bytes(&piece)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(out, data).map_err(|e| format!("failed to write {}: {e}", out.display()))?;
    let manifest_path = manifest_path_for(out);
    let provenance = format!(
        "gmeow music import {} -o {}",
        source
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("source.musicxml"),
        out.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("out.gts")
    );
    std::fs::write(
        &manifest_path,
        import_manifest_turtle(source, &piece.iri, Some(&provenance))?,
    )
    .map_err(|e| format!("failed to write {}: {e}", manifest_path.display()))?;
    Ok(vec![out.to_path_buf(), manifest_path])
}

#[cfg(test)]
pub(crate) fn fixture_piece() -> Piece {
    Piece {
        iri: "urn:gmeow:piece:test".to_string(),
        title: Some("Rust Music Package Fixture".to_string()),
        composer: Some("GMEOW".to_string()),
        voices: vec![Voice {
            iri: "urn:gmeow:voice:1".to_string(),
            label: Some("lead".to_string()),
            tuning: Some(TuningSystem {
                iri: gm("tuningSystem12EDO"),
                label: "12-EDO".to_string(),
                division_count: Some(12),
                degrees_cents: None,
            }),
            time_frame: Some(TimeFrame {
                iri: "urn:gmeow:timeframe:1".to_string(),
                label: "4/4".to_string(),
                beats_per_measure: Some(4),
                beat_unit: Some(4),
            }),
            events: vec![
                ToneEvent {
                    onset: Fraction::from_i64(0),
                    duration: Fraction::from_i64(1),
                    pitch: Some(PitchValue::from_midi_number(60.0)),
                    is_unpitched: false,
                    dynamics: None,
                    articulation: None,
                    timbre: None,
                },
                ToneEvent {
                    onset: Fraction::from_i64(1),
                    duration: Fraction::from_i64(1),
                    pitch: Some(PitchValue::from_midi_number(62.0)),
                    is_unpitched: false,
                    dynamics: None,
                    articulation: None,
                    timbre: None,
                },
                ToneEvent {
                    onset: Fraction::from_i64(2),
                    duration: Fraction::from_i64(2),
                    pitch: Some(PitchValue::from_midi_number(64.0)),
                    is_unpitched: false,
                    dynamics: None,
                    articulation: None,
                    timbre: None,
                },
            ],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_graph_gts_round_trip_preserves_events() {
        let piece = fixture_piece();
        let bytes = piece_to_gts_bytes(&piece).expect("gts");
        let round = piece_from_gts_bytes(&bytes).expect("read");
        assert_eq!(round.title.as_deref(), Some("Rust Music Package Fixture"));
        assert_eq!(round.voices.len(), 1);
        assert_eq!(round.voices[0].events.len(), 3);
        assert_eq!(round.voices[0].events[2].duration, Fraction::from_i64(2));
        assert!((round.voices[0].events[2].pitch.as_ref().unwrap().cents - 400.0).abs() < 0.001);
    }

    #[test]
    fn renderers_emit_expected_notation_surfaces() {
        let piece = fixture_piece();
        let musicxml = match render_piece(&piece, "musicxml").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("musicxml is text"),
        };
        assert!(musicxml.contains("<score-partwise version=\"4.0\">"));
        assert!(musicxml.contains("<step>C</step>"));
        let lilypond = match render_piece(&piece, "lilypond").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("lilypond is text"),
        };
        assert!(lilypond.contains("\\version \"2.24.0\""));
        let abc = match render_piece(&piece, "abc").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("abc is text"),
        };
        assert!(abc.contains("X:1"));
        assert!(abc.contains("C"));
        let midi = match render_piece(&piece, "midi").unwrap() {
            Rendered::Binary(bytes) => bytes,
            Rendered::Text(_) => panic!("midi is binary"),
        };
        assert!(midi.starts_with(b"MThd"));
        let scl = match render_piece(&piece, "scl").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("scl is text"),
        };
        assert!(scl.contains("Projection profile"));
        assert!(scl.contains("\n12\n"));
    }

    #[test]
    fn musicxml_import_reconstructs_events() {
        let piece = piece_from_musicxml_text(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>Imported</work-title></work>
  <part-list><score-part id="P1"><part-name>P1</part-name></score-part></part-list>
  <part id="P1"><measure number="1"><attributes><divisions>48</divisions></attributes>
    <note><pitch><step>C</step><octave>4</octave></pitch><duration>48</duration></note>
    <note><pitch><step>D</step><octave>4</octave></pitch><duration>48</duration></note>
    <note><pitch><step>E</step><octave>4</octave></pitch><duration>96</duration></note>
  </measure></part>
</score-partwise>"#,
        )
        .expect("import");
        assert_eq!(piece.title.as_deref(), Some("Imported"));
        assert_eq!(piece.voices[0].events.len(), 3);
        assert_eq!(piece.voices[0].events[1].onset, Fraction::from_i64(1));
        assert!((piece.voices[0].events[2].pitch.as_ref().unwrap().cents - 400.0).abs() < 0.001);
    }

    #[test]
    fn manifest_profiles_match_ontology_blocks() {
        let module = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("slices")
                .join("extensions")
                .join("music")
                .join("module.ttl"),
        )
        .expect("module");
        for profile in PROFILES {
            let local = match profile.format {
                "abc" => "profileABC",
                "graphic" => "profileGraphic",
                "kern" => "profileKern",
                "lilypond" => "profileLilyPond",
                "mei" => "profileMEI",
                "mensural" => "profileMensural",
                "midi" => "profileMIDI",
                "musicxml" => "profileMusicXML",
                "scl" => "profileSCL",
                "tab" => "profileTablature",
                _ => unreachable!(),
            };
            let start = module
                .find(&format!("gmeow:{local}\n"))
                .expect("profile block");
            let rest = &module[start..];
            let end = rest.find("\n\n").unwrap_or(rest.len());
            let block = &rest[..end];
            assert!(
                block.contains(&profile.notation_system.replace(GM, "gmeow:")),
                "{} notation system drifted",
                profile.format
            );
            assert!(
                block.contains(&profile.projection_function.replace(GM, "gmeow:")),
                "{} projection function drifted",
                profile.format
            );
            for loss in profile.declared_losses {
                assert!(
                    block.contains(&loss.replace(GM, "gmeow:")),
                    "{} missing loss {loss}",
                    profile.format
                );
            }
            for parameter in profile.representable_parameters {
                assert!(
                    block.contains(&parameter.replace(GM, "gmeow:")),
                    "{} missing parameter {parameter}",
                    profile.format
                );
            }
        }
    }

    #[test]
    fn manifests_carry_sidecar_shape() {
        let manifest = manifest_turtle("musicxml", Some("render provenance")).expect("manifest");
        assert!(manifest.contains("gmeow:targetNotationSystem"));
        assert!(manifest.contains("gmeow:projectionFunction"));
        assert!(manifest.contains("gmeow:declaredLoss"));
        let import = import_manifest_turtle(
            Path::new("fixtures/source.musicxml"),
            "urn:gmeow:piece:imported",
            Some("import provenance"),
        )
        .expect("import manifest");
        assert!(import.contains("prov:wasDerivedFrom"));
        assert!(import.contains("file://"));
    }
}

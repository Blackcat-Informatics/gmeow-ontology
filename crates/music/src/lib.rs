// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-owned music-package toolchain.
//!
//! This crate is the authority for GMEOW music-package model conversion,
//! GTS package I/O, notation renderers, MusicXML import, and loss manifests.

pub mod error;

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gmeow_errors::{Diag, ResultExt};
use purrdf::gts::model::{Term, TermKind};
use purrdf::gts_compose::SnapshotBuilder;
use purrdf::{NativeRdfFormat, parse_dataset};

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

fn gcd(a: i64, b: i64) -> u64 {
    let mut a = a.unsigned_abs();
    let mut b = b.unsigned_abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    if a == 0 { 1 } else { a }
}

/// Rational musical time value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    pub numerator: i64,
    pub denominator: i64,
}

impl Fraction {
    pub fn new(numerator: i64, denominator: i64) -> gmeow_errors::Result<Self> {
        if denominator == 0 {
            return Err(Diag::of_kind(error::InvalidFraction {
                detail: "fraction denominator must not be zero".to_owned(),
            }));
        }
        if numerator == i64::MIN || denominator == i64::MIN {
            return Err(Diag::of_kind(error::InvalidFraction {
                detail: "fraction values must not be i64::MIN".to_owned(),
            }));
        }
        let sign = if denominator < 0 { -1 } else { 1 };
        let g = gcd(numerator, denominator) as i64;
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

    pub fn from_f64(value: f64, max_denominator: i64) -> gmeow_errors::Result<Self> {
        if !value.is_finite() {
            return Err(Diag::of_kind(error::InvalidFraction {
                detail: "cannot convert non-finite value to fraction".to_owned(),
            }));
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
        (self.numerator as i128 * other.denominator as i128)
            .cmp(&(other.numerator as i128 * self.denominator as i128))
    }
}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn fraction_from_i128(numerator: i128, denominator: i128) -> Option<Fraction> {
    if numerator < i128::from(i64::MIN)
        || numerator > i128::from(i64::MAX)
        || denominator < i128::from(i64::MIN)
        || denominator > i128::from(i64::MAX)
    {
        return None;
    }
    Fraction::new(numerator as i64, denominator as i64).ok()
}

fn fraction_add(left: Fraction, right: Fraction) -> Option<Fraction> {
    fraction_from_i128(
        i128::from(left.numerator) * i128::from(right.denominator)
            + i128::from(right.numerator) * i128::from(left.denominator),
        i128::from(left.denominator) * i128::from(right.denominator),
    )
}

fn fraction_sub(left: Fraction, right: Fraction) -> Option<Fraction> {
    fraction_from_i128(
        i128::from(left.numerator) * i128::from(right.denominator)
            - i128::from(right.numerator) * i128::from(left.denominator),
        i128::from(left.denominator) * i128::from(right.denominator),
    )
}

fn positive_gap(start: Fraction, cursor: Fraction) -> Option<Fraction> {
    if start > cursor {
        fraction_sub(start, cursor).filter(|gap| gap.to_f64() > 0.0)
    } else {
        None
    }
}

fn sorted_positive_events(voice: &Voice) -> Vec<&ToneEvent> {
    let mut events = voice
        .events
        .iter()
        .filter(|event| event.duration.to_f64() > 0.0)
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.onset);
    events
}

fn timeline_tokens(
    voice: &Voice,
    mut event_token: impl FnMut(&ToneEvent) -> String,
    mut rest_token: impl FnMut(Fraction) -> String,
) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cursor = Fraction::from_i64(0);
    for event in sorted_positive_events(voice) {
        if let Some(gap) = positive_gap(event.onset, cursor) {
            tokens.push(rest_token(gap));
            cursor = event.onset;
        }
        tokens.push(event_token(event));
        if let Some(end) = fraction_add(event.onset, event.duration)
            && end > cursor
        {
            cursor = end;
        }
    }
    tokens
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

pub fn get_profile(format_name: &str) -> gmeow_errors::Result<&'static NotationProfile> {
    let normalized = format_name.to_ascii_lowercase();
    PROFILES
        .iter()
        .find(|profile| profile.format == normalized)
        .ok_or_else(|| {
            Diag::of_kind(error::UnsupportedFormat {
                format: format_name.to_owned(),
            })
        })
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
                let pitch_iri = format!("{event_iri}#pitch");
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

pub fn piece_to_gts_bytes(piece: &Piece) -> gmeow_errors::Result<Vec<u8>> {
    let turtle = piece_to_turtle(piece);
    let dataset = parse_dataset(
        turtle.as_bytes(),
        NativeRdfFormat::Turtle.media_type(),
        None,
    )
    .map_err(|e| {
        Diag::of_kind(error::RdfPipelineFailed {
            detail: e.to_string(),
        })
    })?;
    let mut builder = SnapshotBuilder::default();
    builder
        .add_dataset(&dataset)
        .map_err(|e| Diag::of_kind(error::RdfPipelineFailed { detail: e }))?;
    gmeow_gts_profile::emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None).map_err(
        |e| {
            Diag::of_kind(error::RdfPipelineFailed {
                detail: e.to_string(),
            })
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Default)]
struct TripleIndex {
    by_subject: HashMap<String, HashMap<String, Vec<Object>>>,
    by_predicate_object: HashMap<(String, Object), Vec<String>>,
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
    let graph = purrdf::gts::reader::read(bytes, false, None);
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

fn index_triples(triples: Vec<Triple>) -> TripleIndex {
    let mut index = TripleIndex::default();
    for triple in triples {
        index
            .by_predicate_object
            .entry((triple.predicate.clone(), triple.object.clone()))
            .or_default()
            .push(triple.subject.clone());
        index
            .by_subject
            .entry(triple.subject)
            .or_default()
            .entry(triple.predicate)
            .or_default()
            .push(triple.object);
    }
    index
}

fn objects<'a>(index: &'a TripleIndex, subject: &str, predicate: &str) -> Option<&'a [Object]> {
    index
        .by_subject
        .get(subject)?
        .get(predicate)
        .map(Vec::as_slice)
}

fn subjects_with_object<'a>(
    index: &'a TripleIndex,
    predicate: &str,
    object: &Object,
) -> Option<&'a [String]> {
    index
        .by_predicate_object
        .get(&(predicate.to_string(), object.clone()))
        .map(Vec::as_slice)
}

fn has_type(index: &TripleIndex, subject: &str, class: &str) -> bool {
    objects(index, subject, RDF_TYPE).is_some_and(|objects| {
        objects
            .iter()
            .any(|object| matches!(object, Object::Iri(value) if value == class))
    })
}

fn first_iri(index: &TripleIndex, subject: &str, predicate: &str) -> Option<String> {
    objects(index, subject, predicate)?
        .iter()
        .find_map(|object| match object {
            Object::Iri(value) | Object::Bnode(value) => Some(value.clone()),
            Object::Literal(_) => None,
        })
}

fn all_iris(index: &TripleIndex, subject: &str, predicate: &str) -> Vec<String> {
    objects(index, subject, predicate)
        .map(|objects| {
            objects
                .iter()
                .filter_map(|object| match object {
                    Object::Iri(value) | Object::Bnode(value) => Some(value.clone()),
                    Object::Literal(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn first_literal(index: &TripleIndex, subject: &str, predicate: &str) -> Option<String> {
    objects(index, subject, predicate)?
        .iter()
        .next()
        .map(|object| match object {
            Object::Literal(value) | Object::Iri(value) | Object::Bnode(value) => value.clone(),
        })
}

fn first_i64(index: &TripleIndex, subject: &str, predicate: &str) -> Option<i64> {
    first_literal(index, subject, predicate)?.parse().ok()
}

fn first_f64(index: &TripleIndex, subject: &str, predicate: &str) -> Option<f64> {
    first_literal(index, subject, predicate)?.parse().ok()
}

fn first_bool(index: &TripleIndex, subject: &str, predicate: &str) -> bool {
    let value = first_literal(index, subject, predicate).unwrap_or_default();
    let trimmed = value.trim();
    trimmed.eq_ignore_ascii_case("true") || trimmed == "1"
}

fn load_tuning(index: &TripleIndex, iri: &str) -> TuningSystem {
    let mut degrees = objects(index, iri, &gm("hasPitchValue"))
        .unwrap_or_default()
        .iter()
        .filter_map(|object| match object {
            Object::Iri(node) | Object::Bnode(node) => {
                first_f64(index, node, &gm("centsFromOrigin"))
            }
            Object::Literal(_) => None,
        })
        .collect::<Vec<_>>();
    degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    TuningSystem {
        iri: iri.to_string(),
        label: first_literal(index, iri, RDFS_LABEL).unwrap_or_else(|| iri.to_string()),
        division_count: first_i64(index, iri, &gm("divisionCount")),
        degrees_cents: if degrees.is_empty() {
            None
        } else {
            Some(degrees)
        },
    }
}

fn load_time_frame(index: &TripleIndex, iri: &str) -> TimeFrame {
    TimeFrame {
        iri: iri.to_string(),
        label: first_literal(index, iri, RDFS_LABEL).unwrap_or_else(|| iri.to_string()),
        beats_per_measure: first_i64(index, iri, &gm("beatsPerMeasure")),
        beat_unit: first_i64(index, iri, &gm("beatUnit")),
    }
}

fn load_pitch(index: &TripleIndex, iri: &str) -> Option<PitchValue> {
    first_f64(index, iri, &gm("centsFromOrigin")).map(|cents| PitchValue {
        cents,
        spelled_name: first_literal(index, iri, RDFS_LABEL),
    })
}

pub fn piece_from_gts_bytes(bytes: &[u8]) -> gmeow_errors::Result<Piece> {
    let index = index_triples(triples_from_gts(bytes));
    let musical_expression = gm("MusicalExpression");
    let musical_work = gm("MusicalWork");
    let mut pieces = index
        .by_subject
        .iter()
        .filter_map(|(subject, predicates)| {
            let has_music_type = predicates.get(RDF_TYPE).is_some_and(|objects| {
                objects.iter().any(|object| {
                    matches!(object, Object::Iri(class) if class == &musical_expression || class == &musical_work)
                })
            });
            has_music_type.then(|| subject.clone())
        })
        .collect::<BTreeSet<_>>();
    let piece_iri = pieces
        .pop_first()
        .ok_or_else(|| Diag::of_kind(error::NoMusicalEntity {}))?;
    let mut piece = Piece {
        title: first_literal(&index, &piece_iri, RDFS_LABEL),
        composer: first_literal(&index, &piece_iri, &gm("composer")),
        iri: piece_iri.clone(),
        voices: Vec::new(),
    };
    for voice_iri in all_iris(&index, &piece_iri, &gm("hasVoice")) {
        let tuning = first_iri(&index, &voice_iri, &gm("voiceTuningFrame"))
            .map(|iri| load_tuning(&index, &iri));
        let time_frame = first_iri(&index, &voice_iri, &gm("voiceTimeFrame"))
            .map(|iri| load_time_frame(&index, &iri));
        let mut voice = Voice {
            iri: voice_iri.clone(),
            label: first_literal(&index, &voice_iri, RDFS_LABEL),
            tuning,
            time_frame,
            events: Vec::new(),
        };
        let segment_of = gm("segmentOf");
        let tone_event = gm("ToneEvent");
        let mut event_iris =
            subjects_with_object(&index, &segment_of, &Object::Iri(voice_iri.clone()))
                .unwrap_or_default()
                .iter()
                .filter(|subject| has_type(&index, subject, &tone_event))
                .cloned()
                .collect::<BTreeSet<_>>();
        for event_iri in std::mem::take(&mut event_iris) {
            let Some(span_iri) = first_iri(&index, &event_iri, &gm("segmentSpan")) else {
                continue;
            };
            let Some(start_num) = first_i64(&index, &span_iri, &gm("timeStartNumerator")) else {
                continue;
            };
            let Some(start_den) = first_i64(&index, &span_iri, &gm("timeStartDenominator")) else {
                continue;
            };
            let Some(dur_num) = first_i64(&index, &span_iri, &gm("timeDurationNumerator")) else {
                continue;
            };
            let Some(dur_den) = first_i64(&index, &span_iri, &gm("timeDurationDenominator")) else {
                continue;
            };
            let pitch = first_iri(&index, &event_iri, &gm("toneEventPitchValue"))
                .and_then(|iri| load_pitch(&index, &iri));
            voice.events.push(ToneEvent {
                onset: Fraction::new(start_num, start_den)?,
                duration: Fraction::new(dur_num, dur_den)?,
                pitch,
                is_unpitched: first_bool(&index, &event_iri, &gm("toneEventIsUnpitched")),
                dynamics: first_literal(&index, &event_iri, &gm("toneEventDynamics")),
                articulation: first_literal(&index, &event_iri, &gm("toneEventArticulation")),
                timbre: first_literal(&index, &event_iri, &gm("toneEventTimbre")),
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

pub fn render_piece(piece: &Piece, format_name: &str) -> gmeow_errors::Result<Rendered> {
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
        _ => Err(Diag::of_kind(error::UnsupportedFormat {
            format: format_name.to_owned(),
        })),
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

fn write_musicxml_note(
    out: &mut String,
    pitch: Option<&PitchValue>,
    is_rest: bool,
    duration: Fraction,
    beat_unit: Fraction,
) {
    out.push_str("      <note>\n");
    if is_rest || pitch.is_none() {
        out.push_str("        <rest/>\n");
    } else if let Some(pitch) = pitch {
        let (step, alter, octave) = pitch_elements(pitch);
        out.push_str("        <pitch>\n");
        let _ = writeln!(out, "          <step>{step}</step>");
        if alter.abs() > 0.001 {
            let _ = writeln!(out, "          <alter>{alter:.2}</alter>");
        }
        let _ = writeln!(out, "          <octave>{octave}</octave>");
        out.push_str("        </pitch>\n");
    }
    let duration_units = (duration.div(beat_unit) * 48.0).round().max(1.0) as i64;
    let _ = writeln!(out, "        <duration>{duration_units}</duration>");
    let _ = writeln!(
        out,
        "        <type>{}</type>",
        note_type(duration, beat_unit)
    );
    out.push_str("      </note>\n");
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
        let mut cursor = Fraction::from_i64(0);
        for event in sorted_positive_events(voice) {
            if let Some(gap) = positive_gap(event.onset, cursor) {
                write_musicxml_note(&mut out, None, true, gap, beat_unit);
                cursor = event.onset;
            }
            write_musicxml_note(
                &mut out,
                event.pitch.as_ref(),
                event.is_unpitched,
                event.duration,
                beat_unit,
            );
            if let Some(end) = fraction_add(event.onset, event.duration)
                && end > cursor
            {
                cursor = end;
            }
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
        let tokens = timeline_tokens(
            voice,
            |event| {
                let head = match (event.is_unpitched, event.pitch.as_ref()) {
                    (true, _) | (_, None) => "r".to_string(),
                    (false, Some(pitch)) => lily_pitch(pitch),
                };
                format!("{head}{}", lily_duration(event.duration, beat_unit))
            },
            |gap| format!("r{}", lily_duration(gap, beat_unit)),
        );
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
        let tokens = timeline_tokens(
            voice,
            |event| {
                let head = match (event.is_unpitched, event.pitch.as_ref()) {
                    (true, _) | (_, None) => "z".to_string(),
                    (false, Some(pitch)) => abc_pitch(pitch),
                };
                format!("{head}{}", abc_duration(event.duration, beat_unit))
            },
            |gap| format!("z{}", abc_duration(gap, beat_unit)),
        );
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
    let mut timed_events = Vec::new();
    for voice in &piece.voices {
        let beat_unit = voice.beat_unit();
        for event in &voice.events {
            let onset = (event.onset.div(beat_unit) * f64::from(DEFAULT_PPQN)).round() as i64;
            let duration = (event.duration.div(beat_unit) * f64::from(DEFAULT_PPQN)).round() as i64;
            if event.is_unpitched || event.pitch.is_none() {
                continue;
            }
            let midi = event.pitch.as_ref().expect("checked").to_midi_number();
            let pitch = (midi.round() as i64).clamp(0, 127) as u8;
            let onset = onset.max(0);
            timed_events.push((onset, 1_u8, vec![0x90, pitch, 96]));
            timed_events.push((onset + duration.max(1), 0_u8, vec![0x80, pitch, 0]));
        }
    }
    timed_events.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let mut current_tick = 0_i64;
    for (tick, _order, message) in timed_events {
        let delta = (tick - current_tick).max(0) as u32;
        events.push((delta, message));
        current_tick = tick;
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
    let pitch_rows = degrees.into_iter().skip(1).collect::<Vec<_>>();
    let _ = writeln!(out, "{}", pitch_rows.len());
    let _ = writeln!(out, "{title}");
    for cents in pitch_rows {
        let _ = writeln!(out, "{cents:.6}");
    }
    out
}

fn render_mei(piece: &Piece, profile: &NotationProfile) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mei><music><body><mdiv><score><scoreDef/><section><!-- {} --><staff n=\"1\"><layer>{}</layer></staff></section></score></mdiv></body></music></mei>\n",
        profile.projection_function,
        escape_xml(piece.title.as_deref().unwrap_or("Untitled"))
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
        for token in timeline_tokens(
            voice,
            |event| {
                if event.is_unpitched || event.pitch.is_none() {
                    "r".to_string()
                } else {
                    "4c".to_string()
                }
            },
            |_gap| "r".to_string(),
        ) {
            let _ = writeln!(out, "{token}");
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

fn musicxml_duration(
    note: roxmltree::Node<'_, '_>,
    divisions: f64,
) -> gmeow_errors::Result<Fraction> {
    let duration_divs = child_text(note, "duration")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(divisions);
    Fraction::from_f64(duration_divs / divisions.max(1.0), 64)
}

fn musicxml_part_label(doc: &roxmltree::Document<'_>, part_id: &str, fallback: &str) -> String {
    doc.descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "score-part"
                && node.attribute("id") == Some(part_id)
        })
        .and_then(|score_part| child_text(score_part, "part-name"))
        .unwrap_or(fallback)
        .to_string()
}

fn musicxml_voice_id(note: roxmltree::Node<'_, '_>) -> String {
    child_text(note, "voice")
        .map(str::trim)
        .filter(|voice| !voice.is_empty())
        .unwrap_or("1")
        .to_string()
}

pub fn piece_from_musicxml_text(text: &str) -> gmeow_errors::Result<Piece> {
    let doc = roxmltree::Document::parse(text).map_err(|e| {
        Diag::of_kind(error::MusicXmlParse {
            detail: format!("MusicXML parse error: {e}"),
        })
    })?;
    let title = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "work-title")
        .and_then(|node| node.text())
        .unwrap_or("Imported piece")
        .to_string();
    let mut voices = Vec::new();
    for (part_idx, part) in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "part")
        .enumerate()
    {
        let part_id = part
            .attribute("id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("P{}", part_idx + 1));
        let part_label = musicxml_part_label(&doc, &part_id, &part_id);
        let mut divisions = 1.0_f64;
        let mut cursor = Fraction::from_i64(0);
        let mut last_note_onset = cursor;
        let mut by_voice: BTreeMap<String, Vec<ToneEvent>> = BTreeMap::new();

        for measure in part
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "measure")
        {
            for child in measure.children().filter(|node| node.is_element()) {
                match child.tag_name().name() {
                    "attributes" => {
                        if let Some(next_divisions) =
                            child_text(child, "divisions").and_then(|value| value.parse().ok())
                        {
                            divisions = next_divisions;
                        }
                    }
                    "backup" => {
                        let duration = musicxml_duration(child, divisions)?;
                        cursor =
                            fraction_sub(cursor, duration).unwrap_or_else(|| Fraction::from_i64(0));
                    }
                    "forward" => {
                        let duration = musicxml_duration(child, divisions)?;
                        cursor = fraction_add(cursor, duration).ok_or_else(|| {
                            Diag::of_kind(error::TimelineOverflow {
                                detail: "MusicXML forward overflowed timeline".to_owned(),
                            })
                        })?;
                    }
                    "note" => {
                        let duration = musicxml_duration(child, divisions)?;
                        let is_rest = child.children().any(|note_child| {
                            note_child.is_element() && note_child.tag_name().name() == "rest"
                        });
                        let is_chord = child.children().any(|note_child| {
                            note_child.is_element() && note_child.tag_name().name() == "chord"
                        });
                        let onset = if is_chord { last_note_onset } else { cursor };
                        let voice_id = musicxml_voice_id(child);
                        by_voice.entry(voice_id).or_default().push(ToneEvent {
                            onset,
                            duration,
                            pitch: if is_rest { None } else { musicxml_pitch(child) },
                            is_unpitched: is_rest,
                            dynamics: None,
                            articulation: None,
                            timbre: None,
                        });
                        if !is_chord {
                            cursor = fraction_add(cursor, duration).ok_or_else(|| {
                                Diag::of_kind(error::TimelineOverflow {
                                    detail: "MusicXML note duration overflowed timeline".to_owned(),
                                })
                            })?;
                            last_note_onset = onset;
                        }
                    }
                    _ => {}
                }
            }
        }

        for (voice_id, mut events) in by_voice {
            events.sort_by_key(|event| event.onset);
            voices.push(Voice {
                iri: format!("urn:gmeow:voice:{}:{voice_id}", part_idx + 1),
                label: Some(format!("{part_label} voice {voice_id}")),
                tuning: Some(TuningSystem {
                    iri: gm("tuningSystem12EDO"),
                    label: "12-EDO".to_string(),
                    division_count: Some(12),
                    degrees_cents: None,
                }),
                time_frame: Some(TimeFrame {
                    iri: format!("urn:gmeow:timeframe:{}", part_idx + 1),
                    label: "4/4".to_string(),
                    beats_per_measure: Some(4),
                    beat_unit: Some(4),
                }),
                events,
            });
        }
    }
    if voices.is_empty() {
        voices.push(Voice {
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
            events: Vec::new(),
        });
    }
    Ok(Piece {
        iri: "urn:gmeow:piece:imported".to_string(),
        title: Some(title),
        composer: None,
        voices,
    })
}

pub fn piece_from_musicxml_file(path: &Path) -> gmeow_errors::Result<Piece> {
    let suffix = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(suffix.as_str(), "xml" | "musicxml") {
        return Err(Diag::of_kind(error::UnsupportedImportSuffix {}));
    }
    let text =
        std::fs::read_to_string(path).with_ctx(|| format!("failed to read {}", path.display()))?;
    piece_from_musicxml_text(&text)
}

fn turtle_multiline_literal(value: &str) -> String {
    format!(
        "\"\"\"{}\"\"\"",
        value.replace('\\', "\\\\").replace("\"\"\"", "\\\"\"\"")
    )
}

pub fn manifest_turtle(
    format_name: &str,
    provenance: Option<&str>,
) -> gmeow_errors::Result<String> {
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

fn percent_encode_path(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn file_uri(path: &Path) -> gmeow_errors::Result<String> {
    let absolute = std::path::absolute(path).ctx("failed to resolve absolute path")?;
    let normalized = absolute.to_string_lossy().replace('\\', "/");
    let prefix = if normalized.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    Ok(format!("{prefix}{}", percent_encode_path(&normalized)))
}

pub fn import_manifest_turtle(
    source: &Path,
    piece_iri: &str,
    provenance: Option<&str>,
) -> gmeow_errors::Result<String> {
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

pub fn render_file(
    source: &Path,
    format_name: &str,
    out: &Path,
) -> gmeow_errors::Result<Vec<PathBuf>> {
    let data = std::fs::read(source).with_ctx(|| format!("failed to read {}", source.display()))?;
    let piece = piece_from_gts_bytes(&data)?;
    let rendered = render_piece(&piece, format_name)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_ctx(|| format!("failed to create {}", parent.display()))?;
    }
    match rendered {
        Rendered::Text(text) => {
            std::fs::write(out, text).with_ctx(|| format!("failed to write {}", out.display()))?;
        }
        Rendered::Binary(bytes) => {
            std::fs::write(out, bytes).with_ctx(|| format!("failed to write {}", out.display()))?;
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
    .with_ctx(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(vec![out.to_path_buf(), manifest_path])
}

pub fn import_file(source: &Path, out: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let piece = piece_from_musicxml_file(source)?;
    let data = piece_to_gts_bytes(&piece)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_ctx(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(out, data).with_ctx(|| format!("failed to write {}", out.display()))?;
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
    .with_ctx(|| format!("failed to write {}", manifest_path.display()))?;
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

    fn read_varlen(data: &[u8], idx: &mut usize) -> u32 {
        let mut value = 0_u32;
        loop {
            let byte = data[*idx];
            *idx += 1;
            value = (value << 7) | u32::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                return value;
            }
        }
    }

    fn midi_note_events(bytes: &[u8]) -> Vec<(u32, u8, u8)> {
        assert!(bytes.starts_with(b"MThd"));
        let track_len_offset = 18;
        assert_eq!(&bytes[14..18], b"MTrk");
        let track_len = u32::from_be_bytes(
            bytes[track_len_offset..track_len_offset + 4]
                .try_into()
                .expect("track length"),
        ) as usize;
        let mut idx = 22;
        let end = idx + track_len;
        let mut tick = 0_u32;
        let mut events = Vec::new();
        while idx < end {
            tick += read_varlen(bytes, &mut idx);
            let status = bytes[idx];
            idx += 1;
            if status == 0xff {
                idx += 1;
                let len = read_varlen(bytes, &mut idx) as usize;
                idx += len;
                continue;
            }
            let pitch = bytes[idx];
            idx += 1;
            idx += 1;
            if matches!(status, 0x80 | 0x90) {
                events.push((tick, status, pitch));
            }
        }
        events
    }

    #[test]
    fn fraction_rejects_i64_min_values() {
        assert!(
            Fraction::new(i64::MIN, 1)
                .unwrap_err()
                .message()
                .contains("i64::MIN")
        );
        assert!(
            Fraction::new(1, i64::MIN)
                .unwrap_err()
                .message()
                .contains("i64::MIN")
        );
    }

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
    fn gts_round_trip_keeps_same_cents_pitch_labels_event_scoped() {
        let mut piece = fixture_piece();
        piece.voices[0].events.truncate(2);
        piece.voices[0].events[0].pitch = Some(PitchValue {
            cents: 0.0,
            spelled_name: Some("C natural".to_string()),
        });
        piece.voices[0].events[1].pitch = Some(PitchValue {
            cents: 0.0,
            spelled_name: Some("B sharp".to_string()),
        });

        let bytes = piece_to_gts_bytes(&piece).expect("gts");
        let round = piece_from_gts_bytes(&bytes).expect("read");
        let labels = round.voices[0]
            .events
            .iter()
            .map(|event| {
                event
                    .pitch
                    .as_ref()
                    .and_then(|pitch| pitch.spelled_name.as_deref())
                    .unwrap_or("")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["C natural", "B sharp"]);
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
        assert!(!scl.lines().any(|line| line == "0."));

        let mut escaped_title_piece = fixture_piece();
        escaped_title_piece.title = Some("A&B <C> \"D\"".to_string());
        let mei = match render_piece(&escaped_title_piece, "mei").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("mei is text"),
        };
        assert!(mei.contains("A&amp;B &lt;C&gt; &quot;D&quot;"));
    }

    #[test]
    fn renderers_preserve_onset_gaps_with_rests() {
        let mut piece = fixture_piece();
        piece.voices[0].events = vec![
            ToneEvent {
                onset: Fraction::from_i64(0),
                duration: Fraction::new(1, 4).expect("quarter"),
                pitch: Some(PitchValue::from_midi_number(60.0)),
                is_unpitched: false,
                dynamics: None,
                articulation: None,
                timbre: None,
            },
            ToneEvent {
                onset: Fraction::new(1, 2).expect("half"),
                duration: Fraction::new(1, 4).expect("quarter"),
                pitch: Some(PitchValue::from_midi_number(62.0)),
                is_unpitched: false,
                dynamics: None,
                articulation: None,
                timbre: None,
            },
        ];

        let musicxml = match render_piece(&piece, "musicxml").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("musicxml is text"),
        };
        assert!(musicxml.contains("<rest/>"));

        let lilypond = match render_piece(&piece, "lilypond").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("lilypond is text"),
        };
        assert!(lilypond.contains("r4"));

        let abc = match render_piece(&piece, "abc").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("abc is text"),
        };
        assert!(abc.contains(" z"));

        let kern = match render_piece(&piece, "kern").unwrap() {
            Rendered::Text(text) => text,
            Rendered::Binary(_) => panic!("kern is text"),
        };
        assert!(kern.lines().any(|line| line == "r"));
    }

    #[test]
    fn midi_renderer_orders_overlapping_notes_by_absolute_tick() {
        let mut piece = fixture_piece();
        piece.voices[0].events = vec![
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
                onset: Fraction::new(1, 2).expect("half beat"),
                duration: Fraction::from_i64(1),
                pitch: Some(PitchValue::from_midi_number(64.0)),
                is_unpitched: false,
                dynamics: None,
                articulation: None,
                timbre: None,
            },
        ];
        let midi = match render_piece(&piece, "midi").unwrap() {
            Rendered::Binary(bytes) => bytes,
            Rendered::Text(_) => panic!("midi is binary"),
        };
        assert_eq!(
            midi_note_events(&midi),
            vec![
                (0, 0x90, 60),
                (960, 0x90, 64),
                (1920, 0x80, 60),
                (2880, 0x80, 64),
            ]
        );
    }

    #[test]
    fn musicxml_import_reconstructs_events() {
        let piece = piece_from_musicxml_text(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>Imported</work-title></work>
  <part-list><score-part id="P1"><part-name>P1</part-name></score-part></part-list>
  <part id="P1"><measure number="1"><attributes><divisions>48</divisions></attributes>
    <note><voice>1</voice><pitch><step>C</step><octave>4</octave></pitch><duration>96</duration></note>
    <backup><duration>96</duration></backup>
    <note><voice>2</voice><pitch><step>E</step><octave>4</octave></pitch><duration>48</duration></note>
    <note><voice>2</voice><pitch><step>G</step><octave>4</octave></pitch><duration>48</duration></note>
  </measure></part>
</score-partwise>"#,
        )
        .expect("import");
        assert_eq!(piece.title.as_deref(), Some("Imported"));
        assert_eq!(piece.voices.len(), 2);
        assert_eq!(piece.voices[0].events.len(), 1);
        assert_eq!(piece.voices[0].events[0].duration, Fraction::from_i64(2));
        assert_eq!(piece.voices[1].events.len(), 2);
        assert_eq!(piece.voices[1].events[0].onset, Fraction::from_i64(0));
        assert_eq!(piece.voices[1].events[1].onset, Fraction::from_i64(1));
        assert!((piece.voices[1].events[0].pitch.as_ref().unwrap().cents - 400.0).abs() < 0.001);
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
            Path::new("fixtures/source file.musicxml"),
            "urn:gmeow:piece:imported",
            Some("import provenance"),
        )
        .expect("import manifest");
        assert!(import.contains("prov:wasDerivedFrom"));
        assert!(import.contains("file://"));
        assert!(import.contains("source%20file.musicxml"));
    }
}

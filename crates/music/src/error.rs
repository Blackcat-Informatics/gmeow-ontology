// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Music-toolchain diagnostic kinds.
//!
//! Every failure surface of the music package I/O, notation renderers, and
//! MusicXML import is a HARD fail (no-optionality): a zero-denominator fraction,
//! an unsupported format, a graph with no musical entity, a MusicXML timeline
//! overflow, a parse error, an RDF-pipeline failure. Each is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind). Filesystem failures are
//! NOT modelled here — they ride the substrate's blanket `From<io::Error>` with a
//! `.with_ctx` frame so the live `io::Error` stays downcastable (the Diag → PyErr
//! bridge keeps the `OSError` class every I/O failure has surfaced).

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A rational musical time value is out of domain (zero denominator, an
    /// `i64::MIN` term, or a non-finite `f64` source).
    pub struct InvalidFraction { detail: String }
    code = "music.fraction.invalid";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The requested notation format is not one the toolchain projects.
    pub struct UnsupportedFormat { format: String }
    code = "music.format.unsupported";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "unsupported format: {}", format;
}

define_diag_kind! {
    /// A MusicXML import was handed a file whose suffix is neither `.xml` nor
    /// `.musicxml`.
    pub struct UnsupportedImportSuffix {}
    code = "music.import.unsupported-suffix";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "MusicXML import only supports .xml and .musicxml files";
}

define_diag_kind! {
    /// The GTS graph carries no `gmeow:MusicalExpression` / `gmeow:MusicalWork`,
    /// so there is no piece to read back.
    pub struct NoMusicalEntity {}
    code = "music.gts.no-musical-entity";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "no MusicalExpression or MusicalWork found in graph";
}

define_diag_kind! {
    /// A MusicXML `forward`/`note` advance overflowed the rational timeline.
    pub struct TimelineOverflow { detail: String }
    code = "music.musicxml.timeline-overflow";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The MusicXML source did not parse as well-formed XML.
    pub struct MusicXmlParse { detail: String }
    code = "music.musicxml.parse";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The shared `purrdf` RDF pipeline (Turtle parse, snapshot build, GTS emit)
    /// reported a failure. Its message is preserved verbatim at the crate boundary.
    pub struct RdfPipelineFailed { detail: String }
    code = "music.gts.rdf-pipeline";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete music diagnostic-code catalog, in registration order.
pub const MUSIC_DIAG_CODES: &[&str] = &[
    InvalidFraction::CODE,
    UnsupportedFormat::CODE,
    UnsupportedImportSuffix::CODE,
    NoMusicalEntity::CODE,
    TimelineOverflow::CODE,
    MusicXmlParse::CODE,
    RdfPipelineFailed::CODE,
];

/// Eagerly intern every music diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        InvalidFraction::register(),
        UnsupportedFormat::register(),
        UnsupportedImportSuffix::register(),
        NoMusicalEntity::register(),
        TimelineOverflow::register(),
        MusicXmlParse::register(),
        RdfPipelineFailed::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_music_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            MUSIC_DIAG_CODES.len(),
            "register_all() and MUSIC_DIAG_CODES must enumerate the same kinds"
        );
        for code in MUSIC_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "music code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = MUSIC_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            MUSIC_DIAG_CODES.len(),
            "duplicate music diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}

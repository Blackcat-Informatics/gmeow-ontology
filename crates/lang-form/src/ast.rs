// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The stratified form AST.
//!
//! Every structured stratum is a [`Form`] variant carrying its sign system and its
//! structural content. Surface realizations are a separate [`SurfaceForm`] type, so
//! byte-level material never leaks into form identity. The node set is closed and
//! total: there is no "other" variant — material that cannot be lifted is held as an
//! explicitly-unanalyzed surface (a runtime concern), never as a silent catch-all.

/// A typed morphological feature — a key drawn from a feature inventory, one or more
/// values (a set; disjunctive/underspecified values are unordered), and an optional
/// layer (the Universal-Dependencies `Number[psor]` convention).
///
/// The feature's contribution to a content key is order-independent in its values:
/// `Case=Nom,Acc` keys identically however the values are listed.
#[derive(Clone, Debug)]
pub struct MorphFeature {
    /// The feature key (e.g. `Number`), an inventory identifier.
    pub key: String,
    /// The feature value(s) (e.g. `Plur`), inventory identifiers. A set: order does
    /// not affect identity.
    pub values: Vec<String>,
    /// The feature layer (e.g. `psor`), where the inventory declares layered features.
    pub layer: Option<String>,
}

/// How far a surface has been analyzed — the graded status that replaces a binary
/// analyzed/unanalyzed flag. Ordered by [`AnalysisLevel::rank`] from raw to parsed;
/// `denoted` is the meaning layer's level and is deliberately not represented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisLevel {
    /// Unanalyzed prose — no segmentation.
    Raw,
    /// Split into segments (graphemes or phones).
    Segmented,
    /// Split into orthographic and syntactic words.
    Tokenized,
    /// Word forms resolved to lexemes with typed morphology.
    MorphAnalyzed,
    /// A full constituency-and-dependency parse.
    Parsed,
}

impl AnalysisLevel {
    /// The integer rank ordering the levels (raw = 0 … parsed = 4), mirroring the
    /// ontology's `lang:levelRank`.
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            AnalysisLevel::Raw => 0,
            AnalysisLevel::Segmented => 1,
            AnalysisLevel::Tokenized => 2,
            AnalysisLevel::MorphAnalyzed => 3,
            AnalysisLevel::Parsed => 4,
        }
    }
}

/// A constituent slot of a [`Composed`](Form::Composed) form. The index carries
/// constituent order (identity-bearing); the role and dependency edge carry the
/// grammatical analysis co-resident with the constituency tree.
#[derive(Clone, Debug)]
pub struct Slot {
    /// The zero-based constituent position.
    pub index: u32,
    /// The grammatical role (a form-role identifier), where analyzed.
    pub role: Option<String>,
    /// The Universal-Dependencies dependency relation to the head, where analyzed.
    pub dep_relation: Option<String>,
    /// The index of the head slot this one depends on, where a dependency analysis
    /// is present.
    pub depends_on: Option<u32>,
    /// The constituent form filling the slot.
    pub form: Form,
}

/// A structured form — the identity-bearing strata of the form AST. A form names
/// exactly one sign system and is identified by its structural content alone.
#[derive(Clone, Debug)]
pub enum Form {
    /// The abstract smallest meaningful unit (identity-bearing morpheme).
    Morpheme {
        /// The sign system the morpheme belongs to.
        sign_system: String,
        /// A stable identifier distinguishing this morpheme within its sign system.
        id: String,
    },
    /// The concrete realization of a morpheme, with its typed features.
    Morph {
        /// The sign system the morph belongs to.
        sign_system: String,
        /// The morpheme this morph realizes.
        morpheme: Box<Form>,
        /// The typed morphological features carried by the morph.
        features: Vec<MorphFeature>,
    },
    /// A dictionary word — the lemma, distinct from its inflections.
    Lexeme {
        /// The sign system the lexeme belongs to.
        sign_system: String,
        /// The canonical lemma identifier of the lexeme.
        lemma: String,
        /// The part of speech, where declared.
        part_of_speech: Option<String>,
    },
    /// An inflected form of a lexeme, carrying the distinguishing features.
    WordForm {
        /// The sign system the word form belongs to.
        sign_system: String,
        /// The lexeme this word form inflects.
        lexeme: Box<Form>,
        /// The morphological features distinguishing this inflection.
        features: Vec<MorphFeature>,
    },
    /// An orthographic token spanning one or more syntactic words (a multiword token).
    OrthographicWord {
        /// The sign system the token belongs to.
        sign_system: String,
        /// The syntactic words the orthographic token spans, in order.
        spans: Vec<Form>,
    },
    /// A structurally-present but surface-absent constituent — a zero morpheme, trace,
    /// elision, or pro-dropped element. Keys distinctly from an absent constituent.
    Covert {
        /// The sign system the covert form belongs to.
        sign_system: String,
        /// The features the covert form carries (e.g. a zero plural's `Number=Plur`).
        features: Vec<MorphFeature>,
    },
    /// A phrase, clause, sentence, or text as a tree over indexed slots, optionally
    /// scoped to a named analysis so competing parses stay distinct.
    Composed {
        /// The matrix sign system of the composed form.
        sign_system: String,
        /// The composition level (e.g. `sentence`), a level identifier.
        level: String,
        /// The analysis this parse belongs to, where co-resident analyses exist. Two
        /// otherwise-identical composed forms in distinct analyses have distinct keys.
        analysis: Option<String>,
        /// The index of the head constituent, where headedness is analyzed.
        head: Option<u32>,
        /// The constituent slots. Authoring order does not affect identity — the key
        /// orders slots by index.
        slots: Vec<Slot>,
    },
}

/// A concrete surface realization of a form: text with a declared script, encoding,
/// Unicode normalization, and collation locale. Interned by its material identity
/// ([`SurfaceForm::surface_key`]), which is deliberately disjoint from form identity.
#[derive(Clone, Debug)]
pub struct SurfaceForm {
    /// The concrete text of the surface.
    pub text: String,
    /// The script the surface is written in (a script identifier).
    pub script: String,
    /// The character encoding of the surface bytes (e.g. `UTF-8`).
    pub encoding: String,
    /// The Unicode normalization form (e.g. `NFC`).
    pub normalization: String,
    /// The collation/case-folding locale (e.g. `en`).
    pub collation: String,
}

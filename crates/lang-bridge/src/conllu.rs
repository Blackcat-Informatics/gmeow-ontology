// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`ConlluBridge`]: lift a CoNLL-U treebank into the analyzed form AST AND round-trip
//! its bytes exactly.
//!
//! The design turns on a **lens complement**. The form AST is a LOSSY VIEW of a CoNLL-U
//! file: it captures the syntactic words, their lemmas, their UPOS, their typed FEATS, and
//! the HEAD/DEPREL dependency edges — but NOT the `# …` comment lines, the raw MISC column
//! (`SpaceAfter=No`, …), the XPOS/DEPS columns, the multiword-token surface ranges, or the
//! enhanced empty nodes (`n.m`). A content-key isomorphism over the form view is therefore
//! NOT byte-equivalence.
//!
//! To re-emit byte-for-byte, the bridge keeps the WHOLE file in a full-fidelity
//! [`ConlluDoc`] — every one of the ten CoNLL-U columns verbatim, every comment line in
//! order, and the blank-line sentence separators — and the form AST is projected OFF that
//! model rather than replacing it. The complement (comments, MISC, XPOS, DEPS, MWT text,
//! empty nodes) lives in the [`ConlluDoc`] and is reproduced on [`serialize`], so nothing is
//! dropped and [`serialize`]`(`[`parse`]`(bytes)) == bytes` holds byte-for-byte for any
//! well-formed input already in the declared normal form.
//!
//! The declared normal form is deliberately minimal — the bridge does NOT reorder FEATS,
//! canonicalize spacing, or rewrite any column; it stores each column's bytes as-is, so a
//! file in the CoNLL-U line grammar (LF line endings, one trailing blank line per sentence)
//! round-trips with no normalization at all. A file NOT in that grammar (wrong column count,
//! malformed ID, bad FEATS syntax, a missing terminating blank line, non-UTF-8) is a HARD
//! FAIL naming the offending construct — never a silent repair.
//!
//! Like every `lang:` bridge, the lift CARRIES a `logic:Correspondence` (an
//! [`Isomorphism`](MorphismClass::Isomorphism) with a discharged `SectionLaw`) rather than a
//! bespoke round-trip harness. The trait stays honest: [`Bridge::emit`] renders only the
//! best-effort surface off the lossy form view, while the byte-exact identity round-trip is
//! the dedicated [`ConlluBridge::round_trip`] over the full-fidelity model.

use gmeow_lang_form::{AnalysisLevel, Form, MorphFeature, Slot, SurfaceForm};
use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeCondition,
    DischargeVerdict, LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind,
};

use crate::bridge::{Bridge, IngestDiagnostic, LangFailure, Lifted};
use crate::emit::digest16;
use crate::plain_text::{UNDETERMINED_SCRIPT, normalization_label};
use crate::registry::{
    EmittedArtifact, LangEmission, LangProjectionInput, LangProjectionTarget, NamedSource,
};

/// The `lang:` namespace base, byte-identical to the other `lang:` producers so every
/// `lang:` local name resolves to the same IRI across bridges.
use gmeow_ns::LANG_NS;

/// The single Universal-Dependencies sign-system IRI a CoNLL-U lift declares for the forms
/// it projects. A per-language sign system would be more precise; a single constant is the
/// honest declaration for a treebank whose language is not resolved here.
pub const UD_SIGN_SYSTEM: &str =
    "https://blackcatinformatics.ca/lang/sign-system/universal-dependencies";

/// The example-instance base the carried round-trip correspondence IRI lives under, matching
/// the base every other `lang:` producer content-addresses its minted individuals under.
const CONLLU_CORR_BASE: &str = "http://example.org/lang/conllu-correspondence/";

/// The `logic:getLeg` program IRI: parse a CoNLL-U byte stream into the full-fidelity model.
const CONLLU_GET_LEG: &str = "https://blackcatinformatics.ca/lang/conlluParseLeg";

/// The `logic:putLeg` program IRI: serialize the full-fidelity model back to bytes.
const CONLLU_PUT_LEG: &str = "https://blackcatinformatics.ca/lang/conlluSerializeLeg";

/// A CoNLL-U token identifier — the first column, whose lexical shape distinguishes a
/// syntactic word, a multiword-token range, and an enhanced empty node. Each variant
/// reconstructs its column text exactly (no leading zeros in well-formed input), so the ID
/// round-trips byte-for-byte off the parsed enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenId {
    /// A syntactic word line: a bare integer (`3`).
    Simple(u32),
    /// A multiword-token range line (`3-4`): the inclusive span of syntactic-word IDs it
    /// covers, `start < end`.
    Mwt(u32, u32),
    /// An enhanced empty-node line (`1.1`): a decimal ID for a null node in the enhanced
    /// dependency graph.
    Empty(u32, u32),
}

impl TokenId {
    /// Reconstruct the exact column-0 text for this ID.
    fn to_column(&self) -> String {
        match self {
            TokenId::Simple(n) => n.to_string(),
            TokenId::Mwt(a, b) => format!("{a}-{b}"),
            TokenId::Empty(a, b) => format!("{a}.{b}"),
        }
    }
}

/// A single CoNLL-U token line — all ten columns retained verbatim. The [`TokenId`] parses
/// column 0 (its text is reconstructed losslessly); every other column is the raw string as
/// it appeared, so the MISC, XPOS, and DEPS complement is reproduced on emit unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConlluToken {
    /// Column 1 — the token ID.
    pub id: TokenId,
    /// Column 2 — the surface `FORM`.
    pub form: String,
    /// Column 3 — the `LEMMA`.
    pub lemma: String,
    /// Column 4 — the universal POS tag `UPOS`.
    pub upos: String,
    /// Column 5 — the language-specific POS tag `XPOS`.
    pub xpos: String,
    /// Column 6 — the raw `FEATS` string (validated but retained verbatim).
    pub feats: String,
    /// Column 7 — the `HEAD` (a token ID or `_`).
    pub head: String,
    /// Column 8 — the dependency relation `DEPREL`.
    pub deprel: String,
    /// Column 9 — the enhanced-dependency `DEPS`.
    pub deps: String,
    /// Column 10 — the `MISC` field (e.g. `SpaceAfter=No`).
    pub misc: String,
}

/// One CoNLL-U sentence: its comment lines (retained verbatim, in order) and its token
/// lines. The blank line that terminates the sentence is structural and reproduced on emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConlluSentence {
    /// The full comment lines (each including its leading `#`), in file order.
    pub comments: Vec<String>,
    /// The token lines, in file order.
    pub tokens: Vec<ConlluToken>,
}

/// A whole CoNLL-U document: an ordered list of sentences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConlluDoc {
    /// The sentences, in file order.
    pub sentences: Vec<ConlluSentence>,
}

/// Hard-fail helper: a CoNLL-U spec violation names the exact construct it could not account
/// for. The bridge refuses rather than proceeding — proceeding would silently drop or
/// misrepresent the unaccountable construct, the `lang:SilentIngestDrop` floor.
fn spec_violation(construct: impl Into<String>) -> IngestDiagnostic {
    IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: construct.into(),
    }
}

/// Parse a raw string ID column into a [`TokenId`], or name the malformed construct.
fn parse_token_id(col: &str) -> Result<TokenId, IngestDiagnostic> {
    if let Some((a, b)) = col.split_once('-') {
        let start: u32 = a
            .parse()
            .map_err(|_| spec_violation(format!("malformed multiword-token range ID: '{col}'")))?;
        let end: u32 = b
            .parse()
            .map_err(|_| spec_violation(format!("malformed multiword-token range ID: '{col}'")))?;
        if start >= end {
            return Err(spec_violation(format!(
                "multiword-token range must have start < end: '{col}'"
            )));
        }
        return Ok(TokenId::Mwt(start, end));
    }
    if let Some((a, b)) = col.split_once('.') {
        let major: u32 = a
            .parse()
            .map_err(|_| spec_violation(format!("malformed empty-node ID: '{col}'")))?;
        let minor: u32 = b
            .parse()
            .map_err(|_| spec_violation(format!("malformed empty-node ID: '{col}'")))?;
        return Ok(TokenId::Empty(major, minor));
    }
    let n: u32 = col
        .parse()
        .map_err(|_| spec_violation(format!("malformed token ID: '{col}'")))?;
    Ok(TokenId::Simple(n))
}

/// Split a FEATS key into its base key and optional `[layer]` (the Universal-Dependencies
/// `Number[psor]` convention), validating that a present layer is non-empty.
fn split_feature_key(key: &str) -> Result<(String, Option<String>), IngestDiagnostic> {
    if let Some(open) = key.find('[') {
        if !key.ends_with(']') {
            return Err(spec_violation(format!(
                "malformed FEATS layer in key: '{key}'"
            )));
        }
        let base = &key[..open];
        let layer = &key[open + 1..key.len() - 1];
        if base.is_empty() || layer.is_empty() {
            return Err(spec_violation(format!(
                "empty FEATS key or layer in: '{key}'"
            )));
        }
        return Ok((base.to_owned(), Some(layer.to_owned())));
    }
    if key.is_empty() {
        return Err(spec_violation("empty FEATS key".to_owned()));
    }
    Ok((key.to_owned(), None))
}

/// Parse a raw FEATS column (`_` or `Key=Val,Val|Key2=Val`) into typed [`MorphFeature`]s.
/// A syntactically malformed FEATS string is a HARD FAIL naming the construct — the raw
/// string is still retained on the token for byte-exact emit, but the form projection cannot
/// invent structure over garbage. Values are a SET: they key order-independently.
pub fn parse_feats(feats: &str) -> Result<Vec<MorphFeature>, IngestDiagnostic> {
    if feats == "_" {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in feats.split('|') {
        let (key, vals) = part
            .split_once('=')
            .ok_or_else(|| spec_violation(format!("malformed FEATS entry (no '='): '{part}'")))?;
        let (base, layer) = split_feature_key(key)?;
        let mut values = Vec::new();
        for v in vals.split(',') {
            if v.is_empty() {
                return Err(spec_violation(format!(
                    "empty FEATS value in entry: '{part}'"
                )));
            }
            if !values.iter().any(|existing: &String| existing == v) {
                values.push(v.to_owned());
            }
        }
        out.push(MorphFeature {
            key: base,
            values,
            layer,
        });
    }
    Ok(out)
}

/// Parse a CoNLL-U byte stream into the full-fidelity [`ConlluDoc`], or HARD FAIL naming the
/// offending construct. Non-UTF-8 input is a [`LangFailure::NonUtf8Surface`]; every other
/// violation (column count, malformed ID, bad FEATS, structural blank-line / newline
/// deviations) is a [`LangFailure::SilentIngestDrop`] — the bridge refuses rather than
/// dropping the unaccountable construct silently.
pub fn parse(bytes: &[u8]) -> Result<ConlluDoc, IngestDiagnostic> {
    let text = std::str::from_utf8(bytes).map_err(|e| IngestDiagnostic {
        failure_class: LangFailure::NonUtf8Surface,
        construct: format!(
            "non-UTF-8 CoNLL-U input: {} byte(s), first invalid byte at index {}",
            bytes.len(),
            e.valid_up_to()
        ),
    })?;
    if text.is_empty() {
        return Err(spec_violation(
            "empty CoNLL-U input (no sentences)".to_owned(),
        ));
    }
    if !text.ends_with('\n') {
        return Err(spec_violation(
            "CoNLL-U input must end with a newline-terminated blank line".to_owned(),
        ));
    }
    // Every physical line was `\n`-terminated; the trailing `\n` yields a final empty
    // element from `split`, which is dropped so it is not mistaken for a blank line.
    let mut lines: Vec<&str> = text.split('\n').collect();
    lines.pop();

    let mut sentences = Vec::new();
    let mut comments = Vec::new();
    let mut tokens = Vec::new();

    for line in lines {
        if line.is_empty() {
            // A blank line terminates the current sentence. A blank line with nothing to
            // terminate (leading or doubled blank) is a structural violation.
            if comments.is_empty() && tokens.is_empty() {
                return Err(spec_violation(
                    "unexpected blank line (no sentence to terminate)".to_owned(),
                ));
            }
            sentences.push(ConlluSentence {
                comments: std::mem::take(&mut comments),
                tokens: std::mem::take(&mut tokens),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            // Comments must precede the token block of their sentence.
            if !tokens.is_empty() {
                return Err(spec_violation(format!(
                    "comment line after tokens in a sentence: '#{rest}'"
                )));
            }
            comments.push(line.to_owned());
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 10 {
            return Err(spec_violation(format!(
                "expected 10 tab-separated columns, got {}: '{line}'",
                cols.len()
            )));
        }
        let id = parse_token_id(cols[0])?;
        // Validate FEATS now (raw string retained for byte-exact emit); a malformed FEATS
        // must hard-fail rather than surface a repaired or partial feature set.
        parse_feats(cols[5])?;
        // A syntactic word must carry an integer HEAD or `_`; MWT/empty lines carry `_`.
        if let TokenId::Simple(_) = id
            && cols[6] != "_"
            && cols[6].parse::<u32>().is_err()
        {
            return Err(spec_violation(format!(
                "malformed HEAD on a syntactic word: '{}'",
                cols[6]
            )));
        }
        tokens.push(ConlluToken {
            id,
            form: cols[1].to_owned(),
            lemma: cols[2].to_owned(),
            upos: cols[3].to_owned(),
            xpos: cols[4].to_owned(),
            feats: cols[5].to_owned(),
            head: cols[6].to_owned(),
            deprel: cols[7].to_owned(),
            deps: cols[8].to_owned(),
            misc: cols[9].to_owned(),
        });
    }

    // A well-formed file terminates its last sentence with a blank line, so nothing may be
    // left un-flushed.
    if !comments.is_empty() || !tokens.is_empty() {
        return Err(spec_violation(
            "last sentence is not terminated by a blank line".to_owned(),
        ));
    }
    if sentences.is_empty() {
        return Err(spec_violation("no sentences parsed".to_owned()));
    }
    Ok(ConlluDoc { sentences })
}

/// Serialize the full-fidelity [`ConlluDoc`] back to CoNLL-U bytes. For any document
/// produced by [`parse`], `serialize(parse(bytes)) == bytes` byte-for-byte: each comment
/// line, each token line (all ten columns, tab-joined, ID reconstructed exactly), and the
/// terminating blank line are reproduced verbatim, with no column normalization.
pub fn serialize(doc: &ConlluDoc) -> Vec<u8> {
    let mut out = String::new();
    for sentence in &doc.sentences {
        for comment in &sentence.comments {
            out.push_str(comment);
            out.push('\n');
        }
        for token in &sentence.tokens {
            let cols = [
                token.id.to_column(),
                token.form.clone(),
                token.lemma.clone(),
                token.upos.clone(),
                token.xpos.clone(),
                token.feats.clone(),
                token.head.clone(),
                token.deprel.clone(),
                token.deps.clone(),
                token.misc.clone(),
            ];
            out.push_str(&cols.join("\t"));
            out.push('\n');
        }
        // The blank line that terminates the sentence.
        out.push('\n');
    }
    out.into_bytes()
}

/// Project one [`ConlluSentence`] into a fully-parsed [`Form::Composed`] over the form AST —
/// the LOSSY VIEW. The syntactic words become the constituent slots; a multiword-token range
/// becomes a [`Form::OrthographicWord`] spanning the syntactic words it covers; the enhanced
/// empty nodes are enhanced-graph-only and do not enter the basic constituency tree (they
/// survive in the [`ConlluDoc`] complement). FEATS parse into [`MorphFeature`]s; HEAD/DEPREL
/// land on the slot as the dependency edge. The composed form sits at
/// [`AnalysisLevel::Parsed`] (see [`analysis_level`]).
pub fn to_forms(sentence: &ConlluSentence) -> Result<Form, IngestDiagnostic> {
    let word_form = |t: &ConlluToken| -> Result<Form, IngestDiagnostic> {
        let part_of_speech = if t.upos == "_" {
            None
        } else {
            Some(t.upos.clone())
        };
        Ok(Form::WordForm {
            sign_system: UD_SIGN_SYSTEM.to_owned(),
            lexeme: Box::new(Form::Lexeme {
                sign_system: UD_SIGN_SYSTEM.to_owned(),
                lemma: t.lemma.clone(),
                part_of_speech,
            }),
            // Validated at parse time, so re-parsing the retained raw string re-surfaces the
            // same typed diagnostic rather than silently defaulting.
            features: parse_feats(&t.feats)?,
        })
    };

    let mut slots = Vec::new();
    let mut head_slot: Option<u32> = None;
    let mut slot_index: u32 = 0;
    let mut i = 0;
    while i < sentence.tokens.len() {
        match &sentence.tokens[i].id {
            TokenId::Mwt(a, b) => {
                // Consume the syntactic words the range covers as its spans.
                let mut spans = Vec::new();
                let mut j = i + 1;
                while j < sentence.tokens.len() {
                    if let TokenId::Simple(n) = sentence.tokens[j].id
                        && n >= *a
                        && n <= *b
                    {
                        spans.push(word_form(&sentence.tokens[j])?);
                        j += 1;
                        continue;
                    }
                    break;
                }
                slots.push(Slot {
                    index: slot_index,
                    role: None,
                    dep_relation: None,
                    depends_on: None,
                    form: Form::OrthographicWord {
                        sign_system: UD_SIGN_SYSTEM.to_owned(),
                        spans,
                    },
                });
                slot_index += 1;
                i = j;
            }
            TokenId::Simple(_) => {
                let t = &sentence.tokens[i];
                let dep_relation = if t.deprel == "_" {
                    None
                } else {
                    Some(t.deprel.clone())
                };
                // HEAD 0 is the root sentinel (no head); any other value is the ID of the
                // token this one depends on.
                let head_value = t.head.parse::<u32>().ok();
                if head_value == Some(0) {
                    head_slot = Some(slot_index);
                }
                let depends_on = head_value.filter(|&h| h != 0);
                slots.push(Slot {
                    index: slot_index,
                    role: None,
                    dep_relation,
                    depends_on,
                    form: word_form(t)?,
                });
                slot_index += 1;
                i += 1;
            }
            TokenId::Empty(_, _) => {
                // Enhanced-graph null node: not a basic-tree constituent. Retained in the
                // ConlluDoc complement, absent from the constituency projection.
                i += 1;
            }
        }
    }

    Ok(Form::Composed {
        sign_system: UD_SIGN_SYSTEM.to_owned(),
        level: "sentence".to_owned(),
        analysis: None,
        head: head_slot,
        slots,
    })
}

/// The analysis level a CoNLL-U lift reaches: a full constituency-and-dependency
/// [`AnalysisLevel::Parsed`]. CoNLL-U carries the strongest analyzed stratum, so the lift
/// records the parsed level honestly rather than the weaker tokenized one.
#[must_use]
pub fn analysis_level() -> AnalysisLevel {
    AnalysisLevel::Parsed
}

/// The get/put [`LegPath`] pair the carried correspondence's round-trip is decided over:
/// the put leg is the structural inverse of the get leg, so
/// [`exact_round_trip_holds`](crate::exact_round_trip_holds) holds by construction.
#[must_use]
pub fn conllu_leg_pair() -> (LegPath, LegPath) {
    let get = LegPath::Seq(vec![
        LegPath::Step(format!("{LANG_NS}parseConlluRow")),
        LegPath::Step(format!("{LANG_NS}retainColumnComplement")),
    ]);
    let put = get.invert();
    (get, put)
}

/// Build the EXACT round-trip `logic:Correspondence` a CoNLL-U lift carries for a document
/// whose serialization hashes to `source_key`: an [`Isomorphism`](MorphismClass::Isomorphism)
/// on the satisfaction-preserving rung, `mnemomorphic` (the full-fidelity model retains the
/// whole source), whose `GetPut`, `PutGet`, and `SectionLaw` claims are conclusively
/// discharged — the identity byte round-trip trivially satisfies them. The IRI is
/// content-addressed on the serialized bytes, so the same document always carries the same
/// correspondence.
pub fn conllu_correspondence(source_key: &str) -> Correspondence {
    let iri = format!(
        "{CONLLU_CORR_BASE}{}",
        digest16("lang-conllu-corr", source_key)
    );
    let discharged = |law: CorrespondenceLaw| LawClaimIr {
        law,
        verdict: DischargeVerdict::ObligationDischarged,
        condition: Some(DischargeCondition::DischargeSyntacticReachability),
    };
    Correspondence::new(
        iri,
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        // The full-fidelity ConlluDoc retains the whole source witness (the complement),
        // which is exactly what lets the byte round-trip claim SectionLaw.
        true,
        Some(Determinacy::Crisp),
        Some(CONLLU_GET_LEG.to_owned()),
        Some(CONLLU_PUT_LEG.to_owned()),
        vec![
            discharged(CorrespondenceLaw::GetPut),
            discharged(CorrespondenceLaw::PutGet),
            discharged(CorrespondenceLaw::SectionLaw),
        ],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("exact CoNLL-U correspondence is well-formed by construction")
}

/// The CoNLL-U bridge: lift a treebank into fully-parsed forms + per-sentence surfaces under
/// an exact round-trip `logic:Correspondence`, and round-trip its bytes exactly.
pub struct ConlluBridge;

impl ConlluBridge {
    /// The byte-exact identity round-trip (Gate 2): `serialize(parse(bytes))`. For any
    /// well-formed input this returns the input bytes unchanged; a malformed input HARD
    /// FAILS with the same [`IngestDiagnostic`] [`parse`] raises. This is the honest place
    /// the byte-exactness lives — NOT [`Bridge::emit`], which renders only the lossy surface.
    pub fn round_trip(&self, bytes: &[u8]) -> Result<Vec<u8>, IngestDiagnostic> {
        let doc = parse(bytes)?;
        Ok(serialize(&doc))
    }
}

impl Bridge for ConlluBridge {
    fn lift(&self, bytes: &[u8]) -> Result<Lifted, IngestDiagnostic> {
        let doc = parse(bytes)?;
        // Content-address the carried correspondence on the round-trip bytes.
        let serialized = serialize(&doc);
        let source_key = String::from_utf8(serialized).map_err(|e| IngestDiagnostic {
            failure_class: LangFailure::NonUtf8Surface,
            construct: format!(
                "serialized CoNLL-U is not UTF-8: first invalid byte at index {}",
                e.utf8_error().valid_up_to()
            ),
        })?;
        let correspondence = conllu_correspondence(&source_key);

        let forms: Vec<Form> = doc
            .sentences
            .iter()
            .map(to_forms)
            .collect::<Result<Vec<_>, _>>()?;

        // One surface per sentence: the `# text = …` value when the treebank supplies it,
        // else the space-joined surface forms of the sentence's surface tokens (multiword
        // ranges take their own FORM; the syntactic words they cover are not re-emitted).
        let surfaces: Vec<SurfaceForm> = doc
            .sentences
            .iter()
            .map(|s| {
                let text = sentence_text(s);
                let normalization = normalization_label(&text).to_owned();
                SurfaceForm {
                    text,
                    script: UNDETERMINED_SCRIPT.to_owned(),
                    encoding: "UTF-8".to_owned(),
                    normalization,
                    collation: "und".to_owned(),
                }
            })
            .collect();

        // One honest ledger row: the full-fidelity model round-trips exactly, so nothing is
        // dropped. A future lossy path would record its residue in `actual_drops`.
        let mut loss = crate::registry::LossLedger::new();
        let ledger = vec![crate::registry::emit_ledger_row(
            &mut loss,
            "conllu".to_owned(),
            String::new(),
            false,
            PreservationKind::Exact,
            "n/a".to_owned(),
            Vec::new(),
            Vec::new(),
        )];

        Ok(Lifted {
            forms,
            surfaces,
            correspondence,
            ledger,
            loss,
        })
    }

    fn emit(&self, lifted: &Lifted) -> Vec<u8> {
        // Best-effort surface off the LOSSY form view only — NOT the byte-exact round-trip
        // (that is `ConlluBridge::round_trip` over the full-fidelity model). Render the
        // lifted per-sentence surfaces, one per line.
        let mut text = lifted
            .surfaces
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text.into_bytes()
    }
}

/// The best-effort surface text of a sentence: the `# text = …` comment value when present
/// (the treebank's own rendering), else the space-joined FORM columns of the surface tokens
/// (a multiword range contributes its own FORM; the syntactic words it covers are omitted so
/// the surface is not double-counted).
fn sentence_text(sentence: &ConlluSentence) -> String {
    for comment in &sentence.comments {
        if let Some(rest) = comment.strip_prefix("# text = ") {
            return rest.to_owned();
        }
        if let Some(rest) = comment.strip_prefix("# text =") {
            return rest.trim_start().to_owned();
        }
    }
    let mut parts = Vec::new();
    let mut covered_until = 0u32;
    for token in &sentence.tokens {
        match &token.id {
            TokenId::Mwt(_, b) => {
                parts.push(token.form.clone());
                covered_until = *b;
            }
            TokenId::Simple(n) => {
                if *n > covered_until {
                    parts.push(token.form.clone());
                }
            }
            TokenId::Empty(_, _) => {}
        }
    }
    parts.join(" ")
}

// ── Forward projection: lang: model → CoNLL-U ───────────────────────────────────────

/// The Universal-Dependencies / CoNLL-U morphosyntax PROJECTION target: lowers each analyzed
/// `lang:ComposedForm` in the composed model FORWARD to CoNLL-U — word forms to token rows,
/// `lang:MorphFeature` pairs to the FEATS column, `lang:slotRole` to the UD deprel, the
/// `lang:formHead` to the dependency root. A form with several co-resident `lang:Analysis`
/// readings emits ONE CoNLL-U artifact per reading — never a silently chosen winner
/// (`lang:ProjectionSilentDisambiguation`). Faithful (Exact) for single-reading morphosyntax:
/// each emitted file byte-round-trips through [`ConlluBridge`].
///
/// This is the forward peer of [`ConlluBridge`] (which lifts CoNLL-U INTO the model for the
/// ingestion/runtime surface); it reads the model through the shared [`crate::rdf_scan`]
/// surface exactly as the TEI / OntoLex targets do.
pub struct ConlluTarget;

impl LangProjectionTarget for ConlluTarget {
    fn name(&self) -> &'static str {
        "conllu"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.lang_models {
            emissions.extend(emit_conllu_source(source)?);
        }
        Ok(emissions)
    }
}

/// One CoNLL-U emission per analyzed `lang:ComposedForm` (one with `lang:formSlot`) in a
/// single `lang:` RDF surface; a form with no slots is not analyzed and is skipped.
fn emit_conllu_source(source: &NamedSource) -> Result<Vec<LangEmission>, IngestDiagnostic> {
    let ds = crate::rdf_scan::parse_lang_turtle(&source.bytes, &source.name)?;
    let mut emissions = Vec::new();
    for form in crate::rdf_scan::subjects_of_type(&ds, &format!("{LANG_NS}ComposedForm")) {
        let slots = crate::rdf_scan::objects(&ds, form, &format!("{LANG_NS}formSlot"));
        if slots.is_empty() {
            continue;
        }
        emissions.push(emit_composed_form(&ds, source, form)?);
    }
    Ok(emissions)
}

/// Lower one analyzed composed form to a per-reading CoNLL-U emission. The co-resident
/// readings are the distinct `lang:Analysis` nodes the form is scoped to (through the form's
/// `lang:inAnalysis`, else the analyses its slots carry); each reading emits its own CoNLL-U
/// artifact, so the two parses of an ambiguous form never collapse to one silently-chosen tree.
fn emit_composed_form(
    ds: &purrdf::RdfDataset,
    source: &NamedSource,
    form: purrdf::TermId,
) -> Result<LangEmission, IngestDiagnostic> {
    let source_iri = crate::rdf_scan::iri_of(ds, form).unwrap_or_else(|| {
        format!(
            "http://example.org/lang/conllu/blank/{}",
            digest16("lang-conllu-blank", &crate::rdf_scan::term_label(ds, form))
        )
    });
    let form_local = crate::rdf_scan::local_name(&source_iri).to_owned();

    // The co-resident readings: the analyses the form is scoped to — either directly through the
    // form's own `lang:inAnalysis` OR through its slots' `lang:inAnalysis` (a form may leave its
    // analyses implicit on the form node and declare them per slot). The UNION is taken so a form
    // whose slots carry distinct analyses is NOT collapsed into a single silently-merged reading.
    // A form with no analysis anywhere is a single (synthetic) reading over all its slots.
    let in_analysis = format!("{LANG_NS}inAnalysis");
    let mut analyses = crate::rdf_scan::objects(ds, form, &in_analysis);
    for slot in crate::rdf_scan::objects(ds, form, &format!("{LANG_NS}formSlot")) {
        for a in crate::rdf_scan::objects(ds, slot, &in_analysis) {
            if !analyses.contains(&a) {
                analyses.push(a);
            }
        }
    }
    // Deterministic reading order independent of scan order.
    analyses.sort_by_cached_key(|&a| crate::rdf_scan::term_label(ds, a));
    let reading_keys: Vec<Option<purrdf::TermId>> = if analyses.is_empty() {
        vec![None]
    } else {
        analyses.into_iter().map(Some).collect()
    };

    let mut artifacts = Vec::with_capacity(reading_keys.len());
    let mut round_trip_holds = true;
    let mut corr_key = String::new();
    for (i, analysis) in reading_keys.iter().enumerate() {
        let sentence = build_sentence(ds, form, *analysis)?;
        let doc = ConlluDoc {
            sentences: vec![sentence],
        };
        let bytes = serialize(&doc);
        // Faithful-fragment discipline: the emitted CoNLL-U byte-round-trips through the bridge
        // (parse ∘ serialize is the identity for a canonical document); a non-round-trip is a
        // real defect, surfaced as round_trip_holds=false (never Exact-claimed dishonestly).
        match (ConlluBridge).round_trip(&bytes) {
            Ok(rt) if rt == bytes => {}
            _ => round_trip_holds = false,
        }
        corr_key.push_str(&String::from_utf8_lossy(&bytes));
        corr_key.push('\u{1f}');
        artifacts.push(EmittedArtifact {
            path_suffix: format!("conllu/{}.{form_local}.reading-{i}.conllu", source.name),
            bytes,
            is_rdf: false,
        });
    }

    let mut loss = crate::registry::LossLedger::new();
    Ok(LangEmission {
        artifacts,
        correspondence: conllu_correspondence(&corr_key),
        ledger: vec![crate::registry::emit_ledger_row(
            &mut loss,
            format!("conllu:{}#{form_local}", source.name),
            String::new(),
            false,
            PreservationKind::Exact,
            "n/a".to_owned(),
            Vec::new(),
            Vec::new(),
        )],
        loss,
        leg_pair: Some(conllu_leg_pair()),
        emitted_reading_count: Some(reading_keys.len() as u64),
        source_iri,
        unsupported: Vec::new(),
        round_trip_holds,
        lossy_kind: PreservationKind::Exact,
        source_rdf: Vec::new(),
    })
}

/// Build one CoNLL-U sentence from a composed form's slots scoped to `analysis` (all slots
/// when `analysis` is `None`). Constituent order is the `lang:slotIndex` order — a missing or
/// non-integer index is a HARD FAIL (word order cannot be inferred), never a silent reorder.
fn build_sentence(
    ds: &purrdf::RdfDataset,
    form: purrdf::TermId,
    analysis: Option<purrdf::TermId>,
) -> Result<ConlluSentence, IngestDiagnostic> {
    let in_analysis = format!("{LANG_NS}inAnalysis");
    let head_wf = crate::rdf_scan::object_iri(ds, form, &format!("{LANG_NS}formHead"));

    // Collect (slotIndex, slotTermId) for the slots in this reading, in index order.
    let mut slots: Vec<(i64, purrdf::TermId)> = Vec::new();
    for slot in crate::rdf_scan::objects(ds, form, &format!("{LANG_NS}formSlot")) {
        // Scope to the reading: a slot with an inAnalysis must match; a slot with none belongs
        // to every reading of the form.
        if let Some(a) = analysis {
            let slot_analyses = crate::rdf_scan::objects(ds, slot, &in_analysis);
            if !slot_analyses.is_empty() && !slot_analyses.contains(&a) {
                continue;
            }
        }
        let idx_lex = crate::rdf_scan::object_literal(ds, slot, &format!("{LANG_NS}slotIndex"))
            .ok_or_else(|| {
                crate::rdf_scan::unrepresentable(format!(
                    "lang:FormSlot {} has no lang:slotIndex; CoNLL-U token order is the \
                     slot-index order and cannot be inferred",
                    crate::rdf_scan::term_label(ds, slot)
                ))
            })?;
        let idx: i64 = idx_lex.trim().parse().map_err(|_| {
            crate::rdf_scan::unrepresentable(format!(
                "lang:slotIndex '{idx_lex}' on {} is not an integer",
                crate::rdf_scan::term_label(ds, slot)
            ))
        })?;
        slots.push((idx, slot));
    }
    slots.sort_by_key(|(idx, _)| *idx);
    // Constituent order is identity-bearing: a duplicate lang:slotIndex is ambiguous word order
    // and a HARD FAIL, never a silently-chosen token order.
    if let Some(dup) = slots.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(crate::rdf_scan::unrepresentable(format!(
            "duplicate lang:slotIndex {} among the slots of composed form {} — CoNLL-U token order \
             is the slot-index order and a repeated index is ambiguous",
            dup[0].0,
            crate::rdf_scan::term_label(ds, form)
        )));
    }

    // The index→ID map (CoNLL-U is 1-based) so a slot's HEAD points at the right token.
    let id_of: std::collections::BTreeMap<i64, u32> = slots
        .iter()
        .enumerate()
        .map(|(i, (idx, _))| (*idx, (i + 1) as u32))
        .collect();
    // The root token's 1-based ID: the slot whose word form is the form's lang:formHead.
    let root_id: Option<u32> = slots.iter().find_map(|(idx, slot)| {
        let wf = crate::rdf_scan::objects(ds, *slot, &format!("{LANG_NS}slotForm"))
            .into_iter()
            .next()?;
        let wf_iri = crate::rdf_scan::iri_of(ds, wf)?;
        (head_wf.as_deref() == Some(wf_iri.as_str())).then(|| id_of[idx])
    });

    let mut tokens = Vec::with_capacity(slots.len());
    for (i, (idx, slot)) in slots.iter().enumerate() {
        let token_id = (i + 1) as u32;
        let wf = crate::rdf_scan::objects(ds, *slot, &format!("{LANG_NS}slotForm"))
            .into_iter()
            .next();
        let form_text = wf
            .and_then(|w| crate::rdf_scan::label_of(ds, w))
            .unwrap_or_else(|| "_".to_owned());
        // LEMMA: the lexeme the word form inflects; UPOS: that lexeme's part of speech.
        let (lemma, upos) = match wf {
            Some(w) => {
                let lex = crate::rdf_scan::object_iri(ds, w, &format!("{LANG_NS}inflectionOf"))
                    .and_then(|lex_iri| crate::rdf_scan::iri_id(ds, &lex_iri));
                let lemma = lex
                    .and_then(|l| crate::rdf_scan::label_of(ds, l))
                    .unwrap_or_else(|| form_text.clone());
                let upos = lex
                    .and_then(|l| {
                        crate::rdf_scan::object_iri(ds, l, &format!("{LANG_NS}partOfSpeech"))
                    })
                    .map(|pos| ud_upos(crate::rdf_scan::local_name(&pos)).to_owned())
                    .unwrap_or_else(|| "X".to_owned());
                (lemma, upos)
            }
            None => (form_text.clone(), "X".to_owned()),
        };
        let feats = wf
            .map(|w| ud_feats(ds, w))
            .unwrap_or_else(|| "_".to_owned());

        // HEAD/DEPREL: the root is the formHead slot (HEAD 0, deprel root). A non-root slot
        // attaches to the slot it lang:dependsOn (else to the root), with the UD deprel read
        // off its lang:slotRole / lang:dependencyRelation.
        let is_root = root_id == Some(token_id);
        let (head, deprel) = if is_root {
            ("0".to_owned(), "root".to_owned())
        } else {
            let dep_target = crate::rdf_scan::objects(ds, *slot, &format!("{LANG_NS}dependsOn"))
                .into_iter()
                .next()
                .and_then(|dep_slot| {
                    crate::rdf_scan::object_literal(ds, dep_slot, &format!("{LANG_NS}slotIndex"))
                })
                .and_then(|l| l.trim().parse::<i64>().ok())
                .and_then(|dep_idx| id_of.get(&dep_idx).copied());
            let head = dep_target
                .or(root_id)
                .map(|h| h.to_string())
                .unwrap_or_else(|| "0".to_owned());
            let role = crate::rdf_scan::object_iri(ds, *slot, &format!("{LANG_NS}slotRole"))
                .or_else(|| {
                    crate::rdf_scan::object_iri(ds, *slot, &format!("{LANG_NS}dependencyRelation"))
                });
            let deprel = role
                .map(|r| ud_deprel(crate::rdf_scan::local_name(&r)).to_owned())
                .unwrap_or_else(|| "dep".to_owned());
            (head, deprel)
        };
        let _ = idx;
        tokens.push(ConlluToken {
            id: TokenId::Simple(token_id),
            form: form_text,
            lemma,
            upos,
            xpos: "_".to_owned(),
            feats,
            head,
            deprel,
            deps: "_".to_owned(),
            misc: "_".to_owned(),
        });
    }

    Ok(ConlluSentence {
        comments: Vec::new(),
        tokens,
    })
}

/// Map a `lang:partOfSpeech` local name to its UD UPOS tag; `X` (the UD "other" tag, a valid
/// faithful encoding) where no specific mapping exists.
fn ud_upos(local: &str) -> &'static str {
    match local {
        "noun" => "NOUN",
        "verb" => "VERB",
        "adjective" => "ADJ",
        "adverb" => "ADV",
        "pronoun" => "PRON",
        "adposition" => "ADP",
        "determiner" => "DET",
        "numeral" => "NUM",
        "conjunction" => "CCONJ",
        "interjection" => "INTJ",
        "properNoun" => "PROPN",
        _ => "X",
    }
}

/// Map a `lang:slotRole` / `lang:dependencyRelation` local name to its UD dependency relation;
/// `dep` (the UD unspecified-dependency label) where no specific mapping exists.
fn ud_deprel(local: &str) -> &'static str {
    match local {
        "subjectRole" => "nsubj",
        "objectRole" => "obj",
        "predicateRole" => "root",
        "obliqueRole" => "obl",
        "modifierRole" => "advmod",
        "determinerRole" => "det",
        "complementRole" => "ccomp",
        _ => "dep",
    }
}

/// Render a word form's `lang:MorphFeature` pairs as the sorted UD FEATS column
/// (`Key=Val|Key2=Val2`), or `_` when the form carries none. Unmapped features are carried
/// through with their model-local key/value so the morphology is never silently dropped.
fn ud_feats(ds: &purrdf::RdfDataset, wf: purrdf::TermId) -> String {
    let mut feats: Vec<String> = Vec::new();
    for feat in crate::rdf_scan::objects(ds, wf, &format!("{LANG_NS}morphFeature")) {
        let key = crate::rdf_scan::object_iri(ds, feat, &format!("{LANG_NS}featureKey"));
        let val = crate::rdf_scan::object_iri(ds, feat, &format!("{LANG_NS}featureValue"));
        if let (Some(k), Some(v)) = (key, val) {
            let (uk, uv) = ud_feature(
                crate::rdf_scan::local_name(&k),
                crate::rdf_scan::local_name(&v),
            );
            feats.push(format!("{uk}={uv}"));
        }
    }
    feats.sort();
    feats.dedup();
    if feats.is_empty() {
        "_".to_owned()
    } else {
        feats.join("|")
    }
}

/// Map a `lang:MorphFeature` (`featureKey` / `featureValue` local names) to a UD (Feature,
/// Value) pair; where no specific mapping exists, the model-local names are title-cased and
/// carried through verbatim (the morphology is coarsened, never dropped).
fn ud_feature(key: &str, value: &str) -> (String, String) {
    let feature = match key {
        "featNumber" => "Number",
        "featTense" => "Tense",
        "featGender" => "Gender",
        "featPerson" => "Person",
        "featCase" => "Case",
        _ => return (title_case(strip_feat(key)), title_case(strip_val(value))),
    };
    let uv = match (key, value) {
        ("featNumber", "valPlur") => "Plur",
        ("featNumber", "valSing") => "Sing",
        ("featNumber", "valDual") => "Dual",
        ("featTense", "valPres") => "Pres",
        ("featTense", "valPast") => "Past",
        ("featTense", "valFut") => "Fut",
        ("featGender", "valMasc") => "Masc",
        ("featGender", "valFem") => "Fem",
        ("featGender", "valNeut") => "Neut",
        _ => return (feature.to_owned(), title_case(strip_val(value))),
    };
    (feature.to_owned(), uv.to_owned())
}

/// Strip a `feat` prefix from a feature-key local name (`featAspect` → `Aspect`).
fn strip_feat(key: &str) -> &str {
    key.strip_prefix("feat").unwrap_or(key)
}

/// Strip a `val` prefix from a feature-value local name (`valPerf` → `Perf`).
fn strip_val(value: &str) -> &str {
    value.strip_prefix("val").unwrap_or(value)
}

/// Title-case an ASCII identifier's first letter (a deterministic FEATS-column fallback).
fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod forward_tests {
    use super::*;
    use crate::is_exact_correspondence;

    const SENTENCE: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .

ex:lexCat a lang:Lexeme ; rdfs:label "cat" ; lang:partOfSpeech lang:noun .
ex:lexChase a lang:Lexeme ; rdfs:label "chase" ; lang:partOfSpeech lang:verb .
ex:lexMouse a lang:Lexeme ; rdfs:label "mouse" ; lang:partOfSpeech lang:noun .
ex:featPlur a lang:MorphFeature ; lang:featureKey lang:featNumber ; lang:featureValue lang:valPlur .
ex:featPres a lang:MorphFeature ; lang:featureKey lang:featTense ; lang:featureValue lang:valPres .
ex:wfCats a lang:WordForm ; rdfs:label "cats" ; lang:inflectionOf ex:lexCat ; lang:morphFeature ex:featPlur .
ex:wfChase a lang:WordForm ; rdfs:label "chase" ; lang:inflectionOf ex:lexChase ; lang:morphFeature ex:featPres .
ex:wfMice a lang:WordForm ; rdfs:label "mice" ; lang:inflectionOf ex:lexMouse ; lang:morphFeature ex:featPlur .

ex:analysis a lang:Analysis .
ex:sent a lang:ComposedForm ; rdfs:label "cats chase mice" ; lang:inAnalysis ex:analysis ;
    lang:formHead ex:wfChase ; lang:formSlot ex:s0 , ex:s1 , ex:s2 .
ex:s0 a lang:FormSlot ; lang:inAnalysis ex:analysis ; lang:slotIndex 0 ; lang:slotForm ex:wfCats ;
    lang:slotRole lang:subjectRole ; lang:dependsOn ex:s1 .
ex:s1 a lang:FormSlot ; lang:inAnalysis ex:analysis ; lang:slotIndex 1 ; lang:slotForm ex:wfChase ;
    lang:slotRole lang:predicateRole .
ex:s2 a lang:FormSlot ; lang:inAnalysis ex:analysis ; lang:slotIndex 2 ; lang:slotForm ex:wfMice ;
    lang:slotRole lang:objectRole ; lang:dependsOn ex:s1 .
"#;

    fn source() -> NamedSource {
        NamedSource {
            name: "s".to_owned(),
            bytes: SENTENCE.as_bytes().to_vec(),
        }
    }

    #[test]
    fn composed_form_lowers_to_a_ud_tree() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let emissions = ConlluTarget.emit(&input).expect("emit");
        assert_eq!(
            emissions.len(),
            1,
            "one emission per analyzed composed form"
        );
        let e = &emissions[0];
        assert_eq!(e.emitted_reading_count, Some(1));
        assert_eq!(
            e.artifacts.len(),
            1,
            "single reading ⇒ one CoNLL-U artifact"
        );
        let text = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();

        // The UD tree: cats(nsubj→chase) chase(root) mice(obj→chase), features lowered.
        assert!(
            text.contains("1\tcats\tcat\tNOUN\t_\tNumber=Plur\t2\tnsubj\t_\t_"),
            "{text}"
        );
        assert!(
            text.contains("2\tchase\tchase\tVERB\t_\tTense=Pres\t0\troot\t_\t_"),
            "{text}"
        );
        assert!(
            text.contains("3\tmice\tmouse\tNOUN\t_\tNumber=Plur\t2\tobj\t_\t_"),
            "{text}"
        );
        assert!(e.artifacts[0].path_suffix.ends_with(".reading-0.conllu"));

        // Faithful morphosyntax: byte round-trips, so the derived kind is Exact.
        assert!(e.round_trip_holds);
        assert!(is_exact_correspondence(&e.correspondence));
        assert_eq!(e.lossy_kind, PreservationKind::Exact);
    }

    #[test]
    fn two_analyses_emit_two_readings_never_one() {
        // One surface form scoped to TWO co-resident analyses that assign different heads —
        // each analysis emits its own CoNLL-U file; no reading is silently dropped.
        let doc = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:wSaw a lang:WordForm ; rdfs:label "saw" .
ex:wDuck a lang:WordForm ; rdfs:label "duck" .
ex:sent a lang:ComposedForm ; rdfs:label "saw duck" ;
    lang:inAnalysis ex:aBird , ex:aCrouch ;
    lang:formSlot ex:b0 , ex:b1 , ex:c0 , ex:c1 .
ex:aBird a lang:Analysis .
ex:aCrouch a lang:Analysis .
ex:b0 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .
ex:b1 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:objectRole ; lang:dependsOn ex:b0 .
ex:c0 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .
ex:c1 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:complementRole ; lang:dependsOn ex:c0 .
"#;
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "amb".to_owned(),
                bytes: doc.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let e = &ConlluTarget.emit(&input).expect("emit")[0];
        assert_eq!(e.emitted_reading_count, Some(2));
        assert_eq!(
            e.artifacts.len(),
            2,
            "two co-resident analyses ⇒ two CoNLL-U artifacts"
        );
    }

    #[test]
    fn slot_scoped_analyses_are_not_collapsed_without_a_form_level_in_analysis() {
        // The form declares NO lang:inAnalysis; its slots carry the two analysis scopes. The
        // readings must be recovered from the slots' scopes (union), never merged into one.
        let doc = "\
@prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex:   <http://example.org/lang/> .\n\
ex:wSaw a lang:WordForm ; rdfs:label \"saw\" .\n\
ex:wDuck a lang:WordForm ; rdfs:label \"duck\" .\n\
ex:sent a lang:ComposedForm ; rdfs:label \"saw duck\" ; lang:formSlot ex:b0 , ex:b1 , ex:c0 , ex:c1 .\n\
ex:aBird a lang:Analysis .\n\
ex:aCrouch a lang:Analysis .\n\
ex:b0 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:b1 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:objectRole ; lang:dependsOn ex:b0 .\n\
ex:c0 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:c1 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:complementRole ; lang:dependsOn ex:c0 .\n";
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "slot-scoped".to_owned(),
                bytes: doc.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let e = &ConlluTarget.emit(&input).expect("emit")[0];
        assert_eq!(
            e.emitted_reading_count,
            Some(2),
            "the two slot-scoped analyses must be recovered as two readings"
        );
        assert_eq!(
            e.artifacts.len(),
            2,
            "never collapsed into one merged reading"
        );
    }

    #[test]
    fn missing_slot_index_hard_fails() {
        let bad = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:w a lang:WordForm ; rdfs:label "x" .
ex:sent a lang:ComposedForm ; lang:formSlot ex:s0 .
ex:s0 a lang:FormSlot ; lang:slotForm ex:w .
"#;
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "bad".to_owned(),
                bytes: bad.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let err = ConlluTarget
            .emit(&input)
            .expect_err("missing slot index must hard-fail");
        assert!(err.construct.contains("no lang:slotIndex"), "{err:?}");
    }

    #[test]
    fn duplicate_slot_index_hard_fails() {
        // Two slots claim index 0 — ambiguous word order is a hard fail, never a silent pick.
        let dup = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:wA a lang:WordForm ; rdfs:label "a" .
ex:wB a lang:WordForm ; rdfs:label "b" .
ex:sent a lang:ComposedForm ; lang:formSlot ex:s0 , ex:s1 .
ex:s0 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wA .
ex:s1 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wB .
"#;
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "dup".to_owned(),
                bytes: dup.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let err = ConlluTarget
            .emit(&input)
            .expect_err("duplicate slot index must hard-fail");
        assert!(
            err.construct.contains("duplicate lang:slotIndex"),
            "{err:?}"
        );
    }

    #[test]
    fn emitter_is_byte_reproducible() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let a = ConlluTarget.emit(&input).expect("a");
        let b = ConlluTarget.emit(&input).expect("b");
        assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
    }
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The deterministic GMN operator ⇄ controlled-NL **verbalizer** (req #15).
//!
//! For every GMN operator form the carrier graph resolves — an executable glyph with a
//! `gmeow:gmnFixity`/`gmeow:gmnArity` signature denoting a term that carries an
//! `rdfs:label` — this module renders a bidirectional training pair: the GMN operator
//! surface (the glyph in operator position with schematic operand placeholders) ⇄ a
//! controlled-NL string (the denoted term's `rdfs:label` arranged around the SAME
//! placeholders). The arrangement is chosen ENTIRELY by the graph-authored fixity and
//! arity — there is **no hardcoded per-operator verb list**; the only per-operator datum
//! is the term's own `rdfs:label`.
//!
//! Two disciplines make the pair a sound *bidirectional* datum rather than a lossy caption:
//!
//! * **Injectivity.** No two DISTINCT operator forms may verbalize to the SAME controlled-NL
//!   string. Two terms that share an `rdfs:label` (a homograph) would collide; the collision
//!   is broken deterministically by appending each colliding form's compact CURIE
//!   (`⟪prefix:local⟫`). If a collision cannot be broken (two forms with the same CURIE and
//!   label), that is a HARD FAIL — "bidirectional" is unsound without injectivity.
//! * **Measured inverse.** The NL→GMN direction is a real *inverse template*
//!   ([`parse_nl`]) that re-parses the controlled-NL string back to `(fixity, label, arity)`
//!   by the placeholder skeleton, then resolves the operator form. Preservation is EXACT
//!   only when that inverse recovers the SAME form for every pair — [`round_trip_holds`]
//!   MEASURES it, never declares it.

use std::collections::BTreeMap;

use crate::gmn1_codec::GmnGlyphRegistry;

/// The four closed GMN fixity individuals (their `gmeow:` IRIs) — the template key. The
/// class is closed by the dialect grammar to exactly these four, so keying the arrangement
/// on the fixity IRI is table-driven over graph data, never an open per-operator dispatch.
pub const FIXITY_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/gmnFixityPrefix";
/// The infix fixity individual IRI.
pub const FIXITY_INFIX: &str = "https://blackcatinformatics.ca/gmeow/gmnFixityInfix";
/// The postfix fixity individual IRI.
pub const FIXITY_POSTFIX: &str = "https://blackcatinformatics.ca/gmeow/gmnFixityPostfix";
/// The bracketing fixity individual IRI.
pub const FIXITY_BRACKETING: &str = "https://blackcatinformatics.ca/gmeow/gmnFixityBracketing";

/// The disambiguation tag delimiters — guillemets that never occur in an `rdfs:label` or a
/// `argN` placeholder, so an appended `⟪curie⟫` is unambiguously separable by the inverse.
const TAG_OPEN: &str = " ⟪";
const TAG_CLOSE: char = '⟫';

/// The compaction prefix map for the four grounding namespaces every GMN denotation target
/// lives in. Used ONLY to render a disambiguation tag on an `rdfs:label` collision; it never
/// participates in resolving meaning (no local-name convention drives semantics here).
const CURIE_PREFIXES: &[(&str, &str)] = &[
    ("logic:", "https://blackcatinformatics.ca/logic/"),
    ("math:", "https://blackcatinformatics.ca/math/"),
    ("lang:", "https://blackcatinformatics.ca/lang/"),
    ("gmeow:", "https://blackcatinformatics.ca/gmeow/"),
];

/// One resolved GMN operator form eligible for verbalization: the executable GMN glyph
/// surface, its graph-authored `(fixity, arity)` signature, and the denoted term with the
/// `rdfs:label` that becomes the controlled-NL nucleus. Ordered by `term_iri` first so the
/// whole verbalization corpus is byte-deterministic independent of registry iteration order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GmnOperatorForm {
    /// The denotation target IRI (`lang:denotationTarget`) — the operator's meaning.
    pub term_iri: String,
    /// The denoted term's `rdfs:label` — the controlled-NL nucleus filled into the template.
    pub term_label: String,
    /// The executable GMN glyph surface (the operator's canonical GMN spelling).
    pub gmn_glyph: String,
    /// The `gmeow:gmnFixity` IRI (one of the four [`FIXITY_PREFIX`] … constants).
    pub fixity: String,
    /// The `gmeow:gmnArity` operand count.
    pub arity: u32,
}

/// One rendered bidirectional pair: the operator form, its GMN operator surface, and its
/// (possibly disambiguated) controlled-NL string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerbalizedPair {
    /// The source operator form.
    pub form: GmnOperatorForm,
    /// The GMN operator surface (glyph in operator position, schematic operands).
    pub gmn_surface: String,
    /// The controlled-NL string (term label in operator position, same operands), with a
    /// `⟪curie⟫` disambiguation tag appended iff its base string collided with a sibling.
    pub nl: String,
}

/// A verbalizer defect: an unresolvable operator form (missing label), an unsupported
/// fixity/arity combination, or an injectivity collision that cannot be broken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerbalizeError(pub String);

impl std::fmt::Display for VerbalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Resolve every verbalizable GMN operator form from the carrier glyph registry: each
/// executable glyph binding that carries a fixity signature (a constant carries none) is an
/// operator, joined to its denotation target's `rdfs:label` from `labels`. An operator with
/// no resolvable label is a HARD FAIL (no-optionality: a selected operator MUST verbalize),
/// never a silent skip. Output is sorted + deduped by the whole form for determinism.
pub fn resolve_operator_forms(
    registry: &GmnGlyphRegistry,
    labels: &BTreeMap<String, String>,
) -> Result<Vec<GmnOperatorForm>, VerbalizeError> {
    let mut forms = Vec::new();
    for (_sigil, glyph, fixity, arity, term) in registry.glyph_binding_rows() {
        // A constant binding carries no fixity/arity signature — only operators verbalize.
        if fixity.is_empty() {
            continue;
        }
        let arity: u32 = arity.parse().map_err(|_| {
            VerbalizeError(format!(
                "GMN operator glyph {glyph:?} for {term} has a non-integer arity {arity:?}"
            ))
        })?;
        let label = labels.get(&term).ok_or_else(|| {
            VerbalizeError(format!(
                "GMN operator {term} (glyph {glyph:?}) has no rdfs:label to verbalize; a \
                 selected operator must carry its controlled-NL nucleus"
            ))
        })?;
        forms.push(GmnOperatorForm {
            term_iri: term,
            term_label: label.clone(),
            gmn_glyph: glyph,
            fixity,
            arity,
        });
    }
    forms.sort();
    forms.dedup();
    Ok(forms)
}

/// The schematic operand placeholders `arg1 … argN` for an operator of the given arity.
fn placeholders(arity: u32) -> Vec<String> {
    (1..=arity).map(|i| format!("arg{i}")).collect()
}

/// Arrange an operator `token` (a glyph on the GMN side, a label on the NL side) with its
/// operand placeholders per the fixity — the single template both directions share, so the
/// GMN and NL surfaces of one pair are structurally identical up to the operator token.
fn arrange(fixity: &str, token: &str, arity: u32) -> Result<String, VerbalizeError> {
    let args = placeholders(arity);
    let rendered = match fixity {
        FIXITY_PREFIX => {
            // Operator first, then operands: "not arg1" / "¬ arg1".
            if args.is_empty() {
                token.to_owned()
            } else {
                format!("{token} {}", args.join(" "))
            }
        }
        FIXITY_POSTFIX => {
            // Operands first, operator last: "arg1 factorial" / "arg1 !".
            if args.is_empty() {
                token.to_owned()
            } else {
                format!("{} {token}", args.join(" "))
            }
        }
        FIXITY_INFIX => {
            // Operands separated by the operator: "arg1 subsumes arg2" / "arg1 ⊑ arg2".
            if arity < 2 {
                return Err(VerbalizeError(format!(
                    "infix operator {token:?} needs arity ≥ 2, got {arity}"
                )));
            }
            args.join(&format!(" {token} "))
        }
        FIXITY_BRACKETING => {
            // Operator around a bracketed operand list: "abs [ arg1 ]" / "| [ arg1 ] |"-style.
            if args.is_empty() {
                return Err(VerbalizeError(format!(
                    "bracketing operator {token:?} needs arity ≥ 1, got {arity}"
                )));
            }
            format!("{token} [ {} ]", args.join(" , "))
        }
        other => {
            return Err(VerbalizeError(format!(
                "unknown GMN fixity {other:?}; the closed class has exactly four individuals"
            )));
        }
    };
    Ok(rendered)
}

/// The compact CURIE of a grounding IRI (`logic:not`, `math:Addition`, …) for a
/// disambiguation tag; the full IRI when no grounding prefix matches (still injective).
fn compact_curie(iri: &str) -> String {
    for (prefix, ns) in CURIE_PREFIXES {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}{local}");
        }
    }
    iri.to_owned()
}

/// Build the full verbalization corpus from resolved forms: render each pair, break any
/// controlled-NL `rdfs:label` collision by appending the colliding forms' CURIEs, and prove
/// the result is injective on BOTH surfaces. `Err` on an unrenderable form or an
/// unbreakable collision — a lossy or non-injective corpus never ships.
pub fn build_verbalization_pairs(
    forms: &[GmnOperatorForm],
) -> Result<Vec<VerbalizedPair>, VerbalizeError> {
    // First pass: the base controlled-NL string per form (no disambiguation tag yet), so we
    // can detect which base strings are shared by two or more DISTINCT forms.
    let mut base_nl: Vec<String> = Vec::with_capacity(forms.len());
    for form in forms {
        base_nl.push(arrange(&form.fixity, &form.term_label, form.arity)?);
    }
    let mut base_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for nl in &base_nl {
        *base_counts.entry(nl.as_str()).or_default() += 1;
    }

    // Second pass: render each pair, tagging the NL string with the form's CURIE exactly when
    // its base string is shared (a homograph collision), so distinct meanings stay distinct.
    let mut pairs = Vec::with_capacity(forms.len());
    for (form, base) in forms.iter().zip(base_nl.iter()) {
        let gmn_surface = arrange(&form.fixity, &form.gmn_glyph, form.arity)?;
        let collided = base_counts.get(base.as_str()).copied().unwrap_or(0) > 1;
        let nl = if collided {
            format!("{base}{TAG_OPEN}{}{TAG_CLOSE}", compact_curie(&form.term_iri))
        } else {
            base.clone()
        };
        pairs.push(VerbalizedPair {
            form: form.clone(),
            gmn_surface,
            nl,
        });
    }

    // Injectivity teeth: after disambiguation, every controlled-NL string is unique, and so
    // is every GMN operator surface. A residual collision (e.g. two forms sharing both label
    // and CURIE, or the same glyph across scopes) is a HARD FAIL — bidirectionality is
    // unsound without an injective map.
    assert_unique(pairs.iter().map(|p| p.nl.as_str()), "controlled-NL string")?;
    assert_unique(
        pairs.iter().map(|p| p.gmn_surface.as_str()),
        "GMN operator surface",
    )?;
    Ok(pairs)
}

/// Hard-fail if `values` are not all distinct, naming the first collision.
fn assert_unique<'a>(
    values: impl Iterator<Item = &'a str>,
    what: &str,
) -> Result<(), VerbalizeError> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(VerbalizeError(format!(
                "verbalizer injectivity violated: two distinct operator forms share the {what} \
                 {value:?}; the GMN⇄NL map must be injective"
            )));
        }
    }
    Ok(())
}

/// The NL→GMN inverse template's parse of a controlled-NL string: the fixity recovered from
/// the placeholder skeleton, the operator label, the operand count, and any CURIE tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNl {
    /// The recovered fixity IRI.
    pub fixity: String,
    /// The recovered operator label.
    pub label: String,
    /// The recovered operand count.
    pub arity: u32,
    /// The disambiguation CURIE, present iff the string carried a `⟪curie⟫` tag.
    pub curie: Option<String>,
}

/// The inverse template: re-parse a controlled-NL string back to `(fixity, label, arity)`
/// PURELY from its placeholder skeleton (`argN` tokens), with no lookup into the forward
/// corpus. Returns `None` for a string that is not a well-formed verbalization. This is a
/// genuine inverse of [`arrange`], so a round-trip through it has real teeth.
#[must_use]
pub fn parse_nl(nl: &str) -> Option<ParsedNl> {
    // Split off an optional `⟪curie⟫` disambiguation tag.
    let (core, curie) = match nl.rsplit_once(TAG_OPEN) {
        Some((head, tag)) if tag.ends_with(TAG_CLOSE) => {
            (head, Some(tag.trim_end_matches(TAG_CLOSE).to_owned()))
        }
        _ => (nl, None),
    };
    let tokens: Vec<&str> = core.split(' ').collect();
    let is_ph = |t: &str| {
        t.len() > 3 && t.starts_with("arg") && t[3..].bytes().all(|b| b.is_ascii_digit())
    };

    // Bracketing: "<label…> [ arg1 , … , argN ]".
    if let Some(open) = tokens.iter().position(|t| *t == "[") {
        if tokens.last() == Some(&"]") {
            let label = tokens[..open].join(" ");
            let inner: Vec<&str> = tokens[open + 1..tokens.len() - 1]
                .iter()
                .copied()
                .filter(|t| *t != ",")
                .collect();
            if !label.is_empty() && !inner.is_empty() && inner.iter().all(|t| is_ph(t)) {
                return Some(ParsedNl {
                    fixity: FIXITY_BRACKETING.to_owned(),
                    label,
                    arity: inner.len() as u32,
                    curie,
                });
            }
        }
        return None;
    }

    let ph_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| is_ph(t))
        .map(|(i, _)| i)
        .collect();
    if ph_positions.is_empty() {
        return None;
    }
    let first = ph_positions[0];
    let last = *ph_positions.last().expect("non-empty");
    let arity = ph_positions.len() as u32;

    // Infix: placeholders bracket the string at both ends, label sits between the first two.
    if first == 0 && last == tokens.len() - 1 && arity >= 2 {
        let label = tokens[1..ph_positions[1]].join(" ");
        return (!label.is_empty()).then_some(ParsedNl {
            fixity: FIXITY_INFIX.to_owned(),
            label,
            arity,
            curie,
        });
    }
    // Postfix: placeholders lead, the operator label trails.
    if first == 0 {
        let label = tokens[last + 1..].join(" ");
        return (!label.is_empty()).then_some(ParsedNl {
            fixity: FIXITY_POSTFIX.to_owned(),
            label,
            arity,
            curie,
        });
    }
    // Prefix: the operator label leads, placeholders trail.
    let label = tokens[..first].join(" ");
    (!label.is_empty()).then_some(ParsedNl {
        fixity: FIXITY_PREFIX.to_owned(),
        label,
        arity,
        curie,
    })
}

/// Invert one controlled-NL string back to the operator form it verbalizes, resolving the
/// parsed `(fixity, label, arity, curie)` skeleton against the forward corpus index. `None`
/// when the string is not a well-formed verbalization or names no known form.
#[must_use]
pub fn invert_nl<'a>(
    nl: &str,
    index: &BTreeMap<(String, String, u32, Option<String>), &'a GmnOperatorForm>,
) -> Option<&'a GmnOperatorForm> {
    let parsed = parse_nl(nl)?;
    index
        .get(&(parsed.fixity, parsed.label, parsed.arity, parsed.curie))
        .copied()
}

/// The forward index keyed by the inverse-recoverable skeleton `(fixity, label, arity,
/// curie)`, used by [`invert_nl`]. Built from the rendered pairs, mirroring exactly how the
/// NL string is tagged (a tagged pair keys under its CURIE, an untagged one under `None`).
#[must_use]
pub fn forward_index(
    pairs: &[VerbalizedPair],
) -> BTreeMap<(String, String, u32, Option<String>), &GmnOperatorForm> {
    let mut index = BTreeMap::new();
    for pair in pairs {
        let curie = parse_nl(&pair.nl).and_then(|p| p.curie);
        index.insert(
            (
                pair.form.fixity.clone(),
                pair.form.term_label.clone(),
                pair.form.arity,
                curie,
            ),
            &pair.form,
        );
    }
    index
}

/// MEASURE the bidirectional round-trip: the inverse template recovers EXACTLY the source
/// operator form for every rendered pair. `true` only when every pair round-trips — the
/// witness the emission's EXACT preservation claim rests on, never asserted on faith.
#[must_use]
pub fn round_trip_holds(pairs: &[VerbalizedPair]) -> bool {
    let index = forward_index(pairs);
    pairs
        .iter()
        .all(|pair| invert_nl(&pair.nl, &index) == Some(&pair.form))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(term: &str, label: &str, glyph: &str, fixity: &str, arity: u32) -> GmnOperatorForm {
        GmnOperatorForm {
            term_iri: term.to_owned(),
            term_label: label.to_owned(),
            gmn_glyph: glyph.to_owned(),
            fixity: fixity.to_owned(),
            arity,
        }
    }

    /// The four fixity templates arrange the operator token and operands as documented, and
    /// each NL string round-trips through the inverse template to its own `(fixity, label,
    /// arity)`.
    #[test]
    fn templates_arrange_per_fixity_and_invert() {
        let cases = [
            (FIXITY_PREFIX, "not", 1u32, "not arg1"),
            (FIXITY_INFIX, "subsumes", 2, "arg1 subsumes arg2"),
            (FIXITY_INFIX, "for all", 2, "arg1 for all arg2"),
            (FIXITY_POSTFIX, "factorial", 1, "arg1 factorial"),
            (FIXITY_BRACKETING, "abs", 1, "abs [ arg1 ]"),
            (FIXITY_BRACKETING, "tuple", 2, "tuple [ arg1 , arg2 ]"),
        ];
        for (fixity, label, arity, expected) in cases {
            let nl = arrange(fixity, label, arity).expect("arrange");
            assert_eq!(nl, expected, "fixity {fixity} arrangement");
            let parsed = parse_nl(&nl).expect("inverse parses");
            assert_eq!(parsed.fixity, fixity, "recovered fixity for {nl:?}");
            assert_eq!(parsed.label, label, "recovered label for {nl:?}");
            assert_eq!(parsed.arity, arity, "recovered arity for {nl:?}");
            assert_eq!(parsed.curie, None);
        }
    }

    /// Perturbing ONLY the fixity changes the verbalization — the template is genuinely
    /// fixity-driven, not a constant caption (falsifiability teeth).
    #[test]
    fn fixity_perturbation_changes_verbalization() {
        let infix = arrange(FIXITY_INFIX, "op", 2).unwrap();
        let prefix = arrange(FIXITY_PREFIX, "op", 2).unwrap();
        let postfix = arrange(FIXITY_POSTFIX, "op", 2).unwrap();
        assert_ne!(infix, prefix);
        assert_ne!(infix, postfix);
        assert_ne!(prefix, postfix);
    }

    /// A homograph (two distinct terms sharing one `rdfs:label`) is disambiguated by CURIE so
    /// the controlled-NL map stays injective, and the tagged strings still round-trip.
    #[test]
    fn homograph_labels_are_disambiguated_and_still_invert() {
        let forms = vec![
            form("math:supersetRel", "contains", "⊃", FIXITY_INFIX, 2),
            form("math:hasElement", "contains", "∋", FIXITY_INFIX, 2),
        ];
        let pairs = build_verbalization_pairs(&forms).expect("pairs build");
        // Both NL strings are distinct (disambiguated) — injectivity held.
        assert_ne!(pairs[0].nl, pairs[1].nl);
        assert!(pairs.iter().all(|p| p.nl.contains("⟪")), "both carry a CURIE tag");
        // And each still round-trips to its OWN form through the inverse + index.
        assert!(round_trip_holds(&pairs), "disambiguated pairs round-trip");
        let index = forward_index(&pairs);
        assert_eq!(invert_nl(&pairs[0].nl, &index), Some(&pairs[0].form));
        assert_eq!(invert_nl(&pairs[1].nl, &index), Some(&pairs[1].form));
    }

    /// An unbreakable collision (same label AND same CURIE on two distinct forms) is a HARD
    /// FAIL — the verbalizer never ships a non-injective map.
    #[test]
    fn unbreakable_collision_hard_fails() {
        let forms = vec![
            form("logic:x", "same", "a", FIXITY_INFIX, 2),
            // A different glyph but the SAME term IRI + label collides irreparably.
            form("logic:x", "same", "b", FIXITY_INFIX, 2),
        ];
        let err = build_verbalization_pairs(&forms).expect_err("must hard-fail");
        assert!(err.0.contains("injectivity"), "{err}");
    }

    /// Distinct forms round-trip and the corpus is injective + deterministic.
    #[test]
    fn distinct_forms_round_trip_and_are_injective() {
        let forms = vec![
            form("logic:not", "not", "¬", FIXITY_PREFIX, 1),
            form("logic:subClassOf", "subsumes", "⊑", FIXITY_INFIX, 2),
            form("math:Addition", "plus", "+", FIXITY_INFIX, 2),
        ];
        let a = build_verbalization_pairs(&forms).expect("a");
        let b = build_verbalization_pairs(&forms).expect("b");
        assert_eq!(a, b, "deterministic");
        assert!(round_trip_holds(&a), "all round-trip");
        // No CURIE tags needed (labels distinct).
        assert!(a.iter().all(|p| !p.nl.contains("⟪")));
    }
}

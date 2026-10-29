// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR for correspondence IR and its complete recursive source laws.

use std::io::{self, Write};

use purrdf_lex::cbor::{self, Value, head};

use super::{
    AxisEvidence, Correspondence, CorrespondenceCaveat, CorrespondenceComposition,
    CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeCondition, DischargeVerdict,
    Formula, LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind, RecoveryCaseIr,
    Term, TransactionProgramIr,
};

impl CorrespondenceRelation {
    /// Retain the correspondence relation variant name, independent of projected labels.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Equiv => "Equiv",
                Self::Subsumes => "Subsumes",
                Self::SubsumedBy => "SubsumedBy",
                Self::Overlaps => "Overlaps",
                Self::RelatedMatch => "RelatedMatch",
                Self::Disjoint => "Disjoint",
            },
        )
    }
}

impl MorphismClass {
    /// Retain the declared morphism-class variant name.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Isomorphism => "Isomorphism",
                Self::SectionRetraction => "SectionRetraction",
                Self::WellBehavedLens => "WellBehavedLens",
                Self::LossyLens => "LossyLens",
                Self::Prism => "Prism",
                Self::AffineCorrespondence => "AffineCorrespondence",
                Self::BridgeView => "BridgeView",
            },
        )
    }
}

impl MorphismKind {
    /// Retain the declared morphism-kind variant name.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::InstitutionMorphism => "InstitutionMorphism",
                Self::CommitmentShiftingBridge => "CommitmentShiftingBridge",
            },
        )
    }
}

impl Determinacy {
    /// Retain the original determinacy variant name.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Crisp => "Crisp",
                Self::Vague => "Vague",
            },
        )
    }
}

impl CorrespondenceLaw {
    /// Retain the named law without discharging its obligation.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::GetPut => "GetPut",
                Self::PutGet => "PutGet",
                Self::PutPut => "PutPut",
                Self::SectionLaw => "SectionLaw",
            },
        )
    }
}

impl DischargeVerdict {
    /// Retain the recorded discharge verdict variant name.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::ObligationDischarged => "ObligationDischarged",
                Self::ObligationUnknown => "ObligationUnknown",
                Self::ObligationViolated => "ObligationViolated",
            },
        )
    }
}

impl DischargeCondition {
    /// Retain the declared discharge-condition variant name.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::DischargeCertifiedFragment => "DischargeCertifiedFragment",
                Self::DischargeFiniteClosure => "DischargeFiniteClosure",
                Self::DischargeSyntacticReachability => "DischargeSyntacticReachability",
                Self::DischargeConservativeExtension => "DischargeConservativeExtension",
                Self::DischargeBoundedCorpus => "DischargeBoundedCorpus",
            },
        )
    }
}

impl PreservationKind {
    /// Retain the preservation variant name rather than its projection spelling.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Exact => "Exact",
                Self::SoundUnder => "SoundUnder",
                Self::CompleteOver => "CompleteOver",
                Self::ValidationOnly => "ValidationOnly",
                Self::InconsistencyPreserving => "InconsistencyPreserving",
                Self::InconsistencyReflecting => "InconsistencyReflecting",
                Self::Unsupported => "Unsupported",
            },
        )
    }
}

impl LawClaimIr {
    /// Retain the complete law, verdict and explicit optional discharge condition.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "law")?;
        self.law.write_cbor(out)?;
        head::write_text(out, "verdict")?;
        self.verdict.write_cbor(out)?;
        head::write_text(out, "condition")?;
        match &self.condition {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }
}

impl AxisEvidence {
    /// Retain every evidence source and both optional interpretation references.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "sources")?;
        head::write_head(out, head::ARRAY, self.sources.len() as u64)?;
        for value in &self.sources {
            head::write_text(out, value)?;
        }
        head::write_text(out, "scale")?;
        match self.scale.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "probability_model")?;
        match self.probability_model.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }
}

impl CorrespondenceCaveat {
    /// Retain the named caveat and its complete ordered literal collection.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "iri")?;
        head::write_text(out, &self.iri)?;
        head::write_text(out, "comments")?;
        super::literal_serde::write_cbor(&self.comments, out)?;
        Ok(())
    }
}

impl RecoveryCaseIr {
    /// Retain the named recovery case and its complete source formula.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "iri")?;
        head::write_text(out, &self.iri)?;
        head::write_text(out, "transform")?;
        self.transform.write_cbor(out)?;
        Ok(())
    }
}

impl CorrespondenceComposition {
    /// Retain each sequential obligation and its original axis and independence claims.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 7)?;
        head::write_text(out, "iri")?;
        head::write_text(out, &self.iri)?;
        head::write_text(out, "first")?;
        head::write_text(out, &self.first)?;
        head::write_text(out, "second")?;
        head::write_text(out, &self.second)?;
        head::write_text(out, "composite")?;
        head::write_text(out, &self.composite)?;
        head::write_text(out, "axis_rules")?;
        head::write_head(out, head::ARRAY, self.axis_rules.len() as u64)?;
        for value in &self.axis_rules {
            head::write_text(out, value)?;
        }
        head::write_text(out, "confidence_independence")?;
        match self.confidence_independence.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "probability_independence")?;
        match self.probability_independence.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }
}

impl TransactionProgramIr {
    /// Retain the transaction identity and complete executable leg-path body.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "iri")?;
        head::write_text(out, &self.iri)?;
        head::write_text(out, "body")?;
        self.body.write_cbor(out)?;
        Ok(())
    }
}

impl Correspondence {
    /// Retain every correspondence field in its existing serialized declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 22)?;
        head::write_text(out, "iri")?;
        head::write_text(out, &self.iri)?;
        head::write_text(out, "relation")?;
        self.relation.write_cbor(out)?;
        head::write_text(out, "morphism_class")?;
        self.morphism_class.write_cbor(out)?;
        head::write_text(out, "morphism_kind")?;
        self.morphism_kind.write_cbor(out)?;
        head::write_text(out, "mnemomorphic")?;
        cbor::write(&Value::Bool(self.mnemomorphic), out)?;
        head::write_text(out, "determinacy")?;
        match &self.determinacy {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "get_leg")?;
        match self.get_leg.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "put_leg")?;
        match self.put_leg.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "law_claims")?;
        head::write_head(out, head::ARRAY, self.law_claims.len() as u64)?;
        for value in &self.law_claims {
            value.write_cbor(out)?;
        }
        head::write_text(out, "caveats")?;
        head::write_head(out, head::ARRAY, self.caveats.len() as u64)?;
        for value in &self.caveats {
            value.write_cbor(out)?;
        }
        head::write_text(out, "loss_evidence")?;
        super::literal_serde::write_cbor(&self.loss_evidence, out)?;
        head::write_text(out, "confidence")?;
        match &self.confidence {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "evidence_strength")?;
        match &self.evidence_strength {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "axis_evidence")?;
        self.axis_evidence.write_cbor(out)?;
        head::write_text(out, "weight")?;
        match &self.weight {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "probability")?;
        match &self.probability {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "according_to")?;
        match self.according_to.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "preservation")?;
        match &self.preservation {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "source_endpoint")?;
        match self.source_endpoint.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "target_endpoint")?;
        match self.target_endpoint.as_deref() {
            Some(value) => head::write_text(out, value)?,
            None => cbor::write(&Value::Null, out)?,
        }
        head::write_text(out, "grounding")?;
        cbor::write(&Value::Bool(self.grounding), out)?;
        head::write_text(out, "recovery_cases")?;
        head::write_head(out, head::ARRAY, self.recovery_cases.len() as u64)?;
        for value in &self.recovery_cases {
            value.write_cbor(out)?;
        }
        Ok(())
    }
}

impl LegPath {
    /// Stream the complete recursive leg path without recursive calls or cloning.
    /// Sequence and alternative order and multiplicity remain unchanged.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let mut pending = vec![self];
        while let Some(path) = pending.pop() {
            head::write_head(out, head::MAP, 1)?;
            match path {
                Self::Step(predicate) => {
                    head::write_text(out, "Step")?;
                    head::write_text(out, predicate)?;
                }
                Self::Inverse(inner) => {
                    head::write_text(out, "Inverse")?;
                    pending.push(inner.as_ref());
                }
                Self::Seq(paths) | Self::Alt(paths) => {
                    head::write_text(
                        out,
                        if matches!(path, Self::Seq(_)) {
                            "Seq"
                        } else {
                            "Alt"
                        },
                    )?;
                    head::write_head(out, head::ARRAY, paths.len() as u64)?;
                    pending.extend(paths.iter().rev());
                }
            }
        }
        Ok(())
    }
}

/// One borrowed source-tree operation; owned values never enter the work list.
enum Item<'a> {
    Formula(&'a Formula),
    Term(&'a Term),
    Formulas(&'a [Formula]),
    Terms(&'a [Term]),
    Key(&'static str),
}

impl Formula {
    /// Stream the complete formula tree using its existing externally tagged wire.
    /// Traversal borrows every term and preserves recorded argument/body order.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        write_tree(Item::Formula(self), out)
    }
}

/// Walk mutually nested formula and term containers on a borrowed work list.
fn write_tree<W: Write + ?Sized>(first: Item<'_>, out: &mut W) -> io::Result<()> {
    let mut pending = vec![first];
    while let Some(item) = pending.pop() {
        match item {
            Item::Key(key) => head::write_text(out, key)?,
            Item::Terms(terms) => {
                head::write_head(out, head::ARRAY, terms.len() as u64)?;
                pending.extend(terms.iter().rev().map(Item::Term));
            }
            Item::Formulas(formulas) => {
                head::write_head(out, head::ARRAY, formulas.len() as u64)?;
                pending.extend(formulas.iter().rev().map(Item::Formula));
            }
            Item::Term(term) => {
                head::write_head(out, head::MAP, 1)?;
                match term {
                    Term::Var(value) => {
                        head::write_text(out, "Var")?;
                        head::write_text(out, value)?;
                    }
                    Term::Iri(value) => {
                        head::write_text(out, "Iri")?;
                        head::write_text(out, value)?;
                    }
                    Term::Literal(value) => {
                        head::write_text(out, "Literal")?;
                        super::literal_serde::single::write_cbor(value, out)?;
                    }
                    Term::SequenceMarker(value) => {
                        head::write_text(out, "SequenceMarker")?;
                        head::write_text(out, value)?;
                    }
                    Term::App { symbol, args } => {
                        head::write_text(out, "App")?;
                        head::write_head(out, head::MAP, 2)?;
                        head::write_text(out, "symbol")?;
                        head::write_text(out, symbol)?;
                        head::write_text(out, "args")?;
                        pending.push(Item::Terms(args));
                    }
                }
            }
            Item::Formula(formula) => {
                head::write_head(out, head::MAP, 1)?;
                match formula {
                    Formula::Atom { relation, args } => {
                        head::write_text(out, "Atom")?;
                        head::write_head(out, head::MAP, 2)?;
                        head::write_text(out, "relation")?;
                        pending.extend([
                            Item::Terms(args),
                            Item::Key("args"),
                            Item::Term(relation),
                        ]);
                    }
                    Formula::Not(inner) => {
                        head::write_text(out, "Not")?;
                        pending.push(Item::Formula(inner.as_ref()));
                    }
                    Formula::And(formulas) | Formula::Or(formulas) => {
                        head::write_text(
                            out,
                            if matches!(formula, Formula::And(_)) {
                                "And"
                            } else {
                                "Or"
                            },
                        )?;
                        pending.push(Item::Formulas(formulas));
                    }
                    Formula::Implies(left, right) | Formula::Iff(left, right) => {
                        head::write_text(
                            out,
                            if matches!(formula, Formula::Implies(_, _)) {
                                "Implies"
                            } else {
                                "Iff"
                            },
                        )?;
                        head::write_head(out, head::ARRAY, 2)?;
                        pending
                            .extend([Item::Formula(right.as_ref()), Item::Formula(left.as_ref())]);
                    }
                    Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
                        head::write_text(
                            out,
                            if matches!(formula, Formula::Forall { .. }) {
                                "Forall"
                            } else {
                                "Exists"
                            },
                        )?;
                        head::write_head(out, head::MAP, 2)?;
                        head::write_text(out, "vars")?;
                        head::write_head(out, head::ARRAY, vars.len() as u64)?;
                        for variable in vars {
                            head::write_text(out, variable)?;
                        }
                        head::write_text(out, "body")?;
                        pending.push(Item::Formula(body.as_ref()));
                    }
                }
            }
        }
    }
    Ok(())
}

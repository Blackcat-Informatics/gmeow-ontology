// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR spelling of the complete relational-core payload wire.

use std::io::{self, Write};

use purrdf_lex::cbor::{self, Value, head};

use super::{NumericOperator, RcAtom, RcNumeric, RcResidue, RcRule, RelationalCoreProgram};

impl RelationalCoreProgram {
    /// Stream every field of the existing relational-core CBOR wire.
    /// Declaration order, recorded vector order and multiplicity are retained;
    /// no program clone, value tree, normalization or RDF rendering is needed.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "facts")?;
        write_atoms(&self.facts, out)?;
        head::write_text(out, "rules")?;
        head::write_head(out, head::ARRAY, self.rules.len() as u64)?;
        for rule in &self.rules {
            write_rule(rule, out)?;
        }
        head::write_text(out, "residue")?;
        head::write_head(out, head::ARRAY, self.residue.len() as u64)?;
        for residue in &self.residue {
            write_residue(residue, out)?;
        }
        head::write_text(out, "source_iri")?;
        match self.source_iri.as_deref() {
            Some(source) => head::write_text(out, source)?,
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }
}

/// Stream the recorded atom sequence without reordering or deduplicating it.
fn write_atoms<W: Write + ?Sized>(atoms: &[RcAtom], out: &mut W) -> io::Result<()> {
    head::write_head(out, head::ARRAY, atoms.len() as u64)?;
    for atom in atoms {
        write_atom(atom, out)?;
    }
    Ok(())
}

/// Retain every signed atom field in its serialized declaration order.
fn write_atom<W: Write + ?Sized>(atom: &RcAtom, out: &mut W) -> io::Result<()> {
    head::write_head(out, head::MAP, 4)?;
    head::write_text(out, "subject")?;
    atom.subject.write_cbor(out)?;
    head::write_text(out, "predicate")?;
    head::write_text(out, &atom.predicate)?;
    head::write_text(out, "object")?;
    atom.object.write_cbor(out)?;
    head::write_text(out, "negated")?;
    cbor::write(&Value::Bool(atom.negated), out)
}

/// Retain the complete rule, including ordered conjunctions and distinct pairs.
fn write_rule<W: Write + ?Sized>(rule: &RcRule, out: &mut W) -> io::Result<()> {
    head::write_head(out, head::MAP, 5)?;
    head::write_text(out, "head")?;
    write_atom(&rule.head, out)?;
    head::write_text(out, "head_conjuncts")?;
    write_atoms(&rule.head_conjuncts, out)?;
    head::write_text(out, "body")?;
    write_atoms(&rule.body, out)?;
    head::write_text(out, "numeric")?;
    head::write_head(out, head::ARRAY, rule.numeric.len() as u64)?;
    for numeric in &rule.numeric {
        write_numeric(numeric, out)?;
    }
    head::write_text(out, "distinct_pairs")?;
    head::write_head(out, head::ARRAY, rule.distinct_pairs.len() as u64)?;
    for (left, right) in &rule.distinct_pairs {
        head::write_head(out, head::ARRAY, 2)?;
        head::write_text(out, left)?;
        head::write_text(out, right)?;
    }
    Ok(())
}

/// Retain the operator name and both inputs with the explicit optional result.
fn write_numeric<W: Write + ?Sized>(numeric: &RcNumeric, out: &mut W) -> io::Result<()> {
    head::write_head(out, head::MAP, 4)?;
    head::write_text(out, "operator")?;
    head::write_text(
        out,
        match numeric.operator {
            NumericOperator::Add => "Add",
            NumericOperator::Subtract => "Subtract",
            NumericOperator::Multiply => "Multiply",
            NumericOperator::Divide => "Divide",
            NumericOperator::ExactDivide => "ExactDivide",
            NumericOperator::Equal => "Equal",
            NumericOperator::NotEqual => "NotEqual",
            NumericOperator::Less => "Less",
            NumericOperator::LessOrEqual => "LessOrEqual",
            NumericOperator::Greater => "Greater",
            NumericOperator::GreaterOrEqual => "GreaterOrEqual",
        },
    )?;
    head::write_text(out, "left")?;
    numeric.left.write_cbor(out)?;
    head::write_text(out, "right")?;
    numeric.right.write_cbor(out)?;
    head::write_text(out, "result")?;
    match &numeric.result {
        Some(result) => result.write_cbor(out),
        None => cbor::write(&Value::Null, out),
    }
}

/// Retain the complete unsupported-construct reason as its owned record.
fn write_residue<W: Write + ?Sized>(residue: &RcResidue, out: &mut W) -> io::Result<()> {
    head::write_head(out, head::MAP, 1)?;
    head::write_text(out, "reason")?;
    head::write_text(out, &residue.reason)
}

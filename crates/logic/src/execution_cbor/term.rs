// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete retained-term reader for the selected execution receipt wire.

use std::io;

use purrdf::{BlankScope, RdfTextDirection, TermBox, TermValue};
use purrdf_lex::cbor::head;

use super::{ExecutionCbor, Reader, Result};

impl ExecutionCbor for RdfTextDirection {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Ltr => "Ltr",
                Self::Rtl => "Rtl",
            },
        )
    }

    crate::__gmeow_execution_unit_depth!(Ltr, Rtl);

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| {
            if tagged {
                return Err(input.invalid("expected unit retained direction"));
            }
            match name {
                "Ltr" => Ok(Self::Ltr),
                "Rtl" => Ok(Self::Rtl),
                _ => Err(input.invalid("unknown retained direction")),
            }
        })
    }
}

impl ExecutionCbor for TermValue {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        out.term(self)
    }

    fn check_depth(&self, remaining: usize) -> Result<()> {
        required_depth(self, remaining).map(|_| ())
    }

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        if !input.definitions() {
            return input.term_value();
        }
        input.variant(|input, name, tagged| {
            if !tagged {
                return Err(input.invalid("expected tagged retained term"));
            }
            match name {
                "Iri" => String::read_cbor(input).map(Self::Iri),
                "Blank" => input.map(|input, count| {
                    if count != 2 {
                        return Err(input.invalid("wrong retained blank field count"));
                    }
                    input.field("label")?;
                    let label = String::read_cbor(input)?;
                    input.field("scope")?;
                    let scope = BlankScope(u32::read_cbor(input)?);
                    Ok(Self::Blank { label, scope })
                }),
                "Literal" => input.map(|input, count| {
                    if count != 4 {
                        return Err(input.invalid("wrong retained literal field count"));
                    }
                    input.field("lexical_form")?;
                    let lexical_form = String::read_cbor(input)?;
                    input.field("datatype")?;
                    let datatype = String::read_cbor(input)?;
                    input.field("language")?;
                    let language = Option::<String>::read_cbor(input)?;
                    input.field("direction")?;
                    let direction = Option::<RdfTextDirection>::read_cbor(input)?;
                    Ok(Self::Literal {
                        lexical_form,
                        datatype,
                        language,
                        direction,
                    })
                }),
                "Triple" => input.map(|input, count| {
                    if count != 3 {
                        return Err(input.invalid("wrong retained triple field count"));
                    }
                    input.field("s")?;
                    let s = TermBox::new(Self::read_cbor(input)?);
                    input.field("p")?;
                    let p = TermBox::new(Self::read_cbor(input)?);
                    input.field("o")?;
                    let o = TermBox::new(Self::read_cbor(input)?);
                    Ok(Self::Triple { s, p, o })
                }),
                _ => Err(input.invalid("unknown retained term variant")),
            }
        })
    }
}

/// One bounded structural walk supplies both the typed preflight and table metadata.
/// Depth is semantic visitor depth, independent of the compact reference spelling.
pub(super) fn required_depth(value: &TermValue, remaining: usize) -> Result<usize> {
    let own: usize = match value {
        TermValue::Iri(_) => 1,
        TermValue::Blank { .. } => 2,
        TermValue::Literal { direction, .. } => {
            if direction.is_some() {
                3
            } else {
                2
            }
        }
        TermValue::Triple { .. } => 2,
    };
    let children_remaining = remaining.checked_sub(own).ok_or_else(|| {
        super::Error::invalid(0, "typed receipt term exceeds its recursion limit")
    })?;
    let child_depth = match value {
        TermValue::Triple { s, p, o } => {
            let mut deepest = 0;
            for child in [s.as_ref(), p.as_ref(), o.as_ref()] {
                deepest = deepest.max(required_depth(child, children_remaining)?);
            }
            deepest
        }
        _ => 0,
    };
    own.checked_add(child_depth)
        .ok_or_else(|| super::Error::invalid(0, "typed receipt term depth overflow"))
}

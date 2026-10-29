// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete retained-term reader for the selected execution receipt wire.

use std::io::{self, Write};

use purrdf::{BlankScope, RdfTextDirection, TermBox, TermValue};
use purrdf_lex::cbor::head;

use super::{ExecutionCbor, Reader, Result};

impl ExecutionCbor for RdfTextDirection {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
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
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        crate::term_serde::write_cbor(self, out)
    }

    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = super::descend(remaining)?;
        match self {
            Self::Iri(iri) => iri.check_depth(remaining),
            Self::Blank { label, scope } => {
                let remaining = super::descend(remaining)?;
                label.check_depth(remaining)?;
                scope.0.check_depth(remaining)
            }
            Self::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => {
                let remaining = super::descend(remaining)?;
                lexical_form.check_depth(remaining)?;
                datatype.check_depth(remaining)?;
                language.check_depth(remaining)?;
                direction.check_depth(remaining)
            }
            Self::Triple { s, p, o } => {
                let remaining = super::descend(remaining)?;
                s.as_ref().check_depth(remaining)?;
                p.as_ref().check_depth(remaining)?;
                o.as_ref().check_depth(remaining)
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
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

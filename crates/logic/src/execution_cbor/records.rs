// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One authoring path for complete fixed receipt records and pure unit visits.

/// Implement an unconditionally emitted named record in its declared field order.
/// The owner supplies the exact existing field-count refusal. Optional/sparse,
/// renamed and variant payload schemas keep their explicit owner implementation.
#[doc(hidden)]
#[macro_export]
macro_rules! __gmeow_execution_record {
    ($type:ty { $($field:ident),+ $(,)? }, $refusal:literal $(,)?) => {
        impl $crate::execution_cbor::ExecutionCbor for $type {
            /// Preflight the complete record visitor and each actual field value.
            fn check_depth(&self, remaining: usize) -> $crate::execution_cbor::Result<()> {
                let remaining = $crate::execution_cbor::descend(remaining)?;
                $(
                    $crate::execution_cbor::ExecutionCbor::check_depth(&self.$field, remaining)?;
                )+
                Ok(())
            }

            /// Stream all declared fields without sorting or reconstructing them.
            fn write_cbor<W: ::std::io::Write + ?Sized>(
                &self,
                out: &mut W,
            ) -> ::std::io::Result<()> {
                const FIELDS: &[&str] = &[$(::core::stringify!($field)),+];
                ::purrdf_lex::cbor::head::write_head(
                    out,
                    ::purrdf_lex::cbor::head::MAP,
                    FIELDS.len() as u64,
                )?;
                $(
                    ::purrdf_lex::cbor::head::write_text(out, ::core::stringify!($field))?;
                    $crate::execution_cbor::ExecutionCbor::write_cbor(&self.$field, out)?;
                )+
                Ok(())
            }

            /// Restore exact recorded fields without normalization or admission.
            fn read_cbor(
                input: &mut $crate::execution_cbor::Reader<'_>,
            ) -> $crate::execution_cbor::Result<Self> {
                const FIELDS: &[&str] = &[$(::core::stringify!($field)),+];
                input.map(|input, length| {
                    if length != FIELDS.len() {
                        return Err(input.invalid($refusal));
                    }
                    $(
                        input.field(::core::stringify!($field))?;
                        let $field = $crate::execution_cbor::ExecutionCbor::read_cbor(input)?;
                    )+
                    Ok(Self { $($field),+ })
                })
            }
        }
    };
}

/// A pure text unit enum consumes one typed visitor and has no child payload.
/// Each owner names its complete unit inventory. Changing a unit into a payload
/// or adding a variant makes this exhaustive match fail compilation.
#[doc(hidden)]
#[macro_export]
macro_rules! __gmeow_execution_unit_depth {
    ($($variant:ident),+ $(,)?) => {
        /// Charge the typed reader's one unit-enum visit.
        fn check_depth(&self, remaining: usize) -> $crate::execution_cbor::Result<()> {
            match self {
                $(Self::$variant)|+ => $crate::execution_cbor::descend(remaining).map(|_| ()),
            }
        }
    };
}

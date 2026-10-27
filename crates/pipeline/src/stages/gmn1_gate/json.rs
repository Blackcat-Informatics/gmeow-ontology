// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native gate evidence transport; decoding does not recompute or classify reports.
use super::{Gmn1CodebookDigestMismatch, Gmn1CodebookDigestReport, Gmn1PackRootReport};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record!(impl ToJson for Gmn1CodebookDigestMismatch {
    "source" => source,
    "envelope" => envelope,
    "declared" => declared,
    "recomputed" => recomputed,
});
impl FromJson for Gmn1CodebookDigestMismatch {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Gmn1CodebookDigestMismatch")?;
        Ok(Self {
            source: fields.required("source")?,
            envelope: fields.required("envelope")?,
            declared: fields.required("declared")?,
            recomputed: fields.required("recomputed")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Gmn1CodebookDigestReport {
    "mismatches" => mismatches,
    "checked" => checked,
});
impl FromJson for Gmn1CodebookDigestReport {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Gmn1CodebookDigestReport")?;
        Ok(Self {
            mismatches: fields.required("mismatches")?,
            checked: fields.required_with("checked", |value| {
                gmeow_errors::json::read_unsigned(value, "usize checked count")
            })?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Gmn1PackRootReport {
    "recomputed_root" => recomputed_root,
    "pack_present" => pack_present,
    "declared_root" => declared_root,
    "pack_rel" => pack_rel,
});
impl FromJson for Gmn1PackRootReport {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Gmn1PackRootReport")?;
        Ok(Self {
            recomputed_root: fields.required("recomputed_root")?,
            pack_present: fields.required("pack_present")?,
            declared_root: fields.optional("declared_root")?,
            pack_rel: fields.required("pack_rel")?,
        })
    }
}

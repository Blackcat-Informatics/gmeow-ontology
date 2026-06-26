// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The RDF/JS [DatasetCore](https://rdf.js.org/dataset-spec/#datasetcore-interface) —
//! an in-memory, mutable quad collection.
//!
//! Wraps the engine's COW [`MutableDataset`](gmeow_rdf::ir::MutableDataset): a shared
//! frozen base plus an append/suppress delta. `parse` builds a frozen base from text
//! and wraps it; `serialize` compacts the effective set (`freeze`) and emits it. The
//! mutation surface (`add`/`delete`/`has`/`match`/iteration) lands in the next commit.

use gmeow_rdf::ir::MutableDataset;
use gmeow_rdf::{
    parse_dataset, serialize_dataset, RdfDatasetBuilder, RdfDiagnostic, SerializeGraph,
};
use wasm_bindgen::prelude::*;

use crate::codec::resolve_media_type;

/// Map an engine diagnostic to a JS error.
pub(crate) fn diag_to_err(diag: RdfDiagnostic) -> JsError {
    JsError::new(&diag.to_string())
}

/// An RDF/JS `DatasetCore` backed by the engine's COW mutable dataset.
#[wasm_bindgen]
pub struct Dataset {
    pub(crate) inner: MutableDataset,
}

impl Dataset {
    /// An empty frozen base — the COW root for a dataset with no parsed content.
    fn empty_base() -> Result<MutableDataset, JsError> {
        let base = RdfDatasetBuilder::new().freeze().map_err(diag_to_err)?;
        Ok(MutableDataset::new(base))
    }
}

#[wasm_bindgen]
impl Dataset {
    /// An empty dataset.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Dataset, JsError> {
        Ok(Dataset {
            inner: Self::empty_base()?,
        })
    }

    /// `parse(input, format, base?)` → a dataset of the parsed quads.
    ///
    /// `format` is a media type or short name (turtle/ntriples/nquads/trig/rdfxml).
    /// Ill-typed literals are preserved verbatim (RDFLib parity), not rejected.
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(input: &str, format: &str, base: Option<String>) -> Result<Dataset, JsError> {
        let media_type = resolve_media_type(format).map_err(|e| JsError::new(&e))?;
        let dataset =
            parse_dataset(input.as_bytes(), media_type, base.as_deref()).map_err(diag_to_err)?;
        Ok(Dataset {
            inner: MutableDataset::new(dataset),
        })
    }

    /// `serialize(format)` → the dataset rendered in `format` (a UTF-8 string).
    ///
    /// Note: a quoted-triple term appearing as a quad object currently round-trips
    /// only through N-Quads (a gmeow-gts serializer limitation for the other formats).
    #[wasm_bindgen(js_name = serialize)]
    pub fn serialize(&self, format: &str) -> Result<String, JsError> {
        let media_type = resolve_media_type(format).map_err(|e| JsError::new(&e))?;
        let frozen = self.inner.freeze().map_err(diag_to_err)?;
        let bytes =
            serialize_dataset(&frozen, media_type, SerializeGraph::Dataset).map_err(diag_to_err)?;
        String::from_utf8(bytes)
            .map_err(|e| JsError::new(&format!("serialization produced non-UTF-8 bytes: {e}")))
    }

    /// `size` — the number of effective quads.
    #[wasm_bindgen(getter)]
    pub fn size(&self) -> usize {
        self.inner.effective_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dataset_has_zero_size() {
        let ds = Dataset::new().unwrap();
        assert_eq!(ds.size(), 0);
    }

    #[test]
    fn parse_then_serialize_round_trips_ntriples() {
        let input = "<https://e/s> <https://e/p> <https://e/o> .\n";
        let ds = Dataset::parse(input, "ntriples", None).unwrap();
        assert_eq!(ds.size(), 1);
        let out = ds.serialize("ntriples").unwrap();
        assert!(out.contains("https://e/s"));
        assert!(out.contains("https://e/p"));
        assert!(out.contains("https://e/o"));
        // Re-parsing the output yields the same single quad.
        let reparsed = Dataset::parse(&out, "ntriples", None).unwrap();
        assert_eq!(reparsed.size(), 1);
    }

    #[test]
    fn parse_turtle_with_base_resolves_relative_iris() {
        let input = "<rel> <https://e/p> <https://e/o> .\n";
        let ds = Dataset::parse(input, "turtle", Some("https://example.org/".to_owned())).unwrap();
        let out = ds.serialize("ntriples").unwrap();
        assert!(out.contains("https://example.org/rel"));
    }

    // The unsupported-format error path builds a JsError (wasm-only); the pure
    // resolver is unit-tested in `codec`, and the node test in Task 5 exercises the
    // JS-boundary error.
}

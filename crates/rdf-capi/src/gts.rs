// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `purrdf_from_gts` / `purrdf_to_gts`: GTS container read/write.
//!
//! libpurrdf statically reuses the permissive `gmeow-gts` Rust crate (via the
//! oxigraph-free `gts_write` / `import_gts_events` core), so a language shim
//! links `libpurrdf` ALONE and still reads/writes `.gts` containers — the spec's
//! "one shared library, not two" clause (PURRDF-PLAN P8).
//!
//! Losslessness is **layer-dependent in v0.1**: the plain-graph (non-star)
//! round-trip is lossless, but the RDF-1.2 **reifier-binding** layer does NOT
//! yet survive — see [`purrdf_from_gts`] and the kernel gap tracked in #1032.
//! This is exactly the surface the 0.1 (beta) ABI pin exists to carry; the ABI
//! is not frozen at 1.0 until #1032 closes.

use std::os::raw::c_char;

use gmeow_rdf::gts_write::to_gts;
use gmeow_rdf::import_gts_graph;
use gmeow_rdf_core::gts::read_graph;
use gmeow_rdf_core::RdfLookaside;

use crate::buffer::PurrdfBuffer;
use crate::cstr_to_str;
use crate::error::PurrdfError;
use crate::handles::PurrdfDataset;
use crate::status::PurrdfStatus;

/// Read a GTS container into a fresh frozen dataset. `*out_dataset` is a
/// caller-owned handle (free with `purrdf_dataset_free`).
///
/// **Losslessness (v0.1):** plain-graph data round-trips losslessly, but the
/// RDF-1.2 reifier-binding layer does NOT survive yet. `to_gts` writes the
/// reifier slot, but the read-back path (`read_graph` + `import_gts_graph`)
/// drops the binding rows and fails with a `gts-missing-reifier-binding`
/// `GtsError` — and the streaming `import_gts_events` reader, which expects a
/// declares-before-reference event ordering, cannot resolve `to_gts`'s
/// forward-referenced reifier bindings either. This is a kernel-side limitation
/// tracked in #1032; it blocks freezing the ABI at 1.0 (the 0.1 beta pin carries
/// it). The failure is honest: a clean `GtsError`, never a panic or silent
/// corruption.
///
/// # Safety
/// `bytes` must be valid for `len` bytes; the out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn purrdf_from_gts(
    bytes: *const u8,
    len: usize,
    out_dataset: *mut *mut PurrdfDataset,
    out_error: *mut *mut PurrdfError,
) -> i32 {
    ffi_try!(out_error, {
        if bytes.is_null() || out_dataset.is_null() {
            return Err(PurrdfError::new(
                PurrdfStatus::NullPointer,
                "null pointer argument to purrdf_from_gts",
            ));
        }
        let slice = std::slice::from_raw_parts(bytes, len);
        // The canonical fold-back: read the GTS container into a graph, then
        // import the graph into the IR (the streaming `import_gts_events` reader
        // expects a different, declares-before-reference event ordering and
        // cannot resolve `to_gts`'s forward-referenced reifier bindings).
        let graph = read_graph(slice, true).map_err(|diagnostic| {
            PurrdfError::from_diagnostic(PurrdfStatus::GtsError, &diagnostic)
        })?;
        let bundle = import_gts_graph(graph).map_err(|diagnostic| {
            PurrdfError::from_diagnostic(PurrdfStatus::GtsError, &diagnostic)
        })?;
        *out_dataset = PurrdfDataset::into_raw(bundle.dataset);
        Ok(PurrdfStatus::Ok)
    })
}

/// Write a frozen dataset to canonical GTS container bytes under `profile`
/// (e.g. `"dist"`). The output goes to `*out_buffer` (free with
/// `purrdf_buffer_free`).
///
/// # Safety
/// `dataset` must be a live handle; `profile` must be a NUL-terminated C string;
/// the out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn purrdf_to_gts(
    dataset: *const PurrdfDataset,
    profile: *const c_char,
    out_buffer: *mut *mut PurrdfBuffer,
    out_error: *mut *mut PurrdfError,
) -> i32 {
    ffi_try!(out_error, {
        if dataset.is_null() || profile.is_null() || out_buffer.is_null() {
            return Err(PurrdfError::new(
                PurrdfStatus::NullPointer,
                "null pointer argument to purrdf_to_gts",
            ));
        }
        let profile = cstr_to_str(profile)?;
        let bytes = to_gts(
            PurrdfDataset::dataset(dataset),
            &RdfLookaside::default(),
            profile,
        )
        .map_err(|diagnostic| PurrdfError::from_diagnostic(PurrdfStatus::GtsError, &diagnostic))?;
        *out_buffer = PurrdfBuffer::into_raw(bytes);
        Ok(PurrdfStatus::Ok)
    })
}

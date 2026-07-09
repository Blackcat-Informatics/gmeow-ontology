// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A minimal, fully in-memory [`typst::World`] over exactly three inputs: the
//! generated `main.typ` source, a `references.bib` byte buffer, and the fonts
//! embedded from [`typst_assets`]. There is no filesystem, no network, and no
//! clock: [`World::today`] returns `None`, so any document that calls
//! `datetime.today()` is a hard COMPILE error — the reproducibility discipline
//! (no wall-clock in a byte-reproducible artifact) is enforced by the type of
//! the world, not by a lint.

use std::sync::OnceLock;

use typst::Library;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;

/// The virtual path of the main Typst source in the world.
pub const MAIN_PATH: &str = "main.typ";
/// The virtual path of the bibliography database in the world.
pub const BIB_PATH: &str = "references.bib";

/// The embedded font set, parsed once. Both the world and the digest reader read
/// this, so a compiled document and its `embedded_font_digest()` are always over
/// the same bytes.
struct EmbeddedFonts {
    /// The raw font-file bytes, in the fixed `typst_assets::fonts()` order.
    raw: Vec<&'static [u8]>,
    /// The parsed faces (one file can yield several faces), index-aligned with
    /// the [`FontBook`].
    faces: Vec<Font>,
    /// Metadata over `faces`.
    book: LazyHash<FontBook>,
}

fn embedded_fonts() -> &'static EmbeddedFonts {
    static FONTS: OnceLock<EmbeddedFonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        // `typst_assets::fonts()` yields the bundled font files in a fixed order.
        let raw: Vec<&'static [u8]> = typst_assets::fonts().collect();
        let mut faces: Vec<Font> = Vec::new();
        for data in &raw {
            let buffer = Bytes::new(*data);
            for face in Font::iter(buffer) {
                faces.push(face);
            }
        }
        let book = FontBook::from_fonts(&faces);
        EmbeddedFonts {
            raw,
            faces,
            book: LazyHash::new(book),
        }
    })
}

/// The BLAKE3 hex digest over the embedded font-file bytes, concatenated in the
/// sorted order of their bytes. Deterministic and independent of the
/// `typst_assets::fonts()` iteration order, so it pins the exact embedded font
/// set a compiled PDF draws from.
pub fn embedded_font_digest() -> String {
    let mut files: Vec<&'static [u8]> = embedded_fonts().raw.clone();
    files.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for data in files {
        // Length-prefix each file so concatenation is unambiguous.
        hasher.update(&(data.len() as u64).to_le_bytes());
        hasher.update(data);
    }
    hasher.finalize().to_hex().to_string()
}

/// The in-memory world exposed to `typst::compile`.
pub struct PrintWorld {
    library: LazyHash<Library>,
    main_id: FileId,
    main: Source,
    bib_id: FileId,
    bib: Bytes,
}

impl PrintWorld {
    /// Build a world over the generated `typ` source and the `bib` bytes.
    pub fn new(typ: &str, bib: &[u8]) -> Self {
        let main_id = FileId::new(None, VirtualPath::new(MAIN_PATH));
        let bib_id = FileId::new(None, VirtualPath::new(BIB_PATH));
        Self {
            library: LazyHash::new(Library::default()),
            main_id,
            main: Source::new(main_id, typ.to_string()),
            bib_id,
            bib: Bytes::new(bib.to_vec()),
        }
    }
}

impl typst::World for PrintWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &embedded_fonts().book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.bib_id {
            Ok(self.bib.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        embedded_fonts().faces.get(index).cloned()
    }

    /// No clock: a byte-reproducible artifact must not embed the wall time, so a
    /// document calling `datetime.today()` fails to compile. This is deliberate.
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

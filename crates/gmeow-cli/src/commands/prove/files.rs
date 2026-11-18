// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Invocation-owned input files and directory-relative output operations.
//!
//! Paths are reporting coordinates after admission. Held directories prevent a
//! switched parent symlink from redirecting publication or stale-output removal.
//! Arbitrary replacement of entries inside those directories is outside this
//! boundary; writes additionally inspect the actual opened file before truncation.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rustix::fs::{AtFlags, Mode, OFlags, openat, unlinkat};
use same_file::Handle;

use super::{
    InputIdentity, LoadFailure, ProveRequest, caused_failure, failure, rdf_media, source_paths,
    with_inputs,
};

/// An admitted basename in the actual opened parent directory.
struct Location {
    parent: Handle,
    name: OsString,
}

impl Location {
    fn open(resolved: &Path) -> io::Result<Self> {
        let name = resolved.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "selected file needs a basename",
            )
        })?;
        let parent = resolved.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "selected file needs a parent")
        })?;
        let directory = rustix::fs::open(
            parent,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Self {
            parent: Handle::from_file(File::from(directory))?,
            name: name.to_owned(),
        })
    }

    fn same_location(&self, other: &Self) -> bool {
        self.parent == other.parent && self.name == other.name
    }

    fn open_file(&self, flags: OFlags) -> io::Result<File> {
        Ok(File::from(openat(
            self.parent.as_file(),
            self.name.as_os_str(),
            flags | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::from_raw_mode(0o666),
        )?))
    }
}

/// The exact file description read by source loading or proof replay.
pub(super) struct OpenedInput {
    pub(super) path: PathBuf,
    location: Location,
    file: Handle,
}

impl OpenedInput {
    pub(super) fn open(path: PathBuf) -> io::Result<Self> {
        let location = Location::open(&path)?;
        let file = location.open_file(OFlags::RDONLY)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "selected input is not a regular file",
            ));
        }
        Ok(Self {
            path,
            location,
            file: Handle::from_file(file)?,
        })
    }

    pub(super) fn read(&self) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut file = self.file.as_file();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// Keep receipts for the admitted prefix if a later discovered file cannot open.
/// The failure path reads held files only; successful loading reads each once.
pub(super) fn open_sources(paths: Vec<PathBuf>) -> Result<Vec<OpenedInput>, LoadFailure> {
    let mut sources: Vec<OpenedInput> = Vec::with_capacity(paths.len());
    for path in paths {
        match OpenedInput::open(path.clone()) {
            Ok(source) => sources.push(source),
            Err(error) => {
                let mut identities = Vec::with_capacity(sources.len());
                for source in &sources {
                    let bytes = source.read().map_err(|error| {
                        LoadFailure::Malformed(with_inputs(
                            caused_failure(
                                "SOURCE_READ_FAILED",
                                error,
                                format!("read admitted source {}", source.path.display()),
                            ),
                            &identities,
                        ))
                    })?;
                    identities.push(InputIdentity::from_bytes(
                        &source.path,
                        "selected-semantic-source",
                        rdf_media(&source.path).unwrap_or("unknown"),
                        Some(format!("file://{}", source.path.display())),
                        &bytes,
                    ));
                }
                return Err(LoadFailure::Malformed(with_inputs(
                    caused_failure(
                        "SOURCE_READ_FAILED",
                        error,
                        format!("open selected source {}", path.display()),
                    ),
                    &identities,
                )));
            }
        }
    }
    Ok(sources)
}

#[cfg(test)]
#[path = "files.tests.rs"]
mod tests;

struct ReplayInput {
    // A missing replay remains a deferred read failure: composed replay refuses
    // its unsupported contract before attempting to decode a proof.
    opened: Result<OpenedInput, gmeow_errors::Diag>,
    location: Option<Location>,
    path: PathBuf,
}

struct Output {
    path: PathBuf,
    location: Location,
    original: Option<Handle>,
}

impl Output {
    fn open(path: &Path) -> io::Result<Self> {
        let location = Location::open(&prospective_file_path(path)?)?;
        let original = match location.open_file(OFlags::RDONLY) {
            Ok(file) => Some(Handle::from_file(file)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            path: path.to_owned(),
            location,
            original,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum Sink {
    Fof,
    Proof,
    Evidence,
}

/// Every filesystem object selected by one invocation, held through reporting.
#[derive(Default)]
pub(super) struct FilePlan {
    pub(super) sources: Vec<OpenedInput>,
    replay: Option<ReplayInput>,
    outputs: [Option<Output>; 3],
}

impl FilePlan {
    pub(super) fn open(request: &ProveRequest<'_>) -> Result<Self, LoadFailure> {
        let paths = source_paths(request.inputs, request.dir).map_err(LoadFailure::Malformed)?;
        let sources = open_sources(paths)?;
        let replay = request.check_proof.map(|path| {
            let mut location = None;
            let opened = (|| -> io::Result<OpenedInput> {
                let resolved = prospective_file_path(path)?;
                location = Some(Location::open(&resolved)?);
                OpenedInput::open(resolved)
            })()
            .map_err(|error| {
                caused_failure(
                    "PROOF_READ_FAILED",
                    error,
                    format!("open proof {}", path.display()),
                )
            });
            ReplayInput {
                opened,
                location,
                path: path.to_owned(),
            }
        });
        let replay = match replay {
            Some(ReplayInput {
                opened: Err(diagnostic),
                location: None,
                ..
            }) if [request.out, request.proof_out, request.evidence_out]
                .into_iter()
                .any(|output| output.is_some()) =>
            {
                return Err(LoadFailure::Malformed(diagnostic));
            }
            replay => replay,
        };
        let mut plan = Self {
            sources,
            replay,
            outputs: [None, None, None],
        };
        for (index, path) in [request.out, request.proof_out, request.evidence_out]
            .into_iter()
            .enumerate()
        {
            let Some(path) = path else { continue };
            let output = Output::open(path).map_err(|error| {
                LoadFailure::Execution(caused_failure(
                    "OUTPUT_PATH_UNAVAILABLE",
                    error,
                    format!("admit output {}", path.display()),
                ))
            })?;
            for source in &plan.sources {
                Self::check_input(&output, &source.path, &source.location, Some(&source.file))?;
            }
            if let Some(replay) = &plan.replay {
                let (location, identity) = match &replay.opened {
                    Ok(input) => (&input.location, Some(&input.file)),
                    Err(_) => (
                        replay.location.as_ref().expect(
                            "selected sinks require an admitted prospective replay location",
                        ),
                        None,
                    ),
                };
                Self::check_input(&output, &replay.path, location, identity)?;
            }
            for prior in plan.outputs.iter().flatten() {
                if output.location.same_location(&prior.location)
                    || output.original.as_ref().is_some_and(|identity| {
                        prior
                            .original
                            .as_ref()
                            .is_some_and(|prior| prior == identity)
                    })
                {
                    return Err(LoadFailure::Malformed(failure(
                        "SELECTED_OUTPUTS_ALIAS",
                        format!(
                            "output {} aliases selected output {}; select separate output paths",
                            path.display(),
                            prior.path.display()
                        ),
                    )));
                }
            }
            plan.outputs[index] = Some(output);
        }
        Ok(plan)
    }

    fn check_input(
        output: &Output,
        path: &Path,
        location: &Location,
        identity: Option<&Handle>,
    ) -> Result<(), LoadFailure> {
        if output.location.same_location(location)
            || output
                .original
                .as_ref()
                .is_some_and(|output| identity == Some(output))
        {
            return Err(LoadFailure::Malformed(failure(
                "OUTPUT_ALIASES_SELECTED_INPUT",
                format!(
                    "output {} aliases selected input {}; select separate output paths",
                    output.path.display(),
                    path.display()
                ),
            )));
        }
        Ok(())
    }

    fn output(&self, sink: Sink) -> &Output {
        self.outputs[match sink {
            Sink::Fof => 0,
            Sink::Proof => 1,
            Sink::Evidence => 2,
        }]
        .as_ref()
        .expect("only an admitted selected sink is used")
    }

    /// Open without truncating, then inspect the actual target before modifying it.
    pub(super) fn writer(&self, sink: Sink) -> io::Result<File> {
        let output = self.output(sink);
        let file = output.location.open_file(OFlags::RDWR | OFlags::CREATE)?;
        let identity = Handle::from_file(file.try_clone()?)?;
        if self.sources.iter().any(|source| source.file == identity)
            || self.replay.as_ref().is_some_and(|replay| {
                replay
                    .opened
                    .as_ref()
                    .is_ok_and(|input| input.file == identity)
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "opened output aliases a selected input; no bytes were truncated",
            ));
        }
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "selected output is not a regular file",
            ));
        }
        if self
            .outputs
            .iter()
            .flatten()
            .any(|other| !std::ptr::eq(other, output) && other.original.as_ref() == Some(&identity))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "opened output aliases another selected sink; no bytes were truncated",
            ));
        }
        file.set_len(0)?;
        Ok(file)
    }

    pub(super) fn write(&self, sink: Sink, bytes: &[u8]) -> io::Result<()> {
        let mut file = self.writer(sink)?;
        file.write_all(bytes)?;
        file.flush()
    }

    pub(super) fn remove(&self, sink: Sink) -> io::Result<()> {
        let output = self.output(sink);
        match unlinkat(
            output.location.parent.as_file(),
            output.location.name.as_os_str(),
            AtFlags::empty(),
        ) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn replay_input(&self) -> Result<&OpenedInput, &gmeow_errors::Diag> {
        self.replay
            .as_ref()
            .expect("proof replay selects an input")
            .opened
            .as_ref()
    }
}

/// Resolve a terminal link, or an absent file's parent and future basename.
fn prospective_file_path(path: &Path) -> io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match std::fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "a selected file path is a dangling symlink",
                    ));
                }
                Ok(_) => return Err(error),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            let absolute = std::path::absolute(path)?;
            let name = absolute.file_name().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "selected file needs a basename",
                )
            })?;
            let parent = absolute.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "selected file needs a parent")
            })?;
            Ok(std::fs::canonicalize(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}

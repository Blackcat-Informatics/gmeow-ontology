// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicitly selected, process-wide libc allocator observations for producer diagnosis.
//!
//! The `glibc` profile records native, all-arena `malloc_info` XML. It does not
//! measure Rust ownership or RSS, and concurrent allocations mean the report is
//! not an atomic process snapshot. Collection allocates a diagnostic thread
//! stack, a native stream, and XML buffers, and can contend for allocator and IO
//! locks. Observed bytes never enter products or cache identities.

use std::fmt;
use std::io::{self, Write};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROFILE_ENV: &str = "GMEOW_ALLOCATOR_OBSERVATIONS";
const INTERVAL: Duration = Duration::from_secs(30);

/// A selected observer whose owner must call [`Self::finish`] before exiting.
#[must_use = "finish the allocator observer before process exit"]
pub struct AllocatorObserver {
    stop: Sender<()>,
    worker: JoinHandle<io::Result<()>>,
    started: Instant,
}

impl AllocatorObserver {
    /// Start only when `GMEOW_ALLOCATOR_OBSERVATIONS=glibc` is selected.
    ///
    /// An absent variable leaves the ordinary CLI path unchanged. Unknown
    /// profiles, unsupported targets, and failed baseline observations are errors.
    ///
    /// # Errors
    /// Returns the profile, capture, output, or thread-start failure.
    pub fn start_from_env() -> io::Result<Option<Self>> {
        match std::env::var(PROFILE_ENV) {
            Err(std::env::VarError::NotPresent) => return Ok(None),
            Ok(profile) if profile == "glibc" => {}
            profile => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{PROFILE_ENV} requires the profile glibc; received {profile:?}"),
                ));
            }
        }
        let started = Instant::now();
        observe("baseline", started)?;
        let (stop, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("allocator-observation".into())
            .spawn(move || {
                loop {
                    match receiver.recv_timeout(INTERVAL) {
                        Ok(()) => return Ok(()),
                        Err(RecvTimeoutError::Timeout) => observe("periodic", started)?,
                        Err(RecvTimeoutError::Disconnected) => {
                            return Err(io::Error::other(
                                "allocator observer owner disconnected without finishing",
                            ));
                        }
                    }
                }
            })
            .map_err(|error| context("start observation thread", error))?;
        Ok(Some(Self {
            stop,
            worker,
            started,
        }))
    }

    /// Stop and join the diagnostic thread, then attempt the final observation.
    ///
    /// A failed periodic observation is retained even if the final one succeeds.
    /// All shutdown steps are attempted, with subsequent failures also reported.
    ///
    /// # Errors
    /// Returns observation, thread-shutdown, or output failures.
    pub fn finish(self) -> io::Result<()> {
        let stop_result = self
            .stop
            .send(())
            .map_err(|error| context("stop allocator observation thread", io::Error::other(error)));
        let worker_result = self
            .worker
            .join()
            .map_err(|_| io::Error::other("allocator observation thread panicked"))
            .and_then(|result| result);
        let final_result = observe("final", self.started);
        combine(combine(worker_result, stop_result), final_result)
    }
}

#[derive(Debug)]
struct ObservationFailure {
    operation: &'static str,
    source: io::Error,
    subsequent: Option<io::Error>,
}

impl fmt::Display for ObservationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.source)?;
        if let Some(error) = &self.subsequent {
            write!(formatter, "; subsequent failure: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ObservationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn context(operation: &'static str, source: io::Error) -> io::Error {
    io::Error::new(
        source.kind(),
        ObservationFailure {
            operation,
            source,
            subsequent: None,
        },
    )
}

fn combine(first: io::Result<()>, subsequent: io::Result<()>) -> io::Result<()> {
    match (first, subsequent) {
        (Err(source), Err(subsequent)) => Err(io::Error::new(
            source.kind(),
            ObservationFailure {
                operation: "allocator observation",
                source,
                subsequent: Some(subsequent),
            },
        )),
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn observe(phase: &str, started: Instant) -> io::Result<()> {
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis();
    let xml = capture_glibc()?;
    let mut stderr = io::stderr().lock();
    let result = (|| {
        writeln!(
            stderr,
            "allocator-observation start profile=glibc pid={} phase={phase} unix_ms={unix_ms} elapsed_ms={} bytes={}",
            std::process::id(),
            started.elapsed().as_millis(),
            xml.len(),
        )?;
        stderr.write_all(&xml)?;
        writeln!(
            stderr,
            "\nallocator-observation end profile=glibc pid={} phase={phase}",
            std::process::id(),
        )?;
        stderr.flush()
    })();
    result.map_err(|error| context("write allocator observation to stderr", error))
}

/// GNU documents `malloc_info` as MT-Safe and covering all arenas, unlike the
/// documented main-arena limitation of `mallinfo2`. Preserve its versioned XML
/// without assuming a fixed schema or deriving allocator in-use arithmetic.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn capture_glibc() -> io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::fd::{AsRawFd, IntoRawFd};

    let mut file = tempfile::tempfile()
        .map_err(|error| context("create allocator observation temporary file", error))?;
    let duplicate = file
        .try_clone()
        .map_err(|error| context("clone allocator observation descriptor", error))?;
    // SAFETY: the live duplicate is writable and uniquely owned here. `fdopen`
    // does not duplicate it. Failure leaves its ownership with Rust; success
    // transfers ownership below, before any other operation uses the stream.
    let stream = unsafe { libc::fdopen(duplicate.as_raw_fd(), c"w".as_ptr()) };
    if stream.is_null() {
        return Err(context(
            "fdopen allocator observation",
            io::Error::last_os_error(),
        ));
    }
    let _transferred_descriptor = duplicate.into_raw_fd();
    // SAFETY: the stream owns the duplicate until the exactly one fclose below.
    // No Rust IO accesses the shared file offset in this interval. glibc declares
    // malloc_info MT-Safe; options=0 is its required argument. These calls and
    // errno captures cannot unwind, so every opened stream reaches fclose even
    // when malloc_info or stream IO fails. No pointer/length from a possibly
    // failed memory-stream flush is ever exposed to Rust.
    let (status, native_error, stream_failed, close_error) = unsafe {
        let status = libc::malloc_info(0, stream);
        let native_error = io::Error::last_os_error();
        let stream_failed = libc::ferror(stream) != 0;
        let close_status = libc::fclose(stream);
        let close_error = (close_status != 0).then(io::Error::last_os_error);
        (status, native_error, stream_failed, close_error)
    };
    let capture_result = if status != 0 {
        Err(context("malloc_info", native_error))
    } else if stream_failed {
        Err(context("malloc_info stream write", native_error))
    } else {
        Ok(())
    };
    let close_result = close_error.map_or(Ok(()), |error| {
        Err(context("fclose allocator observation", error))
    });
    combine(capture_result, close_result)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| context("rewind allocator observation", error))?;
    let mut xml = Vec::new();
    file.read_to_end(&mut xml)
        .map_err(|error| context("read allocator observation", error))?;
    Ok(xml)
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn capture_glibc() -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the selected glibc allocator observation profile requires GNU/Linux",
    ))
}

//! Rewrites the extensions other tools' metadata is written with into the
//! native `x-forge-*` ones, in a JSON document.
//!
//! This is the compatibility layer run as a rewrite instead of a read. The
//! key a tool writes is found with the same tables `forge` reads a document
//! with, at the same kinds of object, so a key is only rewritten where `forge`
//! would have read it: a key of an object that `forge` does not read there is
//! left alone, since rewriting it would give it a meaning it did not have.
//!
//! A native key is never replaced. When a document says the same thing with a
//! native key and another, the native one is kept, as `forge` would read it,
//! and the other is reported if it says something else.
//!
//! The document is edited, not written again: what is decided about a key is
//! applied to the bytes it was read from, so everything but the keys that are
//! rewritten, such as the text of every number, the whitespace and the order
//! of the keys, stays as it was. It is walked once, as bytes, and not parsed:
//! only the values of the keys that are rewritten are read. It is checked to
//! be JSON beside the walk, on another thread when it is long, so that what
//! is written is JSON without being checked again: an edit that replaces or
//! removes whole keys of a document that is JSON leaves it JSON. The edits
//! are made as the document is written out, with [`Edited::write_to`].

mod scan;
mod walk;

pub use scan::Edited;

use std::collections::BTreeMap;
use std::fmt;
use std::thread;

use serde::de::IgnoredAny;

/// How long a document has to be for it to be checked on another thread, as
/// it costs more than it saves for one that is shorter.
const PARALLEL: usize = 1 << 18;

/// What [`normalize`] does.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Keeps each key that is rewritten, beside the native key that replaces
    /// it, for what still reads the old one.
    pub leave_old_extensions: bool,
}

/// What [`normalize`] did.
#[derive(Debug, Default)]
pub struct Report {
    /// How many keys were rewritten, or removed for a native key that
    /// already said it, by the name of the key.
    pub migrated: BTreeMap<String, usize>,
    /// The keys that say something other than the native key beside them.
    pub conflicts: Vec<Conflict>,
    /// How many group paths were written again, as the names joined by dots,
    /// which is the one value that is not copied as it was.
    pub joined: usize,
}

impl Report {
    /// Returns whether the document is different from what it was.
    pub fn changed(&self) -> bool {
        !self.migrated.is_empty()
    }
}

/// A key that says something other than the native key beside it, which
/// wins.
#[derive(Debug)]
pub struct Conflict {
    /// Where the object is, as a JSON Pointer.
    pub location: String,
    /// The key that lost.
    pub key: String,
    /// The native key that won.
    pub native: &'static str,
}

/// A document after [`normalize`].
#[derive(Debug)]
pub struct Normalized<'j> {
    /// The document, if it is different from what was read.
    pub json: Option<Edited<'j>>,
    /// What was done to it.
    pub report: Report,
}

/// Why a document could not be normalized.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        Error(err.to_string())
    }
}

/// Rewrites the keys of the document `json` that a native key replaces.
pub fn normalize<'j>(json: &'j [u8], options: &Options) -> Result<Normalized<'j>, Error> {
    if json.starts_with(b"\xEF\xBB\xBF") {
        return Err(Error(
            "the document starts with a byte order mark, which JSON does not have: save it as UTF-8 without one"
                .to_owned(),
        ));
    }
    if json.starts_with(b"\xFF\xFE") || json.starts_with(b"\xFE\xFF") {
        return Err(Error("the document is UTF-16, and has to be UTF-8".to_owned()));
    }
    // Without this, a document that is not JSON, YAML for one, would be found
    // to have nothing to rewrite, as the keys are not written as JSON writes
    // them.
    match json.iter().find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r')) {
        None => return Err(Error("the document is empty".to_owned())),
        Some(b'{') => {}
        Some(_) => {
            return Err(Error(
                "the document is not a JSON object: it does not start with `{`".to_owned(),
            ));
        }
    }

    let (edited, report) = check_and_walk(json, options, json.len() >= PARALLEL)?;
    let Some(edited) = edited else {
        return Ok(Normalized { json: None, report });
    };
    debug_assert!(check(&edited.to_vec()).is_ok(), "the edits made the document not JSON");
    Ok(Normalized {
        json: Some(edited),
        report,
    })
}

/// Checks that `json` is a JSON document, and walks it, beside each other when
/// `threaded`. The walk is the slower of the two, so that what is written does
/// not have to be checked: a document that is JSON is still JSON when whole
/// keys of it are replaced or removed. A document that is not is an error
/// whatever the walk made of it, so it is the check that has the last word.
pub(crate) fn check_and_walk<'j>(
    json: &'j [u8],
    options: &Options,
    threaded: bool,
) -> Result<(Option<Edited<'j>>, Report), Error> {
    if threaded {
        let both = thread::scope(|scope| {
            // Not a thread to be had, which is possible, is not a failure.
            let checking = thread::Builder::new().spawn_scoped(scope, || check(json)).ok()?;
            let walked = walk::walk(json, options);
            let checked = checking.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            Some((checked, walked))
        });
        if let Some((checked, walked)) = both {
            checked?;
            return walked;
        }
    }
    check(json)?;
    walk::walk(json, options)
}

/// Checks that `json` is a JSON document.
pub(crate) fn check(json: &[u8]) -> Result<(), Error> {
    // Reading a document without keeping it does not check that its strings
    // are UTF-8.
    if let Err(err) = std::str::from_utf8(json) {
        return Err(Error(format!(
            "the document is not UTF-8: it is not from byte {}",
            err.valid_up_to()
        )));
    }
    serde_json::from_slice::<IgnoredAny>(json)?;
    Ok(())
}

/// Walks `json` without checking it, which `normalize` only does to a document
/// that is JSON, for tests of what the walk does to one that is not.
#[cfg(test)]
pub(crate) fn walk_unchecked<'j>(json: &'j [u8], options: &Options) -> Result<(Option<Edited<'j>>, Report), Error> {
    walk::walk(json, options)
}

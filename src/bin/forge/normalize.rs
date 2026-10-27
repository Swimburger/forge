//! `forge normalize`, which rewrites the extensions other tools' metadata is
//! written with into the native ones, in the files it is given.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use forge::Err;
use forge::normalize::{Edited, Normalized, Options, Report, normalize};
use rayon::prelude::*;

/// What `forge normalize` is given, which says what it does in `main.rs`.
#[derive(clap::Args)]
pub struct Args {
    /// The OpenAPI JSON documents to rewrite, as paths, as folders, which
    /// have every `.json` file below them rewritten, except those that are
    /// hidden or in a node_modules, or as glob patterns such as
    /// 'specs/**/*.json', which are taken as they are written. Or - to read one from standard input and write
    /// it to standard output.
    #[arg(required = true)]
    inputs: Vec<String>,
    /// Keep each key that is rewritten, beside the native key that replaces
    /// it, for what still reads the old one.
    #[arg(long)]
    leave_old_extensions: bool,
    /// Say what would be rewritten, and write nothing. It fails if there is
    /// anything to rewrite, to check that documents are normalized.
    #[arg(long)]
    check: bool,
    /// Say how many times each key was rewritten.
    #[arg(short, long)]
    verbose: bool,
}

/// A file to rewrite.
struct Input {
    /// The path it was given by, which is how it is named.
    shown: PathBuf,
    /// The file it is, with every link followed, which is what is read and
    /// written: a link stays a link, and a file given twice is rewritten once.
    real: PathBuf,
}

/// How a run says what it did.
#[derive(Clone, Copy)]
struct Say {
    check: bool,
    verbose: bool,
}

pub fn run(args: Args) -> Result<(), Err> {
    let options = Options {
        leave_old_extensions: args.leave_old_extensions,
    };
    let say = Say {
        check: args.check,
        verbose: args.verbose,
    };
    if args.inputs.iter().any(|input| input == "-") {
        if args.inputs.len() > 1 {
            return Err("- cannot be given with other inputs".into());
        }
        return filter(&options, say);
    }

    let results: Vec<(PathBuf, Result<Report, String>)> = expand(&args.inputs)?
        .into_par_iter()
        .map(|input| {
            let result = rewrite(&input.real, &options, say).map_err(|err| format!("{}: {err}", input.shown.display()));
            (input.shown, result)
        })
        .collect();

    let (mut rewritten, mut unchanged, mut failed, mut warnings) = (0, 0, 0, 0);
    for (path, result) in results {
        match result {
            Ok(report) => {
                if report.changed() {
                    rewritten += 1;
                } else {
                    unchanged += 1;
                }
                warnings += report.conflicts.len();
                print(&path.display().to_string(), &report, say);
            }
            Err(err) => {
                failed += 1;
                eprintln!("forge: {err}");
            }
        }
    }
    let total = rewritten + unchanged;
    let warned = match warnings {
        0 => String::new(),
        n => format!(", {n} {}", plural(n, "warning", "warnings")),
    };
    if failed > 0 {
        return Err(format!("{failed} {} could not be processed", plural(failed, "file", "files")).into());
    }
    if say.check {
        if rewritten > 0 {
            return Err(format!(
                "{rewritten} of {total} {} would be rewritten{warned}",
                plural(total, "file", "files")
            )
            .into());
        }
        eprintln!(
            "forge: {total} {} checked, none would be rewritten{warned}",
            plural(total, "file", "files")
        );
    } else {
        eprintln!(
            "forge: {total} {}: {rewritten} rewritten, {unchanged} unchanged{warned}",
            plural(total, "file", "files")
        );
    }
    Ok(())
}

/// Returns `one` if `n` is 1, and `many` if it is not.
fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// Rewrites the document read from standard input to standard output.
fn filter(options: &Options, say: Say) -> Result<(), Err> {
    let mut json = Vec::new();
    io::stdin().read_to_end(&mut json)?;
    let normalized = normalize(&json, options).map_err(|err| format!("stdin: {err}"))?;
    if say.check {
        print("stdin", &normalized.report, say);
        if normalized.report.changed() {
            return Err("the document would be rewritten".into());
        }
        return Ok(());
    }
    let mut out = io::stdout().lock();
    match &normalized.json {
        Some(edited) => edited.write_to(&mut out)?,
        None => out.write_all(&json)?,
    }
    // Standard output is held until a line is whole, and what is left at the
    // end, which is all of a document that is one line, is not otherwise
    // known to have been written.
    out.flush()?;
    print("stdin", &normalized.report, say);
    Ok(())
}

/// Returns the files `inputs` name: each is a path, a folder, which is every
/// `.json` file below it, or a glob pattern if it has any of `*?[` and is not
/// a file or a folder by that name, which must match a file.
fn expand(inputs: &[String]) -> Result<Vec<Input>, Err> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for input in inputs {
        let found = if Path::new(input).is_file() {
            vec![PathBuf::from(input)]
        } else if Path::new(input).is_dir() {
            let pattern = format!("{}/**/*.json", glob::Pattern::escape(input.trim_end_matches('/')));
            let found = glob_files(&pattern, input, true)?;
            if found.is_empty() {
                return Err(format!("{input}: no .json files below it").into());
            }
            found
        } else if input.contains(['*', '?', '[']) {
            let found = glob_files(input, input, false)?;
            if found.is_empty() {
                return Err(format!("{input}: no files match").into());
            }
            found
        } else {
            // Not there, which reading it says.
            vec![PathBuf::from(input)]
        };
        for shown in found {
            let real = fs::canonicalize(&shown).unwrap_or_else(|_| shown.clone());
            if seen.insert(Identity::of(&real)) {
                files.push(Input { shown, real });
            }
        }
    }
    Ok(files)
}

/// Which file a path is, so that a file with more than one name, which a hard
/// link is, is found to be one, and is not rewritten by two threads at once.
#[derive(PartialEq, Eq, Hash)]
enum Identity {
    /// The device it is on, and where on it.
    #[cfg(unix)]
    Inode(u64, u64),
    Path(PathBuf),
}

impl Identity {
    fn of(real: &Path) -> Identity {
        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(real) {
            use std::os::unix::fs::MetadataExt;

            return Identity::Inode(metadata.dev(), metadata.ino());
        }
        Identity::Path(real.to_owned())
    }
}

/// Returns the files the glob `pattern` matches, which came from `input`. A
/// folder that is given has the files below it that are not hidden, and not in
/// a `node_modules`, which is `folder`; a pattern is taken as it is written.
fn glob_files(pattern: &str, input: &str, folder: bool) -> Result<Vec<PathBuf>, Err> {
    let options = glob::MatchOptions {
        require_literal_leading_dot: folder,
        ..glob::MatchOptions::new()
    };
    let mut files = Vec::new();
    for path in glob::glob_with(pattern, options).map_err(|err| format!("{input}: {err}"))? {
        let path = path.map_err(|err| format!("{input}: {err}"))?;
        let packaged = folder
            && path
                .strip_prefix(input.trim_end_matches('/'))
                .is_ok_and(|below| below.components().any(|part| part.as_os_str() == "node_modules"));
        if path.is_file() && !packaged {
            files.push(path);
        }
    }
    Ok(files)
}

/// Rewrites the file at `path`, if it has a key to rewrite, and says what it
/// found, which is all it does when it is only checking.
fn rewrite(path: &Path, options: &Options, say: Say) -> Result<Report, Err> {
    let yaml = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml"));
    if yaml {
        return Err("YAML is not supported yet: convert the document to JSON first".into());
    }
    let json = fs::read(path)?;
    let Normalized { json, report } = normalize(&json, options)?;
    if let Some(edited) = &json
        && !say.check
    {
        if fs::metadata(path)?.permissions().readonly() {
            return Err("the file is read-only, so it is not rewritten".into());
        }
        write(path, edited)?;
    }
    Ok(report)
}

/// Writes `json` to the file at `path`. It is written beside it first, and put
/// in its place, so that it is never left half written. Where a new file cannot
/// take the place of this one, because it has other links, which would be left
/// with the old one, or because its folder cannot be written to, it is written
/// in place.
fn write(path: &Path, edited: &Edited) -> Result<(), Err> {
    if linked(path) {
        return Ok(write_in_place(path, edited)?);
    }
    match write_atomically(path, edited) {
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => Ok(write_in_place(path, edited)?),
        written => Ok(written?),
    }
}

fn write_in_place(path: &Path, edited: &Edited) -> io::Result<()> {
    edited.write_to(&mut fs::File::create(path)?)
}

/// Returns whether the file at `path` has other names.
#[cfg(unix)]
fn linked(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    fs::metadata(path).is_ok_and(|metadata| metadata.nlink() > 1)
}

#[cfg(not(unix))]
fn linked(_: &Path) -> bool {
    false
}

fn write_atomically(path: &Path, edited: &Edited) -> io::Result<()> {
    let name = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.forge-normalize-{}", std::process::id()));
    let permissions = fs::metadata(path)?.permissions();
    let written = create_private(&temporary)
        .and_then(|mut file| {
            // Before it has anything in it, so that it is not readable by more
            // than the file it replaces is, at any time.
            file.set_permissions(permissions)?;
            edited.write_to(&mut file)
        })
        .and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Creates the file at `path`, which is not there, and is not a link to
/// another file that is, and that only its owner can read.
fn create_private(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    options.open(path)
}

/// Says what was done to the document called `name`, and what was not.
fn print(name: &str, report: &Report, say: Say) {
    if report.changed() {
        let total: usize = report.migrated.values().sum();
        let did = if say.check { "would rewrite" } else { "rewrote" };
        eprintln!("{name}: {did} {total} {}", plural(total, "key", "keys"));
        if say.verbose {
            for (key, count) in &report.migrated {
                eprintln!("  {key}: {count}");
            }
            if report.joined > 0 {
                eprintln!(
                    "  {} group {} written as a dotted string",
                    report.joined,
                    plural(report.joined, "path", "paths")
                );
            }
        }
    }
    for conflict in &report.conflicts {
        eprintln!(
            "{name}: warning: {}: {} says something other than {}, which is kept",
            conflict.location, conflict.key, conflict.native
        );
    }
}

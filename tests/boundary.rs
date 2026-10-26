//! The boundary of the compatibility layer: no other tool's extension is named
//! anywhere but in it.
//!
//! The compiler keeps what the layer reads out of reach of the rest of the
//! crate, but it cannot see a name in a doc comment, a match arm or a test
//! document, so this searches for them. It reads the engine, its tests, and
//! the npm package that ships its docs and its WIT, and fails with the file
//! and line of each name it finds outside the layer and its tests.

use std::fs;
use std::path::{Path, PathBuf};

/// What the extensions of each tool forge reads begin with.
const VENDORS: [&str; 2] = ["x-fern", "x-stainless"];

/// Returns whether `line` names `vendor`, as written or as an identifier
/// spells it, with `_` for `-`, as in a test named after a key.
fn names(line: &str, vendor: &str) -> bool {
    line.contains(vendor) || line.contains(&vendor.replace('-', "_"))
}

/// Where those names may be: the layer, its tests, and this file, which
/// writes them to look for them.
const ALLOWED: [&str; 3] = ["src/compat/", "tests/compat/", "tests/boundary.rs"];

/// What is searched, from the root of the repository.
const SEARCHED: [&str; 3] = ["src", "tests", "packages/forge"];

/// The folders that hold what is built or installed, not what is written.
const SKIPPED: [&str; 4] = ["target", "node_modules", "dist", ".git"];

/// Adds every file under `dir` to `out`, leaving out the `SKIPPED` folders.
fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|name| name.to_str());
            if name.is_some_and(|name| SKIPPED.contains(&name)) {
                continue;
            }
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Returns each file under `SEARCHED` as its path from the root, with `/`
/// between folders, and what it says. A file that is not text is left out.
fn searched() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    for dir in SEARCHED {
        files(&root.join(dir), &mut paths);
    }
    let mut searched = Vec::new();
    for path in paths {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        searched.push((relative, text));
    }
    searched
}

#[test]
fn no_extension_of_another_tool_is_named_outside_the_compatibility_layer() {
    let mut found = Vec::new();
    for (path, text) in searched() {
        if ALLOWED.iter().any(|allowed| path.starts_with(allowed)) {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            if VENDORS.iter().any(|vendor| names(line, vendor)) {
                found.push(format!("{path}:{}: {}", i + 1, line.trim()));
            }
        }
    }

    assert!(
        found.is_empty(),
        "the extensions of other tools are named only in src/compat/ and tests/compat/, but:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_search_covers_the_engine_its_tests_and_the_npm_package() {
    let searched = searched();
    let has = |path: &str| searched.iter().any(|(searched, _)| searched == path);

    for path in [
        "src/lib.rs",
        "src/link/openapi.rs",
        "src/plugin/wasm.rs",
        "tests/parse/main.rs",
        "packages/forge/src/index.ts",
        "packages/forge/plugin/forge.wit",
    ] {
        assert!(has(path), "{path} is not searched");
    }
}

#[test]
fn the_search_finds_the_names_it_looks_for_where_they_are_allowed() {
    // So that finding none elsewhere means something.
    let searched = searched();
    for vendor in VENDORS {
        assert!(
            searched
                .iter()
                .any(|(path, text)| path.starts_with("src/compat/") && text.contains(vendor)),
            "src/compat/ does not name {vendor}"
        );
    }
}

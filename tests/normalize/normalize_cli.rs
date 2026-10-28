//! `forge normalize`, the command, on files and on standard input.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

/// A document with one operation, written on one line, with no final newline.
const FERN: &str = r#"{"openapi":"3.0.3","info":{"title":"t","version":"1"},"paths":{"/a":{"get":{"x-fern-sdk-group-name":["a"],"x-fern-sdk-method-name":"get","responses":{"204":{"description":"ok"}}}}}}"#;

/// The same document, written native.
const NATIVE: &str = r#"{"openapi":"3.0.3","info":{"title":"t","version":"1"},"paths":{"/a":{"get":{"x-forge-group-name":"a","x-forge-method-name":"get","responses":{"204":{"description":"ok"}}}}}}"#;

/// A folder of its own to write files in, which is empty.
fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("normalize").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `forge normalize` with `args`, writing `stdin` to it.
fn normalize(args: &[&str], stdin: &str) -> Output {
    forge(&[&["normalize"], args].concat(), stdin)
}

/// Runs `forge` with `args`, writing `stdin` to it.
fn forge(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Beside the output being read: a child that has written more than a pipe
    // holds is not reading its input until that is read.
    let mut input = child.stdin.take().unwrap();
    let stdin = stdin.to_owned();
    // A child that exits before it has read it all is not a failure to write.
    let writer = std::thread::spawn(move || input.write_all(stdin.as_bytes()));
    let output = child.wait_with_output().unwrap();
    let _ = writer.join().unwrap();
    output
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[track_caller]
fn succeeded(out: &Output) {
    assert!(out.status.success(), "forge failed: {}", text(&out.stderr));
}

#[track_caller]
fn failed(out: &Output) -> &str {
    assert!(!out.status.success(), "forge succeeded");
    text(&out.stderr)
}

#[test]
fn a_file_is_rewritten_in_place_as_it_was_written() {
    let dir = dir("in-place");
    let file = dir.join("spec.json");
    fs::write(&file, FERN).unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        NATIVE,
        "still on one line, with no final newline"
    );
    let err = text(&out.stderr);
    assert!(err.contains("rewrote 2 keys"), "{err}");
    assert!(
        !err.contains("x-fern-sdk-method-name: 1"),
        "the keys are listed only with -v: {err}"
    );
    assert!(err.contains("1 file: 1 rewritten, 0 unchanged"), "{err}");
    let leftovers: Vec<_> = fs::read_dir(&dir).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no file is left beside it");
}

#[test]
fn nothing_but_the_keys_that_are_rewritten_changes() {
    let dir = dir("untouched");
    let file = dir.join("spec.json");
    // Indented by four spaces, with a number that has more digits than a
    // float keeps, another written as an exponent, and escapes.
    let before = "{\n    \"openapi\": \"3.0.3\",\n    \"info\": {\"title\": \"t\\u00e9\\/\", \"version\": \"1\"},\n    \"paths\": {\"/a\": {\"get\": {\n        \"x-fern-sdk-group-name\": [\"a\"],\n        \"x-fern-sdk-method-name\": \"get\",\n        \"responses\": {\"200\": {\"description\": \"ok\", \"content\": {\"application/json\": {\"schema\": {\"type\": \"number\", \"example\": 116.11599994199997, \"minimum\": 0.000005, \"maximum\": 1E5}}}}}\n    }}}\n}\n";
    fs::write(&file, before).unwrap();

    succeeded(&normalize(&[file.to_str().unwrap()], ""));

    let after = before
        .replace("\"x-fern-sdk-group-name\": [\"a\"]", "\"x-forge-group-name\": \"a\"")
        .replace("x-fern-sdk-method-name", "x-forge-method-name");
    assert_eq!(fs::read_to_string(&file).unwrap(), after);
}

#[test]
fn a_file_that_needs_nothing_is_left_as_it_is() {
    let dir = dir("native");
    let file = dir.join("spec.json");
    let written = format!("{NATIVE}\n\n");
    fs::write(&file, &written).unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    assert_eq!(fs::read_to_string(&file).unwrap(), written);
    assert!(
        text(&out.stderr).contains("1 file: 0 rewritten, 1 unchanged"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn the_old_keys_can_be_left() {
    let dir = dir("leave");
    let file = dir.join("spec.json");
    fs::write(&file, FERN).unwrap();

    succeeded(&normalize(&["--leave-old-extensions", file.to_str().unwrap()], ""));

    let value: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let operation = &value["paths"]["/a"]["get"];
    assert_eq!(operation["x-fern-sdk-method-name"], "get");
    assert_eq!(operation["x-forge-method-name"], "get");
}

#[test]
fn standard_input_is_written_to_standard_output() {
    let out = normalize(&["-"], FERN);

    succeeded(&out);
    assert_eq!(text(&out.stdout), NATIVE);
}

#[test]
fn standard_input_that_needs_nothing_comes_out_as_it_went_in() {
    let document = format!("  {NATIVE}  \n");

    let out = normalize(&["-"], &document);

    succeeded(&out);
    assert_eq!(text(&out.stdout), document);
}

#[test]
fn standard_input_cannot_be_given_with_files() {
    let out = normalize(&["-", "spec.json"], "");

    let err = failed(&out);

    assert!(err.contains("- cannot be given with other inputs"), "{err}");
}

#[test]
fn a_file_that_is_missing_is_an_error_that_names_it() {
    let out = normalize(&["missing.json"], "");

    let err = failed(&out);
    assert!(err.contains("forge: missing.json: "), "{err}");
}

#[test]
fn a_file_that_is_not_json_is_an_error_and_the_others_are_still_rewritten() {
    let dir = dir("invalid");
    fs::write(dir.join("a.json"), FERN).unwrap();
    // It has to look like it might change, or it is not parsed.
    fs::write(dir.join("b.json"), r#"{"x-fern-ignore": "#).unwrap();

    let (a, b) = (dir.join("a.json"), dir.join("b.json"));

    let out = normalize(&[a.to_str().unwrap(), b.to_str().unwrap()], "");

    let err = failed(&out);
    assert!(err.contains("b.json"), "{err}");
    assert!(err.contains("1 file could not be processed"), "{err}");
    assert_eq!(fs::read_to_string(dir.join("a.json")).unwrap(), NATIVE);
    assert_eq!(fs::read_to_string(dir.join("b.json")).unwrap(), r#"{"x-fern-ignore": "#);
}

#[test]
fn a_native_key_wins_and_the_one_that_differs_is_reported() {
    let dir = dir("conflict");
    let file = dir.join("spec.json");
    fs::write(
        &file,
        r#"{"openapi":"3.0.3","info":{"title":"t","version":"1"},"paths":{"/a":{"get":{"x-fern-sdk-method-name":"old","x-forge-method-name":"new","x-forge-group-name":["a"],"responses":{"204":{"description":"ok"}}}}}}"#,
    )
    .unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    let err = text(&out.stderr);
    assert!(err.contains("warning"), "{err}");
    assert!(
        err.contains("/paths/~1a/get: x-fern-sdk-method-name says something other than x-forge-method-name"),
        "{err}"
    );
    let written = fs::read_to_string(&file).unwrap();
    assert!(written.contains(r#""x-forge-method-name":"new""#), "{written}");
    assert!(!written.contains("x-fern"), "{written}");
}

#[test]
fn a_group_that_cannot_be_a_dotted_string_is_kept_and_nothing_is_said_about_it() {
    let dir = dir("kept");
    let file = dir.join("spec.json");
    fs::write(&file, FERN.replace(r#"["a"]"#, r#"["a.b"]"#)).unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    let err = text(&out.stderr);
    assert!(
        !err.contains("note") && !err.contains("warning"),
        "an array that is kept is expected: {err}"
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        NATIVE.replace(r#""x-forge-group-name":"a""#, r#""x-forge-group-name":["a.b"]"#),
        "the array is kept, and only the keys are renamed"
    );
}

#[test]
fn what_it_did_is_a_line_for_each_file_and_the_keys_are_listed_with_v() {
    let dir = dir("verbose");
    for name in ["a.json", "b.json"] {
        fs::write(dir.join(name), FERN).unwrap();
    }
    let (a, b) = (dir.join("a.json"), dir.join("b.json"));

    let quiet = normalize(&[a.to_str().unwrap()], "");
    let verbose = normalize(&["-v", b.to_str().unwrap()], "");

    let (quiet, verbose) = (text(&quiet.stderr), text(&verbose.stderr));
    assert_eq!(quiet.lines().count(), 2, "{quiet}");
    assert!(verbose.contains("  x-fern-sdk-group-name: 1"), "{verbose}");
    assert!(verbose.contains("  x-fern-sdk-method-name: 1"), "{verbose}");
}

#[test]
fn check_writes_nothing_and_fails_when_there_is_something_to_rewrite() {
    let dir = dir("check");
    let (old, native) = (dir.join("old.json"), dir.join("native.json"));
    fs::write(&old, FERN).unwrap();
    fs::write(&native, NATIVE).unwrap();

    let out = normalize(&["--check", old.to_str().unwrap(), native.to_str().unwrap()], "");

    let err = failed(&out);
    assert!(err.contains("old.json: would rewrite 2 keys"), "{err}");
    assert!(err.contains("forge: 1 of 2 files would be rewritten"), "{err}");
    assert_eq!(fs::read_to_string(&old).unwrap(), FERN, "nothing is written");
}

#[test]
fn check_succeeds_when_there_is_nothing_to_rewrite() {
    let dir = dir("check-clean");
    let file = dir.join("spec.json");
    fs::write(&file, NATIVE).unwrap();

    let out = normalize(&["--check", file.to_str().unwrap()], "");

    succeeded(&out);
    assert!(
        text(&out.stderr).contains("1 file checked, none would be rewritten"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn check_on_standard_input_writes_nothing_and_fails_when_there_is_something_to_rewrite() {
    let out = normalize(&["--check", "-"], FERN);

    let err = failed(&out);
    assert!(err.contains("the document would be rewritten"), "{err}");
    assert!(out.stdout.is_empty(), "nothing is written");
    assert!(normalize(&["--check", "-"], NATIVE).status.success());
}

#[test]
fn a_yaml_file_is_an_error_and_is_not_reported_as_native() {
    let dir = dir("yaml");
    for name in ["spec.yml", "spec.YAML"] {
        fs::write(dir.join(name), "openapi: 3.0.3\nx-fern-ignore: true\n").unwrap();
        let file = dir.join(name);

        let out = normalize(&[file.to_str().unwrap()], "");

        let err = failed(&out);
        assert!(err.contains("YAML is not supported yet"), "{name}: {err}");
        assert!(err.contains(name), "{name}: {err}");
        assert!(!err.contains("unchanged"), "{name}: {err}");
    }
}

#[test]
fn a_file_that_is_not_an_object_is_an_error_whatever_it_is_named() {
    let dir = dir("not-an-object");
    for (name, content) in [
        ("yaml.json", "openapi: 3.0.3\nx-fern-ignore: true\n"),
        ("array.json", "[]"),
        ("empty.json", ""),
        ("blank.json", "  \n"),
    ] {
        fs::write(dir.join(name), content).unwrap();
        let file = dir.join(name);

        let out = normalize(&[file.to_str().unwrap()], "");

        let err = failed(&out);
        assert!(err.contains(name), "{name}: {err}");
        assert!(!err.contains("unchanged"), "{name}: {err}");
        assert_eq!(fs::read_to_string(&file).unwrap(), content, "{name} is left as it is");
    }
}

#[test]
fn the_usage_says_forge_normalize_and_the_help_of_forge_lists_the_command() {
    let help = normalize(&["--help"], "");
    let general = forge(&["--help"], "");

    assert!(
        text(&help.stdout).contains("Usage: forge normalize"),
        "{}",
        text(&help.stdout)
    );
    assert!(text(&help.stdout).contains("--check"), "{}", text(&help.stdout));
    assert!(
        text(&general.stdout).contains("Commands:") && text(&general.stdout).contains("  normalize  "),
        "{}",
        text(&general.stdout)
    );
    assert!(
        text(&general.stdout).contains("forge <COMMAND>"),
        "{}",
        text(&general.stdout)
    );
    let missing = normalize(&[], "");
    assert!(failed(&missing).contains("Usage: forge normalize"));
}

#[cfg(unix)]
#[test]
fn a_link_stays_a_link_and_the_file_it_points_to_is_rewritten() {
    let dir = dir("symlink");
    fs::create_dir_all(dir.join("real")).unwrap();
    fs::create_dir_all(dir.join("links")).unwrap();
    fs::write(dir.join("real/spec.json"), FERN).unwrap();
    std::os::unix::fs::symlink("../real/spec.json", dir.join("links/spec.json")).unwrap();

    let out = normalize(&[dir.join("links/spec.json").to_str().unwrap()], "");

    succeeded(&out);
    assert!(
        fs::symlink_metadata(dir.join("links/spec.json"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link was replaced by a file"
    );
    assert_eq!(fs::read_to_string(dir.join("real/spec.json")).unwrap(), NATIVE);
    assert!(
        text(&out.stderr).contains("links/spec.json: rewrote 2 keys"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_file_given_more_than_once_is_rewritten_once() {
    let dir = dir("twice");
    let file = dir.join("spec.json");
    fs::write(&file, FERN).unwrap();
    let again = format!("{}/./spec.json", dir.display());

    for _ in 0..5 {
        fs::write(&file, FERN).unwrap();
        let out = normalize(&[file.to_str().unwrap(), &again], "");

        succeeded(&out);
        assert!(
            text(&out.stderr).contains("1 file: 1 rewritten"),
            "{}",
            text(&out.stderr)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), NATIVE);
    }
}

#[test]
fn a_folder_is_an_error_that_says_so() {
    let dir = dir("a-folder");
    fs::write(dir.join("a.json"), FERN).unwrap();

    let out = normalize(&[dir.to_str().unwrap()], "");

    let err = failed(&out);
    assert!(err.contains("is a folder, not a file"), "{err}");
    assert_eq!(
        fs::read_to_string(dir.join("a.json")).unwrap(),
        FERN,
        "nothing in it is rewritten"
    );
}

#[test]
fn a_pattern_is_for_the_shell_and_is_not_expanded() {
    let dir = dir("pattern");
    fs::write(dir.join("a.json"), FERN).unwrap();

    let out = normalize(&[&format!("{}/*.json", dir.display())], "");

    let err = failed(&out);
    assert!(err.contains("*.json: "), "{err}");
    assert_eq!(fs::read_to_string(dir.join("a.json")).unwrap(), FERN);
}

#[test]
fn a_file_whose_name_has_glob_characters_is_that_file() {
    let dir = dir("brackets");
    let file = dir.join("spec[1].json");
    fs::write(&file, FERN).unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    assert_eq!(fs::read_to_string(&file).unwrap(), NATIVE);
}

#[cfg(unix)]
#[test]
fn a_read_only_file_is_not_rewritten() {
    use std::os::unix::fs::PermissionsExt;

    let dir = dir("read-only");
    let file = dir.join("spec.json");
    fs::write(&file, FERN).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");
    let check = normalize(&["--check", file.to_str().unwrap()], "");

    let err = failed(&out);
    assert!(
        err.contains("spec.json: the file is read-only, so it is not rewritten"),
        "{err}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), FERN);
    assert!(
        text(&check.stderr).contains("would rewrite 2 keys"),
        "a check only reads: {}",
        text(&check.stderr)
    );
}

#[test]
fn a_byte_order_mark_is_named_and_the_file_is_left_as_it_is() {
    let dir = dir("bom");
    let file = dir.join("spec.json");
    let content = format!("\u{feff}{FERN}");
    fs::write(&file, &content).unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    let err = failed(&out);
    assert!(
        err.contains("spec.json: the document starts with a byte order mark"),
        "{err}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), content);
}

#[test]
fn a_key_written_twice_is_a_warning_that_says_where_and_the_file_is_rewritten() {
    let dir = dir("duplicate");
    let file = dir.join("spec.json");
    fs::write(
        &file,
        "{\"openapi\":\"3.0.3\",\"info\":{\"title\":\"t\",\"version\":\"1\"},\"paths\":{\"/a\":{\"get\":{\n  \"x-fern-sdk-group-name\":[\"a\"],\n  \"x-fern-sdk-method-name\":\"get\",\n  \"x-fern-sdk-method-name\":\"again\"}}}}",
    )
    .unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    let err = text(&out.stderr);
    assert!(
        err.contains(
            "spec.json: warning: /paths/~1a/get: x-fern-sdk-method-name is written twice in this object, and is left as it is"
        ),
        "{err}"
    );
    assert!(err.contains("1 file: 1 rewritten, 0 unchanged, 1 warning"), "{err}");
    let written = fs::read_to_string(&file).unwrap();
    assert_eq!(written.matches("\"x-fern-sdk-method-name\"").count(), 2, "{written}");
    assert!(written.contains("\"x-forge-group-name\""), "{written}");
}

#[test]
fn the_summary_counts_the_warnings() {
    let dir = dir("warnings");
    let file = dir.join("spec.json");
    fs::write(
        &file,
        r#"{"openapi":"3.0.3","info":{"title":"t","version":"1"},"paths":{"/a":{"get":{"x-fern-sdk-method-name":"old","x-forge-method-name":"new","x-forge-group-name":"a","responses":{"204":{"description":"ok"}}}}}}"#,
    )
    .unwrap();

    let out = normalize(&[file.to_str().unwrap()], "");

    succeeded(&out);
    assert!(
        text(&out.stderr).contains("1 file: 1 rewritten, 0 unchanged, 1 warning"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn standard_input_is_called_stdin_and_a_failure_on_it_says_so() {
    let out = normalize(&["-"], FERN);
    let broken = normalize(&["-"], "{\"x-fern-ignore\": ");

    assert!(
        text(&out.stderr).starts_with("stdin: rewrote 2 keys"),
        "{}",
        text(&out.stderr)
    );
    assert!(failed(&broken).contains("forge: stdin: "), "{}", text(&broken.stderr));
}

#[test]
fn v_says_how_many_group_paths_were_joined() {
    let dir = dir("joined");
    let file = dir.join("spec.json");
    fs::write(&file, FERN).unwrap();

    let out = normalize(&["-v", file.to_str().unwrap()], "");

    assert!(
        text(&out.stderr).contains("  1 group path written as a dotted string"),
        "{}",
        text(&out.stderr)
    );
}

#[cfg(unix)]
#[test]
fn a_hard_link_stays_a_link_to_the_file_that_is_rewritten() {
    let dir = dir("hard-link");
    let (a, b) = (dir.join("a.json"), dir.join("b.json"));
    fs::write(&a, FERN).unwrap();
    fs::hard_link(&a, &b).unwrap();

    succeeded(&normalize(&[a.to_str().unwrap()], ""));

    assert_eq!(fs::read_to_string(&a).unwrap(), NATIVE);
    assert_eq!(
        fs::read_to_string(&b).unwrap(),
        NATIVE,
        "the other name is the same file, which is rewritten"
    );
}

#[cfg(unix)]
#[test]
fn two_names_of_one_file_are_one_file_and_are_rewritten_once() {
    let dir = dir("hard-links-given");
    let (a, b) = (dir.join("a.json"), dir.join("b.json"));
    fs::write(&a, FERN).unwrap();
    fs::hard_link(&a, &b).unwrap();

    let out = normalize(&[a.to_str().unwrap(), b.to_str().unwrap()], "");

    succeeded(&out);
    assert!(
        text(&out.stderr).contains("1 file: 1 rewritten"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(fs::read_to_string(&b).unwrap(), NATIVE);
}

#[cfg(unix)]
#[test]
fn a_file_in_a_folder_that_cannot_be_written_to_is_rewritten_in_place() {
    use std::os::unix::fs::PermissionsExt;

    let dir = dir("read-only-folder");
    let file = dir.join("spec.json");
    fs::write(&file, FERN).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    // The folder cannot be written to, unless this is run as the owner of
    // everything, which it is not a test of.
    if fs::write(dir.join("probe"), "").is_ok() {
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let out = normalize(&[file.to_str().unwrap()], "");

    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    succeeded(&out);
    assert_eq!(fs::read_to_string(&file).unwrap(), NATIVE);
}

/// Every file below `dir`, by its path below it.
fn files(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut found = std::collections::BTreeMap::new();
    let mut folders = vec![dir.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.insert(path.strip_prefix(dir).unwrap().to_owned(), fs::read(&path).unwrap());
            }
        }
    }
    found
}

const OF_ANOTHER_TOOL: &str = r#"{"openapi": "3.0.3", "info": {"title": "t", "version": "1"}, "paths": {"/a": {"get": {
  "x-fern-sdk-group-name": ["things", "parts"], "x-fern-sdk-method-name": "list", "x-fern-availability": "beta",
  "responses": {"200": {"description": "ok", "content": {"application/json": {"schema": {"type": "object",
    "properties": {"id": {"type": "string"}, "secret": {"type": "string", "x-sensitive": true}}}}}}}}}}}"#;

const NATIVE_TWIN: &str = r#"{"openapi": "3.0.3", "info": {"title": "t", "version": "1"}, "paths": {"/a": {"get": {
  "x-forge-group-name": "things.parts", "x-forge-method-name": "list", "x-forge-availability": "beta",
  "responses": {"200": {"description": "ok", "content": {"application/json": {"schema": {"type": "object",
    "properties": {"id": {"type": "string"}, "secret": {"type": "string", "x-forge-sensitive": true}}}}}}}}}}}"#;

/// Until the release that stops it, `forge` rewrites the keys of other tools in
/// memory first, so a document that has not been through `forge normalize`
/// builds what its native twin does.
#[test]
fn forge_builds_a_document_of_another_tool_as_it_builds_its_native_twin() {
    let dir = dir("in-memory");
    let (other, native) = (dir.join("other.json"), dir.join("native.json"));
    fs::write(&other, OF_ANOTHER_TOOL).unwrap();
    fs::write(&native, NATIVE_TWIN).unwrap();

    for (spec, out) in [(&other, "sdk-other"), (&native, "sdk-native")] {
        let out = dir.join(out);
        succeeded(&forge(
            &[spec.to_str().unwrap(), "--target", "ts", "--out", out.to_str().unwrap()],
            "",
        ));
    }

    let (other, native) = (files(&dir.join("sdk-other")), files(&dir.join("sdk-native")));
    assert!(
        !other.is_empty() && other == native,
        "{:?} against {:?}",
        other.keys(),
        native.keys()
    );
    let client: Vec<&Vec<u8>> = other.values().collect();
    assert!(
        client
            .iter()
            .any(|text| text.windows(4).any(|window| window == b"list")),
        "the method is built"
    );
    // The file is read, and not changed: that is for `forge normalize`.
    assert_eq!(fs::read_to_string(dir.join("other.json")).unwrap(), OF_ANOTHER_TOOL);
}

#[test]
fn forge_rewrites_standard_input_in_memory_too() {
    let dir = dir("in-memory-stdin");
    let out = dir.join("sdk");

    let from_stdin = forge(
        &["-", "--target", "ts", "--out", out.to_str().unwrap()],
        OF_ANOTHER_TOOL,
    );

    succeeded(&from_stdin);
    let native = dir.join("sdk-native");
    succeeded(&forge(
        &["-", "--target", "ts", "--out", native.to_str().unwrap()],
        NATIVE_TWIN,
    ));
    let built = files(&out);
    assert!(!built.is_empty() && built == files(&native));
}

/// The documents and the errors are those of `forge` before it left reading
/// these keys to `forge normalize`: each message is what that `forge` said of
/// the document, word for word, down to the line and the column.
#[test]
fn forge_fails_on_a_key_written_twice_in_the_words_it_always_used() {
    let dir = dir("in-memory-error");
    let base = |keys: &str| {
        format!(
            r#"{{"openapi":"3.0.3","info":{{"title":"t","version":"1"}},"paths":{{"/a":{{"get":{{{keys},"responses":{{"204":{{"description":"ok"}}}}}}}}}}}}"#
        )
    };
    for (name, document, message) in [
        (
            "one-line",
            base(r#""x-fern-sdk-group-name":["a"],"x-fern-sdk-method-name":"get","x-fern-ignore":false,"x-fern-ignore":true"#),
            "duplicate field `x-fern-ignore` at line 1 column 174",
        ),
        (
            "several-lines",
            "{\n  \"openapi\": \"3.0.3\",\n  \"info\": {\"title\": \"t\", \"version\": \"1\"},\n  \"paths\": {\"/a\": {\"get\": {\n      \"x-fern-sdk-group-name\": [\"a\"],\n      \"x-fern-sdk-method-name\": \"get\",\n      \"x-fern-sdk-method-name\": \"again\",\n      \"responses\": {\"204\": {\"description\": \"ok\"}}}}}}\n".to_owned(),
            "duplicate field `x-fern-sdk-method-name` at line 7 column 30",
        ),
        (
            "three-copies",
            base(r#""x-fern-sdk-group-name":["a"],"x-fern-sdk-method-name":"a","x-fern-sdk-method-name":"b","x-fern-sdk-method-name":"c""#),
            "duplicate field `x-fern-sdk-method-name` at line 1 column 159",
        ),
        (
            "not-ascii-before-it",
            base(r#""description":"café 中","x-fern-sdk-group-name":["a"],"x-fern-sdk-method-name":"a","x-fern-sdk-method-name":"b""#),
            "duplicate field `x-fern-sdk-method-name` at line 1 column 185",
        ),
    ] {
        let spec = dir.join(format!("{name}.json"));
        fs::write(&spec, document).unwrap();
        let out = dir.join(format!("sdk-{name}"));

        let run = forge(&[spec.to_str().unwrap(), "--target", "ts", "--out", out.to_str().unwrap()], "");

        let err = failed(&run);
        assert_eq!(err.trim_end(), format!("forge: {}: {message}", spec.display()), "{name}");
        assert!(!out.exists(), "{name}: nothing was written");
    }
}

/// A value that cannot be read is for `forge` to say, as it reads it, in the
/// words it always used, and on the line and column it always gave.
#[test]
fn forge_says_where_a_value_cannot_be_read_as_it_always_did() {
    let dir = dir("in-memory-unreadable");
    let tail = r#""responses":{"204":{"description":"ok"}}}}}}"#;
    for (name, document, message) in [
        (
            "vendor-value",
            format!(
                "{{\"openapi\":\"3.0.3\",\"info\":{{\"title\":\"t\",\"version\":\"1\"}},\"paths\":{{\"/a\":{{\"get\":{{\n\"x-fern-sdk-group-name\":[\"a\"],\n\"x-fern-sdk-method-name\":\"\\ud800\",\n{tail}"
            ),
            "unexpected end of hex escape at line 3 column 33",
        ),
        (
            "native-value",
            format!(
                "{{\"openapi\":\"3.0.3\",\"info\":{{\"title\":\"t\",\"version\":\"1\"}},\"paths\":{{\"/a\":{{\"get\":{{\n\"x-forge-group-name\":\"a\",\n\"x-forge-method-name\":\"\\ud800\",\n{tail}"
            ),
            "unexpected end of hex escape at line 3 column 30",
        ),
        (
            "key",
            format!(
                "{{\"openapi\":\"3.0.3\",\"info\":{{\"title\":\"t\",\"version\":\"1\"}},\"paths\":{{\"/a\":{{\"get\":{{\n\"x-fern-sdk-group-name\":[\"a\"],\n\"x\\ud800\":1,\n\"x-fern-sdk-method-name\":\"get\",\n{tail}"
            ),
            "unexpected end of hex escape at line 3 column 9",
        ),
    ] {
        let spec = dir.join(format!("{name}.json"));
        fs::write(&spec, document).unwrap();
        let out = dir.join(format!("sdk-{name}"));

        let run = forge(
            &[spec.to_str().unwrap(), "--target", "ts", "--out", out.to_str().unwrap()],
            "",
        );

        assert_eq!(
            failed(&run).trim_end(),
            format!("forge: {}: {message}", spec.display()),
            "{name}"
        );
    }
}

#[test]
fn forge_says_nothing_of_a_document_that_it_could_rewrite() {
    let dir = dir("in-memory-quiet");
    let spec = dir.join("spec.json");
    fs::write(&spec, OF_ANOTHER_TOOL).unwrap();
    let out = dir.join("sdk");

    let run = forge(
        &[spec.to_str().unwrap(), "--target", "ts", "--out", out.to_str().unwrap()],
        "",
    );

    succeeded(&run);
    assert_eq!(text(&run.stderr), "", "forge says nothing of the document");
    assert!(!files(&out).is_empty());
}

#[test]
fn forge_names_the_file_when_the_document_is_not_json() {
    let dir = dir("in-memory-not-json");
    let spec = dir.join("spec.json");
    fs::write(&spec, "openapi: 3.0.3").unwrap();
    let out = dir.join("sdk");

    let run = forge(
        &[spec.to_str().unwrap(), "--target", "ts", "--out", out.to_str().unwrap()],
        "",
    );

    assert!(failed(&run).contains("spec.json: "), "{}", text(&run.stderr));
}

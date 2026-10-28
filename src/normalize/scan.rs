//! Reading a JSON document as bytes, and editing it where it is read.
//!
//! Nothing here knows what a document means: it passes over values, hands out
//! the members of an object with where they are, and writes the edits it is
//! given. That is all the walk of [`walk`](super::walk) needs of the format.
//! It does not check the document either, which is done beside it, and nothing
//! here panics or loops on one that is not JSON.

use std::borrow::Cow;
use std::fmt;
use std::io::{self, IoSlice, Write};

use serde::de::DeserializeOwned;

use super::Error;

/// The bytes that a value that is an object or an array has inside it that are
/// not just passed over: the quotes, which are strings, and the brackets.
static STRUCTURAL: [bool; 256] = {
    let mut table = [false; 256];
    table[b'"' as usize] = true;
    table[b'{' as usize] = true;
    table[b'[' as usize] = true;
    table[b'}' as usize] = true;
    table[b']' as usize] = true;
    table
};

/// Returns where the first `"` or `\` is in `rest`. Most strings are short, a
/// key or a name, and are looked through eight bytes at a time, which costs
/// less for them than a search for a byte does.
fn end_of_run(rest: &[u8]) -> Option<usize> {
    const LOW: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;
    let mut at = 0;
    while at < 32 {
        let Some(chunk) = rest.get(at..).and_then(|rest| rest.first_chunk::<8>()) else {
            break;
        };
        let word = u64::from_le_bytes(*chunk);
        let quotes = word ^ (LOW * u64::from(b'"'));
        let slashes = word ^ (LOW * u64::from(b'\\'));
        // A byte of a word that is zero has its high bit set in what is
        // left, and so can others above it, but not below it.
        let found = (quotes.wrapping_sub(LOW) & !quotes | slashes.wrapping_sub(LOW) & !slashes) & HIGH;
        if found != 0 {
            return Some(at + (found.trailing_zeros() / 8) as usize);
        }
        at += 8;
    }
    memchr::memchr2(b'"', b'\\', rest.get(at..).unwrap_or_default()).map(|found| at + found)
}

/// A member of an object, as the bytes it is made of.
#[derive(Clone, Copy)]
pub(super) struct Member {
    /// Where its leading whitespace starts, which is just after the `{` or
    /// the `,` before it.
    pub lead: usize,
    /// The opening quote of its key.
    pub key_start: usize,
    /// Just after the closing quote of its key.
    pub key_end: usize,
    /// Whether the key has an escape in it, so that its bytes are not its
    /// name.
    pub escaped: bool,
    pub value_start: usize,
    pub value_end: usize,
}

/// Where an object is up to, which [`Scan::next_member`] keeps.
pub(super) struct Open {
    lead: usize,
}

/// What is done to a member of an object.
pub(super) enum Action {
    /// The key is written as the native key, with its value, or with this one.
    Rename(&'static str, Option<String>),
    /// The member is removed, since a native key says what it says, or it
    /// says nothing.
    Remove,
    /// The native key is written before it, with its value, or with this one,
    /// and it stays.
    Insert(&'static str, Option<String>),
}

/// What is written in place of a part of the document.
enum Piece {
    /// A key, which is a name in quotes.
    Quoted(&'static str),
    Bytes(Vec<u8>),
    Nothing,
}

/// A part of the document, from `start` to `end`, that is written as `with`.
struct Replacement {
    start: usize,
    end: usize,
    with: Piece,
}

/// A position in a document, which values are read from, and the edits to it.
pub(super) struct Scan<'j> {
    json: &'j [u8],
    pos: usize,
    replacements: Vec<Replacement>,
}

impl<'j> Scan<'j> {
    pub(super) fn new(json: &'j [u8]) -> Scan<'j> {
        Scan {
            json,
            pos: 0,
            replacements: Vec::new(),
        }
    }

    pub(super) fn error(&self, what: &str) -> Error {
        self.error_at(self.pos, what)
    }

    /// Returns the line, from 1, and the column, which is how many bytes of the
    /// line there are before `at`, that a parser that has read up to `at` says
    /// it is at: `serde_json` says it so, and an error that names where is
    /// worded the same.
    pub(super) fn consumed(&self, at: usize) -> (usize, usize) {
        let before = &self.json[..at.min(self.json.len())];
        let line = memchr::memchr_iter(b'\n', before).count() + 1;
        let column = before.len() - memchr::memrchr(b'\n', before).map_or(0, |newline| newline + 1);
        (line, column)
    }

    /// Returns an error that says where in the document, as a line and a
    /// column that count from 1, `what` is.
    pub(super) fn error_at(&self, at: usize, what: &str) -> Error {
        let before = &self.json[..at.min(self.json.len())];
        let line = memchr::memchr_iter(b'\n', before).count() + 1;
        let column = before.len() - memchr::memrchr(b'\n', before).map_or(0, |newline| newline + 1) + 1;
        Error(format!("line {line}, column {column}: {what}"))
    }

    /// Returns the bytes from `start` to `end`, or none if they are not in the
    /// document.
    pub(super) fn raw(&self, start: usize, end: usize) -> &'j [u8] {
        debug_assert!(
            start <= end && end <= self.json.len(),
            "{start}..{end} is not in the document"
        );
        self.json.get(start..end).unwrap_or_default()
    }

    /// Passes over whitespace, which is mostly the spaces that a document is
    /// indented with, so they are passed over eight at a time.
    pub(super) fn skip_whitespace(&mut self) {
        let mut pos = self.pos;
        // There is mostly none, or one space, as there is after a colon.
        if let Some(&[first, second]) = self.json.get(pos..).and_then(|rest| rest.first_chunk::<2>()) {
            if first > b' ' {
                return;
            }
            if first == b' ' && second > b' ' {
                self.pos = pos + 1;
                return;
            }
        }
        loop {
            let Some(chunk) = self.json.get(pos..).and_then(|rest| rest.first_chunk::<8>()) else {
                while self
                    .json
                    .get(pos)
                    .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                {
                    pos += 1;
                }
                break;
            };
            let not_spaces = u64::from_le_bytes(*chunk) ^ 0x2020_2020_2020_2020;
            if not_spaces == 0 {
                pos += 8;
                continue;
            }
            pos += (not_spaces.trailing_zeros() / 8) as usize;
            if matches!(self.json[pos], b'\t' | b'\n' | b'\r') {
                pos += 1;
            } else {
                break;
            }
        }
        self.pos = pos;
    }

    fn peek(&self) -> Result<u8, Error> {
        self.json
            .get(self.pos)
            .copied()
            .ok_or_else(|| self.error("the document ends"))
    }

    /// Returns whether the value at the position is an object.
    pub(super) fn at_object(&self) -> bool {
        self.json.get(self.pos) == Some(&b'{')
    }

    /// Passes over the string at the position, and returns whether it has an
    /// escape in it.
    fn skip_string(&mut self) -> Result<bool, Error> {
        if self.peek()? != b'"' {
            return Err(self.error("a string is expected"));
        }
        self.pos += 1;
        let mut escaped = false;
        loop {
            let Some(at) = self.json.get(self.pos..).and_then(end_of_run) else {
                return Err(self.error("a string does not end"));
            };
            self.pos += at;
            if self.json[self.pos] == b'"' {
                self.pos += 1;
                return Ok(escaped);
            }
            escaped = true;
            self.pos += 2;
        }
    }

    /// Passes over the value at the position.
    pub(super) fn skip_value(&mut self) -> Result<(), Error> {
        match self.peek()? {
            b'"' => self.skip_string().map(|_| ()),
            b'{' | b'[' => {
                let mut depth = 0usize;
                loop {
                    while self.json.get(self.pos).is_some_and(|&b| !STRUCTURAL[b as usize]) {
                        self.pos += 1;
                    }
                    match self.peek()? {
                        b'"' => {
                            self.skip_string()?;
                        }
                        b'{' | b'[' => {
                            depth += 1;
                            self.pos += 1;
                        }
                        _ => {
                            depth -= 1;
                            self.pos += 1;
                            if depth == 0 {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            _ => {
                let start = self.pos;
                while self
                    .json
                    .get(self.pos)
                    .is_some_and(|b| !matches!(b, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'))
                {
                    self.pos += 1;
                }
                if self.pos == start {
                    return Err(self.error("a value is expected"));
                }
                Ok(())
            }
        }
    }

    /// Enters the object at the position, if it is one.
    pub(super) fn open_object(&mut self) -> Result<Option<Open>, Error> {
        if self.peek()? != b'{' {
            return Ok(None);
        }
        self.pos += 1;
        Ok(Some(Open { lead: self.pos }))
    }

    /// Returns the next member of the object, which stands at its value, or
    /// none at the end of the object. The value has to be passed over, by
    /// whatever reads it, before [`end_member`](Scan::end_member).
    pub(super) fn next_member(&mut self, open: &mut Open) -> Result<Option<Member>, Error> {
        self.skip_whitespace();
        if self.peek()? == b'}' {
            self.pos += 1;
            return Ok(None);
        }
        let key_start = self.pos;
        let escaped = self.skip_string()?;
        let key_end = self.pos;
        self.skip_whitespace();
        if self.peek()? != b':' {
            return Err(self.error("a key has no value"));
        }
        self.pos += 1;
        self.skip_whitespace();
        Ok(Some(Member {
            lead: open.lead,
            key_start,
            key_end,
            escaped,
            value_start: self.pos,
            value_end: self.pos,
        }))
    }

    /// Ends `member`, whose value has been passed over.
    pub(super) fn end_member(&mut self, open: &mut Open, member: &mut Member) {
        member.value_end = self.pos;
        self.skip_whitespace();
        if self.json.get(self.pos) == Some(&b',') {
            self.pos += 1;
            open.lead = self.pos;
        }
    }

    /// Enters the array at the position, if it is one.
    pub(super) fn open_array(&mut self) -> Result<bool, Error> {
        if self.peek()? != b'[' {
            return Ok(false);
        }
        self.pos += 1;
        Ok(true)
    }

    /// Returns whether the array has another element, which the position
    /// stands at, or else is past the end of the array.
    pub(super) fn next_element(&mut self) -> Result<bool, Error> {
        self.skip_whitespace();
        if self.peek()? == b']' {
            self.pos += 1;
            return Ok(false);
        }
        Ok(true)
    }

    /// Ends an element, which has been passed over.
    pub(super) fn end_element(&mut self) {
        self.skip_whitespace();
        if self.json.get(self.pos) == Some(&b',') {
            self.pos += 1;
        }
    }

    /// Returns the bytes that `member` is called, which are the bytes of its
    /// key as they are, unless it has an escape in it.
    /// A key that cannot be read, an escape that is a lone surrogate, is called
    /// by what it is written as, which is no name there is a use for.
    pub(super) fn name(&self, member: &Member) -> Cow<'j, [u8]> {
        if member.escaped
            && let Ok(key) = self.key(member)
        {
            return Cow::Owned(key.into_owned().into_bytes());
        }
        Cow::Borrowed(self.raw_key(member))
    }

    /// Returns the key of `member` between its quotes.
    pub(super) fn raw_key(&self, member: &Member) -> &'j [u8] {
        self.raw(member.key_start + 1, member.key_end.saturating_sub(1))
    }

    /// Returns the key of `member`, with its escapes read.
    pub(super) fn key(&self, member: &Member) -> Result<Cow<'j, str>, Error> {
        let name = self.raw(member.key_start + 1, member.key_end.saturating_sub(1));
        if memchr::memchr(b'\\', name).is_none() {
            return std::str::from_utf8(name)
                .map(Cow::Borrowed)
                .map_err(|_| self.error_at(member.key_start, "a key is not UTF-8"));
        }
        self.decode(member.key_start, member.key_end).map(Cow::Owned)
    }

    /// Returns whether the key of `member` is `name`.
    pub(super) fn key_is(&self, member: &Member, name: &str) -> bool {
        if member.escaped {
            self.key(member).is_ok_and(|key| key == name)
        } else {
            self.raw_key(member) == name.as_bytes()
        }
    }

    /// Reads the JSON from `start` to `end` as a `T`.
    pub(super) fn decode<T: DeserializeOwned>(&self, start: usize, end: usize) -> Result<T, Error> {
        serde_json::from_slice(self.raw(start, end)).map_err(|err| self.error_at(start, &err.to_string()))
    }

    /// Writes down what to do to the members of an object, each as its place
    /// among them and the action for it.
    pub(super) fn edit(&mut self, members: &[Member], actions: Vec<(usize, Action)>) {
        let mut removed = vec![false; members.len()];
        for (at, action) in actions {
            let member = &members[at];
            match action {
                Action::Rename(native, value) => {
                    self.replace(member.key_start, member.key_end, Piece::Quoted(native));
                    if let Some(value) = value {
                        self.replace(member.value_start, member.value_end, Piece::Bytes(value.into_bytes()));
                    }
                }
                Action::Insert(native, value) => {
                    // The native key goes first, so that the old one is left
                    // as it is, to the comma after it: a diff of the document
                    // is only the lines that are added. It has the whitespace
                    // the old one has, before it and between it and the value.
                    let mut bytes = format!("\"{native}\"").into_bytes();
                    bytes.extend_from_slice(self.raw(member.key_end, member.value_start));
                    match value {
                        Some(value) => bytes.extend_from_slice(value.as_bytes()),
                        None => bytes.extend_from_slice(self.raw(member.value_start, member.value_end)),
                    }
                    bytes.push(b',');
                    bytes.extend_from_slice(self.raw(member.lead, member.key_start));
                    self.replace(member.key_start, member.key_start, Piece::Bytes(bytes));
                }
                Action::Remove => removed[at] = true,
            }
        }

        // Removed members go with their commas, so that one is left between
        // each two that are kept, and each that is kept keeps its whitespace.
        match removed.iter().position(|removed| !removed) {
            None if members.is_empty() => {}
            None => self.replace(members[0].lead, members[members.len() - 1].value_end, Piece::Nothing),
            Some(first) => {
                if first > 0 {
                    self.replace(members[0].lead, members[first].lead, Piece::Nothing);
                }
                for (i, gone) in removed.iter().enumerate().skip(first + 1) {
                    if *gone {
                        self.replace(members[i - 1].value_end, members[i].value_end, Piece::Nothing);
                    }
                }
            }
        }
    }

    fn replace(&mut self, start: usize, end: usize, with: Piece) {
        self.replacements.push(Replacement { start, end, with });
    }

    /// Returns the document with the edits to be made, or none if there are
    /// none.
    pub(super) fn finish(mut self) -> Result<Option<Edited<'j>>, Error> {
        if self.replacements.is_empty() {
            return Ok(None);
        }
        self.replacements.sort_unstable_by_key(|r| (r.start, r.end));
        let mut parts = Vec::with_capacity(self.replacements.len() * 2 + 1);
        let mut written = Vec::new();
        let mut at = 0;
        for r in &self.replacements {
            if r.start < at {
                return Err(Error("two edits overlap".to_owned()));
            }
            if r.start > at {
                parts.push(Part::Document(at, r.start));
            }
            let from = written.len();
            match &r.with {
                Piece::Quoted(name) => {
                    written.push(b'"');
                    written.extend_from_slice(name.as_bytes());
                    written.push(b'"');
                }
                Piece::Bytes(bytes) => written.extend_from_slice(bytes),
                Piece::Nothing => {}
            }
            if written.len() > from {
                parts.push(Part::Written(from, written.len()));
            }
            at = r.end;
        }
        if at < self.json.len() {
            parts.push(Part::Document(at, self.json.len()));
        }
        Ok(Some(Edited {
            json: self.json,
            written,
            parts,
        }))
    }
}

/// A part of an edited document, as where it is.
enum Part {
    /// Bytes of the document that was read, from here to there.
    Document(usize, usize),
    /// Bytes that were written in place of others, from here to there.
    Written(usize, usize),
}

/// How many parts are handed to one vectored write, which is as many as
/// the system takes at least.
const PARTS_AT_ONCE: usize = 512;

/// A document with its edits made, which are made as it is written out: the
/// parts of the document that are left as they are are not copied to be put
/// together, and are written from where they were read.
pub struct Edited<'j> {
    json: &'j [u8],
    written: Vec<u8>,
    parts: Vec<Part>,
}

impl fmt::Debug for Edited<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edited")
            .field("parts", &self.parts.len())
            .finish_non_exhaustive()
    }
}

impl Edited<'_> {
    /// Writes the document to `out`.
    pub fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let mut batch: Vec<IoSlice<'_>> = Vec::with_capacity(PARTS_AT_ONCE);
        for part in &self.parts {
            let bytes = match *part {
                Part::Document(start, end) => &self.json[start..end],
                Part::Written(start, end) => &self.written[start..end],
            };
            batch.push(IoSlice::new(bytes));
            if batch.len() == PARTS_AT_ONCE {
                write_all(out, &mut batch)?;
            }
        }
        write_all(out, &mut batch)
    }

    /// Returns the document as bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.json.len() + self.json.len() / 16);
        self.write_to(&mut bytes).expect("writing to the memory cannot fail");
        bytes
    }
}

/// Writes all of `batch`, and empties it.
fn write_all<W: Write>(out: &mut W, batch: &mut Vec<IoSlice<'_>>) -> io::Result<()> {
    let mut left = &mut batch[..];
    while !left.is_empty() {
        match out.write_vectored(left) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => IoSlice::advance_slices(&mut left, written),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    batch.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the first `"` or `\` is, read a byte at a time.
    fn naive_end_of_run(bytes: &[u8]) -> Option<usize> {
        bytes.iter().position(|&b| b == b'"' || b == b'\\')
    }

    #[test]
    fn a_run_ends_where_a_quote_or_a_backslash_is_at_any_offset() {
        for filler in [b'a', b' ', 0x7F, 0x80, 0xC3, 0xFF, 0x01, b'#'] {
            for len in 0..80 {
                for end in *b"\"\\" {
                    let none = vec![filler; len];
                    assert_eq!(end_of_run(&none), None, "{filler:#x} x {len}");
                    for at in 0..len {
                        let mut bytes = vec![filler; len];
                        bytes[at] = end;
                        assert_eq!(end_of_run(&bytes), Some(at), "{filler:#x} x {len}, {end:#x} at {at}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_first_of_a_quote_and_a_backslash_ends_a_run_even_beside_bytes_that_borrow() {
        // 0x01 and 0x00 below a quote are where a subtraction borrows from it.
        for prefix in [&b"\x01\x01"[..], b"\x00\x01", b"\x22\\", b"\\\x22", b"a\x23\x21"] {
            for pad in 0..40 {
                let mut bytes = vec![b'z'; pad];
                bytes.extend_from_slice(prefix);
                bytes.extend_from_slice(b"\"tail\\");
                assert_eq!(end_of_run(&bytes), naive_end_of_run(&bytes), "{prefix:?} after {pad}");
            }
        }
    }

    /// Passes over whitespace a byte at a time.
    fn naive_skip_whitespace(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
            .unwrap_or(bytes.len())
    }

    #[test]
    fn whitespace_is_passed_over_as_it_is_a_byte_at_a_time() {
        let alphabet = [b' ', b'\n', b'\t', b'\r', b'x', 0x0C];
        let mut documents: Vec<Vec<u8>> = vec![Vec::new()];
        // Every document of up to six bytes from the alphabet.
        for len in 1..=6 {
            let mut longer = Vec::new();
            for document in documents.iter().filter(|document| document.len() == len - 1) {
                for &byte in &alphabet {
                    let mut document = document.clone();
                    document.push(byte);
                    longer.push(document);
                }
            }
            documents.extend(longer);
        }
        // Long runs of spaces, which are eight at a time, that end in each of the ways.
        for spaces in 0..70 {
            for end in [&b""[..], b"x", b"\n", b"\n    x", b"\r\n\t x", b"\x0C"] {
                let mut document = vec![b' '; spaces];
                document.extend_from_slice(end);
                documents.push(document);
            }
        }
        for document in documents {
            let mut scan = Scan::new(&document);
            scan.skip_whitespace();
            assert_eq!(scan.pos, naive_skip_whitespace(&document), "{document:?}");
        }
    }

    /// Returns the object `json` with the members called `keys` removed.
    fn without(json: &str, keys: &[&str]) -> String {
        let mut scan = Scan::new(json.as_bytes());
        let mut open = scan.open_object().unwrap().unwrap();
        let mut members = Vec::new();
        while let Some(mut member) = scan.next_member(&mut open).unwrap() {
            scan.skip_value().unwrap();
            scan.end_member(&mut open, &mut member);
            members.push(member);
        }
        let removals = keys
            .iter()
            .map(|key| {
                (
                    members.iter().position(|member| scan.key_is(member, key)).unwrap(),
                    Action::Remove,
                )
            })
            .collect();
        scan.edit(&members, removals);
        let edited = scan.finish().unwrap().expect("something was removed");
        String::from_utf8(edited.to_vec()).unwrap()
    }

    #[test]
    fn an_object_whose_members_are_all_removed_is_left_empty_as_it_was_written() {
        // What was between the last member and the end of the object is left.
        assert_eq!(without(r#"{"a": 1, "b": [2]}"#, &["a", "b"]), "{}");
        assert_eq!(without("{ \"a\": 1,\n  \"b\": [2] }", &["a", "b"]), "{ }");
        assert_eq!(without(r#"{"a": 1}"#, &["a"]), "{}");
    }

    #[test]
    fn the_members_that_are_kept_keep_one_comma_between_them_wherever_the_others_were() {
        let json = r#"{"a": 1, "b": 2, "c": 3, "d": 4}"#;

        // A member that is kept has the whitespace it was written with.
        assert_eq!(without(json, &["a"]), r#"{ "b": 2, "c": 3, "d": 4}"#);
        assert_eq!(without(json, &["d"]), r#"{"a": 1, "b": 2, "c": 3}"#);
        assert_eq!(without(json, &["b", "c"]), r#"{"a": 1, "d": 4}"#);
        assert_eq!(without(json, &["a", "b", "c"]), r#"{ "d": 4}"#);
        assert_eq!(without(json, &["b", "c", "d"]), r#"{"a": 1}"#);
        assert_eq!(without(json, &["a", "c"]), r#"{ "b": 2, "d": 4}"#);
    }
}

//! What a document means, which is where a key is to be rewritten.
//!
//! The walk follows the document as `forge` reads it: down from `paths` and
//! `components` to the operations, the parameters and the schemas, which are
//! the objects that have keys to rewrite, and no further. Everything else is
//! passed over. It decides what to do to each key as the object it is in
//! ends, and writes it down as an edit, so a document is read once.

use serde_json::Value;

use super::scan::{Action, Edited, Member, Scan};
use super::vendor::{self, Key};
use super::{Conflict, Error, Left, Options, Report, UNREADABLE, WRITTEN_TWICE};
use crate::metadata::{self, DefinitionField, ParameterField, SchemaField};

/// How deep schemas may be nested, where a document is not to be followed any
/// further. The check of a document puts no limit on how deep it is, and a
/// document could be nested as deep as it is long.
const DEPTH: usize = 256;

/// A kind of object with extensions.
trait Level {
    type Field: Copy;

    /// Returns what `key` says on the object, if another tool wrote it.
    fn read(key: &str) -> Option<Key<Self::Field>>;

    /// Returns the native key that says what `field` says.
    fn native(field: Self::Field) -> &'static str;

    /// Returns the JSON that `value`, written as `field` says, is written as
    /// when it is written as the native key says, if that is not as it is.
    fn written(_field: Self::Field, _value: &Value) -> Option<String> {
        None
    }

    /// Returns whether `a` and `b`, written as `field` says, say the same.
    fn same(_field: Self::Field, a: &Value, b: &Value) -> bool {
        a == b
    }
}

struct Definition;
struct Parameter;
struct Schema;

impl Level for Definition {
    type Field = DefinitionField;

    fn read(key: &str) -> Option<Key<DefinitionField>> {
        vendor::definition(key)
    }

    fn native(field: DefinitionField) -> &'static str {
        metadata::definition::key(field)
    }

    /// A group path is written as the names joined by dots, as the native key
    /// is written in Cloudflare's documents. One that a dotted string cannot
    /// say, because a name has a dot in it, say, stays an array.
    fn written(field: DefinitionField, value: &Value) -> Option<String> {
        let (DefinitionField::Group, Value::Array(names)) = (field, value) else {
            return None;
        };
        let path = dotted(names)?;
        Some(serde_json::to_string(&path).expect("a string is written as JSON"))
    }

    fn same(field: DefinitionField, a: &Value, b: &Value) -> bool {
        match field {
            DefinitionField::Group => group_path(a).is_some_and(|a| group_path(b) == Some(a)) || a == b,
            _ => a == b,
        }
    }
}

/// Returns the names of an array joined by dots, if each is a name that a
/// dotted string reads back as it is.
fn dotted(names: &[Value]) -> Option<String> {
    let mut path = String::new();
    for (i, name) in names.iter().enumerate() {
        let name = name.as_str().filter(|name| !name.is_empty() && !name.contains('.'))?;
        if i > 0 {
            path.push('.');
        }
        path.push_str(name);
    }
    Some(path)
}

/// Returns the names a group path says: a string is read as names separated
/// by dots, and the elements of an array are the names, which are not split.
fn group_path(value: &Value) -> Option<Vec<&str>> {
    match value {
        Value::String(path) if path.is_empty() => Some(Vec::new()),
        Value::String(path) => Some(path.split('.').collect()),
        Value::Array(names) => names.iter().map(Value::as_str).collect(),
        _ => None,
    }
}

impl Level for Parameter {
    type Field = ParameterField;

    fn read(key: &str) -> Option<Key<ParameterField>> {
        vendor::parameter(key)
    }

    fn native(field: ParameterField) -> &'static str {
        metadata::parameter::key(field)
    }
}

impl Level for Schema {
    type Field = SchemaField;

    fn read(key: &str) -> Option<Key<SchemaField>> {
        vendor::schema(key)
    }

    fn native(field: SchemaField) -> &'static str {
        metadata::schema::key(field)
    }
}

/// A step on the way to an object, kept as where it is in the document: it is
/// only written out for a conflict.
enum Step {
    /// A key, as its quotes and what is between them.
    Key(usize, usize),
    Index(usize),
}

/// Walks a document, and returns the document with the edits made, if there
/// are any, and what was done.
pub(super) fn walk<'j>(json: &'j [u8], options: &Options) -> Result<(Option<Edited<'j>>, Report), Error> {
    let mut walk = Walk {
        scan: Scan::new(json),
        options,
        report: Report::default(),
        members: Vec::new(),
        path: Vec::new(),
        depth: 0,
    };
    walk.scan.skip_whitespace();
    walk.document()?;
    let Walk { scan, report, .. } = walk;
    Ok((scan.finish()?, report))
}

/// Notes that the key `key` of the object being read was left as it is
/// written, for `why`. A macro and not a method, so that what is borrowed of the
/// walk to decide it can be borrowed still.
macro_rules! leave {
    ($walk:ident, $key:expr, $why:expr, $at:expr) => {{
        let location = $walk.pointer();
        let (line, column) = $walk.scan.consumed($at);
        $walk.report.left.push(Left {
            location,
            key: $key.to_string(),
            why: $why,
            line,
            column,
        });
    }};
}

struct Walk<'j, 'o> {
    scan: Scan<'j>,
    options: &'o Options,
    report: Report,
    /// The members of the objects being read that have keys to rewrite, the
    /// innermost last, which is all of one object after its own members and
    /// none of those of the objects in it.
    members: Vec<Member>,
    path: Vec<Step>,
    depth: usize,
}

impl Walk<'_, '_> {
    fn document(&mut self) -> Result<(), Error> {
        let Some(mut open) = self.scan.open_object()? else {
            return self.scan.skip_value();
        };
        while let Some(mut member) = self.scan.next_member(&mut open)? {
            let key = self.scan.name(&member);
            self.path.push(Step::Key(member.key_start, member.key_end));
            match &*key {
                b"paths" => self.each_value(Self::path_item)?,
                b"components" => self.components()?,
                _ => self.scan.skip_value()?,
            }
            self.path.pop();
            self.scan.end_member(&mut open, &mut member);
        }
        Ok(())
    }

    /// Reads each member of the object at the position with `child`, which
    /// has to pass over the value, and skips a value that is not an object.
    fn each_value(&mut self, child: fn(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        let Some(mut open) = self.scan.open_object()? else {
            return self.scan.skip_value();
        };
        while let Some(mut member) = self.scan.next_member(&mut open)? {
            self.path.push(Step::Key(member.key_start, member.key_end));
            child(self)?;
            self.path.pop();
            self.scan.end_member(&mut open, &mut member);
        }
        Ok(())
    }

    /// Reads each element of the array at the position with `child`, and skips
    /// a value that is not an array.
    fn each_element(&mut self, child: fn(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        if !self.scan.open_array()? {
            return self.scan.skip_value();
        }
        let mut index = 0;
        while self.scan.next_element()? {
            self.path.push(Step::Index(index));
            child(self)?;
            self.path.pop();
            self.scan.end_element();
            index += 1;
        }
        Ok(())
    }

    /// Reads the member of the object at the position that is called `name`
    /// with `child`, and passes over the others.
    fn only(&mut self, name: &str, child: fn(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        let Some(mut open) = self.scan.open_object()? else {
            return self.scan.skip_value();
        };
        while let Some(mut member) = self.scan.next_member(&mut open)? {
            if self.scan.key_is(&member, name) {
                self.path.push(Step::Key(member.key_start, member.key_end));
                child(self)?;
                self.path.pop();
            } else {
                self.scan.skip_value()?;
            }
            self.scan.end_member(&mut open, &mut member);
        }
        Ok(())
    }

    fn components(&mut self) -> Result<(), Error> {
        let Some(mut open) = self.scan.open_object()? else {
            return self.scan.skip_value();
        };
        while let Some(mut member) = self.scan.next_member(&mut open)? {
            let key = self.scan.name(&member);
            self.path.push(Step::Key(member.key_start, member.key_end));
            match &*key {
                b"schemas" => self.each_value(Self::schema)?,
                b"parameters" => self.each_value(Self::parameter)?,
                b"requestBodies" | b"responses" => self.each_value(Self::content_of)?,
                b"pathItems" => self.each_value(Self::path_item)?,
                _ => self.scan.skip_value()?,
            }
            self.path.pop();
            self.scan.end_member(&mut open, &mut member);
        }
        Ok(())
    }

    fn path_item(&mut self) -> Result<(), Error> {
        let Some(mut open) = self.scan.open_object()? else {
            return self.scan.skip_value();
        };
        while let Some(mut member) = self.scan.next_member(&mut open)? {
            let key = self.scan.name(&member);
            self.path.push(Step::Key(member.key_start, member.key_end));
            match &*key {
                b"parameters" => self.each_element(Self::parameter)?,
                b"get" | b"put" | b"post" | b"delete" | b"options" | b"head" | b"patch" | b"trace" | b"query"
                | b"connect" => {
                    self.operation()?;
                }
                b"additionalOperations" => self.each_value(Self::operation)?,
                _ => self.scan.skip_value()?,
            }
            self.path.pop();
            self.scan.end_member(&mut open, &mut member);
        }
        Ok(())
    }

    fn operation(&mut self) -> Result<(), Error> {
        self.object::<Definition>(Self::operation_children)
    }

    /// Reads the member `key` of an operation that has objects to read in
    /// it, and returns whether it did.
    fn operation_children(&mut self, key: &[u8]) -> Result<bool, Error> {
        match key {
            b"parameters" => self.each_element(Self::parameter)?,
            b"requestBody" => self.content_of()?,
            b"responses" => self.each_value(Self::content_of)?,
            b"x-forge-aliases" => self.each_element(Self::alias)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn alias(&mut self) -> Result<(), Error> {
        self.object::<Definition>(|_, _| Ok(false))
    }

    fn parameter(&mut self) -> Result<(), Error> {
        self.object::<Parameter>(Self::parameter_children)
    }

    fn parameter_children(&mut self, key: &[u8]) -> Result<bool, Error> {
        match key {
            b"schema" => self.schema()?,
            b"content" => self.media_types()?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Reads a request body or a response, which have nothing but a `content`
    /// to read.
    fn content_of(&mut self) -> Result<(), Error> {
        self.only("content", Self::media_types)
    }

    fn media_types(&mut self) -> Result<(), Error> {
        self.each_value(Self::media)
    }

    fn media(&mut self) -> Result<(), Error> {
        self.only("schema", Self::schema)
    }

    fn schema(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > DEPTH {
            return Err(self.scan.error("the schemas are nested too deeply"));
        }
        let done = self.object::<Schema>(Self::schema_children);
        self.depth -= 1;
        done
    }

    fn schema_children(&mut self, key: &[u8]) -> Result<bool, Error> {
        match key {
            b"properties" | b"$defs" => self.each_value(Self::schema)?,
            b"additionalProperties" | b"unevaluatedProperties" | b"not" => self.schema()?,
            b"items" if self.scan.at_object() => self.schema()?,
            b"items" | b"prefixItems" | b"allOf" | b"anyOf" | b"oneOf" => self.each_element(Self::schema)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Reads the object at the position, which is of the kind `L`: each member
    /// with `children`, which says whether it passed over its value, and then
    /// the keys of the object that are to be rewritten.
    fn object<L: Level>(&mut self, children: fn(&mut Self, &[u8]) -> Result<bool, Error>) -> Result<(), Error> {
        let Some(mut open) = self.scan.open_object()? else {
            return self.scan.skip_value();
        };
        let base = self.members.len();
        // Most objects have no key that begins with `x-`, and are done with
        // when they end.
        let mut extension = false;
        while let Some(mut member) = self.scan.next_member(&mut open)? {
            extension |= member.escaped || self.scan.raw_key(&member).starts_with(b"x-");
            let name = self.scan.name(&member);
            self.path.push(Step::Key(member.key_start, member.key_end));
            if !children(self, &name)? {
                self.scan.skip_value()?;
            }
            self.path.pop();
            self.scan.end_member(&mut open, &mut member);
            self.members.push(member);
        }
        if extension {
            self.rewrite::<L>(base);
        }
        self.members.truncate(base);
        Ok(())
    }

    /// Decides what to do to the keys of the object whose members start at
    /// `base`, which is of the kind `L`, that a native key replaces.
    fn rewrite<L: Level>(&mut self, base: usize) {
        let mut actions: Vec<(usize, Action)> = Vec::new();
        for at in 0..self.members.len() - base {
            let member = self.members[base + at];
            // Most keys are not extensions, which is told without reading
            // them, unless they are written with an escape.
            if !member.escaped && !self.scan.raw_key(&member).starts_with(b"x-") {
                continue;
            }
            // A key that cannot be read is no key there is a use for.
            let Ok(key) = self.scan.key(&member) else {
                continue;
            };
            if !key.starts_with("x-") {
                continue;
            }
            let Some(hit) = L::read(&key) else {
                continue;
            };
            let native = L::native(hit.field);
            let members = &self.members[base..];
            // A key written twice has no one thing to say: each is left, and
            // said once, at the first.
            if let Some(other) = members
                .iter()
                .position(|other| other.key_start != member.key_start && self.scan.key_is(other, &key))
            {
                if member.key_start < members[other].key_start {
                    // As a parser that read it would say: after the second.
                    let after_the_second = members[other].key_end;
                    leave!(self, &key, WRITTEN_TWICE, after_the_second);
                }
                continue;
            }

            let Ok(value) = self.scan.decode::<Value>(member.value_start, member.value_end) else {
                leave!(self, &key, UNREADABLE, member.value_start);
                continue;
            };
            // A list of targets is something no native key says: it is left
            // as it is written, and not for a rewrite to say some other way
            // or to drop.
            if hit.targets && value.is_array() {
                continue;
            }
            // The native key that is beside is the one that decides, so it has
            // to be one, and to have a value to read. If it is not, what
            // `forge` makes of it is for `forge` to say.
            let mut named = members.iter().filter(|other| self.scan.key_is(other, native));
            let beside: Option<Value> = match named.next() {
                Some(other) => {
                    if let Some(second) = named.next() {
                        let after_the_second = second.key_end;
                        leave!(self, native, WRITTEN_TWICE, after_the_second);
                        continue;
                    }
                    match self.scan.decode(other.value_start, other.value_end) {
                        Ok(beside) => Some(beside),
                        Err(_) => {
                            leave!(self, native, UNREADABLE, other.value_start);
                            continue;
                        }
                    }
                }
                None => None,
            };
            if beside
                .as_ref()
                .is_some_and(|beside| !L::same(hit.field, &value, beside))
            {
                let location = self.pointer();
                self.report.conflicts.push(Conflict {
                    location,
                    key: key.to_string(),
                    native,
                });
            }

            let writes = beside.is_none();
            let text = L::written(hit.field, &value).filter(|_| writes);
            if text.is_some() {
                self.report.joined += 1;
            }
            let action = match (self.options.leave_old_extensions, writes) {
                (false, true) => Action::Rename(native, text),
                (false, false) => Action::Remove,
                (true, true) => Action::Insert(native, text),
                (true, false) => continue,
            };
            match self.report.migrated.get_mut(&*key) {
                Some(count) => *count += 1,
                None => {
                    self.report.migrated.insert(key.to_string(), 1);
                }
            }
            actions.push((at, action));
        }
        if !actions.is_empty() {
            self.scan.edit(&self.members[base..], actions);
        }
    }

    /// Returns where the object being read is, as a JSON Pointer.
    fn pointer(&self) -> String {
        let mut out = String::new();
        for step in &self.path {
            out.push('/');
            match *step {
                Step::Key(start, end) => {
                    let key: String = serde_json::from_slice(self.scan.raw(start, end)).unwrap_or_default();
                    out.push_str(&key.replace('~', "~0").replace('/', "~1"));
                }
                Step::Index(index) => out.push_str(&index.to_string()),
            }
        }
        out
    }
}

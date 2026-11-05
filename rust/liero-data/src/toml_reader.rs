//! Port of `gvl::toml::reader` (src/gvl/serialization/toml.hpp), the small
//! TOML dialect TC folders are written in. It accepts and rejects the same
//! files as the C++ reader, quirks included:
//!
//! - Every line ends with `\r` or `\n`, but the file must not end with one: a
//!   line break at the very end starts a line with no `=`, which is an error.
//! - There are no comments, and keys are made of `A-Z a-z 0-9 _`.
//! - When a key appears twice, the first value is kept.
//! - `\u00XX` escapes are read as the byte `XX` (the first two digits are
//!   skipped); only `\\`, `\/` and `\"` are other escapes.
//! - Integers wrap around instead of overflowing.
//! - An array of tables, `[[a]]`, adds a table each time it appears, and
//!   `[a.b]` after it goes into the last one.
//!
//! Values are read back with the same calls the C++ archive functions make:
//! `obj`, `arr`, `i32`, `b`, `str`, `reference`. A missing key is an error.

use crate::Error;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

type Object = Rc<RefCell<BTreeMap<Vec<u8>, Value>>>;
type Array = Rc<RefCell<Vec<Value>>>;

#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i32),
    Str(Vec<u8>),
    Obj(Object),
    Arr(Array),
}

fn new_object() -> Value {
    Value::Obj(Rc::new(RefCell::new(BTreeMap::new())))
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    /// The current character, 0 at the end like `get_def`.
    c: u8,
    name: &'a str,
}

impl<'a> Parser<'a> {
    fn error(&self) -> Error {
        Error(format!("Could not parse {} near byte {}", self.name, self.pos))
    }

    fn get(&mut self) -> u8 {
        match self.data.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                b
            }
            None => 0,
        }
    }

    fn next(&mut self) {
        self.c = self.get();
    }

    fn at_end(&self) -> bool {
        self.c == 0 && self.pos >= self.data.len()
    }

    fn skipws(&mut self) {
        while self.c == b'\t' || self.c == b' ' {
            self.next();
        }
    }

    fn skipallws(&mut self) {
        while matches!(self.c, b' ' | b'\r' | b'\n' | b'\t') {
            self.next();
        }
    }

    fn check(&mut self, e: u8) -> Result<(), Error> {
        if self.c != e {
            return Err(self.error());
        }
        self.next();
        Ok(())
    }

    fn test(&mut self, e: u8) -> bool {
        if self.c != e {
            return false;
        }
        self.next();
        true
    }

    fn dotted(&mut self) -> Vec<Vec<u8>> {
        let mut ret = vec![Vec::new()];
        loop {
            if self.c.is_ascii_alphanumeric() || self.c == b'_' {
                ret.last_mut().unwrap().push(self.c);
                self.next();
            } else if self.c == b'.' {
                ret.push(Vec::new());
                self.next();
            } else {
                break;
            }
        }
        ret
    }

    fn val_str(&mut self) -> Result<Vec<u8>, Error> {
        let mut s = Vec::new();
        self.check(b'"')?;
        while self.c != b'"' {
            // The C++ reader loops forever on an unterminated string.
            if self.at_end() {
                return Err(self.error());
            }
            if self.c == b'\\' {
                self.next();
                if matches!(self.c, b'\\' | b'/' | b'"') {
                    s.push(self.c);
                    self.next();
                } else if self.c == b'u' {
                    self.get();
                    self.get();
                    let c2 = self.get();
                    let c3 = self.get();
                    s.push((from_hex(c2) << 4).wrapping_add(from_hex(c3)));
                    self.next();
                } else {
                    return Err(self.error());
                }
            } else {
                s.push(self.c);
                self.next();
            }
        }
        self.check(b'"')?;
        Ok(s)
    }

    fn val(&mut self) -> Result<Value, Error> {
        match self.c {
            b'[' => {
                let mut a = Vec::new();
                self.next();
                self.skipallws();
                while self.c != b']' {
                    a.push(self.val()?);
                    self.skipallws();
                    if self.c == 0 || self.c == b']' {
                        break;
                    }
                    self.check(b',')?;
                    self.skipallws();
                }
                self.check(b']')?;
                Ok(Value::Arr(Rc::new(RefCell::new(a))))
            }
            b't' => {
                self.next();
                for &e in b"rue" {
                    self.check(e)?;
                }
                Ok(Value::Bool(true))
            }
            b'f' => {
                self.next();
                for &e in b"alse" {
                    self.check(e)?;
                }
                Ok(Value::Bool(false))
            }
            b'n' => {
                self.next();
                for &e in b"ull" {
                    self.check(e)?;
                }
                Ok(Value::Null)
            }
            b'"' => Ok(Value::Str(self.val_str()?)),
            b'-' | b'0'..=b'9' => {
                let neg = self.c == b'-';
                if neg {
                    self.next();
                }
                let mut v: i32 = 0;
                while self.c.is_ascii_digit() {
                    v = v.wrapping_mul(10).wrapping_add((self.c - b'0') as i32);
                    self.next();
                }
                Ok(Value::Int(if neg { v.wrapping_neg() } else { v }))
            }
            _ => Err(self.error()),
        }
    }

    /// `reader::find`: walks `path` from `o`, creating tables on the way. With
    /// `make_array` the last step adds a table to an array of tables; with
    /// `assign` the last name is left for the caller to insert.
    fn find(&self, o: &Value, path: &[Vec<u8>], make_array: bool, assign: bool) -> Result<Value, Error> {
        let mut c = o.clone();
        let count = if assign { path.len() - 1 } else { path.len() };
        for (i, e) in path[..count].iter().enumerate() {
            let Value::Obj(obj) = &c else { return Err(self.error()) };
            let obj = obj.clone();
            let mut map = obj.borrow_mut();
            let v = map.entry(e.clone()).or_insert(Value::Null);
            if matches!(v, Value::Null) {
                if make_array && i == count - 1 {
                    let table = new_object();
                    *v = Value::Arr(Rc::new(RefCell::new(vec![table.clone()])));
                    c = table;
                } else {
                    let table = new_object();
                    *v = table.clone();
                    c = table;
                }
            } else {
                c = v.clone();
                drop(map);
                while let Value::Arr(a) = c.clone() {
                    let mut a = a.borrow_mut();
                    if make_array && i == count - 1 {
                        a.push(new_object());
                    } else if a.is_empty() {
                        return Err(self.error());
                    }
                    c = a.last().unwrap().clone();
                }
                if !matches!(c, Value::Obj(_)) {
                    return Err(self.error());
                }
            }
        }
        Ok(c)
    }

    fn parse(&mut self) -> Result<Value, Error> {
        let root = new_object();
        self.next();
        let mut cur = root.clone();
        loop {
            self.skipws();
            if self.c == b'[' {
                self.next();
                let is_array_obj = self.test(b'[');
                let name = self.dotted();
                self.check(b']')?;
                if is_array_obj {
                    self.check(b']')?;
                }
                cur = self.find(&root, &name, is_array_obj, false)?;
            } else if self.c == b'\n' || self.c == b'\r' {
                // Empty line
            } else {
                let name = self.dotted();
                self.skipws();
                self.check(b'=')?;
                self.skipws();
                let v = self.val()?;
                self.skipws();
                let Value::Obj(o) = self.find(&cur, &name, false, true)? else { return Err(self.error()) };
                o.borrow_mut().entry(name.last().unwrap().clone()).or_insert(v);
            }
            if !(self.test(b'\n') || self.test(b'\r')) {
                break;
            }
        }
        // Like the C++, a NUL byte ends the file too.
        self.check(0)?;
        Ok(root)
    }
}

fn from_hex(c: u8) -> u8 {
    match c {
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        b'0'..=b'9' => c - b'0',
        _ => 0,
    }
}

/// A parsed file, read with the same calls as the C++ archive functions.
pub struct Reader {
    cur: Value,
    name: String,
}

impl Reader {
    pub fn parse(data: &[u8], name: &str) -> Result<Self, Error> {
        let mut p = Parser { data, pos: 0, c: 0, name };
        let root = p.parse()?;
        Ok(Reader { cur: root, name: name.to_string() })
    }

    fn error(&self, what: &str) -> Error {
        Error(format!("{}: {}", self.name, what))
    }

    /// `reader::f`: the field `name` of the current table, or the current
    /// value itself for `None`.
    fn f(&self, name: Option<&str>) -> Result<Value, Error> {
        match name {
            None => Ok(self.cur.clone()),
            Some(name) => {
                let Value::Obj(o) = &self.cur else { return Err(self.error(&format!("{name} is not in a table"))) };
                let v = o.borrow().get(name.as_bytes()).cloned();
                v.ok_or_else(|| self.error(&format!("missing {name}")))
            }
        }
    }

    pub fn obj<T>(&mut self, name: Option<&str>, body: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        let v = self.f(name)?;
        let parent = std::mem::replace(&mut self.cur, v);
        let r = body(self);
        self.cur = parent;
        r
    }

    /// An array (or array of tables): calls `item` with each element as the
    /// current value.
    pub fn arr<T>(&mut self, name: &str, mut item: impl FnMut(&mut Self) -> Result<T, Error>) -> Result<Vec<T>, Error> {
        let Value::Arr(a) = self.f(Some(name))? else { return Err(self.error(&format!("{name} is not an array"))) };
        let elements = a.borrow().clone();
        let parent = self.cur.clone();
        let mut out = Vec::with_capacity(elements.len());
        for e in elements {
            self.cur = e;
            let r = item(self);
            if r.is_err() {
                self.cur = parent;
                return r.map(|_| Vec::new());
            }
            out.push(r?);
        }
        self.cur = parent;
        Ok(out)
    }

    /// An array that must have exactly `N` elements, like the C++ fixed arrays.
    pub fn arr_n<T: Default + Copy, const N: usize>(
        &mut self,
        name: &str,
        item: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<[T; N], Error> {
        let v = self.arr(name, item)?;
        v.try_into().map_err(|v: Vec<T>| self.error(&format!("{name} has {} entries, not {N}", v.len())))
    }

    pub fn i32(&mut self, name: Option<&str>) -> Result<i32, Error> {
        match self.f(name)? {
            Value::Int(i) => Ok(i),
            _ => Err(self.error(&format!("{} is not an integer", name.unwrap_or("value")))),
        }
    }

    pub fn b(&mut self, name: &str) -> Result<bool, Error> {
        match self.f(Some(name))? {
            Value::Bool(b) => Ok(b),
            _ => Err(self.error(&format!("{name} is not true or false"))),
        }
    }

    pub fn str(&mut self, name: Option<&str>) -> Result<Vec<u8>, Error> {
        match self.f(name)? {
            Value::Str(s) => Ok(s),
            _ => Err(self.error(&format!("{} is not a string", name.unwrap_or("value")))),
        }
    }

    /// A reference to an object by id: its index in `ids`, or -1 for `null`.
    /// (The C++ resolver runs past the end of the list for an unknown id.)
    pub fn reference(&mut self, name: &str, ids: &[String]) -> Result<i32, Error> {
        match self.f(Some(name))? {
            Value::Null => Ok(-1),
            Value::Str(s) => ids
                .iter()
                .position(|id| id.as_bytes() == s.as_slice())
                .map(|i| i as i32)
                .ok_or_else(|| self.error(&format!("{name} refers to unknown {}", String::from_utf8_lossy(&s)))),
            _ => Err(self.error(&format!("{name} is not a string or null"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Reader, Error> {
        Reader::parse(s.as_bytes(), "test")
    }

    #[test]
    fn reads_tables_arrays_and_values() {
        let mut r = parse("\r\n[a]\r\nx = -12\r\ny = true\r\n\r\n[a.b]\r\ns = \"q\\\"\\u00e5\"\r\nl = [1, 2,\r\n 3]\r\nn = null").unwrap();
        r.obj(Some("a"), |r| {
            assert_eq!(r.i32(Some("x"))?, -12);
            assert!(r.b("y")?);
            r.obj(Some("b"), |r| {
                assert_eq!(r.str(Some("s"))?, b"q\"\xe5");
                assert_eq!(r.arr("l", |r| r.i32(None))?, vec![1, 2, 3]);
                assert_eq!(r.reference("n", &[])?, -1);
                Ok(())
            })
        })
        .unwrap();
    }

    #[test]
    fn arrays_of_tables_collect_each_section() {
        let mut r = parse("[[t]]\r\n  v = 1\r\n[[t]]\r\n  v = 2").unwrap();
        assert_eq!(r.arr("t", |r| r.i32(Some("v"))).unwrap(), vec![1, 2]);
    }

    #[test]
    fn keeps_the_first_of_two_keys() {
        let mut r = parse("x = 1\r\nx = 2").unwrap();
        assert_eq!(r.i32(Some("x")).unwrap(), 1);
    }

    #[test]
    fn integers_wrap() {
        let mut r = parse("x = 4294967297").unwrap();
        assert_eq!(r.i32(Some("x")).unwrap(), 1);
    }

    #[test]
    fn rejects_what_the_cpp_reader_rejects() {
        assert!(parse("x = 1\r\n").is_err(), "a line break at the end");
        assert!(parse("x = 1 # comment").is_err());
        assert!(parse("x = \"open").is_err());
        assert!(parse("x = \"\\n\"").is_err());
        assert!(parse("x = yes").is_err());
        let mut r = parse("x = 1").unwrap();
        assert!(r.i32(Some("y")).is_err());
        assert!(r.b("x").is_err());
        assert!(r.reference("x", &[]).is_err());
    }
}

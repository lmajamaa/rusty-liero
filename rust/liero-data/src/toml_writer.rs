//! Port of `gvl::toml::writer` (src/gvl/serialization/toml.hpp). It reproduces the
//! C++ output byte for byte, including the CRLF line breaks and blank lines, so that
//! converted TCs are identical to the ones the C++ tctool writes.

pub struct Writer {
    out: Vec<u8>,
    indent: usize,
    chain: Vec<&'static str>,
    in_object: bool,
}

impl Writer {
    pub fn new() -> Self {
        Writer {
            out: Vec::new(),
            indent: 0,
            chain: Vec::new(),
            in_object: false,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    fn put(&mut self, s: &str) {
        self.out.extend_from_slice(s.as_bytes());
    }

    fn windent(&mut self) {
        self.put("\r\n");
        for _ in 0..self.indent {
            self.put("  ");
        }
    }

    fn put_chain(&mut self) {
        let path = self.chain.join(".");
        self.put(&path);
    }

    fn field(&mut self, name: Option<&str>) {
        if let Some(name) = name {
            if !self.in_object {
                if !self.chain.is_empty() {
                    self.put("\r\n");
                    self.windent();
                    self.put("[");
                    self.put_chain();
                    self.put("]\r\n");
                }
                self.in_object = true;
            }
            self.windent();
            self.put(name);
            self.put(" = ");
        }
    }

    /// A table. Unnamed tables add no header, as used for the per-object files.
    pub fn obj(&mut self, name: Option<&'static str>, body: impl FnOnce(&mut Self)) {
        if let Some(name) = name {
            self.chain.push(name);
            self.in_object = false;
        }
        body(self);
        if name.is_some() {
            self.chain.pop();
            self.in_object = false;
        }
    }

    /// An array of tables, one `[[name]]` section per item.
    pub fn array_obj<T>(&mut self, name: &'static str, items: &[T], mut body: impl FnMut(&mut Self, &T)) {
        self.chain.push(name);
        self.in_object = false;

        for item in items {
            self.put("\r\n");
            self.windent();
            self.put("[[");
            self.put_chain();
            self.put("]]\r\n");
            self.indent += 1;
            self.in_object = true;
            body(self, item);
            self.indent -= 1;
        }

        self.chain.pop();
        self.in_object = false;
    }

    /// An inline array.
    pub fn arr<T>(&mut self, name: &str, items: &[T], mut item: impl FnMut(&mut Self, &T)) {
        self.field(Some(name));
        self.put("[");
        for (i, e) in items.iter().enumerate() {
            if i > 0 {
                self.put(", ");
            }
            item(self, e);
        }
        self.put("]");
    }

    pub fn i32_value(&mut self, v: i32) {
        let s = v.to_string();
        self.put(&s);
    }

    pub fn i32(&mut self, name: &str, v: i32) {
        self.field(Some(name));
        self.i32_value(v);
    }

    pub fn bool(&mut self, name: &str, v: bool) {
        self.field(Some(name));
        self.put(if v { "true" } else { "false" });
    }

    /// Strings are bytes from the EXE; anything outside printable ASCII becomes `\u00xx`.
    pub fn str_value(&mut self, s: &[u8]) {
        self.out.push(b'"');
        for &c in s {
            if (0x20..=0x7e).contains(&c) {
                if c == b'"' || c == b'\\' {
                    self.out.push(b'\\');
                }
                self.out.push(c);
            } else {
                let escaped = format!("\\u{:04x}", c);
                self.put(&escaped);
            }
        }
        self.out.push(b'"');
    }

    pub fn str(&mut self, name: &str, s: &[u8]) {
        self.field(Some(name));
        self.str_value(s);
    }

    /// A reference to another object by id, or `null` for a negative index.
    pub fn reference(&mut self, name: &str, index: i32, ids: &[&str]) {
        self.field(Some(name));
        match usize::try_from(index).ok().and_then(|i| ids.get(i)) {
            Some(id) => self.str_value(id.as_bytes()),
            None => self.put("null"),
        }
    }
}

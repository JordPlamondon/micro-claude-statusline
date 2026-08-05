//! Minimal JSON reader, enough for the status line payload.
//!
//! Hand-rolled rather than serde so the binary stays small and builds fast. It
//! keeps each number's original text alongside the parsed f64, because Python
//! prints `12` and `12.0` differently and the output must match byte for byte.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    /// Parsed value plus the literal text it came from.
    Num(f64, String),
    Str(String),
    Arr(Vec<Value>),
    Obj(HashMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(map) => map.get(key),
            _ => None,
        }
    }

    /// Follows a dotted path, returning None if any step is missing or the wrong
    /// shape. Mirrors the `(data.get("a") or {}).get("b")` chains in the original.
    pub fn path(&self, path: &str) -> Option<&Value> {
        let mut cur = self;
        for key in path.split('.') {
            cur = cur.get(key)?;
        }
        match cur {
            Value::Null => None,
            other => Some(other),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n, _) => Some(*n),
            _ => None,
        }
    }

    pub fn as_raw(&self) -> Option<&str> {
        match self {
            Value::Num(_, raw) => Some(raw),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Arr(items) => Some(items),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Num(n, _) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Arr(a) => !a.is_empty(),
            Value::Obj(o) => !o.is_empty(),
            Value::Null => false,
        }
    }

    /// Trimmed string, or None when absent, non-string or empty after trimming.
    pub fn str_path(&self, path: &str) -> Option<String> {
        let s = self.path(path)?.as_str()?.trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

pub fn parse(input: &str) -> Option<Value> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    // Trailing content means malformed input, which the original treats as fatal.
    if p.pos != p.bytes.len() {
        return None;
    }
    Some(v)
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn lit(&mut self, word: &str) -> bool {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Value> {
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Value::Str),
            b't' => self.lit("true").then_some(Value::Bool(true)),
            b'f' => self.lit("false").then_some(Value::Bool(false)),
            b'n' => self.lit("null").then_some(Value::Null),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Option<Value> {
        self.pos += 1; // {
        let mut map = HashMap::new();
        self.ws();
        if self.peek()? == b'}' {
            self.pos += 1;
            return Some(Value::Obj(map));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if self.peek()? != b':' {
                return None;
            }
            self.pos += 1;
            self.ws();
            let val = self.value()?;
            map.insert(key, val);
            self.ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Some(Value::Obj(map));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self) -> Option<Value> {
        self.pos += 1; // [
        let mut items = Vec::new();
        self.ws();
        if self.peek()? == b']' {
            self.pos += 1;
            return Some(Value::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    return Some(Value::Arr(items));
                }
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        if self.peek()? != b'"' {
            return None;
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            let b = self.peek()?;
            self.pos += 1;
            match b {
                b'"' => return Some(out),
                b'\\' => {
                    let esc = self.peek()?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self.bytes.get(self.pos..self.pos + 4)?;
                            self.pos += 4;
                            let mut code =
                                u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
                            // Surrogate pair: \uD800-\uDBFF must be followed by a low surrogate.
                            if (0xD800..0xDC00).contains(&code) {
                                if self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u") {
                                    self.pos += 2;
                                    let hex2 = self.bytes.get(self.pos..self.pos + 4)?;
                                    self.pos += 4;
                                    let low =
                                        u32::from_str_radix(std::str::from_utf8(hex2).ok()?, 16)
                                            .ok()?;
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                }
                            }
                            out.push(char::from_u32(code)?);
                        }
                        _ => return None,
                    }
                }
                _ => {
                    // Copy the whole UTF-8 sequence, not just the lead byte.
                    let len = utf8_len(b);
                    let end = self.pos - 1 + len;
                    out.push_str(std::str::from_utf8(self.bytes.get(self.pos - 1..end)?).ok()?);
                    self.pos = end;
                }
            }
        }
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')) {
            self.pos += 1;
        }
        if start == self.pos {
            return None;
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
        Some(Value::Num(raw.parse().ok()?, raw.to_string()))
    }
}

fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

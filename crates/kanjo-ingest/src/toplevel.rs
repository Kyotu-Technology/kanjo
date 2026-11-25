//! Top-level shape of one transcript line, found on raw bytes. Strings are
//! skipped with memchr, so multi-megabyte content is never decoded; the scan
//! can stop as soon as the record type is known.

use memchr::memchr2;
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};

pub(crate) struct Shape<'a> {
    pub kind: Option<&'a [u8]>,
    pub version: Option<&'a [u8]>,
    pub keys: Vec<&'a [u8]>,
}

fn ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

/// Index of the quote closing the string whose content starts at `i`.
fn string_end(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let j = i + memchr2(b'"', b'\\', b.get(i..)?)?;
        if b[j] == b'"' {
            return Some(j);
        }
        i = j + 2;
    }
}

/// Index just past the JSON value starting at `i`.
fn skip_value(b: &[u8], i: usize) -> Option<usize> {
    match *b.get(i)? {
        b'"' => Some(string_end(b, i + 1)? + 1),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut k = i;
            loop {
                match *b.get(k)? {
                    b'"' => k = string_end(b, k + 1)? + 1,
                    b'{' | b'[' => {
                        depth += 1;
                        k += 1;
                    }
                    b'}' | b']' => {
                        depth = depth.checked_sub(1)?;
                        k += 1;
                        if depth == 0 {
                            return Some(k);
                        }
                    }
                    _ => k += 1,
                }
            }
        }
        _ => {
            let mut k = i;
            while k < b.len() && !matches!(b[k], b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n') {
                k += 1;
            }
            (k > i).then_some(k)
        }
    }
}

/// Walks the top-level object. When `stop` accepts the type value the scan
/// ends there and `keys` is incomplete. `None` for anything that is not a
/// well-formed top-level object as far as the scan can tell.
pub(crate) fn scan<'a>(b: &'a [u8], stop: impl Fn(&[u8]) -> bool) -> Option<Shape<'a>> {
    let mut shape = Shape { kind: None, version: None, keys: Vec::new() };
    let mut i = ws(b, 0);
    if *b.get(i)? != b'{' {
        return None;
    }
    i = ws(b, i + 1);
    if *b.get(i)? == b'}' {
        return (ws(b, i + 1) == b.len()).then_some(shape);
    }
    loop {
        if *b.get(i)? != b'"' {
            return None;
        }
        let key_end = string_end(b, i + 1)?;
        let key = &b[i + 1..key_end];
        i = ws(b, key_end + 1);
        if *b.get(i)? != b':' {
            return None;
        }
        let start = ws(b, i + 1);
        i = skip_value(b, start)?;
        shape.keys.push(key);
        if b[start] == b'"' {
            let value = &b[start + 1..i - 1];
            if key == b"type" {
                shape.kind = Some(value);
                if stop(value) {
                    return Some(shape);
                }
            } else if key == b"version" {
                shape.version = Some(value);
            }
        }
        i = ws(b, i);
        match *b.get(i)? {
            b',' => i = ws(b, i + 1),
            b'}' => return (ws(b, i + 1) == b.len()).then_some(shape),
            _ => return None,
        }
    }
}

/// The same shape through serde, for lines the byte scan rejects.
pub(crate) struct OwnedShape {
    pub kind: Option<String>,
    pub version: Option<String>,
    pub keys: Vec<String>,
}

struct ProbeVisitor;

impl<'de> Visitor<'de> for ProbeVisitor {
    type Value = OwnedShape;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<OwnedShape, A::Error> {
        let mut shape = OwnedShape { kind: None, version: None, keys: Vec::new() };
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "type" => shape.kind = map.next_value::<crate::record::Str>()?.into_owned(),
                "version" => shape.version = map.next_value::<crate::record::Str>()?.into_owned(),
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            shape.keys.push(key);
        }
        Ok(shape)
    }
}

pub(crate) fn probe(line: &[u8]) -> Option<OwnedShape> {
    let mut de = serde_json::Deserializer::from_slice(line);
    let shape = (&mut de).deserialize_map(ProbeVisitor).ok()?;
    de.end().ok()?;
    Some(shape)
}

fn clean(raw: &[u8], max: usize) -> Option<&str> {
    let ok = !raw.is_empty()
        && raw.len() <= max
        && raw.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'+' | b':'));
    if ok {
        std::str::from_utf8(raw).ok()
    } else {
        None
    }
}

/// A record type, reduced to a safe identifier.
pub(crate) fn clean_type(raw: Option<&[u8]>) -> String {
    match raw {
        None => "<none>".into(),
        Some(r) => clean(r, 64).unwrap_or("<odd>").to_string(),
    }
}

pub(crate) fn clean_version(raw: Option<&[u8]>) -> Option<String> {
    raw.and_then(|r| clean(r, 32)).map(str::to_string)
}

/// Sorted, de-duplicated top-level key names. Odd names become `?`.
pub(crate) fn signature<'k>(keys: impl Iterator<Item = &'k [u8]>) -> String {
    let mut names: Vec<&str> = keys.map(|k| clean(k, 64).unwrap_or("?")).collect();
    names.sort_unstable();
    names.dedup();
    let mut sig = names.join(",");
    if sig.len() > 1024 {
        let mut cut = 1024;
        while !sig.is_char_boundary(cut) {
            cut -= 1;
        }
        sig.truncate(cut);
    }
    sig
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(line: &str) -> Option<String> {
        scan(line.as_bytes(), |_| false).map(|s| s.kind.map(|k| String::from_utf8_lossy(k).into_owned()).unwrap_or_default())
    }

    fn probe_kind(line: &str) -> Option<String> {
        probe(line.as_bytes()).map(|s| s.kind.unwrap_or_default())
    }

    #[test]
    fn scanner_agrees_with_serde_on_tricky_lines() {
        let lines = [
            r#"{"type":"user","message":{"content":"say \"type\":\"assistant\" {[}]"}}"#,
            r#"{"message":{"type":"message","content":[{"type":"text","text":"a\\"}]},"type":"assistant"}"#,
            r#"{"a":"\\\"","b":[1,2,{"type":"x"}],"type":"summary"}"#,
            r#" { "x" : -1.5e3 , "y" : null , "z" : true , "type" : "mode" } "#,
            r#"{"nested":{"type":"user"},"type":"brand-new"}"#,
            r#"{"type":"assistant"}"#,
            r#"{}"#,
            r#"{"no_type":1}"#,
        ];
        for line in lines {
            assert_eq!(kind(line), probe_kind(line), "{line}");
        }
    }

    #[test]
    fn malformed_lines_are_rejected() {
        for line in [r#"{"type":"user""#, r#"{"type":"user"} trailing"#, "[1,2]", r#"{"a":}"#, r#"{"a":"unterminated}"#, ""] {
            assert!(scan(line.as_bytes(), |_| false).is_none(), "{line}");
        }
    }

    #[test]
    fn stops_early_and_cleans_shapes() {
        let line = br#"{"type":"user","message":{"broken"#;
        let s = scan(line, |t| t == b"user").unwrap();
        assert_eq!(s.kind, Some(&b"user"[..]));
        assert_eq!(signature([&b"b"[..], b"a", b"b", b"we ird"].into_iter()), "?,a,b");
        assert_eq!(clean_type(Some(b"has space")), "<odd>");
        assert_eq!(clean_version(Some(b"2.1.300")), Some("2.1.300".into()));
    }
}

//! Ordered, duplicate-rejecting JSON for ClothingBase's first-template fallback.
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use std::fmt;
#[derive(Debug)]
pub(crate) enum Node {
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    Number(u32),
    Text(String),
}
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a ClothingBase object, array, uint32 or string")
            }
            fn visit_u64<E: de::Error>(self, n: u64) -> Result<Node, E> {
                Ok(Node::Number(
                    n.try_into().map_err(|_| E::custom("uint32 overflow"))?,
                ))
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Node, E> {
                Ok(Node::Text(s.into()))
            }
            fn visit_string<E: de::Error>(self, s: String) -> Result<Node, E> {
                Ok(Node::Text(s))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
                let mut v = Vec::new();
                while let Some(n) = seq.next_element()? {
                    if v.len() >= 4096 {
                        return Err(de::Error::custom("ClothingBase array exceeds 4096 entries"));
                    }
                    v.push(n);
                }
                Ok(Node::Array(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
                let mut v = Vec::new();
                let mut seen = std::collections::BTreeSet::new();
                while let Some((k, n)) = map.next_entry::<String, Node>()? {
                    if v.len() >= 4096 {
                        return Err(de::Error::custom(
                            "ClothingBase object exceeds 4096 entries",
                        ));
                    }
                    if !seen.insert(k.to_ascii_lowercase()) {
                        return Err(de::Error::custom(format!("Duplicate field {k}")));
                    }
                    v.push((k, n));
                }
                Ok(Node::Object(v))
            }
        }
        d.deserialize_any(Read)
    }
}
pub(crate) fn number(n: Node) -> Result<u32, String> {
    match n {
        Node::Number(n) => Ok(n),
        Node::Text(s) => parse_id(&s),
        _ => Err("Expected an unsigned integer or decimal/0x string".into()),
    }
}
pub(crate) fn parse_id(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        s.parse()
    }
    .map_err(|_| format!("Invalid uint32 {s}"))
}
pub(crate) fn object(n: Node) -> Result<Vec<(String, Node)>, String> {
    match n {
        Node::Object(o) => Ok(o),
        _ => Err("Expected an object".into()),
    }
}
pub(crate) fn array(n: Node) -> Result<Vec<Node>, String> {
    match n {
        Node::Array(a) => Ok(a),
        _ => Err("Expected an array".into()),
    }
}
pub(crate) fn take(o: &mut Vec<(String, Node)>, name: &str) -> Option<Node> {
    o.iter()
        .position(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|i| o.remove(i).1)
}
pub(crate) fn uint(o: &mut Vec<(String, Node)>, name: &str) -> Result<u32, String> {
    take(o, name).map(number).unwrap_or(Ok(0))
}
pub(crate) fn list(o: &mut Vec<(String, Node)>, name: &str) -> Result<Vec<Node>, String> {
    take(o, name).map(array).unwrap_or(Ok(Vec::new()))
}
pub(crate) fn finish(o: Vec<(String, Node)>) -> Result<(), String> {
    if let Some((k, _)) = o.first() {
        Err(format!("Unsupported ClothingBase field {k}"))
    } else {
        Ok(())
    }
}
/// The mod permits trailing commas. Remove only commas followed by a closing
/// bracket outside quoted strings; normal JSON parsing still validates everything.
pub(crate) fn without_trailing_commas(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    let mut quote = false;
    let mut escaped = false;
    for i in 0..bytes.len() {
        let c = bytes[i];
        if escaped {
            escaped = false;
            continue;
        }
        if quote && c == b'\\' {
            escaped = true;
            continue;
        }
        if c == b'"' {
            quote = !quote;
            continue;
        }
        if !quote
            && c == b','
            && bytes[i + 1..]
                .iter()
                .find(|c| !c.is_ascii_whitespace())
                .is_some_and(|c| matches!(c, b'}' | b']'))
        {
            bytes[i] = b' ';
        }
    }
    bytes
}

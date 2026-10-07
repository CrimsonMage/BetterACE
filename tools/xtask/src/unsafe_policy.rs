use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::visit::Visit;

const CODEC: &str = "bace-storage-codec";
const BOUNDARY: &str = "crates/storage/bace-storage-codec/src/mapping.rs";

pub fn workspace_lints(manifest: &toml::Value) -> Vec<String> {
    if level(
        manifest
            .get("workspace")
            .and_then(|v| v.get("lints"))
            .and_then(|v| v.get("rust"))
            .and_then(|v| v.get("unsafe_code")),
    ) != Some("forbid")
    {
        vec!["PACK-03: workspace unsafe_code MUST remain forbid".into()]
    } else {
        vec![]
    }
}

pub fn package_lints(name: &str, manifest: &toml::Value, workspace: &toml::Value) -> Vec<String> {
    let lints = manifest.get("lints");
    if name != CODEC {
        if lints
            .and_then(|v| v.get("workspace"))
            .and_then(toml::Value::as_bool)
            != Some(true)
        {
            return vec![format!("PACK-03: {name} MUST inherit workspace lints")];
        }
        return vec![];
    }
    let mut errors = Vec::new();
    if level(
        lints
            .and_then(|v| v.get("rust"))
            .and_then(|v| v.get("unsafe_code")),
    ) != Some("deny")
        || lints.and_then(|v| v.get("workspace")).is_some()
    {
        errors.push(format!(
            "PACK-03: {CODEC} alone MUST explicitly deny unsafe_code"
        ));
    }
    // Opting out of workspace lint inheritance must not discard other rules.
    if let Some(groups) = workspace
        .get("workspace")
        .and_then(|v| v.get("lints"))
        .and_then(toml::Value::as_table)
    {
        for (group, entries) in groups {
            if let Some(entries) = entries.as_table() {
                for (lint, expected) in entries {
                    if group == "rust" && lint == "unsafe_code" {
                        continue;
                    }
                    if lints.and_then(|v| v.get(group)).and_then(|v| v.get(lint)) != Some(expected)
                    {
                        errors.push(format!(
                            "PACK-03: {CODEC} MUST preserve workspace {group}.{lint}"
                        ));
                    }
                }
            }
        }
    }
    errors
}

fn level(value: Option<&toml::Value>) -> Option<&str> {
    value.and_then(|v| {
        v.as_str()
            .or_else(|| v.get("level").and_then(toml::Value::as_str))
    })
}

pub fn source(path: &str, text: &str) -> Vec<String> {
    let Ok(tokens) = text.parse::<TokenStream>() else {
        return vec![];
    };
    if path == BOUNDARY {
        // This tiny boundary is intentionally exact, not merely "unsafe allowed
        // somewhere in mapping.rs". Any change requires an explicit audit and
        // corresponding policy update. Documentation/comments may change.
        let expected: TokenStream = APPROVED_BOUNDARY.parse().expect("policy fixture parses");
        if without_docs(tokens).to_string() != without_docs(expected).to_string()
            || !text.contains("SAFETY:")
        {
            return vec![format!(
                "{path}: PACK-03 mapping MUST match reviewed open_immutable boundary with SAFETY contract"
            )];
        }
        return vec![];
    }
    let mut errors = Vec::new();
    if contains(tokens, "unsafe") {
        errors.push(format!(
            "{path}: PACK-03 unsafe is only permitted in reviewed mapping boundary"
        ));
    }
    if let Ok(file) = syn::parse_file(text) {
        let mut lowering = Lowering(false);
        lowering.visit_file(&file);
        if lowering.0 {
            errors.push(format!(
                "{path}: PACK-03 unsafe_code lint MUST NOT be lowered"
            ));
        }
    }
    errors
}

fn contains(tokens: TokenStream, name: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == name,
        TokenTree::Group(group) => contains(group.stream(), name),
        _ => false,
    })
}

struct Lowering(bool);
impl<'ast> Visit<'ast> for Lowering {
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if let syn::Meta::List(list) = &node.meta {
            let mentions_unsafe = contains(list.tokens.clone(), "unsafe_code");
            let lowering = ["allow", "warn", "expect"]
                .iter()
                .any(|name| list.path.is_ident(name) || contains(list.tokens.clone(), name));
            if mentions_unsafe && lowering {
                self.0 = true;
            }
        }
        syn::visit::visit_attribute(self, node);
    }
}

fn without_docs(tokens: TokenStream) -> TokenStream {
    let all: Vec<_> = tokens.into_iter().collect();
    let mut kept = TokenStream::new();
    let mut index = 0;
    while index < all.len() {
        if matches!(&all[index], TokenTree::Punct(p) if p.as_char() == '#') {
            let mut next = index + 1;
            if matches!(all.get(next), Some(TokenTree::Punct(p)) if p.as_char() == '!') {
                next += 1;
            }
            if let Some(TokenTree::Group(group)) = all.get(next)
                && group.delimiter() == Delimiter::Bracket
                && matches!(group.stream().into_iter().next(), Some(TokenTree::Ident(i)) if i == "doc")
            {
                index = next + 1;
                continue;
            }
        }
        kept.extend(std::iter::once(all[index].clone()));
        index += 1;
    }
    kept
}

const APPROVED_BOUNDARY: &str = r#"
use std::fs::File;
pub(crate) fn open_immutable(file: &File) -> std::io::Result<memmap2::Mmap> {
    #[allow(unsafe_code)]
    let map = unsafe { memmap2::MmapOptions::new().map(file)? };
    Ok(map)
}
"#;

#[cfg(test)]
#[path = "unsafe_policy_tests.rs"]
mod tests;

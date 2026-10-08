//! Prepared authoritative character-name policy. No SQL, assets or clocks.
//! Taboo matching follows pinned ACE TabooTableEntry word-local '*' patterns.
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameApproval {
    original: String,
    normalized: String,
}
impl NameApproval {
    pub fn normalized_name(&self) -> &str {
        &self.normalized
    }
    pub(crate) fn matches(&self, original: &str) -> bool {
        self.original == original
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    InvalidSyntax,
    NotRepresentable,
    TooLong,
    Banned,
    CreatureName,
    PolicyCapacity,
    UnsupportedPattern,
}
/// Inputs must come from the first authored taboo category and the complete
/// accepted creature-name index. An empty list is an explicit policy choice.
#[derive(Debug)]
pub struct NamePolicy {
    patterns: Vec<String>,
    creatures: BTreeSet<String>,
}
impl NamePolicy {
    pub fn prepare(patterns: &[String], creature_names: &[String]) -> Result<Self, NameError> {
        if patterns.len() > 65_536 || creature_names.len() > 1_000_000 {
            return Err(NameError::PolicyCapacity);
        }
        let mut prepared = Vec::with_capacity(patterns.len());
        for pattern in patterns {
            if pattern.len() > 1024 || pattern.is_empty() {
                return Err(NameError::UnsupportedPattern);
            }
            // ACE interprets these as regex. Admit only the documented literal
            // word + wildcard grammar rather than silently misreading regex.
            if pattern
                .chars()
                .any(|c| !c.is_alphabetic() && !matches!(c, '*' | '-' | '\''))
            {
                return Err(NameError::UnsupportedPattern);
            }
            prepared.push(pattern.to_lowercase());
        }
        let mut creatures = BTreeSet::new();
        for name in creature_names {
            if name.len() > 1024 {
                return Err(NameError::PolicyCapacity);
            }
            creatures.insert(name.to_lowercase());
        }
        Ok(Self {
            patterns: prepared,
            creatures,
        })
    }
    pub fn approve(&self, name: &str) -> Result<NameApproval, NameError> {
        if name.len() > 100 {
            return Err(NameError::TooLong);
        }
        if name.chars().any(|c| !cp1252(c)) {
            return Err(NameError::NotRepresentable);
        }
        if name.is_empty()
            || name.trim() != name
            || !name.chars().next().is_some_and(char::is_uppercase)
            || name
                .chars()
                .any(|c| !c.is_alphabetic() && !matches!(c, ' ' | '-' | '\''))
            || name.contains("  ")
        {
            return Err(NameError::InvalidSyntax);
        }
        let all_upper = name
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase);
        let normalized = if all_upper {
            let mut chars = name.chars();
            let mut s = chars.next().expect("nonempty name").to_string();
            s.extend(chars.flat_map(char::to_lowercase));
            s
        } else {
            name.to_owned()
        };
        if normalized.len() > 100 || normalized.chars().any(|c| !cp1252(c)) {
            return Err(NameError::NotRepresentable);
        }
        let lower = normalized.to_lowercase();
        if lower.split(' ').any(|word| {
            self.patterns
                .iter()
                .any(|pattern| wildcard(pattern.as_bytes(), word.as_bytes()))
        }) {
            return Err(NameError::Banned);
        }
        if self.creatures.contains(&lower) {
            return Err(NameError::CreatureName);
        }
        Ok(NameApproval {
            original: name.to_owned(),
            normalized,
        })
    }
}
fn cp1252(c: char) -> bool {
    matches!(c as u32, 0x20..=0x7e | 0xa0..=0xff) || "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".contains(c)
}
// Linear wildcard matcher with one remembered star. All other bytes literal;
// case-folding has already occurred and '*' cannot split a Unicode character.
fn wildcard(pattern: &[u8], word: &[u8]) -> bool {
    let (mut p, mut w, mut star, mut retry) = (0, 0, None, 0);
    while w < word.len() {
        if p < pattern.len() && pattern[p] == word[w] {
            p += 1;
            w += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = w;
        } else if let Some(s) = star {
            retry += 1;
            w = retry;
            p = s + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

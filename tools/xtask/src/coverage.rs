//! Offline structural validation of the pinned networking coverage register.
use serde::Deserialize;
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Deserialize)]
struct Inventory {
    version: u32,
    repository: String,
    commit: String,
    source_count: usize,
    sources: Vec<Source>,
}
#[derive(Deserialize)]
struct Source {
    source: String,
    sha256: String,
    git_blob: String,
    owner: String,
    status: String,
    evidence: Vec<String>,
}

pub fn check() -> Result<(), Vec<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let inspect = || -> Result<(), Box<dyn std::error::Error>> {
        let manifest: Inventory = toml::from_str(&fs::read_to_string(
            root.join("docs/network-coverage.toml"),
        )?)?;
        let baseline: toml::Value =
            toml::from_str(&fs::read_to_string(root.join("docs/baselines.toml"))?)?;
        let architecture: toml::Value =
            toml::from_str(&fs::read_to_string(root.join("architecture.toml"))?)?;
        let owners: BTreeSet<_> = architecture["crates"]
            .as_array()
            .ok_or("crate inventory absent")?
            .iter()
            .filter_map(|v| v["name"].as_str())
            .collect();
        let errors = validate(
            &root,
            &manifest,
            baseline["ace"]["commit"].as_str().ok_or("ACE pin absent")?,
            &owners,
        );
        if !errors.is_empty() {
            return Err(errors.join("\n").into());
        }
        Ok(())
    };
    inspect().map_err(|e| vec![format!("network coverage: {e}")])
}

fn hex(value: &str, width: usize) -> bool {
    value.len() == width && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn validate(root: &Path, inventory: &Inventory, pin: &str, owners: &BTreeSet<&str>) -> Vec<String> {
    let mut errors = Vec::new();
    if inventory.version != 1
        || inventory.repository != "https://github.com/ACEmulator/ACE"
        || inventory.commit != pin
    {
        errors.push("inventory must name the official ACE baseline".into());
    }
    if inventory.sources.is_empty() || inventory.source_count != inventory.sources.len() {
        errors.push("source count does not match nonempty inventory".into());
    }
    let mut paths = BTreeSet::new();
    for source in &inventory.sources {
        if !paths.insert(&source.source)
            || !source.source.starts_with("Source/")
            || source.source.contains("..")
        {
            errors.push(format!("invalid or duplicate source {}", source.source));
        }
        if !hex(&source.sha256, 64) || !hex(&source.git_blob, 40) {
            errors.push(format!("missing source fingerprint: {}", source.source));
        }
        if !owners.contains(source.owner.as_str()) {
            errors.push(format!("unassigned source {}", source.source));
        }
        if !["unsupported", "foundation", "implemented"].contains(&source.status.as_str()) {
            errors.push(format!("invalid coverage status: {}", source.source));
        }
        if source.status != "unsupported" && source.evidence.is_empty() {
            errors.push(format!("coverage without evidence: {}", source.source));
        }
        for evidence in &source.evidence {
            if Path::new(evidence).is_absolute()
                || evidence.contains("..")
                || !root.join(evidence).is_file()
            {
                errors.push(format!("missing evidence {evidence}"));
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests;

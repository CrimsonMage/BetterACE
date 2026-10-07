use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::{CrateType, MetadataCommand, TargetKind};
use serde::Deserialize;

#[derive(Deserialize)]
struct Policy {
    soft_line_limit: usize,
    hard_line_limit: usize,
    crates: Vec<CratePolicy>,
}

#[derive(Deserialize)]
struct CratePolicy {
    name: String,
    path: String,
    layer: String,
    status: String,
    allowed_dependencies: Vec<String>,
    allowed_external_dependencies: Vec<String>,
}

#[derive(Deserialize)]
struct SplitPlans {
    plans: Vec<SplitPlan>,
}

#[derive(Deserialize)]
struct SplitPlan {
    path: String,
    owner: String,
    destinations: Vec<String>,
    rationale: String,
}

pub fn check_workspace() -> Result<String, Vec<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    check(&root).map_err(|e| vec![format!("architecture check could not run: {e}")])?
}

fn check(root: &Path) -> Result<Result<String, Vec<String>>, Box<dyn std::error::Error>> {
    let canonical_root = root.canonicalize()?;
    let root = canonical_root.as_path();
    let policy: Policy = toml::from_str(&fs::read_to_string(root.join("architecture.toml"))?)?;
    let splits: SplitPlans =
        toml::from_str(&fs::read_to_string(root.join("docs/split-plans.toml"))?)?;
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()?;
    let mut errors = Vec::new();
    let workspace_manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    errors.extend(crate::unsafe_policy::workspace_lints(&workspace_manifest));
    if policy.soft_line_limit != 1000 || policy.hard_line_limit != 1500 {
        errors.push("line thresholds MUST remain 1000 / 1500".into());
    }
    let policies: BTreeMap<_, _> = policy.crates.iter().map(|p| (p.name.as_str(), p)).collect();
    if policies.len() != policy.crates.len() {
        errors.push("duplicate crate ownership entries".into());
    }
    let names: BTreeSet<_> = metadata.packages.iter().map(|p| p.name.as_str()).collect();
    let mut source_count = 0;
    let mut owned_sources = BTreeSet::new();
    let mut biggest = (0, String::new());
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    for package in &metadata.packages {
        let Some(cp) = policies.get(package.name.as_str()) else {
            errors.push(format!("{} has no ownership policy", package.name));
            continue;
        };
        if !["scaffolded", "foundation", "implemented"].contains(&cp.status.as_str()) {
            errors.push(format!("{} has unknown implementation status", cp.name));
        }
        if ![
            "foundation",
            "assets",
            "content",
            "network",
            "simulation",
            "gameplay",
            "storage",
            "application",
            "adapter",
            "verification",
        ]
        .contains(&cp.layer.as_str())
        {
            errors.push(format!("{} has unknown layer {}", cp.name, cp.layer));
        }
        let manifest = root.join(&cp.path).join("Cargo.toml").canonicalize()?;
        if manifest != package.manifest_path.as_std_path().canonicalize()? {
            errors.push(format!("{} ownership path does not match Cargo", cp.name));
        }
        let owner_root = root.join(&cp.path).canonicalize()?;
        let package_manifest: toml::Value = toml::from_str(&fs::read_to_string(&manifest)?)?;
        errors.extend(crate::unsafe_policy::package_lints(
            &cp.name,
            &package_manifest,
            &workspace_manifest,
        ));
        for target in &package.targets {
            let source = target.src_path.as_std_path();
            if source.extension().is_none_or(|extension| extension != "rs") {
                errors.push(format!(
                    "{}: Cargo target sources MUST be inspectable .rs files",
                    source.display()
                ));
            }
            match source.canonicalize() {
                Ok(canonical) if canonical.starts_with(&owner_root) => {}
                _ => errors.push(format!(
                    "{}: Cargo target MUST remain inside owning crate {}",
                    source.display(),
                    cp.name
                )),
            }
        }
        let mut edges = Vec::new();
        for dependency in &package.dependencies {
            if !names.contains(dependency.name.as_str()) {
                if !cp.allowed_external_dependencies.contains(&dependency.name) {
                    errors.push(format!(
                        "{} -> external {} is not allowed",
                        cp.name, dependency.name
                    ));
                }
                if ["simulation", "gameplay"].contains(&cp.layer.as_str())
                    && [
                        "tokio",
                        "sqlx",
                        "reqwest",
                        "mio",
                        "postgres",
                        "mysql",
                        "mysql_async",
                        "rusqlite",
                        "async-std",
                    ]
                    .contains(&dependency.name.as_str())
                {
                    errors.push(format!(
                        "{} MUST NOT depend on I/O runtime {}",
                        cp.name, dependency.name
                    ));
                }
                continue;
            }
            edges.push(dependency.name.clone());
            if !cp.allowed_dependencies.contains(&dependency.name) {
                errors.push(format!("{} -> {} is not allowed", cp.name, dependency.name));
            }
            if cp.layer == "gameplay"
                && policies
                    .get(dependency.name.as_str())
                    .is_some_and(|p| p.layer == "gameplay")
            {
                errors.push(format!(
                    "gameplay sibling dependency: {} -> {}",
                    cp.name, dependency.name
                ));
            }
        }
        if cp.layer == "adapter"
            && package
                .dependencies
                .iter()
                .any(|d| !cp.allowed_dependencies.contains(&d.name))
        {
            errors.push(format!(
                "{} executable may only depend on its application crate",
                cp.name
            ));
        }
        graph.insert(cp.name.clone(), edges);
        let mut sources = Vec::new();
        rust_files(root, &owner_root, &mut sources)?;
        for source in sources {
            owned_sources.insert(source.clone());
            source_count += 1;
            let text = fs::read_to_string(&source)?;
            let relative = source
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let lines = text.lines().count();
            errors.extend(crate::unsafe_policy::source(&relative, &text));
            if lines > biggest.0 {
                biggest = (lines, relative.clone());
            }
            if lines > policy.hard_line_limit {
                errors.push(format!(
                    "{relative}: {lines} lines exceeds 1500; MUST split"
                ));
            }
            if lines >= policy.soft_line_limit
                && !splits.plans.iter().any(|p| {
                    p.path == relative
                        && p.owner == cp.name
                        && valid_destinations(p, cp)
                        && !p.rationale.trim().is_empty()
                })
            {
                errors.push(format!(
                    "{relative}: {lines} lines requires a concrete split plan"
                ));
            }
            let filename = source.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let target = package.targets.iter().find(|target| {
                target.src_path.as_std_path().canonicalize().ok().as_deref()
                    == Some(source.as_path())
            });
            let explicit_main = syn::parse_file(&text).is_ok_and(|file| {
                file.items.iter().any(
                    |item| matches!(item, syn::Item::Fn(function) if function.sig.ident == "main"),
                )
            });
            // Tests/benches normally receive a generated harness main and retain
            // named test implementations. A hand-written main is still thin.
            let binary = filename == "main.rs"
                || target.is_some_and(|target| {
                    target.is_kind(TargetKind::Bin)
                        || target.is_kind(TargetKind::CustomBuild)
                        || (target.is_kind(TargetKind::Example)
                            && target.crate_types.contains(&CrateType::Bin))
                        || ((target.is_kind(TargetKind::Test) || target.is_kind(TargetKind::Bench))
                            && explicit_main)
                });
            let library = filename == "lib.rs"
                || target.is_some_and(|target| {
                    target.crate_types.iter().any(|kind| {
                        matches!(
                            kind,
                            CrateType::Lib
                                | CrateType::RLib
                                | CrateType::DyLib
                                | CrateType::CDyLib
                                | CrateType::StaticLib
                                | CrateType::ProcMacro
                        )
                    })
                });
            if (binary || library)
                && let Err(message) = crate::roots::check_root(&text, binary)
            {
                errors.push(format!("{relative}: {message}"));
            }
            if cp.layer == "adapter" && filename != "main.rs" && filename != "lib.rs" {
                errors.push(format!(
                    "{relative}: executable implementation MUST move to its application crate"
                ));
            }
            match syn::parse_file(&text) {
                Ok(parsed) => {
                    let mut paths = ModulePaths::new(
                        &source,
                        &owner_root,
                        target.is_some() || binary || library,
                    );
                    syn::visit::Visit::visit_file(&mut paths, &parsed);
                    for message in paths.errors {
                        errors.push(format!("{relative}: {message}"));
                    }
                    let mut visitor = IncludeCheck(false);
                    syn::visit::Visit::visit_file(&mut visitor, &parsed);
                    if visitor.0 {
                        errors.push(format!(
                            "{relative}: include! cannot bypass Rust file boundaries"
                        ));
                    }
                }
                Err(error) => errors.push(format!(
                    "{relative}: Rust syntax could not be inspected: {error}"
                )),
            }
        }
    }
    let mut all_sources = Vec::new();
    rust_files(root, root, &mut all_sources)?;
    for source in all_sources {
        if !owned_sources.contains(&source) {
            errors.push(format!(
                "{}: Rust source MUST belong to an inventoried crate",
                source.display()
            ));
        }
    }
    for name in policies.keys() {
        if !names.contains(name) {
            errors.push(format!("policy crate {name} is absent from workspace"));
        }
    }
    for name in graph.keys() {
        if reaches(name, name, &graph, &mut BTreeSet::new()) {
            errors.push(format!("dependency cycle involving {name}"));
        }
    }
    if errors.is_empty() {
        Ok(Ok(format!(
            "Architecture passed: {} crates, {source_count} Rust files; largest {} lines ({}).",
            names.len(),
            biggest.0,
            biggest.1
        )))
    } else {
        Ok(Err(errors))
    }
}

fn valid_destinations(plan: &SplitPlan, owner: &CratePolicy) -> bool {
    let distinct: BTreeSet<_> = plan.destinations.iter().collect();
    distinct.len() >= 2
        && distinct.len() == plan.destinations.len()
        && distinct.iter().all(|value| {
            let path = Path::new(value);
            !path.is_absolute()
                && path.starts_with(&owner.path)
                && path.extension().is_some_and(|s| s == "rs")
                && value.as_str() != plan.path
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
        })
}

fn reaches(
    start: &str,
    current: &str,
    graph: &BTreeMap<String, Vec<String>>,
    seen: &mut BTreeSet<String>,
) -> bool {
    if !seen.insert(current.into()) {
        return false;
    }
    graph.get(current).is_some_and(|edges| {
        edges
            .iter()
            .any(|next| next == start || reaches(start, next, graph, seen))
    })
}

fn rust_files(root: &Path, path: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            if entry.path().is_dir() || entry.path().extension().is_some_and(|s| s == "rs") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "{}: source/directory symlinks MUST NOT bypass inspection",
                        entry.path().display()
                    ),
                ));
            }
            continue;
        }
        if kind.is_dir() {
            if path != root
                || !["target", ".git", ".reference", ".local"]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
            {
                rust_files(root, &entry.path(), output)?;
            }
        } else if entry.path().extension().is_some_and(|s| s == "rs") {
            output.push(entry.path());
        }
    }
    Ok(())
}

struct IncludeCheck(bool);
impl<'ast> syn::visit::Visit<'ast> for IncludeCheck {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if imports_include(&node.tree) {
            self.0 = true;
        }
        syn::visit::visit_item_use(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "include")
            || contains_include(node.tokens.clone())
        {
            self.0 = true;
        }
        syn::visit::visit_macro(self, node);
    }
}

fn imports_include(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Path(path) => imports_include(&path.tree),
        syn::UseTree::Name(name) => name.ident == "include",
        syn::UseTree::Rename(rename) => rename.ident == "include",
        syn::UseTree::Group(group) => group.items.iter().any(imports_include),
        syn::UseTree::Glob(_) => false,
    }
}

// Explicit module paths are checked in every source file, including inactive
// cfg branches. Normal external modules are discovered by the source walker.
// Conditional paths must all remain inspectable; a configuration must never
// activate source outside the crate's inventory later.
struct ModulePaths<'a> {
    owner: &'a Path,
    explicit_base: PathBuf,
    inline_base: PathBuf,
    errors: Vec<String>,
}
impl<'a> ModulePaths<'a> {
    fn new(source: &Path, owner: &'a Path, target_root: bool) -> Self {
        let directory = source.parent().unwrap_or(owner).to_path_buf();
        let inline_base = if target_root || source.file_name().is_some_and(|name| name == "mod.rs")
        {
            directory.clone()
        } else {
            directory.join(source.file_stem().unwrap_or_default())
        };
        Self {
            owner,
            explicit_base: directory,
            inline_base,
            errors: Vec::new(),
        }
    }
    fn inspect_meta(&mut self, meta: &syn::Meta, inline: bool) {
        if meta.path().is_ident("cfg_attr") {
            let syn::Meta::List(list) = meta else {
                self.errors
                    .push("invalid conditional module attributes".into());
                return;
            };
            use syn::parse::Parser;
            let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
            match parser.parse2(list.tokens.clone()) {
                Ok(attributes) if attributes.len() >= 2 => {
                    for nested in attributes.iter().skip(1) {
                        self.inspect_meta(nested, inline);
                    }
                }
                _ => self
                    .errors
                    .push("invalid conditional module attributes".into()),
            }
        } else if meta.path().is_ident("path") {
            let syn::Meta::NameValue(value) = meta else {
                self.errors
                    .push("module path MUST be a string literal".into());
                return;
            };
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) = &value.value
            else {
                self.errors
                    .push("module path MUST be a string literal".into());
                return;
            };
            // Directory remapping on inline modules would change the resolution
            // base for all descendants. Keep this policy explicit and simple.
            if inline {
                self.errors.push(
                    "inline module path remapping is not permitted; use named .rs modules".into(),
                );
                return;
            }
            let path = PathBuf::from(value.value());
            if path.is_absolute()
                || path.extension().is_none_or(|extension| extension != "rs")
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                self.errors
                    .push("module paths MUST name local .rs files without parent traversal".into());
                return;
            }
            match self.explicit_base.join(&path).canonicalize() {
                Ok(resolved) if resolved.starts_with(self.owner) && resolved.is_file() => {}
                _ => self.errors.push(format!(
                    "module path {} MUST resolve inside owning crate",
                    path.display()
                )),
            }
        }
    }
}
impl<'ast> syn::visit::Visit<'ast> for ModulePaths<'_> {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        for attribute in &module.attrs {
            self.inspect_meta(&attribute.meta, module.content.is_some());
        }
        if let Some((_, items)) = &module.content {
            let previous_explicit = self.explicit_base.clone();
            let previous_inline = self.inline_base.clone();
            self.inline_base.push(module.ident.to_string());
            self.explicit_base = self.inline_base.clone();
            for item in items {
                syn::visit::Visit::visit_item(self, item);
            }
            self.explicit_base = previous_explicit;
            self.inline_base = previous_inline;
        }
    }
}

fn contains_include(tokens: proc_macro2::TokenStream) -> bool {
    let mut include = false;
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Group(group) if contains_include(group.stream()) => {
                return true;
            }
            proc_macro2::TokenTree::Ident(ident) => include = ident == "include",
            proc_macro2::TokenTree::Punct(punct) if include && punct.as_char() == '!' => {
                return true;
            }
            _ => include = false,
        }
    }
    false
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;

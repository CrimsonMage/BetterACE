use super::check;
use std::fs;
use std::path::Path;

fn workspace() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/sample/src")).unwrap();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver='3'\nmembers=['crates/sample']\n[workspace.lints.rust]\nunsafe_code='forbid'\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/sample/Cargo.toml"),
        "[package]\nname='sample'\nversion='0.1.0'\nedition='2024'\n[lints]\nworkspace=true\n",
    )
    .unwrap();
    fs::write(root.join("crates/sample/src/lib.rs"), "//! Empty root\n").unwrap();
    fs::write(root.join("docs/split-plans.toml"), "plans=[]\n").unwrap();
    fs::write(root.join("architecture.toml"), "soft_line_limit=1000\nhard_line_limit=1500\n[[crates]]\nname='sample'\npath='crates/sample'\nlayer='application'\nstatus='scaffolded'\nallowed_dependencies=[]\nallowed_external_dependencies=[]\n").unwrap();
    temp
}

fn rejected(root: &Path, needle: &str) {
    let errors = check(root).unwrap().unwrap_err();
    assert!(errors.iter().any(|e| e.contains(needle)), "{errors:?}");
}

#[test]
fn normal_workspace_and_line_thresholds() {
    let temp = workspace();
    let root = temp.path();
    assert!(check(root).unwrap().is_ok());
    let file = root.join("crates/sample/src/large.rs");
    fs::write(&file, "// physical line\n".repeat(999)).unwrap();
    assert!(check(root).unwrap().is_ok());
    fs::write(&file, "// physical line\n".repeat(1000)).unwrap();
    rejected(root, "split plan");
    fs::write(root.join("docs/split-plans.toml"), "[[plans]]\npath='crates/sample/src/large.rs'\nowner='sample'\ndestinations=['','']\nrationale='bogus'\n").unwrap();
    rejected(root, "split plan");
    fs::write(root.join("docs/split-plans.toml"), "[[plans]]\npath='crates/sample/src/large.rs'\nowner='sample'\ndestinations=['crates/sample/src/one.rs','crates/sample/src/two.rs']\nrationale='split by responsibility'\n").unwrap();
    fs::write(&file, "// physical line\n".repeat(1500)).unwrap();
    assert!(check(root).unwrap().is_ok());
    fs::write(&file, "// physical line\n".repeat(1501)).unwrap();
    rejected(root, "exceeds 1500");
}

#[test]
fn conventional_and_custom_binary_roots_cannot_hide_implementation() {
    let temp = workspace();
    let root = temp.path();
    fs::create_dir(root.join("crates/sample/src/bin")).unwrap();
    fs::write(
        root.join("crates/sample/src/bin/hidden.rs"),
        "fn main() { let x=1; println!(\"{x}\"); }",
    )
    .unwrap();
    rejected(root, "main MUST");
    fs::remove_file(root.join("crates/sample/src/bin/hidden.rs")).unwrap();
    let manifest = root.join("crates/sample/Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[[bin]]\nname='hidden'\npath='src/custom.rs'\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        root.join("crates/sample/src/custom.rs"),
        "fn main() { let x=1; println!(\"{x}\"); }",
    )
    .unwrap();
    rejected(root, "main MUST");
}

#[test]
fn includes_and_macro_wrapped_includes_are_rejected() {
    let temp = workspace();
    let root = temp.path();
    for source in [
        "std::include!(\"hidden.txt\");",
        "macro_rules! load { () => { include!(\"hidden.txt\"); } }",
    ] {
        fs::write(root.join("crates/sample/src/hidden.rs"), source).unwrap();
        rejected(root, "include!");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_sources_are_rejected() {
    let temp = workspace();
    let root = temp.path();
    std::os::unix::fs::symlink("lib.rs", root.join("crates/sample/src/alias.rs")).unwrap();
    assert!(check(root).is_err());
}

#[test]
fn non_root_module_paths_and_conditional_aliases_cannot_escape_inspection() {
    let temp = workspace();
    let root = temp.path();
    let source = root.join("crates/sample/src/module.rs");
    fs::write(root.join("crates/sample/src/child.rs"), "//! owned child\n").unwrap();
    fs::write(&source, "#[path=\"child.rs\"] mod child;").unwrap();
    assert!(check(root).unwrap().is_ok());
    fs::write(&source, "#[cfg_attr(any(), path=\"child.rs\")] mod child;").unwrap();
    assert!(check(root).unwrap().is_ok());
    for text in [
        "#[path=\"child.txt\"] mod child;",
        "#[path=\"../child.rs\"] mod child;",
        "#[path=\"/tmp/child.rs\"] mod child;",
        "#[path=\"missing.rs\"] mod child;",
        "#[cfg_attr(any(), path=\"child.txt\")] mod child;",
        "#[cfg_attr(any(), cfg_attr(any(), path=\"../child.rs\"))] mod child;",
        "#[path=\"remapped\"] mod inner { mod child; }",
    ] {
        fs::write(&source, text).unwrap();
        rejected(root, "module path");
    }
}

#[test]
fn module_path_bases_inside_inline_modules_follow_rust_layout() {
    let temp = workspace();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/sample/src/module/nested")).unwrap();
    fs::write(
        root.join("crates/sample/src/module/nested/child.rs"),
        "//! nested child\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/sample/src/module.rs"),
        "mod nested { #[path=\"child.rs\"] mod child; }",
    )
    .unwrap();
    assert!(check(root).unwrap().is_ok());
    fs::write(
        root.join("crates/sample/src/module.rs"),
        "mod nested { #[path=\"child.txt\"] mod child; }",
    )
    .unwrap();
    rejected(root, "module paths");
}

#[test]
fn include_import_aliases_cannot_hide_macro_expansion() {
    let temp = workspace();
    let root = temp.path();
    for text in [
        "use std::include as payload; payload!(\"large.txt\");",
        "use std::{include as payload}; payload!(\"large.txt\");",
        "pub use core::include as payload;",
    ] {
        fs::write(root.join("crates/sample/src/module.rs"), text).unwrap();
        rejected(root, "include!");
    }
}

fn external_dependency(root: &Path, name: &str) {
    let directory = root.join(".reference").join(name);
    fs::create_dir_all(directory.join("src")).unwrap();
    fs::write(directory.join("src/lib.rs"), "//! external fixture\n").unwrap();
    fs::write(
        directory.join("Cargo.toml"),
        format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n"),
    )
    .unwrap();
    let workspace_manifest = root.join("Cargo.toml");
    fs::write(
        &workspace_manifest,
        fs::read_to_string(&workspace_manifest).unwrap().replace(
            "[workspace]\n",
            &format!("[workspace]\nexclude=['.reference/{name}']\n"),
        ),
    )
    .unwrap();
    let manifest = root.join("crates/sample/Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[dependencies]\nrenamed={{package='{name}',path='../../.reference/{name}'}}\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
}

#[test]
fn external_allowlists_check_real_package_names_not_aliases() {
    let temp = workspace();
    let root = temp.path();
    external_dependency(root, "fixture-external");
    rejected(root, "external fixture-external is not allowed");
    let policy = root.join("architecture.toml");
    fs::write(
        &policy,
        fs::read_to_string(&policy).unwrap().replace(
            "allowed_external_dependencies=[]",
            "allowed_external_dependencies=['fixture-external']",
        ),
    )
    .unwrap();
    assert!(check(root).unwrap().is_ok());
}

#[test]
fn simulation_io_dependencies_and_unknown_layers_are_rejected() {
    let temp = workspace();
    let root = temp.path();
    external_dependency(root, "sqlx");
    let policy = root.join("architecture.toml");
    let original = fs::read_to_string(&policy).unwrap();
    fs::write(
        &policy,
        original
            .replace("layer='application'", "layer='simulation'")
            .replace(
                "allowed_external_dependencies=[]",
                "allowed_external_dependencies=['sqlx']",
            ),
    )
    .unwrap();
    rejected(root, "MUST NOT depend on I/O runtime sqlx");
    fs::write(
        &policy,
        original.replace("layer='application'", "layer='adaptor'"),
    )
    .unwrap();
    rejected(root, "unknown layer adaptor");
}

#[test]
fn custom_target_extensions_and_outside_paths_are_rejected() {
    let temp = workspace();
    let root = temp.path();
    let manifest = root.join("crates/sample/Cargo.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(
        root.join("crates/sample/src/payload.txt"),
        "pub fn large_implementation() {}\n",
    )
    .unwrap();
    fs::write(
        &manifest,
        format!("{original}\n[lib]\npath='src/payload.txt'\n"),
    )
    .unwrap();
    rejected(root, "Cargo target sources MUST be inspectable .rs");
    fs::write(root.join("crates/outside.rs"), "//! outside\n").unwrap();
    fs::write(
        &manifest,
        format!("{original}\n[lib]\npath='../outside.rs'\n"),
    )
    .unwrap();
    rejected(root, "Cargo target MUST remain inside owning crate");
}

#[test]
fn inactive_source_syntax_errors_are_not_silently_skipped() {
    let temp = workspace();
    let root = temp.path();
    fs::write(root.join("crates/sample/src/inactive.rs"), "fn broken(").unwrap();
    rejected(root, "Rust syntax could not be inspected");
}

#[test]
fn directory_binaries_and_canonical_target_aliases_are_enforced() {
    let temp = workspace();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/sample/src/bin/tool")).unwrap();
    fs::write(
        root.join("crates/sample/src/bin/tool/main.rs"),
        "fn main() { let x=1; println!(\"{x}\"); }",
    )
    .unwrap();
    rejected(root, "main MUST");
    fs::remove_dir_all(root.join("crates/sample/src/bin")).unwrap();
    let manifest = root.join("crates/sample/Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[[bin]]\nname='alias'\npath='src/../src/custom.rs'\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        root.join("crates/sample/src/custom.rs"),
        "fn main() { let x=1; println!(\"{x}\"); }",
    )
    .unwrap();
    rejected(root, "main MUST");
}

#[test]
fn example_binaries_libraries_and_build_scripts_have_thin_roots() {
    let temp = workspace();
    let root = temp.path();
    fs::create_dir(root.join("crates/sample/examples")).unwrap();
    let example = root.join("crates/sample/examples/demo.rs");
    fs::write(&example, "fn main() { let x=1; println!(\"{x}\"); }").unwrap();
    rejected(root, "main MUST");
    fs::write(&example, "fn main() { sample::entry::run() }").unwrap();
    assert!(check(root).unwrap().is_ok());
    fs::remove_file(&example).unwrap();
    let manifest = root.join("crates/sample/Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[[example]]\nname='api'\npath='examples/api.rs'\ncrate-type=['lib']\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        root.join("crates/sample/examples/api.rs"),
        "pub fn hidden_implementation() {}",
    )
    .unwrap();
    rejected(root, "crate roots MUST contain");
    fs::write(
        root.join("crates/sample/examples/api.rs"),
        "//! thin library example\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/sample/build.rs"),
        "fn main() { println!(\"cargo:rustc-cfg=generated\"); }",
    )
    .unwrap();
    rejected(root, "main MUST");
}

#[test]
fn harness_targets_keep_tests_but_validate_explicit_main_and_module_paths() {
    let temp = workspace();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/sample/tests/nested")).unwrap();
    fs::write(
        root.join("crates/sample/tests/nested/child.rs"),
        "//! owned test helper\n",
    )
    .unwrap();
    let test = root.join("crates/sample/tests/contracts.rs");
    fs::write(
        &test,
        "mod nested { #[path=\"child.rs\"] mod child; } #[test] fn works() { assert_eq!(1,1); }",
    )
    .unwrap();
    assert!(check(root).unwrap().is_ok());
    fs::write(&test, "fn main() { let x=1; println!(\"{x}\"); }").unwrap();
    rejected(root, "main MUST");
}

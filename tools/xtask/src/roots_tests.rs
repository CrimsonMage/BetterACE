use super::check_root;

#[test]
fn roots_accept_declarations_and_single_delegate() {
    assert!(check_root("//! API\npub mod ids; pub use ids::Id;", false).is_ok());
    assert!(
        check_root(
            "fn main() -> std::process::ExitCode { bace_runtime::entry::run() }",
            true
        )
        .is_ok()
    );
}

#[test]
fn roots_reject_implementation_and_macro_bypasses() {
    for source in [
        "pub fn handler() {}",
        "mod tests { #[test] fn test() {} }",
        "pub struct State;",
        "const LIMIT: usize = 1;",
        "include!(\"implementation.rs\");",
    ] {
        assert!(check_root(source, false).is_err(), "{source}");
    }
    for source in [
        "fn main() { let config = load(); run(config); }",
        "fn main() { run(load()) }",
        "async fn main() { run() }",
        "fn main() { if true { run() } }",
    ] {
        assert!(check_root(source, true).is_err(), "{source}");
    }
}

#[test]
fn roots_reject_procedural_attributes_and_path_aliases() {
    for source in [
        "#[make_implementation] mod module;",
        "#[make_implementation] use core::fmt;",
        "#[make_implementation] extern crate core;",
        "#![make_implementation]\nmod module;",
        "#[cfg_attr(any(), make_implementation)] mod module;",
        "#[path=\"module.txt\"] mod module;",
        "#[path=\"../module.rs\"] mod module;",
        "#[path=\"/tmp/module.rs\"] mod module;",
    ] {
        assert!(check_root(source, false).is_err(), "{source}");
    }
    assert!(check_root("#[cfg(test)] #[path=\"tests.rs\"] mod tests;", false).is_ok());
    assert!(check_root("#[make_implementation] fn main() { entry::run() }", true).is_err());
    assert!(check_root("fn main() { entry::run::<{1+1}>() }", true).is_err());
}

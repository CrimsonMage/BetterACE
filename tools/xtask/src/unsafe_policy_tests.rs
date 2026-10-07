use super::*;

fn manifest(text: &str) -> toml::Value {
    toml::from_str(text).unwrap()
}

#[test]
fn workspace_forbid_and_single_explicit_manifest_exception() {
    let workspace = manifest(
        "[workspace.lints.rust]\nunsafe_code='forbid'\n[workspace.lints.clippy]\ntodo='deny'\n",
    );
    assert!(workspace_lints(&workspace).is_empty());
    assert!(!workspace_lints(&manifest("[workspace.lints.rust]\nunsafe_code='deny'\n")).is_empty());
    assert!(package_lints("sample", &manifest("[lints]\nworkspace=true"), &workspace).is_empty());
    assert!(
        !package_lints(
            "sample",
            &manifest("[lints.rust]\nunsafe_code='deny'"),
            &workspace
        )
        .is_empty()
    );
    let codec = manifest("[lints.rust]\nunsafe_code='deny'\n[lints.clippy]\ntodo='deny'");
    assert!(package_lints(CODEC, &codec, &workspace).is_empty());
    assert!(
        !package_lints(
            CODEC,
            &manifest("[lints.rust]\nunsafe_code='allow'"),
            &workspace
        )
        .is_empty()
    );
    assert!(
        !package_lints(
            CODEC,
            &manifest("[lints.rust]\nunsafe_code='deny'"),
            &workspace
        )
        .is_empty()
    );
}

#[test]
fn rejects_unsafe_blocks_signatures_impls_externs_and_macro_bodies() {
    for text in [
        "fn f() { unsafe { something() } }",
        "unsafe fn f() {}",
        "unsafe impl Send for A {}",
        "unsafe extern \"C\" { fn x(); }",
        "macro_rules! hide { () => { unsafe { x() } } }",
        "#[cfg(any())] fn f() { unsafe { x() } }",
    ] {
        assert!(
            !source("crates/other/src/code.rs", text).is_empty(),
            "{text}"
        );
    }
    assert!(
        source(
            "crates/other/src/code.rs",
            "// unsafe\nconst NOTE: &str = \"unsafe {x()}\";"
        )
        .is_empty()
    );
}

#[test]
fn conditional_lint_lowering_cannot_expand_exception() {
    for text in [
        "#![allow(unsafe_code)]",
        "#![warn(unsafe_code)]",
        "#![expect(unsafe_code)]",
        "#![cfg_attr(any(), allow(unsafe_code))]",
        "#[cfg_attr(any(), cfg_attr(any(), allow(unsafe_code)))] fn f() {}",
    ] {
        assert!(
            !source("crates/storage/bace-storage-codec/src/other.rs", text).is_empty(),
            "{text}"
        );
    }
    assert!(source("crates/other/src/code.rs", "#![forbid(unsafe_code)]").is_empty());
}

#[test]
fn only_exact_reviewed_boundary_is_allowed() {
    let approved = format!("//! Boundary docs\n// SAFETY: contract\n{APPROVED_BOUNDARY}");
    assert!(source(BOUNDARY, &approved).is_empty());
    assert!(!source("crates/other/src/mapping.rs", &approved).is_empty());
    assert!(!source(BOUNDARY, APPROVED_BOUNDARY).is_empty());
    for altered in [
        approved.replace("map(file)", "map_mut(file)"),
        approved.replace("pub(crate)", "pub"),
        format!("{approved}\nfn another() {{ unsafe {{ extra() }} }}"),
        approved.replace("open_immutable", "anything"),
    ] {
        assert!(!source(BOUNDARY, &altered).is_empty());
    }
}

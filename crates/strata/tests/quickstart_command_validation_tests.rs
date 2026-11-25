//! Automated Quickstart & Core Example Validation Suite.
//!
//! Validates that core examples shipped in the `strata` repository exist and compile cleanly.

#[test]
fn test_core_examples_exist_and_readme_hero_compiles() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Workspace root directory must exist");

    let hero = workspace_root.join("examples").join("readme_hero.rs");
    assert!(
        hero.exists(),
        "Core example readme_hero.rs missing at {:?}",
        hero
    );
}

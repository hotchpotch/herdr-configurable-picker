//! Guards the version fields that must be bumped in two places:
//! Cargo.toml (crate version) and herdr-plugin.toml (plugin manifest).

use std::path::Path;

fn version_of(path: &Path) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let doc: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    match &doc {
        toml::Value::Table(t) if t.contains_key("package") => doc["package"]["version"]
            .as_str()
            .expect("Cargo.toml package.version must be a string")
            .to_string(),
        _ => doc["version"]
            .as_str()
            .expect("herdr-plugin.toml version must be a string")
            .to_string(),
    }
}

#[test]
fn crate_and_manifest_versions_match() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crate_version = version_of(&root.join("Cargo.toml"));
    let manifest_version = version_of(&root.join("herdr-plugin.toml"));
    assert_eq!(
        crate_version, manifest_version,
        "Cargo.toml and herdr-plugin.toml versions must be bumped together"
    );
}

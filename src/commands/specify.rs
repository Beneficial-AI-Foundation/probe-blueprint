use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use super::stubify;

/// Stub entry from stubs.json (only fields we need)
#[derive(Debug, Deserialize)]
struct Stub {
    #[serde(rename = "spec-ok")]
    spec_ok: bool,
}

/// Spec entry for specs.json
#[derive(Debug, Serialize)]
struct Spec {
    specified: bool,
}

/// Extract function specifications
pub fn run(project_path: &str, output: &str, regenerate_stubs: bool) -> Result<(), Box<dyn Error>> {
    let project_path = Path::new(project_path);
    let verilib_dir = project_path.join(".verilib");
    let stubs_path = verilib_dir.join("stubs.json");

    // Check if stubs.json exists, generate if needed
    if regenerate_stubs || !stubs_path.exists() {
        if regenerate_stubs {
            eprintln!("Regenerating stubs.json...");
        } else {
            eprintln!("stubs.json not found, running stubify...");
        }

        stubify::run(
            project_path.to_str().ok_or("Invalid project path")?,
            stubs_path.to_str().ok_or("Invalid stubs path")?,
        )?;
    }

    // Read stubs.json
    let stubs_content = fs::read_to_string(&stubs_path)?;
    let stubs: HashMap<String, Stub> = serde_json::from_str(&stubs_content)?;

    // Transform stubs into specs
    let mut specs: HashMap<String, Spec> = HashMap::new();

    for (stub_name, stub) in stubs {
        // Extract the label (last part after "/") from the stub name
        let label = stub_name
            .split('/')
            .next_back()
            .unwrap_or(&stub_name)
            .to_string();

        specs.insert(
            label,
            Spec {
                specified: stub.spec_ok,
            },
        );
    }

    // Write output
    let output_path = Path::new(output);
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let json = serde_json::to_string_pretty(&specs)?;
    fs::write(output_path, json)?;

    eprintln!("Wrote {} specs to {}", specs.len(), output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spec_serialization() {
        let spec = Spec { specified: true };

        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(json, r#"{"specified":true}"#);
    }

    #[test]
    fn test_spec_serialization_false() {
        let spec = Spec { specified: false };

        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(json, r#"{"specified":false}"#);
    }

    #[test]
    fn test_stub_deserialization() {
        let json = r#"{
            "stub-type": "theorem",
            "stub-path": "chapter/theorems.tex",
            "stub-spec": { "lines-start": 10, "lines-end": 20 },
            "labels": ["thm1"],
            "spec-ok": true,
            "mathlib-ok": false,
            "not-ready": false,
            "spec-dependencies": []
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert!(stub.spec_ok);
    }

    #[test]
    fn test_stub_deserialization_spec_not_ok() {
        let json = r#"{
            "stub-type": "theorem",
            "stub-path": "chapter/theorems.tex",
            "stub-spec": { "lines-start": 10, "lines-end": 20 },
            "labels": ["thm1"],
            "spec-ok": false,
            "mathlib-ok": false,
            "not-ready": false,
            "spec-dependencies": []
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert!(!stub.spec_ok);
    }
}

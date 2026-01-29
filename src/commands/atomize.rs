use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use super::stubify;

/// Line range for source locations (input format from stubs.json)
#[derive(Debug, Deserialize)]
struct LineRange {
    #[serde(rename = "lines-start")]
    lines_start: usize,
    #[serde(rename = "lines-end")]
    lines_end: usize,
}

/// Stub entry from stubs.json
#[derive(Debug, Deserialize)]
struct Stub {
    #[serde(rename = "stub-path")]
    stub_path: String,
    #[serde(rename = "stub-spec")]
    stub_spec: LineRange,
    #[serde(rename = "spec-dependencies", default)]
    spec_dependencies: Vec<String>,
    #[serde(rename = "proof-dependencies")]
    proof_dependencies: Option<Vec<String>>,
}

/// Output line range format for atoms.json
#[derive(Debug, Serialize)]
struct AtomLineRange {
    #[serde(rename = "lines-start")]
    lines_start: usize,
    #[serde(rename = "lines-end")]
    lines_end: usize,
}

/// Atom entry for atoms.json
#[derive(Debug, Serialize)]
struct Atom {
    #[serde(rename = "display-name")]
    display_name: String,
    dependencies: Vec<String>,
    #[serde(rename = "stub-path")]
    stub_path: String,
    #[serde(rename = "stub-text")]
    stub_text: AtomLineRange,
}

/// Generate call graph atoms with line numbers
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

    // Transform stubs into atoms
    let mut atoms: HashMap<String, Atom> = HashMap::new();

    for (stub_name, stub) in stubs {
        // Extract the label (last part after "/") from the stub name
        let label = stub_name
            .split('/')
            .next_back()
            .unwrap_or(&stub_name)
            .to_string();

        // display-name is the label
        let display_name = label.clone();

        // dependencies is the concatenation of spec-dependencies and proof-dependencies
        let mut dependencies = stub.spec_dependencies;
        if let Some(proof_deps) = stub.proof_dependencies {
            dependencies.extend(proof_deps);
        }

        // stub-text is from stub-spec
        let stub_text = AtomLineRange {
            lines_start: stub.stub_spec.lines_start,
            lines_end: stub.stub_spec.lines_end,
        };

        atoms.insert(
            label,
            Atom {
                display_name,
                dependencies,
                stub_path: stub.stub_path,
                stub_text,
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

    let json = serde_json::to_string_pretty(&atoms)?;
    fs::write(output_path, json)?;

    eprintln!("Wrote {} atoms to {}", atoms.len(), output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atom_serialization() {
        let atom = Atom {
            display_name: "my_theorem".to_string(),
            dependencies: vec!["dep1".to_string(), "dep2".to_string()],
            stub_path: "chapter/theorems.tex".to_string(),
            stub_text: AtomLineRange {
                lines_start: 10,
                lines_end: 20,
            },
        };

        let json = serde_json::to_string(&atom).unwrap();
        assert!(json.contains("\"display-name\":\"my_theorem\""));
        assert!(json.contains("\"stub-path\":\"chapter/theorems.tex\""));
        assert!(json.contains("\"stub-text\":{\"lines-start\":10,\"lines-end\":20}"));
    }

    #[test]
    fn test_stub_deserialization() {
        let json = r#"{
            "stub-path": "chapter/theorems.tex",
            "stub-spec": { "lines-start": 10, "lines-end": 20 },
            "labels": ["thm1"],
            "spec-ok": true,
            "mathlib-ok": false,
            "not-ready": false,
            "spec-dependencies": ["dep1", "dep2"],
            "proof-dependencies": ["dep3"]
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert_eq!(stub.stub_path, "chapter/theorems.tex");
        assert_eq!(stub.stub_spec.lines_start, 10);
        assert_eq!(stub.stub_spec.lines_end, 20);
        assert_eq!(stub.spec_dependencies, vec!["dep1", "dep2"]);
        assert_eq!(stub.proof_dependencies, Some(vec!["dep3".to_string()]));
    }

    #[test]
    fn test_stub_deserialization_no_proof_deps() {
        let json = r#"{
            "stub-path": "chapter/theorems.tex",
            "stub-spec": { "lines-start": 10, "lines-end": 20 },
            "labels": ["thm1"],
            "spec-ok": true,
            "mathlib-ok": false,
            "not-ready": false,
            "spec-dependencies": []
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert!(stub.proof_dependencies.is_none());
        assert!(stub.spec_dependencies.is_empty());
    }
}

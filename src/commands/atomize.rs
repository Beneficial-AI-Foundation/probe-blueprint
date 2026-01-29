use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use super::stubify;

/// Stub entry from stubs.json
#[derive(Debug, Deserialize)]
struct Stub {
    label: String,
    #[serde(rename = "code-name")]
    code_name: Option<String>,
    #[serde(rename = "spec-dependencies", default)]
    spec_dependencies: Vec<String>,
    #[serde(rename = "proof-dependencies")]
    proof_dependencies: Option<Vec<String>>,
}

/// Atom entry for atoms.json
#[derive(Debug, Serialize)]
struct Atom {
    #[serde(rename = "display-name")]
    display_name: String,
    dependencies: Vec<String>,
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

    // Build a mapping from stub-name to code-name
    let stub_name_to_code_name: HashMap<String, String> = stubs
        .iter()
        .filter_map(|(stub_name, stub)| {
            stub.code_name
                .as_ref()
                .map(|code_name| (stub_name.clone(), code_name.clone()))
        })
        .collect();

    // Transform stubs into atoms (only stubs with code-name)
    let mut atoms: HashMap<String, Atom> = HashMap::new();

    for stub in stubs.values() {
        // Skip stubs without code-name
        let code_name = match &stub.code_name {
            Some(cn) => cn,
            None => continue,
        };

        // display-name is the label
        let display_name = stub.label.clone();

        // Map dependencies from stub-names to code-names
        let mut dependencies = Vec::new();
        for dep_stub_name in &stub.spec_dependencies {
            if let Some(dep_code_name) = stub_name_to_code_name.get(dep_stub_name) {
                dependencies.push(dep_code_name.clone());
            }
        }
        if let Some(proof_deps) = &stub.proof_dependencies {
            for dep_stub_name in proof_deps {
                if let Some(dep_code_name) = stub_name_to_code_name.get(dep_stub_name) {
                    dependencies.push(dep_code_name.clone());
                }
            }
        }

        atoms.insert(
            code_name.clone(),
            Atom {
                display_name,
                dependencies,
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
            dependencies: vec!["probe:Dep1".to_string(), "probe:Dep2".to_string()],
        };

        let json = serde_json::to_string(&atom).unwrap();
        assert!(json.contains("\"display-name\":\"my_theorem\""));
        assert!(json.contains("\"dependencies\":[\"probe:Dep1\",\"probe:Dep2\"]"));
    }

    #[test]
    fn test_stub_deserialization() {
        let json = r#"{
            "label": "thm1",
            "code-name": "probe:MyTheorem",
            "spec-ok": true,
            "mathlib-ok": false,
            "not-ready": false,
            "spec-dependencies": ["path/dep1", "path/dep2"],
            "proof-dependencies": ["path/dep3"]
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert_eq!(stub.label, "thm1");
        assert_eq!(stub.code_name, Some("probe:MyTheorem".to_string()));
        assert_eq!(stub.spec_dependencies, vec!["path/dep1", "path/dep2"]);
        assert_eq!(stub.proof_dependencies, Some(vec!["path/dep3".to_string()]));
    }

    #[test]
    fn test_stub_deserialization_no_code_name() {
        let json = r#"{
            "label": "thm1",
            "stub-type": "theorem",
            "stub-path": "chapter/theorems.tex",
            "spec-dependencies": ["path/child1", "path/child2"]
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert_eq!(stub.label, "thm1");
        assert!(stub.code_name.is_none());
        assert_eq!(stub.spec_dependencies, vec!["path/child1", "path/child2"]);
    }

    #[test]
    fn test_stub_deserialization_no_proof_deps() {
        let json = r#"{
            "label": "thm1",
            "code-name": "probe:MyTheorem",
            "spec-ok": true,
            "spec-dependencies": []
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert!(stub.proof_dependencies.is_none());
        assert!(stub.spec_dependencies.is_empty());
    }
}

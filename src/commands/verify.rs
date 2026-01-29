use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use super::stubify;

/// Stub entry from stubs.json (only fields we need)
#[derive(Debug, Deserialize)]
struct Stub {
    #[serde(rename = "code-name")]
    code_name: Option<String>,
    #[serde(rename = "proof-ok")]
    proof_ok: Option<bool>,
}

/// Proof entry for proofs.json
#[derive(Debug, Serialize)]
struct Proof {
    verified: bool,
    status: String,
}

/// Extract proof verification status
pub fn run(
    project_path: &str,
    output: &str,
    regenerate_stubs: bool,
    _with_atoms: Option<Option<String>>,
) -> Result<(), Box<dyn Error>> {
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

    // Transform stubs into proofs (only stubs with code-name)
    let mut proofs: HashMap<String, Proof> = HashMap::new();

    for stub in stubs.values() {
        // Skip stubs without code-name
        let code_name = match &stub.code_name {
            Some(cn) => cn,
            None => continue,
        };

        let proof_ok = stub.proof_ok.unwrap_or(false);

        proofs.insert(
            code_name.clone(),
            Proof {
                verified: proof_ok,
                status: if proof_ok {
                    "success".to_string()
                } else {
                    "sorries".to_string()
                },
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

    let json = serde_json::to_string_pretty(&proofs)?;
    fs::write(output_path, json)?;

    eprintln!("Wrote {} proofs to {}", proofs.len(), output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_serialization_success() {
        let proof = Proof {
            verified: true,
            status: "success".to_string(),
        };

        let json = serde_json::to_string(&proof).unwrap();
        assert_eq!(json, r#"{"verified":true,"status":"success"}"#);
    }

    #[test]
    fn test_proof_serialization_sorries() {
        let proof = Proof {
            verified: false,
            status: "sorries".to_string(),
        };

        let json = serde_json::to_string(&proof).unwrap();
        assert_eq!(json, r#"{"verified":false,"status":"sorries"}"#);
    }

    #[test]
    fn test_stub_deserialization_proof_ok() {
        let json = r#"{
            "label": "thm1",
            "code-name": "probe:MyTheorem",
            "spec-ok": true,
            "proof-ok": true
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert_eq!(stub.code_name, Some("probe:MyTheorem".to_string()));
        assert_eq!(stub.proof_ok, Some(true));
    }

    #[test]
    fn test_stub_deserialization_proof_not_ok() {
        let json = r#"{
            "label": "thm1",
            "code-name": "probe:MyTheorem",
            "spec-ok": true,
            "proof-ok": false
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert_eq!(stub.proof_ok, Some(false));
    }

    #[test]
    fn test_stub_deserialization_no_proof_ok() {
        let json = r#"{
            "label": "thm1",
            "code-name": "probe:MyTheorem",
            "spec-ok": true
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert!(stub.proof_ok.is_none());
    }

    #[test]
    fn test_stub_deserialization_no_code_name() {
        let json = r#"{
            "label": "parent_thm",
            "stub-type": "theorem",
            "stub-path": "chapter/theorems.tex",
            "spec-dependencies": ["path/child1", "path/child2"]
        }"#;

        let stub: Stub = serde_json::from_str(json).unwrap();
        assert!(stub.code_name.is_none());
        assert!(stub.proof_ok.is_none());
    }
}

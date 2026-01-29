use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Project-level configuration extracted from LaTeX files
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dochome: Option<String>,
}

/// Default LaTeX environments to look for (from leanblueprint defaults)
const DEFAULT_ENVS: &[&str] = &["definition", "lemma", "proposition", "theorem", "corollary"];

/// Line range for source locations
#[derive(Debug, Serialize, Clone)]
pub struct LineRange {
    #[serde(rename = "lines-start")]
    pub lines_start: usize,
    #[serde(rename = "lines-end")]
    pub lines_end: usize,
}

#[derive(Debug, Serialize)]
pub struct Stub {
    pub label: String,
    #[serde(rename = "stub-type")]
    pub stub_type: String,
    #[serde(rename = "stub-path")]
    pub stub_path: String,
    #[serde(rename = "stub-spec")]
    pub stub_spec: LineRange,
    #[serde(rename = "stub-proof", skip_serializing_if = "Option::is_none")]
    pub stub_proof: Option<LineRange>,
    pub labels: Vec<String>,
    #[serde(rename = "code-name", skip_serializing_if = "Option::is_none")]
    pub code_name: Option<String>,
    #[serde(rename = "code-names", skip_serializing_if = "Option::is_none")]
    pub lean_names: Option<Vec<String>>,
    #[serde(rename = "spec-ok")]
    pub spec_ok: bool,
    #[serde(rename = "mathlib-ok")]
    pub mathlib_ok: bool,
    #[serde(rename = "not-ready")]
    pub not_ready: bool,
    #[serde(rename = "discussion", skip_serializing_if = "Vec::is_empty")]
    pub discussion: Vec<String>,
    #[serde(rename = "spec-dependencies")]
    pub spec_dependencies: Vec<String>,
    #[serde(rename = "proof-ok", skip_serializing_if = "Option::is_none")]
    pub proof_ok: Option<bool>,
    #[serde(rename = "proof-mathlib-ok", skip_serializing_if = "Option::is_none")]
    pub proof_mathlib_ok: Option<bool>,
    #[serde(rename = "proof-not-ready", skip_serializing_if = "Option::is_none")]
    pub proof_not_ready: Option<bool>,
    #[serde(rename = "proof-discussion", skip_serializing_if = "Option::is_none")]
    pub proof_discussion: Option<Vec<String>>,
    #[serde(rename = "proof-dependencies", skip_serializing_if = "Option::is_none")]
    pub proof_dependencies: Option<Vec<String>>,
    #[serde(rename = "proof-lean-names", skip_serializing_if = "Option::is_none")]
    pub proof_lean_names: Option<Vec<String>>,
}

/// Extract environment types from the `thms` option in web.tex
/// e.g., \usepackage[thms=dfn+lem+prop+thm+cor]{blueprint}
fn parse_thms_option(web_tex_content: &str) -> Vec<String> {
    // Look for \usepackage[...thms=...]{blueprint}
    let re = Regex::new(r"\\usepackage\s*\[([^\]]*)\]\s*\{blueprint\}").unwrap();

    if let Some(caps) = re.captures(web_tex_content) {
        let options = &caps[1];
        // Look for thms=xxx+yyy+zzz
        let thms_re = Regex::new(r"thms\s*=\s*([a-zA-Z+_]+)").unwrap();
        if let Some(thms_caps) = thms_re.captures(options) {
            let thms_str = &thms_caps[1];
            return thms_str.split('+').map(|s| s.trim().to_string()).collect();
        }
    }

    // Return default environments if no thms option found
    DEFAULT_ENVS.iter().map(|s| s.to_string()).collect()
}

/// Strip LaTeX comments from content, preserving line structure
/// Comments start with % and go to end of line, but \% is an escaped percent sign
fn strip_latex_comments(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Escaped character - include both the backslash and next char
            result.push(c);
            if chars.peek().is_some() {
                result.push(chars.next().unwrap());
            }
        } else if c == '%' {
            // Comment - skip until end of line, but preserve the newline
            while let Some(&next) = chars.peek() {
                if next == '\n' {
                    break;
                }
                chars.next();
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Strip nested environments from content (e.g., equation, align, etc. inside a proof)
/// This ensures we only extract top-level labels, not labels from nested environments
fn strip_nested_environments(content: &str) -> String {
    let begin_re = Regex::new(r"\\begin\{([^}]+)\}").unwrap();

    let mut result = String::new();
    let mut pos = 0;

    while pos < content.len() {
        // Look for \begin{...} starting from current position
        if let Some(caps) = begin_re.captures(&content[pos..]) {
            let full_match = caps.get(0).unwrap();
            let env_name = &caps[1];

            // Add content before this \begin
            let begin_start = pos + full_match.start();
            result.push_str(&content[pos..begin_start]);

            // Find the matching \end{env_name}
            let end_pattern = format!(r"\\end\{{{}\}}", regex::escape(env_name));
            let end_re = Regex::new(&end_pattern).unwrap();

            let search_start = pos + full_match.end();
            if let Some(end_match) = end_re.find(&content[search_start..]) {
                // Skip past the entire nested environment
                pos = search_start + end_match.end();
            } else {
                // No matching \end found, include the \begin and continue
                result.push_str(full_match.as_str());
                pos += full_match.end();
            }
        } else {
            // No more \begin found, add remaining content
            result.push_str(&content[pos..]);
            break;
        }
    }

    result
}

/// Extract all top-level labels from \label{...} in order of appearance
/// Labels inside nested environments (like equation, align) are ignored
fn extract_all_labels(content: &str) -> Vec<String> {
    // First strip nested environments to only get top-level labels
    let top_level_content = strip_nested_environments(content);

    let re = Regex::new(r"\\label\{([^}]+)\}").unwrap();
    re.captures_iter(&top_level_content)
        .map(|caps| caps[1].to_string())
        .collect()
}

/// Extract lean declarations from \lean{...}
/// Returns a list of declaration names (comma-separated in the macro)
fn extract_lean(content: &str) -> Vec<String> {
    let re = Regex::new(r"\\lean\{([^}]+)\}").unwrap();
    if let Some(caps) = re.captures(content) {
        let lean_str = &caps[1];
        return lean_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

/// Check for \mathlibok macro
fn extract_mathlibok(content: &str) -> bool {
    content.contains(r"\mathlibok")
}

/// Check for \notready macro
fn extract_notready(content: &str) -> bool {
    content.contains(r"\notready")
}

/// Extract discussion issue numbers from \discussion{...}
/// Can appear multiple times, so returns a list
fn extract_discussion(content: &str) -> Vec<String> {
    let re = Regex::new(r"\\discussion\{([^}]+)\}").unwrap();
    re.captures_iter(content)
        .map(|caps| caps[1].trim().to_string())
        .collect()
}

/// Extract labels from \proves{...}
/// Returns a list of labels that this proof proves
fn extract_proves(content: &str) -> Vec<String> {
    let re = Regex::new(r"\\proves\{([^}]+)\}").unwrap();
    if let Some(caps) = re.captures(content) {
        let proves_str = &caps[1];
        return proves_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

/// Extract dependencies from \uses{...}
fn extract_uses(content: &str) -> Vec<String> {
    let re = Regex::new(r"\\uses\{([^}]+)\}").unwrap();
    if let Some(caps) = re.captures(content) {
        let uses_str = &caps[1];
        return uses_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

/// Extract \home{url} from content
fn extract_home(content: &str) -> Option<String> {
    let re = Regex::new(r"\\home\{([^}]+)\}").unwrap();
    re.captures(content).map(|caps| caps[1].trim().to_string())
}

/// Extract \github{url} from content
fn extract_github(content: &str) -> Option<String> {
    let re = Regex::new(r"\\github\{([^}]+)\}").unwrap();
    re.captures(content).map(|caps| caps[1].trim().to_string())
}

/// Extract \dochome{url} from content
fn extract_dochome(content: &str) -> Option<String> {
    let re = Regex::new(r"\\dochome\{([^}]+)\}").unwrap();
    re.captures(content).map(|caps| caps[1].trim().to_string())
}

/// Extract project config from content
fn extract_config(content: &str) -> Config {
    Config {
        home: extract_home(content),
        github: extract_github(content),
        dochome: extract_dochome(content),
    }
}

/// Merge two configs, preferring values from `other` if present
fn merge_config(base: Config, other: Config) -> Config {
    Config {
        home: other.home.or(base.home),
        github: other.github.or(base.github),
        dochome: other.dochome.or(base.dochome),
    }
}

/// Generate a fresh label in the form "a0000000000"
fn generate_label(counter: u64) -> String {
    format!("a{:010}", counter)
}

/// Convert a byte position to a 1-indexed line number
fn byte_pos_to_line(content: &str, pos: usize) -> usize {
    content[..pos].chars().filter(|&c| c == '\n').count() + 1
}

/// Parsed environment before label validation
struct ParsedEnv {
    env_type: String,
    relative_path: String,
    spec_lines: LineRange,
    proof_lines: Option<LineRange>,
    labels: Vec<String>,
    code_name: Option<String>,
    lean_names: Option<Vec<String>>,
    spec_ok: bool,
    mathlib_ok: bool,
    not_ready: bool,
    discussion: Vec<String>,
    spec_dependencies: Vec<String>,
    proof_ok: Option<bool>,
    proof_mathlib_ok: Option<bool>,
    proof_not_ready: Option<bool>,
    proof_discussion: Option<Vec<String>>,
    proof_dependencies: Option<Vec<String>>,
    proof_lean_names: Option<Vec<String>>,
}

/// A standalone proof that uses \proves to reference its statement
struct StandaloneProof {
    proves_labels: Vec<String>,
    lines: LineRange,
    proof_ok: bool,
    mathlib_ok: bool,
    not_ready: bool,
    discussion: Vec<String>,
    dependencies: Vec<String>,
    lean_names: Vec<String>,
}

/// Proof match result with content and line range
struct ProofMatch {
    content: String,
    lines: LineRange,
    /// Labels from \proves{...} - if present, this is a standalone proof
    proves_labels: Vec<String>,
}

/// Find the proof environment that immediately follows a position in the content
/// Returns the proof content and line range if found
fn find_following_proof(content: &str, after_pos: usize) -> Option<ProofMatch> {
    let remaining = &content[after_pos..];

    // Look for \begin{proof} that appears next (allowing only whitespace before it)
    let proof_re = Regex::new(r"(?s)^\s*(\\begin\{proof\})(.*?)\\end\{proof\}").unwrap();

    proof_re.captures(remaining).map(|caps| {
        // Get the position of \begin{proof} itself, not the leading whitespace
        let begin_match = caps.get(1).unwrap();
        let proof_start = after_pos + begin_match.start();
        let full_match = caps.get(0).unwrap();
        let proof_end = after_pos + full_match.end();
        let proof_content = caps[2].to_string();

        // Extract \proves{...} labels if present
        let proves_labels = extract_proves(&proof_content);

        ProofMatch {
            content: proof_content,
            lines: LineRange {
                lines_start: byte_pos_to_line(content, proof_start),
                lines_end: byte_pos_to_line(content, proof_end - 1), // -1 to get line of last char
            },
            proves_labels,
        }
    })
}

/// Find all standalone proofs (those with \proves) in a file
fn find_standalone_proofs(content: &str, relative_path: &str) -> Vec<StandaloneProof> {
    let mut proofs = Vec::new();

    // Strip LaTeX comments before parsing
    let content = strip_latex_comments(content);

    // Find all \begin{proof}...\end{proof} environments
    let proof_re = Regex::new(r"(?s)\\begin\{proof\}(.*?)\\end\{proof\}").unwrap();

    for caps in proof_re.captures_iter(&content) {
        let full_match = caps.get(0).unwrap();
        let proof_content = &caps[1];

        // Check if this proof has \proves
        let proves_labels = extract_proves(proof_content);
        if proves_labels.is_empty() {
            continue; // Not a standalone proof
        }

        let lines = LineRange {
            lines_start: byte_pos_to_line(&content, full_match.start()),
            lines_end: byte_pos_to_line(&content, full_match.end() - 1),
        };

        proofs.push(StandaloneProof {
            proves_labels,
            lines,
            proof_ok: proof_content.contains(r"\leanok"),
            mathlib_ok: extract_mathlibok(proof_content),
            not_ready: extract_notready(proof_content),
            discussion: extract_discussion(proof_content),
            dependencies: extract_uses(proof_content),
            lean_names: extract_lean(proof_content),
        });
    }

    // Store relative_path for error messages (currently unused but available)
    let _ = relative_path;

    proofs
}

/// Parse a single .tex file and extract environments
fn parse_tex_file(content: &str, relative_path: &str, env_types: &[String]) -> Vec<ParsedEnv> {
    let mut envs = Vec::new();

    // Strip LaTeX comments before parsing (preserves line structure)
    let content = strip_latex_comments(content);

    // Collect all environment matches with their positions
    struct EnvMatch {
        env_type: String,
        start_pos: usize,
        end_pos: usize,
        env_content: String,
    }

    let mut all_matches: Vec<EnvMatch> = Vec::new();

    // Build regex pattern for all environment types
    // Match \begin{env}...\end{env} including nested content
    for env_type in env_types {
        let pattern = format!(
            r"(?s)\\begin\{{{}\}}(.*?)\\end\{{{}\}}",
            regex::escape(env_type),
            regex::escape(env_type)
        );
        let env_re = Regex::new(&pattern).unwrap();

        for caps in env_re.captures_iter(&content) {
            let full_match = caps.get(0).unwrap();
            all_matches.push(EnvMatch {
                env_type: env_type.clone(),
                start_pos: full_match.start(),
                end_pos: full_match.end(),
                env_content: caps[1].to_string(),
            });
        }
    }

    // Sort by position to process in order
    all_matches.sort_by_key(|m| m.start_pos);

    for env_match in all_matches {
        let env_content = &env_match.env_content;

        // Calculate line numbers for the spec environment
        let spec_lines = LineRange {
            lines_start: byte_pos_to_line(&content, env_match.start_pos),
            lines_end: byte_pos_to_line(&content, env_match.end_pos - 1),
        };

        // Extract all \label{...} in order from the statement
        let mut labels = extract_all_labels(env_content);

        // Extract \lean{...} - returns list of declarations with "probe:" prefix
        let lean_names_list = extract_lean(env_content);
        let code_name = lean_names_list
            .first()
            .map(|name| format!("probe:{}", name));
        let lean_names = if lean_names_list.len() > 1 {
            Some(
                lean_names_list
                    .iter()
                    .map(|name| format!("probe:{}", name))
                    .collect(),
            )
        } else {
            None
        };

        // Check for \leanok
        let spec_ok = env_content.contains(r"\leanok");

        // Check for \mathlibok
        let mathlib_ok = extract_mathlibok(env_content);

        // Check for \notready
        let not_ready = extract_notready(env_content);

        // Extract \discussion{...}
        let discussion = extract_discussion(env_content);

        // Extract \uses{...}
        let spec_dependencies = extract_uses(env_content);

        // Look for a following proof environment
        let (
            proof_lines,
            proof_ok,
            proof_mathlib_ok,
            proof_not_ready,
            proof_discussion,
            proof_dependencies,
            proof_lean_names,
        ) = if let Some(proof_match) = find_following_proof(&content, env_match.end_pos) {
            // Skip proofs that use \proves (they will be handled separately)
            if !proof_match.proves_labels.is_empty() {
                (None, None, None, None, None, None, None)
            } else {
                // Add proof labels to the labels list
                let proof_labels = extract_all_labels(&proof_match.content);
                labels.extend(proof_labels);

                // Check for \leanok in proof
                let p_ok = if proof_match.content.contains(r"\leanok") {
                    Some(true)
                } else {
                    None
                };

                // Check for \mathlibok in proof
                let p_mathlib = if extract_mathlibok(&proof_match.content) {
                    Some(true)
                } else {
                    None
                };

                // Check for \notready in proof
                let p_not_ready = if extract_notready(&proof_match.content) {
                    Some(true)
                } else {
                    None
                };

                // Extract \discussion{...} from proof
                let p_discussion = extract_discussion(&proof_match.content);
                let p_discussion = if p_discussion.is_empty() {
                    None
                } else {
                    Some(p_discussion)
                };

                // Extract \uses{...} from proof
                let p_deps = extract_uses(&proof_match.content);
                let p_deps = if p_deps.is_empty() {
                    None
                } else {
                    Some(p_deps)
                };

                // Extract \lean{...} from proof
                let p_lean = extract_lean(&proof_match.content);
                let p_lean = if p_lean.is_empty() {
                    None
                } else {
                    Some(p_lean)
                };

                (
                    Some(proof_match.lines),
                    p_ok,
                    p_mathlib,
                    p_not_ready,
                    p_discussion,
                    p_deps,
                    p_lean,
                )
            }
        } else {
            (None, None, None, None, None, None, None)
        };

        envs.push(ParsedEnv {
            env_type: env_match.env_type,
            relative_path: relative_path.to_string(),
            spec_lines,
            proof_lines,
            labels,
            code_name,
            lean_names,
            spec_ok,
            mathlib_ok,
            not_ready,
            discussion,
            spec_dependencies,
            proof_ok,
            proof_mathlib_ok,
            proof_not_ready,
            proof_discussion,
            proof_dependencies,
            proof_lean_names,
        });
    }

    envs
}

/// Run the stubify command
pub fn run(project_path: &str, output: &str) -> Result<(), Box<dyn Error>> {
    let project_path = Path::new(project_path);
    let blueprint_src = project_path.join("blueprint").join("src");

    if !blueprint_src.exists() {
        return Err(format!(
            "blueprint/src directory not found at {}",
            blueprint_src.display()
        )
        .into());
    }

    // Parse web.tex for environment types and config
    let web_tex_path = blueprint_src.join("web.tex");
    let (env_types, mut project_config) = if web_tex_path.exists() {
        let web_tex_content = fs::read_to_string(&web_tex_path)?;
        let envs = parse_thms_option(&web_tex_content);
        let config = extract_config(&web_tex_content);
        (envs, config)
    } else {
        (
            DEFAULT_ENVS.iter().map(|s| s.to_string()).collect(),
            Config::default(),
        )
    };

    eprintln!("Looking for environments: {}", env_types.join(", "));

    // Collect all parsed environments and standalone proofs
    let mut all_envs: Vec<ParsedEnv> = Vec::new();
    let mut all_standalone_proofs: Vec<(String, StandaloneProof)> = Vec::new(); // (relative_path, proof)

    // Walk through all .tex files in blueprint/src
    for entry in WalkDir::new(&blueprint_src)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "tex") {
            // Skip web.tex and print.tex (they're not content files)
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if file_name == "web.tex" || file_name == "print.tex" {
                continue;
            }

            let content = fs::read_to_string(path)?;

            // Extract config from content files as well (in case macros are there)
            let file_config = extract_config(&content);
            project_config = merge_config(project_config, file_config);

            // Get path relative to blueprint/src
            let relative_path = path
                .strip_prefix(&blueprint_src)?
                .to_str()
                .ok_or("Invalid UTF-8 in path")?;

            let envs = parse_tex_file(&content, relative_path, &env_types);
            all_envs.extend(envs);

            // Find standalone proofs with \proves
            let standalone_proofs = find_standalone_proofs(&content, relative_path);
            for proof in standalone_proofs {
                all_standalone_proofs.push((relative_path.to_string(), proof));
            }
        }
    }

    // Track all seen labels for duplicate detection
    let mut seen_labels: HashSet<String> = HashSet::new();
    let mut label_counter: u64 = 0;
    let mut all_stubs: HashMap<String, Stub> = HashMap::new();

    // Process each environment
    for mut env in all_envs {
        // Check existing labels for duplicates
        for label in &env.labels {
            if seen_labels.contains(label) {
                return Err(format!("Duplicate label found: {}", label).into());
            }
        }

        // If no labels, generate one
        if env.labels.is_empty() {
            loop {
                let generated = generate_label(label_counter);
                label_counter += 1;
                if !seen_labels.contains(&generated) {
                    env.labels.push(generated);
                    break;
                }
            }
        }

        // Add all labels to seen set
        for label in &env.labels {
            seen_labels.insert(label.clone());
        }

        // Use the last label for stub-name
        let primary_label = env.labels.last().unwrap();
        let stub_name = format!("{}/{}", env.relative_path, primary_label);

        all_stubs.insert(
            stub_name,
            Stub {
                label: primary_label.clone(),
                stub_type: env.env_type,
                stub_path: env.relative_path,
                stub_spec: env.spec_lines,
                stub_proof: env.proof_lines,
                labels: env.labels,
                code_name: env.code_name,
                lean_names: env.lean_names,
                spec_ok: env.spec_ok,
                mathlib_ok: env.mathlib_ok,
                not_ready: env.not_ready,
                discussion: env.discussion,
                spec_dependencies: env.spec_dependencies,
                proof_ok: env.proof_ok,
                proof_mathlib_ok: env.proof_mathlib_ok,
                proof_not_ready: env.proof_not_ready,
                proof_discussion: env.proof_discussion,
                proof_dependencies: env.proof_dependencies,
                proof_lean_names: env.proof_lean_names,
            },
        );
    }

    eprintln!("Found {} stubs", all_stubs.len());

    // Build a map from label to stub name for quick lookup
    let mut label_to_stub_name: HashMap<String, String> = HashMap::new();
    for (stub_name, stub) in &all_stubs {
        for label in &stub.labels {
            label_to_stub_name.insert(label.clone(), stub_name.clone());
        }
    }

    // Merge standalone proofs (those with \proves) into their corresponding stubs
    for (relative_path, proof) in all_standalone_proofs {
        for proves_label in &proof.proves_labels {
            if let Some(stub_name) = label_to_stub_name.get(proves_label) {
                if let Some(stub) = all_stubs.get_mut(stub_name) {
                    // Merge proof fields into the stub
                    stub.stub_proof = Some(proof.lines.clone());
                    if proof.proof_ok {
                        stub.proof_ok = Some(true);
                    }
                    if proof.mathlib_ok {
                        stub.proof_mathlib_ok = Some(true);
                    }
                    if proof.not_ready {
                        stub.proof_not_ready = Some(true);
                    }
                    if !proof.discussion.is_empty() {
                        stub.proof_discussion = Some(proof.discussion.clone());
                    }
                    if !proof.dependencies.is_empty() {
                        stub.proof_dependencies = Some(proof.dependencies.clone());
                    }
                    if !proof.lean_names.is_empty() {
                        stub.proof_lean_names = Some(proof.lean_names.clone());
                    }
                }
            } else {
                eprintln!(
                    "Warning: \\proves{{{}}} in {} references unknown label",
                    proves_label, relative_path
                );
            }
        }
    }

    // Resolve dependency labels to canonical stub-names
    // Dependencies in .tex files are labels (possibly non-canonical), which we
    // resolve to stub-names using the label_to_stub_name mapping
    for (stub_name, stub) in all_stubs.iter_mut() {
        // Resolve spec-dependencies labels to stub-names
        let mut resolved_spec_deps = Vec::new();
        for dep_label in &stub.spec_dependencies {
            if let Some(dep_stub_name) = label_to_stub_name.get(dep_label) {
                resolved_spec_deps.push(dep_stub_name.clone());
            } else {
                return Err(format!(
                    "Unknown label '{}' in spec-dependencies of stub '{}'",
                    dep_label, stub_name
                )
                .into());
            }
        }
        stub.spec_dependencies = resolved_spec_deps;

        // Resolve proof-dependencies labels to stub-names
        if let Some(proof_deps) = &stub.proof_dependencies {
            let mut resolved_proof_deps = Vec::new();
            for dep_label in proof_deps {
                if let Some(dep_stub_name) = label_to_stub_name.get(dep_label) {
                    resolved_proof_deps.push(dep_stub_name.clone());
                } else {
                    return Err(format!(
                        "Unknown label '{}' in proof-dependencies of stub '{}'",
                        dep_label, stub_name
                    )
                    .into());
                }
            }
            stub.proof_dependencies = Some(resolved_proof_deps);
        }
    }

    // Write output (create parent directory if needed)
    let output_path = Path::new(output);
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let json = serde_json::to_string_pretty(&all_stubs)?;
    fs::write(output_path, json)?;

    eprintln!("Wrote stubs to {output}");

    // Write config to .verilib/config.json if any config values were found
    if project_config.home.is_some()
        || project_config.github.is_some()
        || project_config.dochome.is_some()
    {
        let verilib_dir = project_path.join(".verilib");
        if !verilib_dir.exists() {
            fs::create_dir_all(&verilib_dir)?;
        }

        let config_path = verilib_dir.join("config.json");

        // Read existing config as a generic JSON object to preserve unknown fields
        let mut config_obj: serde_json::Map<String, serde_json::Value> = if config_path.exists() {
            let existing_content = fs::read_to_string(&config_path)?;
            serde_json::from_str(&existing_content).unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

        // Only update fields that were found in LaTeX files
        if let Some(home) = project_config.home {
            config_obj.insert("home".to_string(), serde_json::Value::String(home));
        }
        if let Some(github) = project_config.github {
            config_obj.insert("github".to_string(), serde_json::Value::String(github));
        }
        if let Some(dochome) = project_config.dochome {
            config_obj.insert("dochome".to_string(), serde_json::Value::String(dochome));
        }

        let config_json = serde_json::to_string_pretty(&config_obj)?;
        fs::write(&config_path, config_json)?;

        eprintln!("Wrote config to {}", config_path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_thms_option_default() {
        let content = r"\usepackage[showmore, dep_graph]{blueprint}";
        let envs = parse_thms_option(content);
        assert_eq!(
            envs,
            DEFAULT_ENVS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_parse_thms_option_custom() {
        let content = r"\usepackage[thms=dfn+lem+prop+thm+cor]{blueprint}";
        let envs = parse_thms_option(content);
        assert_eq!(envs, vec!["dfn", "lem", "prop", "thm", "cor"]);
    }

    #[test]
    fn test_extract_all_labels_single() {
        let labels = extract_all_labels(r"\label{foo}");
        assert_eq!(labels, vec!["foo"]);
    }

    #[test]
    fn test_extract_all_labels_multiple() {
        let labels = extract_all_labels(r"\label{first}\label{second}\label{third}");
        assert_eq!(labels, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_extract_all_labels_none() {
        let labels = extract_all_labels(r"no labels here");
        assert!(labels.is_empty());
    }

    #[test]
    fn test_extract_lean() {
        assert_eq!(
            extract_lean(r"\lean{Subgraph.Equation387_implies_Equation43}"),
            vec!["Subgraph.Equation387_implies_Equation43"]
        );
        assert_eq!(extract_lean(r"no lean here"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_lean_multiple() {
        assert_eq!(
            extract_lean(r"\lean{Decl1, Decl2, Decl3}"),
            vec!["Decl1", "Decl2", "Decl3"]
        );
    }

    #[test]
    fn test_extract_mathlibok() {
        assert!(extract_mathlibok(r"\mathlibok"));
        assert!(extract_mathlibok(r"some text \mathlibok more text"));
        assert!(!extract_mathlibok(r"no mathlib here"));
    }

    #[test]
    fn test_extract_notready() {
        assert!(extract_notready(r"\notready"));
        assert!(extract_notready(r"some text \notready more text"));
        assert!(!extract_notready(r"ready for formalization"));
    }

    #[test]
    fn test_extract_discussion() {
        assert_eq!(extract_discussion(r"\discussion{123}"), vec!["123"]);
        assert_eq!(
            extract_discussion(r"\discussion{123}\discussion{456}"),
            vec!["123", "456"]
        );
        assert_eq!(extract_discussion(r"no discussion"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_proves() {
        assert_eq!(extract_proves(r"\proves{thm1}"), vec!["thm1"]);
        assert_eq!(extract_proves(r"\proves{thm1, thm2}"), vec!["thm1", "thm2"]);
        assert_eq!(extract_proves(r"no proves"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_uses() {
        assert_eq!(extract_uses(r"\uses{eq387,eq43}"), vec!["eq387", "eq43"]);
        assert_eq!(extract_uses(r"\uses{r, s, t}"), vec!["r", "s", "t"]);
        assert_eq!(extract_uses(r"no uses"), Vec::<String>::new());
    }

    #[test]
    fn test_generate_label() {
        assert_eq!(generate_label(0), "a0000000000");
        assert_eq!(generate_label(1), "a0000000001");
        assert_eq!(generate_label(123), "a0000000123");
        assert_eq!(generate_label(9999999999), "a9999999999");
    }

    #[test]
    fn test_byte_pos_to_line() {
        let content = "line1\nline2\nline3";
        assert_eq!(byte_pos_to_line(content, 0), 1); // Start of line1
        assert_eq!(byte_pos_to_line(content, 5), 1); // End of line1 (before \n)
        assert_eq!(byte_pos_to_line(content, 6), 2); // Start of line2
        assert_eq!(byte_pos_to_line(content, 12), 3); // Start of line3
    }

    #[test]
    fn test_parse_tex_file_theorem_with_labels() {
        let content = r#"
\begin{theorem}[387 implies 43]\label{387_implies_43}\uses{eq387,eq43}\lean{Subgraph.Equation387_implies_Equation43}\leanok
  Some content here.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "chapter/implications.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].env_type, "theorem");
        assert_eq!(envs[0].labels, vec!["387_implies_43"]);
        assert_eq!(
            envs[0].code_name,
            Some("probe:Subgraph.Equation387_implies_Equation43".to_string())
        );
        assert!(envs[0].spec_ok);
        assert_eq!(envs[0].spec_dependencies, vec!["eq387", "eq43"]);
        assert_eq!(envs[0].proof_ok, None);
        assert_eq!(envs[0].proof_dependencies, None);
        // Line numbers: starts on line 2, ends on line 4
        assert_eq!(envs[0].spec_lines.lines_start, 2);
        assert_eq!(envs[0].spec_lines.lines_end, 4);
    }

    #[test]
    fn test_parse_tex_file_different_env_types() {
        let content = r#"
\begin{definition}\label{def1}
  A definition.
\end{definition}

\begin{lemma}\label{lem1}
  A lemma.
\end{lemma}

\begin{theorem}\label{thm1}
  A theorem.
\end{theorem}

\begin{dfn}\label{dfn1}
  A dfn (short form).
\end{dfn}
"#;
        let env_types: Vec<String> = vec![
            "definition".to_string(),
            "lemma".to_string(),
            "theorem".to_string(),
            "dfn".to_string(),
        ];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 4);
        assert_eq!(envs[0].env_type, "definition");
        assert_eq!(envs[0].labels, vec!["def1"]);
        assert_eq!(envs[1].env_type, "lemma");
        assert_eq!(envs[1].labels, vec!["lem1"]);
        assert_eq!(envs[2].env_type, "theorem");
        assert_eq!(envs[2].labels, vec!["thm1"]);
        assert_eq!(envs[3].env_type, "dfn");
        assert_eq!(envs[3].labels, vec!["dfn1"]);
    }

    #[test]
    fn test_parse_tex_file_multiple_labels() {
        let content = r#"
\begin{theorem}\label{first_label}\label{second_label}\label{primary_label}
  Content with multiple labels.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(
            envs[0].labels,
            vec!["first_label", "second_label", "primary_label"]
        );
    }

    #[test]
    fn test_parse_tex_file_no_label() {
        let content = r#"
\begin{lemma}
  A lemma without any label.
\end{lemma}
"#;
        let env_types: Vec<String> = vec!["lemma".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert!(envs[0].labels.is_empty());
    }

    #[test]
    fn test_stub_uses_last_label() {
        // Simulate the processing logic
        let mut seen_labels: HashSet<String> = HashSet::new();
        let labels = vec![
            "first".to_string(),
            "second".to_string(),
            "primary".to_string(),
        ];

        for label in &labels {
            seen_labels.insert(label.clone());
        }

        let primary_label = labels.last().unwrap();
        let stub_name = format!("{}/{}", "file.tex", primary_label);

        assert_eq!(stub_name, "file.tex/primary");
    }

    #[test]
    fn test_strip_latex_comments_simple() {
        let content = "hello % this is a comment\nworld";
        let stripped = strip_latex_comments(content);
        assert_eq!(stripped, "hello \nworld");
    }

    #[test]
    fn test_strip_latex_comments_escaped_percent() {
        let content = r"50\% discount";
        let stripped = strip_latex_comments(content);
        assert_eq!(stripped, r"50\% discount");
    }

    #[test]
    fn test_strip_latex_comments_full_line() {
        let content = "% full line comment\nactual content";
        let stripped = strip_latex_comments(content);
        assert_eq!(stripped, "\nactual content");
    }

    #[test]
    fn test_strip_latex_comments_no_comments() {
        let content = r"\begin{theorem}\label{foo}\end{theorem}";
        let stripped = strip_latex_comments(content);
        assert_eq!(stripped, content);
    }

    #[test]
    fn test_parse_tex_file_commented_out_env() {
        let content = r#"
% \begin{theorem}\label{commented_out}
%   This theorem is commented out.
% \end{theorem}

\begin{theorem}\label{active_theorem}
  This theorem is active.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        // Only the active theorem should be found
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].labels, vec!["active_theorem"]);
    }

    #[test]
    fn test_parse_tex_file_partially_commented() {
        let content = r#"
\begin{theorem}\label{my_theorem}
  % \label{commented_label}
  Active content here.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // Only the non-commented label should be found
        assert_eq!(envs[0].labels, vec!["my_theorem"]);
    }

    #[test]
    fn test_parse_tex_file_with_proof() {
        let content = r#"
\begin{theorem}\label{my_theorem}\lean{MyTheorem}\leanok
  Statement of the theorem.
\end{theorem}

\begin{proof}\leanok\uses{lemma1,lemma2}
  The proof goes here.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].labels, vec!["my_theorem"]);
        assert!(envs[0].spec_ok);
        assert_eq!(envs[0].proof_ok, Some(true));
        assert_eq!(
            envs[0].proof_dependencies,
            Some(vec!["lemma1".to_string(), "lemma2".to_string()])
        );
        // Check proof lines are captured
        assert!(envs[0].proof_lines.is_some());
        let proof_lines = envs[0].proof_lines.as_ref().unwrap();
        assert_eq!(proof_lines.lines_start, 6);
        assert_eq!(proof_lines.lines_end, 8);
    }

    #[test]
    fn test_parse_tex_file_proof_with_label() {
        let content = r#"
\begin{theorem}\label{thm_label}\leanok
  Statement.
\end{theorem}

\begin{proof}\label{proof_label}\leanok
  Proof content.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // Proof label should be added to the end
        assert_eq!(envs[0].labels, vec!["thm_label", "proof_label"]);
        // The stub name should use the last label (proof_label)
    }

    #[test]
    fn test_parse_tex_file_proof_without_leanok() {
        let content = r#"
\begin{theorem}\label{my_theorem}\leanok
  Statement.
\end{theorem}

\begin{proof}\uses{dep1}
  Proof without leanok.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert!(envs[0].spec_ok);
        // proof_ok should be None (not present) when \leanok is not in proof
        assert_eq!(envs[0].proof_ok, None);
        assert_eq!(envs[0].proof_dependencies, Some(vec!["dep1".to_string()]));
    }

    #[test]
    fn test_parse_tex_file_no_proof() {
        let content = r#"
\begin{definition}\label{my_def}\lean{MyDef}\leanok
  A definition without proof.
\end{definition}
"#;
        let env_types: Vec<String> = vec!["definition".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].labels, vec!["my_def"]);
        assert_eq!(envs[0].proof_ok, None);
        assert_eq!(envs[0].proof_dependencies, None);
        assert!(envs[0].proof_lines.is_none());
    }

    #[test]
    fn test_parse_tex_file_theorem_then_other_content_then_proof() {
        // Proof should only be found if it immediately follows (with whitespace only)
        let content = r#"
\begin{theorem}\label{thm1}\leanok
  First theorem.
\end{theorem}

Some intervening text here.

\begin{proof}\leanok
  This proof should NOT be associated with thm1.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // The proof should not be associated because there's intervening text
        assert_eq!(envs[0].proof_ok, None);
        assert!(envs[0].proof_lines.is_none());
    }

    #[test]
    fn test_strip_nested_environments() {
        let content =
            r#"Top level content \begin{equation}\label{nested}\end{equation} more content"#;
        let stripped = strip_nested_environments(content);
        assert_eq!(stripped, "Top level content  more content");
    }

    #[test]
    fn test_strip_nested_environments_multiple() {
        let content =
            r#"\begin{align}\label{a1}\end{align} text \begin{equation}\label{a2}\end{equation}"#;
        let stripped = strip_nested_environments(content);
        assert_eq!(stripped, " text ");
    }

    #[test]
    fn test_extract_labels_ignores_nested() {
        // Labels inside nested environments should be ignored
        let content = r#"\label{top_level}
Some text here.
\begin{equation}\label{nested_eq}
  x = y
\end{equation}
More text.
\begin{align}\label{nested_align}
  a = b
\end{align}"#;
        let labels = extract_all_labels(content);
        assert_eq!(labels, vec!["top_level"]);
    }

    #[test]
    fn test_parse_tex_file_proof_with_nested_equation() {
        let content = r#"
\begin{theorem}\label{my_thm}\leanok
  Statement of theorem.
\end{theorem}

\begin{proof}\label{my_proof}\leanok
  We have
  \begin{equation}\label{eq1}
    x = y
  \end{equation}
  and therefore the result follows.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // Only top-level labels: my_thm from theorem, my_proof from proof
        // eq1 from nested equation should be ignored
        assert_eq!(envs[0].labels, vec!["my_thm", "my_proof"]);
        assert_eq!(envs[0].proof_ok, Some(true));
    }

    #[test]
    fn test_parse_tex_file_theorem_with_nested_env() {
        let content = r#"
\begin{theorem}\label{main_thm}
  For all $x$, we have
  \begin{equation}\label{internal_eq}
    f(x) = g(x)
  \end{equation}
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // Only main_thm should be captured, not internal_eq
        assert_eq!(envs[0].labels, vec!["main_thm"]);
    }

    #[test]
    fn test_parse_tex_file_line_numbers() {
        let content = r#"\begin{theorem}\label{thm1}
Line 2 content.
Line 3 content.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].spec_lines.lines_start, 1);
        assert_eq!(envs[0].spec_lines.lines_end, 4);
    }

    #[test]
    fn test_parse_tex_file_with_mathlibok() {
        let content = r#"
\begin{theorem}\label{my_thm}\lean{MyThm}\mathlibok
  A theorem in mathlib.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert!(envs[0].mathlib_ok);
        // mathlibok should imply leanok for the spec
        assert!(!envs[0].spec_ok); // leanok was not explicitly present
    }

    #[test]
    fn test_parse_tex_file_with_notready() {
        let content = r#"
\begin{theorem}\label{my_thm}\notready
  A theorem not ready for formalization.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert!(envs[0].not_ready);
        assert!(!envs[0].spec_ok);
    }

    #[test]
    fn test_parse_tex_file_with_discussion() {
        let content = r#"
\begin{theorem}\label{my_thm}\discussion{123}\discussion{456}
  A theorem with discussions.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].discussion, vec!["123", "456"]);
    }

    #[test]
    fn test_parse_tex_file_with_multiple_lean_names() {
        let content = r#"
\begin{theorem}\label{my_thm}\lean{Thm1, Thm2, Thm3}\leanok
  A theorem with multiple lean names.
\end{theorem}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].code_name, Some("probe:Thm1".to_string()));
        assert_eq!(
            envs[0].lean_names,
            Some(vec![
                "probe:Thm1".to_string(),
                "probe:Thm2".to_string(),
                "probe:Thm3".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_tex_file_proof_with_proves_not_associated() {
        // When a proof has \proves, it should NOT be associated with the preceding theorem
        let content = r#"
\begin{theorem}\label{thm1}\leanok
  First theorem.
\end{theorem}

\begin{proof}\proves{some_other_thm}\leanok
  This proof is for a different theorem.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // The proof should NOT be associated since it has \proves
        assert!(envs[0].proof_lines.is_none());
        assert_eq!(envs[0].proof_ok, None);
    }

    #[test]
    fn test_find_standalone_proofs() {
        let content = r#"
\begin{theorem}\label{thm1}
  A theorem.
\end{theorem}

\begin{proof}\proves{thm1}\leanok\uses{lemma1}\lean{TheoremProof}
  The proof.
\end{proof}
"#;
        let proofs = find_standalone_proofs(content, "file.tex");

        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].proves_labels, vec!["thm1"]);
        assert!(proofs[0].proof_ok);
        assert_eq!(proofs[0].dependencies, vec!["lemma1"]);
        assert_eq!(proofs[0].lean_names, vec!["TheoremProof"]);
    }

    #[test]
    fn test_find_standalone_proofs_with_mathlibok() {
        let content = r#"
\begin{proof}\proves{thm1}\mathlibok
  A mathlib proof.
\end{proof}
"#;
        let proofs = find_standalone_proofs(content, "file.tex");

        assert_eq!(proofs.len(), 1);
        assert!(proofs[0].mathlib_ok);
    }

    #[test]
    fn test_find_standalone_proofs_with_notready() {
        let content = r#"
\begin{proof}\proves{thm1}\notready
  A proof not ready.
\end{proof}
"#;
        let proofs = find_standalone_proofs(content, "file.tex");

        assert_eq!(proofs.len(), 1);
        assert!(proofs[0].not_ready);
    }

    #[test]
    fn test_find_standalone_proofs_with_discussion() {
        let content = r#"
\begin{proof}\proves{thm1}\discussion{789}
  A proof with discussion.
\end{proof}
"#;
        let proofs = find_standalone_proofs(content, "file.tex");

        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].discussion, vec!["789"]);
    }

    #[test]
    fn test_proof_with_all_macros() {
        let content = r#"
\begin{theorem}\label{my_thm}\lean{MyTheorem}\leanok\mathlibok\discussion{100}
  Statement.
\end{theorem}

\begin{proof}\leanok\mathlibok\uses{dep1}\lean{MyProof}\discussion{200}
  Proof.
\end{proof}
"#;
        let env_types: Vec<String> = vec!["theorem".to_string()];
        let envs = parse_tex_file(content, "file.tex", &env_types);

        assert_eq!(envs.len(), 1);
        // Spec fields
        assert!(envs[0].spec_ok);
        assert!(envs[0].mathlib_ok);
        assert_eq!(envs[0].discussion, vec!["100"]);
        assert_eq!(envs[0].code_name, Some("probe:MyTheorem".to_string()));

        // Proof fields
        assert_eq!(envs[0].proof_ok, Some(true));
        assert_eq!(envs[0].proof_mathlib_ok, Some(true));
        assert_eq!(envs[0].proof_discussion, Some(vec!["200".to_string()]));
        assert_eq!(envs[0].proof_dependencies, Some(vec!["dep1".to_string()]));
        assert_eq!(envs[0].proof_lean_names, Some(vec!["MyProof".to_string()]));
    }

    #[test]
    fn test_extract_home() {
        assert_eq!(
            extract_home(r"\home{https://example.com/project}"),
            Some("https://example.com/project".to_string())
        );
        assert_eq!(extract_home(r"no home here"), None);
    }

    #[test]
    fn test_extract_github() {
        assert_eq!(
            extract_github(r"\github{https://github.com/user/repo}"),
            Some("https://github.com/user/repo".to_string())
        );
        assert_eq!(extract_github(r"no github here"), None);
    }

    #[test]
    fn test_extract_dochome() {
        assert_eq!(
            extract_dochome(r"\dochome{https://docs.example.com/}"),
            Some("https://docs.example.com/".to_string())
        );
        assert_eq!(extract_dochome(r"no dochome here"), None);
    }

    #[test]
    fn test_extract_config() {
        let content = r#"
\home{https://example.com}
\github{https://github.com/user/repo}
\dochome{https://docs.example.com}
"#;
        let config = extract_config(content);
        assert_eq!(config.home, Some("https://example.com".to_string()));
        assert_eq!(
            config.github,
            Some("https://github.com/user/repo".to_string())
        );
        assert_eq!(config.dochome, Some("https://docs.example.com".to_string()));
    }

    #[test]
    fn test_extract_config_partial() {
        let content = r#"\github{https://github.com/user/repo}"#;
        let config = extract_config(content);
        assert_eq!(config.home, None);
        assert_eq!(
            config.github,
            Some("https://github.com/user/repo".to_string())
        );
        assert_eq!(config.dochome, None);
    }

    #[test]
    fn test_merge_config() {
        let base = Config {
            home: Some("base_home".to_string()),
            github: Some("base_github".to_string()),
            dochome: None,
        };
        let other = Config {
            home: None,
            github: Some("other_github".to_string()),
            dochome: Some("other_dochome".to_string()),
        };
        let merged = merge_config(base, other);
        assert_eq!(merged.home, Some("base_home".to_string())); // kept from base
        assert_eq!(merged.github, Some("other_github".to_string())); // overridden by other
        assert_eq!(merged.dochome, Some("other_dochome".to_string())); // added from other
    }
}

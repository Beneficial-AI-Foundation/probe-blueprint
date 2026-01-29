use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

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
    #[serde(rename = "stub-path")]
    pub stub_path: String,
    #[serde(rename = "stub-spec")]
    pub stub_spec: LineRange,
    #[serde(rename = "stub-proof", skip_serializing_if = "Option::is_none")]
    pub stub_proof: Option<LineRange>,
    pub labels: Vec<String>,
    #[serde(rename = "code-name", skip_serializing_if = "Option::is_none")]
    pub code_name: Option<String>,
    #[serde(rename = "spec-ok")]
    pub spec_ok: bool,
    #[serde(rename = "spec-dependencies")]
    pub spec_dependencies: Vec<String>,
    #[serde(rename = "proof-ok", skip_serializing_if = "Option::is_none")]
    pub proof_ok: Option<bool>,
    #[serde(rename = "proof-dependencies", skip_serializing_if = "Option::is_none")]
    pub proof_dependencies: Option<Vec<String>>,
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

/// Extract lean declaration from \lean{...}
fn extract_lean(content: &str) -> Option<String> {
    let re = Regex::new(r"\\lean\{([^}]+)\}").unwrap();
    re.captures(content).map(|caps| caps[1].trim().to_string())
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
    relative_path: String,
    spec_lines: LineRange,
    proof_lines: Option<LineRange>,
    labels: Vec<String>,
    code_name: Option<String>,
    spec_ok: bool,
    spec_dependencies: Vec<String>,
    proof_ok: Option<bool>,
    proof_dependencies: Option<Vec<String>>,
}

/// Proof match result with content and line range
struct ProofMatch {
    content: String,
    lines: LineRange,
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

        ProofMatch {
            content: caps[2].to_string(), // Content is now in group 2
            lines: LineRange {
                lines_start: byte_pos_to_line(content, proof_start),
                lines_end: byte_pos_to_line(content, proof_end - 1), // -1 to get line of last char
            },
        }
    })
}

/// Parse a single .tex file and extract environments
fn parse_tex_file(content: &str, relative_path: &str, env_types: &[String]) -> Vec<ParsedEnv> {
    let mut envs = Vec::new();

    // Strip LaTeX comments before parsing (preserves line structure)
    let content = strip_latex_comments(content);

    // Collect all environment matches with their positions
    struct EnvMatch {
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

        // Extract \lean{...}
        let code_name = extract_lean(env_content);

        // Check for \leanok
        let spec_ok = env_content.contains(r"\leanok");

        // Extract \uses{...}
        let spec_dependencies = extract_uses(env_content);

        // Look for a following proof environment
        let (proof_lines, proof_ok, proof_dependencies) =
            if let Some(proof_match) = find_following_proof(&content, env_match.end_pos) {
                // Add proof labels to the labels list
                let proof_labels = extract_all_labels(&proof_match.content);
                labels.extend(proof_labels);

                // Check for \leanok in proof
                let p_ok = if proof_match.content.contains(r"\leanok") {
                    Some(true)
                } else {
                    None
                };

                // Extract \uses{...} from proof
                let p_deps = extract_uses(&proof_match.content);
                let p_deps = if p_deps.is_empty() {
                    None
                } else {
                    Some(p_deps)
                };

                (Some(proof_match.lines), p_ok, p_deps)
            } else {
                (None, None, None)
            };

        envs.push(ParsedEnv {
            relative_path: relative_path.to_string(),
            spec_lines,
            proof_lines,
            labels,
            code_name,
            spec_ok,
            spec_dependencies,
            proof_ok,
            proof_dependencies,
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

    // Parse web.tex for environment types
    let web_tex_path = blueprint_src.join("web.tex");
    let env_types = if web_tex_path.exists() {
        let web_tex_content = fs::read_to_string(&web_tex_path)?;
        parse_thms_option(&web_tex_content)
    } else {
        DEFAULT_ENVS.iter().map(|s| s.to_string()).collect()
    };

    eprintln!("Looking for environments: {}", env_types.join(", "));

    // Collect all parsed environments
    let mut all_envs: Vec<ParsedEnv> = Vec::new();

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

            // Get path relative to blueprint/src
            let relative_path = path
                .strip_prefix(&blueprint_src)?
                .to_str()
                .ok_or("Invalid UTF-8 in path")?;

            let envs = parse_tex_file(&content, relative_path, &env_types);
            all_envs.extend(envs);
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
                stub_path: env.relative_path,
                stub_spec: env.spec_lines,
                stub_proof: env.proof_lines,
                labels: env.labels,
                code_name: env.code_name,
                spec_ok: env.spec_ok,
                spec_dependencies: env.spec_dependencies,
                proof_ok: env.proof_ok,
                proof_dependencies: env.proof_dependencies,
            },
        );
    }

    eprintln!("Found {} stubs", all_stubs.len());

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
            Some("Subgraph.Equation387_implies_Equation43".to_string())
        );
        assert_eq!(extract_lean(r"no lean here"), None);
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
        assert_eq!(envs[0].labels, vec!["387_implies_43"]);
        assert_eq!(
            envs[0].code_name,
            Some("Subgraph.Equation387_implies_Equation43".to_string())
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
}

use std::error::Error;

/// Convert .md files with YAML frontmatter to JSON
pub fn run(path: &str, output: &str) -> Result<(), Box<dyn Error>> {
    eprintln!("stubify: path={path}, output={output}");
    todo!("stubify command not yet implemented")
}

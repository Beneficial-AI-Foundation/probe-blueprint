use std::error::Error;

/// Generate call graph atoms with line numbers
pub fn run(project_path: &str, output: &str) -> Result<(), Box<dyn Error>> {
    eprintln!("atomize: project_path={project_path}, output={output}");
    todo!("atomize command not yet implemented")
}

use std::error::Error;

/// Run Blueprint verification and analyze results
pub fn run(
    project_path: Option<&str>,
    output: &str,
    with_atoms: Option<Option<String>>,
) -> Result<(), Box<dyn Error>> {
    eprintln!("verify: project_path={project_path:?}, output={output}, with_atoms={with_atoms:?}");
    todo!("verify command not yet implemented")
}

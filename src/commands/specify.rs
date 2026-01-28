use std::error::Error;

/// Extract function specifications
pub fn run(path: &str, output: &str, with_atoms: Option<&str>) -> Result<(), Box<dyn Error>> {
    eprintln!("specify: path={path}, output={output}, with_atoms={with_atoms:?}");
    todo!("specify command not yet implemented")
}

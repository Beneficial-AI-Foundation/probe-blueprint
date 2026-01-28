use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "probe-blueprint")]
#[command(
    about = "Probe Blueprint projects: generate call graph atoms and analyze verification results for Lean 4"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert .md files with YAML frontmatter to JSON
    Stubify {
        /// Path to the directory containing .md files
        path: String,

        /// Output file path
        #[arg(short, long, default_value = "stubs.json")]
        output: String,
    },

    /// Generate call graph atoms with line numbers
    Atomize {
        /// Path to the project
        project_path: String,

        /// Output file path
        #[arg(short, long, default_value = "atoms.json")]
        output: String,
    },

    /// Extract function specifications
    Specify {
        /// Path to the project
        path: String,

        /// Output file path
        #[arg(short, long, default_value = "specs.json")]
        output: String,

        /// Path to atoms.json for code-name lookup
        #[arg(short = 'a', long = "with-atoms")]
        with_atoms: Option<String>,
    },

    /// Run Blueprint verification and analyze results
    Verify {
        /// Path to the project
        project_path: Option<String>,

        /// Output file path
        #[arg(short, long, default_value = "proofs.json")]
        output: String,

        /// Enrich results with code-names from atoms.json
        #[arg(short = 'a', long = "with-atoms")]
        with_atoms: Option<Option<String>>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Stubify { path, output } => commands::stubify::run(&path, &output),
        Commands::Atomize {
            project_path,
            output,
        } => commands::atomize::run(&project_path, &output),
        Commands::Specify {
            path,
            output,
            with_atoms,
        } => commands::specify::run(&path, &output, with_atoms.as_deref()),
        Commands::Verify {
            project_path,
            output,
            with_atoms,
        } => commands::verify::run(project_path.as_deref(), &output, with_atoms),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

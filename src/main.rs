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
    /// Extract Blueprint stubs from LaTeX files in blueprint/src
    Stubify {
        /// Path to the project root (must contain blueprint/src)
        project_path: String,

        /// Output file path
        #[arg(short, long, default_value = ".verilib/stubs.json")]
        output: String,
    },

    /// Generate call graph atoms with line numbers
    Atomize {
        /// Path to the project root (must contain blueprint/src)
        project_path: String,

        /// Output file path
        #[arg(short, long, default_value = ".verilib/atoms.json")]
        output: String,

        /// Regenerate stubs.json even if it exists
        #[arg(long)]
        regenerate_stubs: bool,
    },

    /// Extract function specifications
    Specify {
        /// Path to the project root (must contain blueprint/src)
        project_path: String,

        /// Output file path
        #[arg(short, long, default_value = ".verilib/specs.json")]
        output: String,

        /// Regenerate stubs.json even if it exists
        #[arg(long)]
        regenerate_stubs: bool,

        /// Enrich results with atoms.json (reserved for future use)
        #[arg(short = 'a', long = "with-atoms")]
        with_atoms: Option<Option<String>>,
    },

    /// Extract proof verification status
    Verify {
        /// Path to the project root (must contain blueprint/src)
        project_path: String,

        /// Output file path
        #[arg(short, long, default_value = ".verilib/proofs.json")]
        output: String,

        /// Regenerate stubs.json even if it exists
        #[arg(long)]
        regenerate_stubs: bool,

        /// Enrich results with atoms.json (reserved for future use)
        #[arg(short = 'a', long = "with-atoms")]
        with_atoms: Option<Option<String>>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Stubify {
            project_path,
            output,
        } => commands::stubify::run(&project_path, &output),
        Commands::Atomize {
            project_path,
            output,
            regenerate_stubs,
        } => commands::atomize::run(&project_path, &output, regenerate_stubs),
        Commands::Specify {
            project_path,
            output,
            regenerate_stubs,
            with_atoms,
        } => commands::specify::run(&project_path, &output, regenerate_stubs, with_atoms),
        Commands::Verify {
            project_path,
            output,
            regenerate_stubs,
            with_atoms,
        } => commands::verify::run(&project_path, &output, regenerate_stubs, with_atoms),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

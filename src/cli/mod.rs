//! CLI for the `raskell` binary.

use clap::{Parser, Subcommand};
use crate::VERSION;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "raskell")]
#[command(version = VERSION)]
#[command(about = "Raskell — write Rust, get idiomatic Haskell", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Type-check / analyse a Rust source without emitting Haskell
    Check {
        /// Input .rs file
        file: PathBuf,
    },
    /// Translate Rust source to Haskell
    Translate {
        /// Input .rs file
        file: PathBuf,
        /// Output .hs file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Explain translation decisions for a Rust source
    Explain {
        /// Input .rs file
        file: PathBuf,
    },
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // clap --help / --version are automatic; without subcommand show brief usage
            eprintln!("raskell {VERSION}");
            eprintln!("Usage: raskell <COMMAND>");
            eprintln!("Commands: check, translate, explain");
            eprintln!("Try `raskell --help` for more information.");
            ExitCode::SUCCESS
        }
        Some(Commands::Check { file }) => match crate::check_file(&file) {
            Ok(diags) => {
                if !diags.is_empty() {
                    eprint!("{diags}");
                }
                if diags.has_errors() {
                    ExitCode::from(1)
                } else {
                    println!("OK — {} is a supported Raskell program", file.display());
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(1)
            }
        },
        Some(Commands::Translate { file, output }) => match crate::translate_file(&file) {
            Ok(result) => {
                if result.diagnostics.has_errors() {
                    eprint!("{}", result.diagnostics);
                    return ExitCode::from(1);
                }
                if !result.diagnostics.is_empty() {
                    eprint!("{}", result.diagnostics);
                }
                match output {
                    Some(path) => {
                        if let Err(e) = std::fs::write(&path, &result.haskell_source) {
                            eprintln!("error: failed to write {}: {e}", path.display());
                            return ExitCode::from(1);
                        }
                        eprintln!("Wrote {}", path.display());
                    }
                    None => {
                        print!("{}", result.haskell_source);
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(1)
            }
        },
        Some(Commands::Explain { file }) => match crate::translate_file(&file) {
            Ok(result) => {
                if result.diagnostics.has_errors() {
                    eprint!("{}", result.diagnostics);
                    return ExitCode::from(1);
                }
                if result.explanations.is_empty() {
                    println!("No special translation patterns detected.");
                    println!("Generated Haskell:\n");
                    println!("{}", result.haskell_source);
                } else {
                    for (i, expl) in result.explanations.iter().enumerate() {
                        if i > 0 {
                            println!();
                        }
                        print!("{}", expl.format());
                    }
                    println!("\n--- Full module ---\n");
                    print!("{}", result.haskell_source);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(1)
            }
        },
    }
}

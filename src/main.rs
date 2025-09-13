//! This crate implements a WIP mypy parser command line tool.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;

mod print_ast;
mod print_tokens;
mod serialize_ast;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
#[expect(clippy::large_enum_variant)]
enum Command {
    /// Print the AST for a given Python file.
    PrintAST(print_ast::Args),
    /// Print the token stream for a given Python file.
    PrintTokens(print_tokens::Args),
    // Serialize AST into mypy format.
    SerializeAST(serialize_ast::Args),
}

fn main() -> Result<ExitCode> {
    let Args { command } = Args::parse();
    #[expect(clippy::print_stdout)]
    match command {
        Command::PrintAST(args) => print_ast::main(&args)?,
        Command::PrintTokens(args) => print_tokens::main(&args)?,
        Command::SerializeAST(args) => serialize_ast::main(&args)?,
    }
    Ok(ExitCode::SUCCESS)
}

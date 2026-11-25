//! Ceasefire Line CLI: balance and asset tools on top of the rules core.

mod assets_check;
mod balance;
mod combat_matrix;

use std::process::ExitCode;

fn usage() -> String {
    "usage:\n  sim combat-matrix [--data DIR] [--runs N] [--seed S] [--strength N] [--readiness PCT]\n  \
     sim assets-check [--assets DIR] [--data DIR] [--report FILE] [--fix]\n  \
     sim balance [--data DIR] [--runs N] [--seed S] [--report FILE]"
        .to_owned()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("combat-matrix") => {
            combat_matrix::parse_options(&args[1..]).and_then(|o| combat_matrix::combat_matrix(&o))
        }
        Some("balance") => balance::parse_options(&args[1..]).and_then(|o| balance::run(&o)),
        Some("assets-check") => {
            assets_check::parse_options(&args[1..]).and_then(|o| assets_check::run(&o))
        }
        _ => Err(usage()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

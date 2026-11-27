//! Ceasefire Line CLI: balance and asset tools on top of the rules core.

mod ai_vs_ai;
mod assets_check;
mod balance;
mod battles;
mod check_data;
mod combat_matrix;
mod supply_reach;
mod tactical_sim;

use std::process::ExitCode;

fn usage() -> String {
    "usage:\n  sim combat-matrix [--data DIR] [--runs N] [--seed S] [--strength N] [--readiness PCT]\n  \
     sim assets-check [--assets DIR] [--data DIR] [--report FILE] [--fix]\n  \
     sim balance [--data DIR] [--runs N] [--seed S] [--report FILE]\n  \
     sim ai-vs-ai [--data DIR] [--scenario ID] [--runs N] [--seed S] [--report FILE] [--json FILE]\n    [--players v1,v1] [--variant NAME,..] [--set section.key=JSON] [--variance binomial|sc] [--watch c,r;c,r] [--dump-battles DIR]\n  \
     sim supply-reach [--data DIR] [--scenario ID] [--side N] [--min S]\n  \
     sim check-data [--data DIR]\n  \
     sim tactical --snapshots DIR [--games N] [--limit M] [--mode alternating|igougo|both] [--report FILE] [--json FILE]"
        .to_owned()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("combat-matrix") => {
            combat_matrix::parse_options(&args[1..]).and_then(|o| combat_matrix::combat_matrix(&o))
        }
        Some("balance") => balance::parse_options(&args[1..]).and_then(|o| balance::run(&o)),
        Some("ai-vs-ai") => ai_vs_ai::parse_options(&args[1..]).and_then(|o| ai_vs_ai::run(&o)),
        Some("check-data") => {
            check_data::parse_options(&args[1..]).and_then(|o| check_data::run(&o))
        }
        Some("tactical") => {
            tactical_sim::parse_options(&args[1..]).and_then(|o| tactical_sim::run(&o))
        }
        Some("supply-reach") => {
            supply_reach::parse_options(&args[1..]).and_then(|o| supply_reach::run(&o))
        }
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

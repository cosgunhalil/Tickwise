//! The tickwise binary. Three commands in v1: compare, diff, inspect.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(tickwise_cli::run(&args))
}

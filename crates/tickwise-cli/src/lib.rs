//! Library side of the `tickwise` binary.
//!
//! The binary is a thin shell over this crate so integration tests can
//! drive the commands directly.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod compare;
pub mod diff;
pub mod inspect;

const USAGE: &str = "\
Tickwise: record, replay, and diff deterministic simulations.

Usage:
  tickwise inspect <session.rec>    show metadata and statistics for a recording
  tickwise compare <a.rec> <b.rec>  find the first divergent tick
  tickwise diff <a.dump> <b.dump>   field-level structural diff of two state dumps

Diff flags:
  --strict             every bit-level float difference counts as exact
  --epsilon-f32 <x>    sub-epsilon threshold for f32, default 1e-5
  --epsilon-f64 <x>    sub-epsilon threshold for f64, default 1e-12
  --all                show every difference instead of the first 100 per tick
  --no-color           plain output, also honored via the NO_COLOR variable

Exit codes for compare and diff: 0 identical, 1 differences found, 2 trouble.

Options:
  -h, --help       show this help
  -V, --version    show the version
";

fn color_allowed() -> bool {
    use std::io::IsTerminal;
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// Runs the CLI with the given arguments, excluding the program name,
/// and returns the process exit code.
pub fn run(args: &[String]) -> u8 {
    let Some((command, rest)) = args.split_first() else {
        eprint!("{USAGE}");
        return 2;
    };

    match command.as_str() {
        "inspect" => match rest {
            [path] => match inspect::render(path) {
                Ok(report) => {
                    print!("{}", report.text);
                    u8::from(report.corrupt)
                }
                Err(err) => {
                    eprintln!("tickwise inspect: {err}");
                    1
                }
            },
            _ => {
                eprintln!("usage: tickwise inspect <session.rec>");
                2
            }
        },
        "compare" => match rest {
            [a, b] => match compare::render(a, b) {
                Ok(output) => {
                    print!("{}", output.text);
                    u8::from(output.diverged)
                }
                Err(err) => {
                    eprintln!("tickwise compare: {err}");
                    2
                }
            },
            _ => {
                eprintln!("usage: tickwise compare <a.rec> <b.rec>");
                2
            }
        },
        "diff" => match diff::parse_args(rest) {
            Ok((a, b, mut options)) => {
                options.color = options.color && color_allowed();
                match diff::render(&a, &b, &options) {
                    Ok(output) => {
                        print!("{}", output.text);
                        u8::from(output.differs)
                    }
                    Err(err) => {
                        eprintln!("tickwise diff: {err}");
                        2
                    }
                }
            }
            Err(message) => {
                eprintln!("{message}");
                2
            }
        },
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            0
        }
        "-V" | "--version" => {
            println!("tickwise {}", env!("CARGO_PKG_VERSION"));
            0
        }
        other => {
            eprintln!("tickwise: unknown command {other}");
            eprint!("{USAGE}");
            2
        }
    }
}

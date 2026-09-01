//! Library side of the `tickwise` binary.
//!
//! The binary is a thin shell over this crate so integration tests can
//! drive the commands directly.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod compare;
pub mod inspect;

const USAGE: &str = "\
Tickwise: record, replay, and diff deterministic simulations.

Usage:
  tickwise inspect <session.rec>    show metadata and statistics for a recording
  tickwise compare <a.rec> <b.rec>  find the first divergent tick
  tickwise diff <a.dump> <b.dump>   field-level structural diff, arrives with M3

Compare exit codes: 0 identical, 1 diverged, 2 trouble.

Options:
  -h, --help       show this help
  -V, --version    show the version
";

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
        "diff" => {
            eprintln!("tickwise diff is not implemented yet, it arrives with M3");
            1
        }
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

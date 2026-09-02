//! The diff command: field-level structural diff of two dump files.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tickwise::diff::{DiffClass, DiffError, DiffReport, FloatPolicy, structural_from};
use tickwise::format::{FormatError, RecReader};

/// Differences shown per tick before the output is truncated, unless
/// `show_all` is set.
const DEFAULT_LIMIT: usize = 100;

/// Command line options for the diff command. The default is plain,
/// uncolored output with the default float policy.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiffOptions {
    /// Float classification policy.
    pub policy: FloatPolicy,
    /// Emit ANSI colors.
    pub color: bool,
    /// Show every difference instead of truncating long lists.
    pub show_all: bool,
}

/// Rendered diff output plus the verdict for the exit code.
pub struct DiffOutput {
    /// Human-readable report text.
    pub text: String,
    /// True when any common tick differs.
    pub differs: bool,
}

/// Parses the arguments after `diff`: two paths plus optional flags.
pub fn parse_args(args: &[String]) -> Result<(String, String, DiffOptions), String> {
    let mut paths = Vec::new();
    let mut options = DiffOptions {
        color: true,
        ..DiffOptions::default()
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--strict" => options.policy = FloatPolicy::strict(),
            "--no-color" => options.color = false,
            "--all" => options.show_all = true,
            "--epsilon-f32" => {
                let value = iter.next().ok_or("--epsilon-f32 needs a value")?;
                options.policy.epsilon_f32 = value
                    .parse()
                    .map_err(|_| format!("--epsilon-f32: {value:?} is not a number"))?;
            }
            "--epsilon-f64" => {
                let value = iter.next().ok_or("--epsilon-f64 needs a value")?;
                options.policy.epsilon_f64 = value
                    .parse()
                    .map_err(|_| format!("--epsilon-f64: {value:?} is not a number"))?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            path => paths.push(path.to_string()),
        }
    }
    match paths.as_slice() {
        [a, b] => Ok((a.clone(), b.clone(), options)),
        _ => Err(
            "usage: tickwise diff <a.dump> <b.dump> [--strict] [--epsilon-f32 X] \
                  [--epsilon-f64 X] [--all] [--no-color]"
                .to_string(),
        ),
    }
}

struct Palette {
    enabled: bool,
}

impl Palette {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
    fn class(&self, class: DiffClass) -> String {
        match class {
            DiffClass::Structural => self.paint("1;31", "structural"),
            DiffClass::Exact => self.paint("33", "exact     "),
            DiffClass::SubEpsilonFloat => self.paint("36", "drift     "),
        }
    }
    fn good(&self, text: &str) -> String {
        self.paint("32", text)
    }
    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
}

fn tick_list(ticks: &[u64]) -> String {
    let shown: Vec<String> = ticks.iter().take(8).map(u64::to_string).collect();
    if ticks.len() > 8 {
        format!("{} and {} more", shown.join(", "), ticks.len() - 8)
    } else {
        shown.join(", ")
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Diffs the dump files at the two paths.
pub fn render<A: AsRef<Path>, B: AsRef<Path>>(
    a: A,
    b: B,
    options: &DiffOptions,
) -> Result<DiffOutput, DiffError> {
    let path_a = a.as_ref();
    let path_b = b.as_ref();
    let mut reader_a = RecReader::open(BufReader::new(
        File::open(path_a).map_err(FormatError::from)?,
    ))?;
    let mut reader_b = RecReader::open(BufReader::new(
        File::open(path_b).map_err(FormatError::from)?,
    ))?;
    let meta_a = reader_a.header().meta.clone();
    let meta_b = reader_b.header().meta.clone();

    let report = structural_from(&mut reader_a, &mut reader_b, options.policy)?;
    let palette = Palette {
        enabled: options.color,
    };

    let mut s = String::new();
    s.push_str(&format!(
        "diffing {} and {}\n\n",
        path_a.display(),
        path_b.display()
    ));
    let common: Vec<u64> = report.ticks.iter().map(|t| t.tick).collect();
    let describe = |meta: &tickwise::SessionMeta, extra: &[u64]| {
        let mut ticks: Vec<u64> = common.iter().chain(extra.iter()).copied().collect();
        ticks.sort_unstable();
        format!(
            "game {}, seed {:#x}, dumps at ticks {}",
            if meta.game_id.is_empty() {
                "unset"
            } else {
                &meta.game_id
            },
            meta.rng_seed,
            tick_list(&ticks)
        )
    };
    s.push_str(&format!(
        "  first          {}\n",
        describe(&meta_a, &report.only_in_a)
    ));
    s.push_str(&format!(
        "  second         {}\n",
        describe(&meta_b, &report.only_in_b)
    ));
    s.push_str(&format!(
        "  float policy   f32 epsilon {:e}, f64 epsilon {:e}\n\n",
        report.policy.epsilon_f32, report.policy.epsilon_f64
    ));

    let mut total = 0;
    for tick in &report.ticks {
        let n = tick.differences.len();
        total += n;
        if tick.is_identical() {
            s.push_str(&format!(
                "tick {:<10} {}\n\n",
                tick.tick,
                palette.good(&format!("identical over {} fields", tick.fields_compared))
            ));
            continue;
        }
        s.push_str(&format!(
            "tick {:<10} {} over {} fields: {} structural, {} exact, {} sub-epsilon float drift\n",
            tick.tick,
            plural(n, "difference", "differences"),
            tick.fields_compared,
            tick.count(DiffClass::Structural),
            tick.count(DiffClass::Exact),
            tick.count(DiffClass::SubEpsilonFloat),
        ));
        let limit = if options.show_all { n } else { DEFAULT_LIMIT };
        for difference in tick.differences.iter().take(limit) {
            s.push_str(&format!(
                "  {}     {}: {}\n",
                palette.class(difference.class),
                difference.path,
                difference.detail
            ));
        }
        if n > limit {
            s.push_str(&palette.dim(&format!(
                "  ... {} more, pass --all to see every difference\n",
                n - limit
            )));
        }
        s.push('\n');
    }

    if !report.only_in_a.is_empty() {
        s.push_str(&format!(
            "  only in first  dumps at ticks {}, no counterpart to diff\n",
            tick_list(&report.only_in_a)
        ));
    }
    if !report.only_in_b.is_empty() {
        s.push_str(&format!(
            "  only in second dumps at ticks {}, no counterpart to diff\n",
            tick_list(&report.only_in_b)
        ));
    }

    let differs = !report.is_identical();
    s.push_str(&format!(
        "  verdict        {}\n",
        verdict_line(&report, total, &palette)
    ));
    s.push_str(&format!("  next           {}\n", next_hint(&report)));

    Ok(DiffOutput { text: s, differs })
}

fn verdict_line(report: &DiffReport, total: usize, palette: &Palette) -> String {
    if report.is_identical() {
        palette.good(&format!(
            "identical at {}",
            plural(report.ticks.len(), "compared tick", "compared ticks")
        ))
    } else {
        format!(
            "{} across {}",
            plural(total, "difference", "differences"),
            plural(report.ticks.len(), "compared tick", "compared ticks")
        )
    }
}

fn next_hint(report: &DiffReport) -> String {
    if report.is_identical() {
        return "the dumps agree. If tickwise compare reported a divergence at this tick, \
                the diverging state is not covered by state_dump, extend it"
            .to_string();
    }
    let structural: usize = report
        .ticks
        .iter()
        .map(|t| t.count(DiffClass::Structural))
        .sum();
    let exact: usize = report.ticks.iter().map(|t| t.count(DiffClass::Exact)).sum();
    if structural > 0 {
        "structural differences usually mean a collection in unspecified order or a field \
         missed by a snapshot. Start with the first structural entry above"
            .to_string()
    } else if exact > 0 {
        "an exact difference at the first divergent tick is your lead. Trace that field's \
         last write backwards through the tick"
            .to_string()
    } else {
        "only sub-epsilon float drift. If you target cross-platform determinism, rerun with \
         --strict to see every bit, and consider fixed-point math for these fields"
            .to_string()
    }
}

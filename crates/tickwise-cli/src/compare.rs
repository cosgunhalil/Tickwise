//! The compare command: the first divergent tick between two recordings.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use tickwise::compare::{CompareError, Outcome, first_divergence_from};
use tickwise::format::{FormatError, RecReader};

/// Rendered comparison output plus the verdict for the exit code.
pub struct CompareOutput {
    /// Human-readable report text.
    pub text: String,
    /// True when the recordings diverge.
    pub diverged: bool,
}

fn describe<R: Read + Seek>(reader: &RecReader<R>) -> String {
    let meta = &reader.header().meta;
    let game = if meta.game_id.is_empty() {
        "unset"
    } else {
        &meta.game_id
    };
    format!(
        "{} ticks, game {game}, seed {:#x}",
        reader.tick_count(),
        meta.rng_seed
    )
}

/// Compares the recordings at the two paths.
pub fn render<A: AsRef<Path>, B: AsRef<Path>>(a: A, b: B) -> Result<CompareOutput, CompareError> {
    let path_a = a.as_ref();
    let path_b = b.as_ref();
    let mut reader_a = RecReader::open(BufReader::new(
        File::open(path_a).map_err(FormatError::from)?,
    ))?;
    let mut reader_b = RecReader::open(BufReader::new(
        File::open(path_b).map_err(FormatError::from)?,
    ))?;

    let mut s = String::new();
    s.push_str(&format!(
        "comparing {} and {}\n\n",
        path_a.display(),
        path_b.display()
    ));
    s.push_str(&format!("  first          {}\n", describe(&reader_a)));
    s.push_str(&format!("  second         {}\n\n", describe(&reader_b)));

    let report = first_divergence_from(&mut reader_a, &mut reader_b)?;

    for warning in &report.warnings {
        s.push_str(&format!("  warning        {warning}\n"));
    }
    if !report.warnings.is_empty() {
        s.push('\n');
    }

    s.push_str(&format!("  verdict        {report}\n\n"));

    let diverged = match &report.outcome {
        Outcome::Identical { .. } => {
            s.push_str(
                "  next           the recordings agree. To self-check your own replay\n\
                 \x20                determinism, replay one session and record it again,\n\
                 \x20                then compare the two recordings\n",
            );
            false
        }
        Outcome::Diverged(d) => {
            let a = path_a.display();
            let b = path_b.display();
            if let Some(at) = report.shared_dump_at_or_after(d.tick) {
                // Both sides carry a dump past the divergence: field level
                // is one command away and no replay is needed.
                s.push_str(&format!(
                    "  next           both recordings carry state dumps. Tick {at} is the first\n\
                     \x20                dump at or after the divergence, so diff there directly:\n\
                     \x20                tickwise diff {a} {b} --at {at}\n"
                ));
                if let Some(before) = report.shared_dump_at_or_before(d.tick.saturating_sub(1)) {
                    s.push_str(&format!(
                        "  \x20              tick {before} holds the last dump where they still agreed,\n\
                         \x20                for a before and after view\n"
                    ));
                }
            } else if report.has_dumps() {
                s.push_str(&format!(
                    "  next           the recordings carry state dumps, but none at a shared tick\n\
                     \x20                at or after the divergence. Pass 2: replay each recording\n\
                     \x20                with dump_at_ticks = [{}] and run tickwise diff on the\n\
                     \x20                results, or record with a dump_interval that lands past\n\
                     \x20                the strike next time\n",
                    d.tick
                ));
            } else {
                s.push_str(&format!(
                    "  next           Pass 2: replay each recording in your own loop with\n\
                     \x20                dump_at_ticks = [{}] to produce two .dump files, then run\n\
                     \x20                tickwise diff a.dump b.dump. If the desync does not\n\
                     \x20                reproduce on replay, record with dump_interval set and\n\
                     \x20                diff the recordings themselves next time\n",
                    d.tick
                ));
            }
            true
        }
    };

    Ok(CompareOutput { text: s, diverged })
}

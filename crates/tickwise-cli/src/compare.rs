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
            s.push_str(&format!(
                "  next           Pass 2: replay each recording in your own loop with\n\
                 \x20                dump_at_ticks = [{}] to produce two .dump files,\n\
                 \x20                then run tickwise diff on them. The replayer and the\n\
                 \x20                diff command arrive with M3\n",
                d.tick
            ));
            true
        }
    };

    Ok(CompareOutput { text: s, diverged })
}

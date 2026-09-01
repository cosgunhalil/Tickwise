//! The inspect command: metadata and statistics for a recording.

use std::collections::BTreeMap;
use std::io::BufReader;
use std::path::Path;
use tickwise::format::{Chunk, FormatError, RecReader, SnapshotPolicy, kind};

/// Rendered inspection output plus an integrity verdict.
pub struct Report {
    /// Human-readable report text.
    pub text: String,
    /// True when the file failed an integrity or structure check.
    pub corrupt: bool,
}

/// Inspects the recording at the given path.
pub fn render<P: AsRef<Path>>(path: P) -> Result<Report, FormatError> {
    let path = path.as_ref();
    let file_size = std::fs::metadata(path)?.len();
    let file = std::fs::File::open(path)?;
    let mut reader = RecReader::open(BufReader::new(file))?;

    let header = reader.header().clone();
    let version = reader.version();
    let tick_count = reader.tick_count();

    let mut input_frames: u64 = 0;
    let mut light_batches: u64 = 0;
    let mut light_hashes: u64 = 0;
    let mut full_hashes: u64 = 0;
    let mut snapshot_ticks: Vec<u64> = Vec::new();
    let mut markers: u64 = 0;
    let mut unknown_chunks: u64 = 0;
    let mut stream_error: Option<FormatError> = None;

    for item in reader.chunks()? {
        match item {
            Ok(Chunk::InputFrame { .. }) => input_frames += 1,
            Ok(Chunk::LightHashBatch { hashes, .. }) => {
                light_batches += 1;
                light_hashes += hashes.len() as u64;
            }
            Ok(Chunk::FullHash { .. }) => full_hashes += 1,
            Ok(Chunk::Snapshot { tick, .. }) => snapshot_ticks.push(tick),
            Ok(Chunk::Marker { .. }) => markers += 1,
            Ok(Chunk::Unknown { .. }) => unknown_chunks += 1,
            Err(err) => {
                stream_error = Some(err);
                break;
            }
        }
    }

    // Byte sizes come from the seek index. BTreeMap keeps the iteration
    // order deterministic, per the project's own determinism rules.
    let mut bytes_by_kind: BTreeMap<u16, u64> = BTreeMap::new();
    let mut index_error: Option<FormatError> = None;
    match reader.read_index() {
        Ok(entries) => {
            for entry in entries {
                *bytes_by_kind.entry(entry.kind).or_insert(0) += u64::from(entry.len) + 6;
            }
        }
        Err(err) => index_error = Some(err),
    }

    let checksum_line = match reader.verify_checksum() {
        Ok(()) => "checksum ok".to_string(),
        Err(FormatError::ChecksumMismatch { stored, computed }) => {
            format!("CHECKSUM MISMATCH, stored {stored:016x}, computed {computed:016x}")
        }
        Err(err) => return Err(err),
    };
    let corrupt =
        checksum_line.starts_with("CHECKSUM") || stream_error.is_some() || index_error.is_some();

    let meta = &header.meta;
    let config = &header.config;
    let mut s = String::new();

    s.push_str(&format!("{}\n", path.display()));
    s.push_str(&format!("  format         version {version}\n"));
    s.push_str(&format!("  game           {}\n", or_unset(&meta.game_id)));
    s.push_str(&format!(
        "  build          {}\n",
        or_unset(&meta.build_hash)
    ));
    s.push_str(&format!("  platform       {}\n", or_unset(&meta.platform)));
    s.push_str(&format!(
        "  tick rate      {} ticks per second\n",
        meta.tick_rate
    ));
    s.push_str(&format!("  rng seed       {:#018x}\n", meta.rng_seed));
    s.push_str(&format!("  created at     unix {}\n", meta.created_at));
    s.push_str(&match config.full_hash_interval {
        0 => "  full hashes    disabled\n".to_string(),
        n => format!("  full hashes    every {n} ticks\n"),
    });
    s.push_str(&match config.snapshot_policy {
        SnapshotPolicy::Off => "  snapshots      off\n".to_string(),
        SnapshotPolicy::Every(n) => format!("  snapshots      every {n} ticks\n"),
    });
    s.push_str(&format!("  hash algo      id {}\n", config.hash_algo_id));
    s.push_str(&format!("  input format   id {}\n", config.input_format_id));
    s.push('\n');
    s.push_str(&format!("  ticks          {tick_count}\n"));
    s.push_str(&format!("  file size      {}\n", human_bytes(file_size)));
    s.push('\n');
    s.push_str("  chunks\n");

    let row = |label: &str, count: u64, bytes: Option<&u64>, note: &str| -> String {
        let size = bytes.map_or("n/a".to_string(), |b| human_bytes(*b));
        let mut line = format!("    {label:<22} {count:>8}   {size:>10}");
        if !note.is_empty() {
            line.push_str("   ");
            line.push_str(note);
        }
        line.push('\n');
        line
    };

    s.push_str(&row(
        "input frames",
        input_frames,
        bytes_by_kind.get(&kind::INPUT_FRAME),
        "repeat suppressed",
    ));
    s.push_str(&row(
        "light hash batches",
        light_batches,
        bytes_by_kind.get(&kind::LIGHT_HASH_BATCH),
        &format!("holding {light_hashes} hashes"),
    ));
    s.push_str(&row(
        "full hashes",
        full_hashes,
        bytes_by_kind.get(&kind::FULL_HASH),
        "",
    ));
    s.push_str(&row(
        "snapshots",
        snapshot_ticks.len() as u64,
        bytes_by_kind.get(&kind::SNAPSHOT),
        &snapshot_note(&snapshot_ticks),
    ));
    s.push_str(&row(
        "markers",
        markers,
        bytes_by_kind.get(&kind::MARKER),
        "",
    ));
    if unknown_chunks > 0 {
        let unknown_bytes: u64 = bytes_by_kind
            .iter()
            .filter(|(k, _)| !is_known_kind(**k))
            .map(|(_, b)| *b)
            .sum();
        s.push_str(&row(
            "unknown kinds",
            unknown_chunks,
            Some(&unknown_bytes),
            "skipped safely",
        ));
    }
    s.push('\n');

    if let Some(err) = &stream_error {
        s.push_str(&format!("  warning        chunk stream error: {err}\n"));
    }
    if let Some(err) = &index_error {
        s.push_str(&format!("  warning        index unreadable: {err}\n"));
    }
    s.push_str(&format!("  integrity      {checksum_line}\n"));
    s.push_str(
        "  next           record a second session, then find the first divergent tick:\n\
         \x20                tickwise compare a.rec b.rec\n",
    );

    Ok(Report { text: s, corrupt })
}

fn is_known_kind(id: u16) -> bool {
    matches!(
        id,
        kind::INPUT_FRAME
            | kind::LIGHT_HASH_BATCH
            | kind::FULL_HASH
            | kind::SNAPSHOT
            | kind::MARKER
    )
}

fn or_unset(value: &str) -> &str {
    if value.is_empty() { "unset" } else { value }
}

fn snapshot_note(ticks: &[u64]) -> String {
    match ticks {
        [] => String::new(),
        few if few.len() <= 8 => {
            let list: Vec<String> = few.iter().map(u64::to_string).collect();
            format!("at ticks {}", list.join(", "))
        }
        many => format!(
            "first at tick {}, last at tick {}",
            many[0],
            many[many.len() - 1]
        ),
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_picks_sane_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn snapshot_notes_stay_short() {
        assert_eq!(snapshot_note(&[]), "");
        assert_eq!(snapshot_note(&[0, 100]), "at ticks 0, 100");
        let many: Vec<u64> = (0..20).map(|i| i * 100).collect();
        assert_eq!(snapshot_note(&many), "first at tick 0, last at tick 1900");
    }
}

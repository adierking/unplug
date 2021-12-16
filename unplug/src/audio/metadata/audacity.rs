// See <https://manual.audacityteam.org/man/importing_and_exporting_labels.html> for details on the
// label track format.

use crate::audio::cue::{self, Cue, CueKind};
use crate::audio::Result;
use std::borrow::Cow;
use std::io::{BufRead, Write};

/// Writes a set of cues to `writer` in Audacity's label track format. `sample_rate` is the sample
/// rate of the source audio stream, needed to calculate time points for cues.
pub fn write_labels(
    mut writer: impl Write,
    cues: impl IntoIterator<Item = Cue>,
    sample_rate: u32,
) -> Result<()> {
    let mut cues = cues.into_iter();
    write_labels_impl(&mut writer, &mut cues, sample_rate)
}

fn write_labels_impl(
    writer: &mut dyn Write,
    cues: &mut dyn Iterator<Item = Cue>,
    sample_rate: u32,
) -> Result<()> {
    for cue in cues {
        let start = (cue.start as f64) / (sample_rate as f64);
        let end = ((cue.start + cue.duration()) as f64) / (sample_rate as f64);
        let name = match cue.kind {
            CueKind::Loop => cue::add_loop_prefix(&*cue.name),
            CueKind::Point | CueKind::Range(_) => Cow::from(&*cue.name),
        };
        write!(writer, "{:.6}\t{:.6}\t{}\r\n", start, end, name)?;
    }
    writer.flush()?;
    Ok(())
}

/// Reads cues from Audacity label track data in `reader`. `sample_rate` is the sample rate of the
/// source audio stream, needed to calculate sample numbers for cues.
pub fn read_labels(mut reader: impl BufRead, sample_rate: u32) -> Result<Vec<Cue>> {
    read_labels_impl(&mut reader, sample_rate)
}

fn read_labels_impl(reader: &mut dyn BufRead, sample_rate: u32) -> Result<Vec<Cue>> {
    let mut cues = vec![];
    loop {
        let mut line = String::new();
        let len = reader.read_line(&mut line)?;
        if len == 0 {
            break;
        }

        // <start> [<end>] <name>
        let mut tokens = line.trim_end_matches("\r\n").split('\t');
        let start = match tokens.next().map(|t| t.parse::<f64>()) {
            Some(Ok(start)) if start.is_finite() => start,
            _ => continue, // Skip bad lines (e.g. lines starting with `/`)
        };
        // If the next token is a number, it is the end point and the next token is the name,
        // otherwise the cue is a single point and this token is the name
        let token = tokens.next().unwrap_or_default();
        let (end, name) = match token.parse::<f64>() {
            Ok(end) if end.is_finite() => (end, tokens.next().unwrap_or_default()),
            _ => (start, token),
        };

        let start_sample = (start * f64::from(sample_rate)).round() as u64;
        let end_sample = (end * f64::from(sample_rate)).round() as u64;
        let cue = if cue::has_loop_prefix(name) {
            Cue::new_loop(name, start_sample)
        } else if end > start {
            Cue::new_range(name, start_sample, end_sample - start_sample)
        } else {
            Cue::new(name, start_sample)
        };
        cues.push(cue);
    }
    Ok(cues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_write_labels() {
        let cues = vec![
            Cue::new("one", 0),
            Cue::new("two", 22050),
            Cue::new_range("three", 44100, 22050),
            Cue::new_loop("four", 88200),
        ];
        let mut labels = vec![];
        write_labels(&mut labels, cues, 44100).unwrap();
        let labels = String::from_utf8(labels).unwrap();
        assert_eq!(
            labels,
            concat!(
                "0.000000\t0.000000\tone\r\n",
                "0.500000\t0.500000\ttwo\r\n",
                "1.000000\t1.500000\tthree\r\n",
                "2.000000\t2.000000\tloop:four\r\n",
            )
        );
    }

    #[test]
    fn test_read_labels() {
        let labels = Cursor::new(concat!(
            "0.000000\t0.000000\tone\r\n",
            "0.500000\ttwo\r\n",
            "1.000000\t1.500000\tthree\r\n",
            "\\\t6928.456055\t6928.456055\r\n",
            "2.000000\t2.000000\tloop:four\r\n",
        ));
        let cues = read_labels(labels, 44100).unwrap();
        assert_eq!(
            cues,
            &[
                Cue::new("one", 0),
                Cue::new("two", 22050),
                Cue::new_range("three", 44100, 22050),
                Cue::new_loop("loop:four", 88200),
            ]
        );
    }
}

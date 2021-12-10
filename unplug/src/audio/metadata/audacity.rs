// See <https://manual.audacityteam.org/man/importing_and_exporting_labels.html> for details on the
// label track format.

use crate::audio::{Cue, Result};
use std::io::Write;

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
        write!(writer, "{:.6}\t{:.6}\t{}\r\n", start, end, cue.name)?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_labels() -> Result<()> {
        let cues =
            vec![Cue::new("one", 0), Cue::new("two", 22050), Cue::new_range("three", 44100, 22050)];
        let mut labels = vec![];
        write_labels(&mut labels, cues, 44100)?;
        let labels = String::from_utf8(labels).unwrap();
        assert_eq!(
            labels,
            concat!(
                "0.000000\t0.000000\tone\r\n",
                "0.500000\t0.500000\ttwo\r\n",
                "1.000000\t1.500000\tthree\r\n",
            )
        );
        Ok(())
    }
}

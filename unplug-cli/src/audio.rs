use crate::common::*;
use crate::opt::*;
use anyhow::{bail, Result};
use log::{info, warn};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;
use unplug::audio::format::PcmS16Le;
use unplug::audio::transport::hps::{HpsStream, PcmHpsBuilder};
use unplug::audio::transport::{FlacReader, Mp3Reader, OggReader, SoundBank, WavReader, WavWriter};
use unplug::audio::ReadSamples;
use unplug::common::{ReadSeek, WriteTo};
use unplug::data::sound::{Sound, SoundDefinition};
use unplug::data::sound_bank::SOUND_BANKS;
use unplug::dvd::OpenFile;

/// The highest sample rate that imported music can have. Music sampled higher than this will be
/// downsampled.
const MAX_MUSIC_SAMPLE_RATE: u32 = 44100;

const SFX_HORI_NAME: &str = "sfx_hori.ssm";
const SFX_HORI_PATH: &str = "qp/sfx_hori.ssm";

/// Opens the sound file at `path` and enqueues it for resampling if the sample rate is higher than
/// `max_sample_rate`.
fn open_sound_file(
    path: &Path,
    max_sample_rate: u32,
) -> Result<Box<dyn ReadSamples<'static, Format = PcmS16Le>>> {
    let name = path.file_name().map(|p| p.to_str().unwrap()).unwrap_or_default().to_owned();
    let ext = path.extension().map(|p| p.to_str().unwrap().to_lowercase()).unwrap_or_default();
    let message = format!("Opening audio file: {}", name);
    let spinner = progress_spinner(message);

    let file = File::open(path)?;
    let tag = name.clone();
    let mut audio: Box<dyn ReadSamples<'_, Format = PcmS16Le>> = match ext.as_str() {
        "flac" => FlacReader::new(file, tag)?.convert(),
        "mp3" => Box::from(Mp3Reader::new(file, tag)?),
        "ogg" => Box::from(OggReader::new(file, tag)?),
        "wav" => Box::from(WavReader::new(file, tag)?),
        other => bail!("unsupported file extension: \"{}\"", other),
    };

    // Using preread_all_samples() here is necessary to have a functioning progress bar with some
    // formats which don't know their size.
    let cached = audio.preread_all_samples()?;
    spinner.finish_using_style();
    if !spinner.is_hidden() {
        info!("Opened audio file: {}", name);
    }

    let rate = cached.front().expect("no audio packets").rate;
    if rate > max_sample_rate {
        warn!("The audio file has a high sample rate ({} Hz)!", rate);
        warn!("It will be automatically resampled to {} Hz.", max_sample_rate);
        Ok(Box::from(cached.resample(max_sample_rate)))
    } else {
        Ok(Box::from(cached))
    }
}

pub fn export_music(opt: ExportMusicOpt) -> Result<()> {
    let start_time = Instant::now();

    let mut iso = open_iso_optional(opt.iso.as_ref())?;
    let mut reader = BufReader::new(open_iso_entry_or_file(iso.as_mut(), &opt.path)?);
    let name = opt.path.file_name().unwrap().to_string_lossy();
    let hps = HpsStream::open(&mut reader, name)?;

    info!("Writing {}", opt.output.display());
    let progress = progress_bar(1);
    if !progress.is_hidden() {
        let out_name = opt.output.file_name().unwrap_or_default().to_string_lossy().into_owned();
        progress.set_message(out_name);
    }

    let out = BufWriter::new(File::create(&opt.output)?);
    WavWriter::new(hps.decoder())
        .on_progress(|p| update_audio_progress(&progress, p))
        .write_to(out)?;

    progress.finish_using_style();
    info!("Export finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn import_music(opt: ImportMusicOpt) -> Result<()> {
    let start_time = Instant::now();

    let mut iso = edit_iso_optional(Some(opt.iso))?.unwrap();
    let entry = iso.files.at(opt.hps.to_str().unwrap())?;

    let audio = open_sound_file(&opt.path, MAX_MUSIC_SAMPLE_RATE)?;
    info!("Analyzing audio waveform");
    let progress = progress_bar(1);
    progress.set_message(audio.tag().name.clone());
    let encoder =
        PcmHpsBuilder::new(audio).on_progress(|p| update_audio_progress(&progress, p)).prepare()?;
    progress.finish_using_style();

    info!("Encoding audio to GameCube format");
    let progress = progress_bar(1);
    progress.set_message(iso.files[entry].name().to_owned());
    let hps = encoder.on_progress(|p| update_audio_progress(&progress, p)).build()?;
    progress.finish_using_style();

    let mut writer = Cursor::new(vec![]);
    hps.write_to(&mut writer)?;
    writer.seek(SeekFrom::Start(0))?;

    info!("Updating ISO");
    iso.replace_file(entry, writer)?;

    info!("Import finished in {:?}", start_time.elapsed());
    Ok(())
}

fn make_sound_filename(bank: &SoundBank, index: usize, have_names: bool) -> String {
    let id = bank.base_index + (index as u32);
    if have_names {
        if let Ok(sound) = Sound::try_from(id) {
            return format!("{}.wav", SoundDefinition::get(sound).name);
        }
    }
    format!("{:>04}.wav", id)
}

/// Reads a sound bank from `reader` named `name` and exports WAV files to `dir`.
fn export_bank<'r>(reader: Box<dyn ReadSeek + 'r>, name: &str, dir: &Path) -> Result<()> {
    export_bank_impl(reader, name, dir, "")
}

/// Reads a sound bank from `reader` named `name` and exports WAV files to a subdirectory of `dir`
/// named after the bank.
fn export_bank_subdir<'r>(reader: Box<dyn ReadSeek + 'r>, name: &str, dir: &Path) -> Result<()> {
    let name_prefix = name.split('.').next().unwrap_or(name); // Strip extension
    let dir = dir.join(name_prefix);
    let display_prefix = format!("{}/", name_prefix);
    export_bank_impl(reader, name, &dir, &display_prefix)
}

fn export_bank_impl<'r>(
    reader: Box<dyn ReadSeek + 'r>,
    name: &str,
    dir: &Path,
    display_prefix: &str,
) -> Result<()> {
    info!("Exporting from {}", name);
    let mut reader = BufReader::new(reader);
    let bank = SoundBank::open(&mut reader, name)?;
    // Omit names for unusable banks (sfx_hori.ssm)
    let have_names = SOUND_BANKS.iter().any(|b| b.sound_base == bank.base_index);
    fs::create_dir_all(&dir)?;
    let progress = progress_bar(bank.sounds.len() as u64);
    for (i, _) in bank.sounds.iter().enumerate() {
        let filename = make_sound_filename(&bank, i, have_names);
        if progress.is_hidden() {
            info!("Writing {}{}", display_prefix, filename);
        } else {
            progress.set_message(format!("{}{}", display_prefix, filename));
        }
        let out_path = dir.join(filename);
        let out = BufWriter::new(File::create(&out_path)?);
        WavWriter::new(bank.decoder(i)).write_to(out)?;
        progress.inc(1);
    }
    progress.finish_using_style();
    Ok(())
}

pub fn export_sounds(opt: ExportSoundsOpt) -> Result<()> {
    let start_time = Instant::now();

    let mut iso = open_iso_optional(opt.iso.as_ref())?;
    if let Some(bank_path) = opt.path {
        // Export single bank
        let reader = open_iso_entry_or_file(iso.as_mut(), &bank_path)?;
        let name = bank_path.file_name().unwrap().to_string_lossy();
        export_bank(reader, &name, &opt.output)?;
    } else {
        // Export registered banks
        let mut iso = iso.expect("no iso path or bank path");
        for bank_def in SOUND_BANKS {
            let reader = iso.open_file_at(&bank_def.path())?;
            let name = format!("{}.ssm", bank_def.name);
            export_bank_subdir(reader, &name, &opt.output)?;
        }
        // Export sfx_hori, which is not a registered bank because it has bogus sound IDs
        let reader = iso.open_file_at(SFX_HORI_PATH)?;
        export_bank_subdir(reader, SFX_HORI_NAME, &opt.output)?;
    }

    info!("Export finished in {:?}", start_time.elapsed());
    Ok(())
}

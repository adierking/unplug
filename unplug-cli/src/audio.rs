use crate::common::*;
use crate::opt::*;
use crate::playback::{self, PlaybackDevice, PlaybackSource};
use crate::terminal::{progress_bar, progress_spinner, update_audio_progress};
use anyhow::{anyhow, bail, Result};
use log::{debug, info, warn};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;
use unplug::audio::format::PcmS16Le;
use unplug::audio::metadata::audacity;
use unplug::audio::metadata::SfxPlaylist;
use unplug::audio::transport::hps::{HpsReader, Looping, PcmHpsWriter};
use unplug::audio::transport::{FlacReader, Mp3Reader, OggReader, SfxBank, WavReader, WavWriter};
use unplug::audio::{Cue, ReadSamples};
use unplug::common::{ReadFrom, ReadSeek};
use unplug::data::music::MUSIC;
use unplug::data::sfx::{PLAYLIST_PATH, SFX};
use unplug::data::sfx_group::{SfxGroup, SfxGroupDefinition, SFX_GROUPS};
use unplug::data::sfx_sample::{SfxSample, SfxSampleDefinition};
use unplug::dvd::OpenFile;

/// The highest sample rate that imported music can have. Music sampled higher than this will be
/// downsampled.
const MAX_MUSIC_SAMPLE_RATE: u32 = 44100;

const SFX_HORI_NAME: &str = "sfx_hori.ssm";
const SFX_HORI_PATH: &str = "qp/sfx_hori.ssm";

/// Extension to use for Audacity label output
const LABELS_EXTENSION: &str = "labels.txt";

/// Opens the sound file at `path`, optionally reads Audacity labels from `labels`, and enqueues it
/// for resampling if the sample rate is higher than `max_sample_rate`.
fn open_sound_file(
    path: &Path,
    labels: Option<&Path>,
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

    let mut rate = cached.front().expect("no audio packets").rate;
    audio = if rate > max_sample_rate {
        warn!("The audio file has a high sample rate ({} Hz)!", rate);
        warn!("It will be automatically resampled to {} Hz.", max_sample_rate);
        rate = max_sample_rate;
        Box::from(cached.resample(rate))
    } else {
        Box::from(cached)
    };

    // Labels should be loaded last to ensure they don't get discarded/ignored by an adapter
    if let Some(labels) = labels {
        info!("Reading label track: {}", labels.display());
        let reader = BufReader::new(File::open(labels)?);
        let cues = audacity::read_labels(reader, rate)?;
        audio = Box::from(audio.with_cues(cues));
    }

    spinner.finish_using_style();
    if !spinner.is_hidden() {
        info!("Opened audio file: {}", name);
    }
    Ok(audio)
}

/// Finds a music file by name and returns its path in the ISO.
fn find_music(name: &str) -> Result<String> {
    let path = MUSIC
        .iter()
        .find(|m| unicase::eq(m.name, name))
        .map(|m| m.path())
        .ok_or_else(|| anyhow!("unknown music: \"{}\"", name))?;
    debug!("Resolved music \"{}\": {}", name, path);
    Ok(path)
}

/// Finds a sound effect by name and returns a `(group, index)` pair.
fn find_sound(playlist: &SfxPlaylist, name: &str) -> Result<(SfxGroup, usize)> {
    let def = match SFX.iter().find(|e| unicase::eq(e.name, name)) {
        Some(def) => def,
        None => bail!("unknown sound effect: \"{}\"", name),
    };
    let index = def.id.material_index();
    let sample = match playlist.sounds[index].sample_id() {
        Some(id) => SfxSample::try_from(id).unwrap(),
        None => bail!("sound effect \"{}\" does not have an associated sample", def.name),
    };
    let group = SfxGroupDefinition::get(def.id.group());
    let sample_index = u32::from(sample) - group.first_sample;
    debug!("Resolved sound \"{}\": group={}, index={}", name, group.name, sample_index);
    Ok((group.id, sample_index as usize))
}

/// Exports Audacity labels alongside a sound file.
fn export_labels(cues: Vec<Cue>, sample_rate: u32, sound_path: &Path) -> Result<()> {
    if !cues.is_empty() {
        let label_path = sound_path.with_extension(LABELS_EXTENSION);
        debug!("Writing label track to {}", label_path.display());
        let labels = BufWriter::new(File::create(label_path)?);
        audacity::write_labels(labels, cues, sample_rate)?;
    }
    Ok(())
}

pub fn export_music(opt: ExportMusicOpt) -> Result<()> {
    let start_time = Instant::now();

    let mut iso = open_iso_optional(opt.iso.as_ref())?;
    let reader = BufReader::new(open_iso_entry_or_file(iso.as_mut(), &opt.path)?);
    let name = opt.path.file_name().unwrap().to_string_lossy();
    let hps = HpsReader::new(reader, name)?;

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

    if opt.settings.labels {
        export_labels(hps.cues().collect(), hps.sample_rate(), &opt.output)?;
    }

    info!("Export finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn import_music(opt: ImportMusicOpt) -> Result<()> {
    let start_time = Instant::now();

    let mut iso = edit_iso_optional(Some(opt.iso))?.unwrap();
    let hps_path = opt.hps;
    info!("Checking {}", hps_path.display());
    let entry = iso.files.at(hps_path.to_str().unwrap())?;
    let hps_name = iso.files[entry].name().to_owned();
    let original_loop = {
        let reader = BufReader::new(iso.open_file(entry)?);
        HpsReader::new(reader, hps_name)?.loop_start()
    };

    let audio = open_sound_file(&opt.path, opt.labels.as_deref(), MAX_MUSIC_SAMPLE_RATE)?;
    info!("Analyzing audio waveform");
    let progress = progress_bar(1);
    progress.set_message(audio.tag().name.clone());
    let encoder =
        PcmHpsWriter::new(audio).on_progress(|p| update_audio_progress(&progress, p)).prepare()?;
    progress.finish_using_style();

    info!("Encoding audio to GameCube format");
    let progress = progress_bar(1);
    progress.set_message(iso.files[entry].name().to_owned());
    // Copy the loop setting from the original HPS
    let looping = if original_loop.is_some() { Looping::Enabled } else { Looping::Disabled };
    let mut writer = Cursor::new(vec![]);
    encoder
        .looping(looping)
        .on_progress(|p| update_audio_progress(&progress, p))
        .write_to(&mut writer)?;
    progress.finish_using_style();

    info!("Updating ISO");
    writer.seek(SeekFrom::Start(0))?;
    iso.replace_file(entry, writer)?;

    info!("Import finished in {:?}", start_time.elapsed());
    Ok(())
}

fn make_sound_filename(bank: &SfxBank, index: usize, have_names: bool) -> String {
    let id = bank.base_index() + (index as u32);
    if have_names {
        if let Ok(sound) = SfxSample::try_from(id) {
            return format!("{}.wav", SfxSampleDefinition::get(sound).name);
        }
    }
    format!("{:>04}.wav", id)
}

/// Reads a sound bank from `reader` named `name` and exports WAV files to `dir`.
fn export_bank<'r>(
    settings: &SoundExportOpt,
    reader: Box<dyn ReadSeek + 'r>,
    name: &str,
    dir: &Path,
) -> Result<()> {
    export_bank_impl(settings, reader, name, dir, "")
}

/// Reads a sound bank from `reader` named `name` and exports WAV files to a subdirectory of `dir`
/// named after the bank.
fn export_bank_subdir<'r>(
    settings: &SoundExportOpt,
    reader: Box<dyn ReadSeek + 'r>,
    name: &str,
    dir: &Path,
) -> Result<()> {
    let name_prefix = name.split('.').next().unwrap_or(name); // Strip extension
    let dir = dir.join(name_prefix);
    let display_prefix = format!("{}/", name_prefix);
    export_bank_impl(settings, reader, name, &dir, &display_prefix)
}

fn export_bank_impl<'r>(
    settings: &SoundExportOpt,
    reader: Box<dyn ReadSeek + 'r>,
    name: &str,
    dir: &Path,
    display_prefix: &str,
) -> Result<()> {
    info!("Exporting from {}", name);
    let mut reader = BufReader::new(reader);
    let bank = SfxBank::open(&mut reader, name)?;
    // Omit names for unusable banks (sfx_hori.ssm)
    let have_names = SFX_GROUPS.iter().any(|g| g.first_sample == bank.base_index());
    fs::create_dir_all(&dir)?;
    let progress = progress_bar(bank.len() as u64);
    for (i, _) in bank.samples().enumerate() {
        let filename = make_sound_filename(&bank, i, have_names);
        if progress.is_hidden() {
            info!("Writing {}{}", display_prefix, filename);
        } else {
            progress.set_message(format!("{}{}", display_prefix, filename));
        }
        let out_path = dir.join(filename);
        let out = BufWriter::new(File::create(&out_path)?);
        let decoder = bank.decoder(i);
        let cues: Vec<_> = decoder.cues().collect();
        WavWriter::new(decoder).write_to(out)?;
        if settings.labels {
            export_labels(cues, bank.sample(i).rate, &out_path)?;
        }
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
        export_bank(&opt.settings, reader, &name, &opt.output)?;
    } else {
        // Export registered banks
        let mut iso = iso.expect("no iso path or bank path");
        for group in SFX_GROUPS {
            let reader = iso.open_file_at(&group.bank_path())?;
            let name = format!("{}.ssm", group.name);
            export_bank_subdir(&opt.settings, reader, &name, &opt.output)?;
        }
        // Export sfx_hori, which is not a registered bank because it has bogus sound IDs
        let reader = iso.open_file_at(SFX_HORI_PATH)?;
        export_bank_subdir(&opt.settings, reader, SFX_HORI_NAME, &opt.output)?;
    }

    info!("Export finished in {:?}", start_time.elapsed());
    Ok(())
}

fn play_audio(
    audio: impl ReadSamples<'static, Format = PcmS16Le> + 'static,
    opt: PlaybackOpt,
) -> Result<()> {
    info!("Checking system audio configuration");
    let mut device = PlaybackDevice::open_default()?;

    info!("Starting playback");
    let name = audio.tag().name.clone();
    let source = PlaybackSource::new(audio)?.with_volume(opt.volume);
    playback::play(&mut device, source, name);

    info!("Playback finished");
    Ok(())
}

pub fn play_music(opt: PlayMusicOpt) -> Result<()> {
    let iso = open_iso_optional(opt.iso.as_ref())?;
    let path = if iso.is_some() {
        find_music(opt.name.to_str().unwrap())?
    } else {
        opt.name.to_str().unwrap().to_owned()
    };
    let name = Path::new(&path).file_name().unwrap().to_string_lossy();
    let reader = BufReader::new(iso_into_entry_or_file(iso, &path)?);
    let hps = HpsReader::new(reader, name)?;
    play_audio(hps.decoder(), opt.playback)?;
    Ok(())
}

pub fn play_sound(opt: PlaySoundOpt) -> Result<()> {
    let mut iso = open_iso_optional(Some(opt.iso))?.unwrap();
    let playlist = {
        let mut reader = BufReader::new(open_iso_entry_or_file(Some(&mut iso), PLAYLIST_PATH)?);
        SfxPlaylist::read_from(&mut reader)?
    };
    let (group, index) = find_sound(&playlist, &opt.sound)?;
    let group = SfxGroupDefinition::get(group);
    let bank = {
        let mut reader = BufReader::new(open_iso_entry_or_file(Some(&mut iso), group.bank_path())?);
        SfxBank::open(&mut reader, group.name)?
    };
    play_audio(bank.decoder(index), opt.playback)?;
    Ok(())
}

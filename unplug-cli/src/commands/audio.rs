use crate::common::{format_duration, output_dir_and_name};
use crate::context::{Context, FileId, OpenContext};
use crate::opt::{
    AudioCommand, AudioExportAllOpt, AudioExportBankOpt, AudioExportOpt, AudioExportSettings,
    AudioImportOpt, AudioImportSettings, AudioInfoOpt, AudioPlayOpt,
};
use crate::playback::{self, PlaybackDevice, PlaybackSource};
use crate::terminal::{progress_bar, progress_spinner, update_audio_progress};
use anyhow::{anyhow, bail, Result};
use log::{debug, info, log_enabled, warn, Level};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Seek, SeekFrom};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use unplug::audio::format::PcmS16Le;
use unplug::audio::metadata::audacity;
use unplug::audio::metadata::SfxPlaylist;
use unplug::audio::transport::hps::{Looping, PcmHpsWriter};
use unplug::audio::transport::ssm::BankSample;
use unplug::audio::transport::{
    FlacReader, HpsReader, Mp3Reader, OggReader, SfxBank, WavReader, WavWriter,
};
use unplug::audio::{Cue, ReadSamples};
use unplug::common::{ReadSeek, ReadWriteSeek, WriteTo};
use unplug::data::{Music, Resource, Sfx, SfxGroup, SfxSample, Sound};

/// The highest sample rate that imported music can have. Music sampled higher than this will be
/// downsampled.
const MAX_MUSIC_SAMPLE_RATE: u32 = 44100;
/// The highest sample rate that imported sound effects can have.
const MAX_SFX_SAMPLE_RATE: u32 = 48000;

/// Path to sfx_hori.ssm, a sample bank never used by the game.
const SFX_HORI_PATH: &str = "qp/sfx_hori.ssm";

/// Extension to use for Audacity label output
const LABELS_EXT: &str = "labels.txt";

/// Opens the sound file at `path`, optionally reads Audacity labels from `labels`, and enqueues it
/// for resampling if the sample rate is higher than `max_sample_rate`.
fn open_sound_file(
    path: &Path,
    settings: &AudioImportSettings,
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
    if settings.labels {
        let labels_path = path.with_extension(LABELS_EXT);
        info!("Reading label track: {}", labels_path.display());
        let reader = BufReader::new(File::open(labels_path)?);
        let cues = audacity::read_labels(reader, rate)?;
        audio = Box::from(audio.with_cues(cues));
    }

    spinner.finish_using_style();
    if !spinner.is_hidden() {
        info!("Opened audio file: {}", name);
    }
    Ok(audio)
}

/// Checks whether we have sample name information for a bank.
fn have_sample_names(bank: &SfxBank) -> bool {
    SfxGroup::iter().any(|g| g.first_sample() == bank.base_index())
}

/// Gets the name of a sound sample.
fn sfx_name(bank: &SfxBank, index: usize, have_names: bool) -> Cow<'static, str> {
    let id = bank.base_index() + (index as u32);
    if have_names {
        if let Ok(sample) = SfxSample::try_from(id) {
            return sample.name().into();
        }
    }
    format!("{:>04}", id).into()
}

/// Locates a sample bank by name or path.
fn find_bank<T: ReadSeek>(ctx: &mut OpenContext<T>, name: &str) -> Result<FileId> {
    if let Some(file) = ctx.explicit_file_at(name)? {
        return Ok(file);
    }
    let group = SfxGroup::find(name).ok_or_else(|| anyhow!("Unknown sample bank: {}", name))?;
    debug!("Resolved bank \"{}\": {:?}", name, group);
    ctx.disc_file_at(group.path())
}

/// Caches playlist and sample bank data so it doesn't get double-loaded.
#[derive(Default)]
struct AudioCache {
    playlist: Option<Rc<SfxPlaylist>>,
    banks: HashMap<FileId, Rc<SfxBank>>,
}

impl AudioCache {
    fn new() -> Self {
        Self::default()
    }

    /// Reads the playlist file if necessary and returns its data.
    fn open_playlist<T: ReadSeek>(&mut self, ctx: &mut OpenContext<T>) -> Result<Rc<SfxPlaylist>> {
        if self.playlist.is_none() {
            debug!("Reading sfx_sample.sem");
            self.playlist = Some(Rc::new(ctx.read_playlist()?));
        }
        Ok(Rc::clone(self.playlist.as_ref().unwrap()))
    }

    /// Reads a sample bank if necessary and returns its data.
    fn open_bank<T: ReadSeek>(
        &mut self,
        ctx: &mut OpenContext<T>,
        file: &FileId,
    ) -> Result<Rc<SfxBank>> {
        if let Some(bank) = self.banks.get(file) {
            return Ok(Rc::clone(bank));
        }
        if log_enabled!(Level::Debug) {
            let name = ctx.query_file(file)?.name;
            debug!("Opening {}", name);
        }
        let bank = Rc::new(ctx.read_bank_file(file)?);
        self.banks.insert(file.clone(), Rc::clone(&bank));
        Ok(bank)
    }
}

/// Holds a pointer to an audio resource.
#[allow(variant_size_differences)]
#[derive(Clone, Hash, PartialEq, Eq)]
enum AudioResource {
    Music(Music),
    MusicFile { file: FileId, name: String },
    Sfx(Sfx),
}

impl AudioResource {
    /// Finds an audio resource by name or path.
    fn find<T: ReadSeek>(ctx: &mut OpenContext<T>, name: &str) -> Result<Self> {
        if let Some(file) = ctx.explicit_file_at(name)? {
            let filename = ctx.query_file(&file)?.name;
            let name = filename.rsplit_once('.').unwrap_or((&filename, "")).0.to_owned();
            return Ok(Self::MusicFile { file, name });
        }

        match Sound::find(name) {
            Some(Sound::Music(music)) => {
                debug!("Resolved music \"{}\": {:?}", name, music);
                Ok(Self::Music(music))
            }
            Some(Sound::Sfx(sfx)) => {
                debug!("Resolved SFX \"{}\": {:?}", name, sfx);
                Ok(Self::Sfx(sfx))
            }
            None => bail!("Unknown audio resource: {}", name),
        }
    }

    /// Gets the name of the audio resource without any extension.
    fn name(&self) -> &str {
        match self {
            Self::Music(music) => music.name(),
            Self::MusicFile { name, .. } => name,
            Self::Sfx(sfx) => sfx.name(),
        }
    }

    /// Gets the corresponding `Sound` if known.
    fn id(&self) -> Option<Sound> {
        match *self {
            Self::Music(music) => Some(music.into()),
            Self::MusicFile { .. } => None,
            Self::Sfx(sfx) => Some(sfx.into()),
        }
    }
}

/// Wraps a file ID for an audio resource.
enum AudioFileId {
    Music(FileId),
    Sfx { file: FileId, index: usize },
}

impl AudioFileId {
    /// Locates the file for `resource`.
    fn get<T: ReadSeek>(
        ctx: &mut OpenContext<T>,
        cache: &mut AudioCache,
        resource: &AudioResource,
    ) -> Result<Self> {
        match resource {
            AudioResource::Music(id) => {
                let file = ctx.disc_file_at(id.path().unwrap())?;
                Ok(Self::Music(file))
            }
            AudioResource::MusicFile { file, .. } => Ok(Self::Music(file.clone())),
            AudioResource::Sfx(id) => {
                let playlist = cache.open_playlist(ctx)?;
                let group = id.group();
                let material = id.material_index();
                let sample = match playlist.sounds[material].sample_id() {
                    Some(id) => SfxSample::try_from(id).unwrap(),
                    None => {
                        bail!("Sound effect \"{}\" does not have an associated sample", id.name())
                    }
                };
                let index = (u32::from(sample) - group.first_sample()) as usize;
                debug!("Resolved sound \"{}\": group={}, index={}", id.name(), group.name(), index);
                let file = ctx.disc_file_at(group.path())?;
                Ok(Self::Sfx { file, index })
            }
        }
    }
}

/// Provides a unified interface for reading from an audio source.
enum AudioReader<'r> {
    Music(HpsReader<'r>),
    Sfx { bank: Rc<SfxBank>, index: usize },
}

impl<'r> AudioReader<'r> {
    /// Opens a reader for `id`.
    fn open<T: ReadSeek>(
        ctx: &'r mut OpenContext<T>,
        cache: &mut AudioCache,
        id: &AudioFileId,
    ) -> Result<Self> {
        match id {
            AudioFileId::Music(file) => {
                if log_enabled!(Level::Debug) {
                    let name = ctx.query_file(file)?.name;
                    debug!("Opening {}", name);
                }
                let hps = ctx.open_music_file(file)?;
                Ok(Self::Music(hps))
            }
            AudioFileId::Sfx { file, index } => {
                let bank = cache.open_bank(ctx, file)?;
                Ok(Self::Sfx { bank, index: *index })
            }
        }
    }

    /// Gets the number of channels in the audio.
    fn channels(&self) -> usize {
        match self {
            Self::Music(hps) => hps.channels(),
            Self::Sfx { bank, index } => bank.sample(*index).channels.len(),
        }
    }

    /// Gets the audio sample rate.
    fn sample_rate(&self) -> u32 {
        match self {
            Self::Music(hps) => hps.sample_rate(),
            Self::Sfx { bank, index } => bank.sample(*index).rate,
        }
    }

    /// Calculates the audio duration.
    fn duration(&self) -> Duration {
        let decoder = self.decoder();
        let total_frames = decoder.data_remaining().unwrap() / (self.channels() as u64);
        Duration::from_secs_f64((total_frames as f64) / (self.sample_rate() as f64))
    }

    /// Creates a decoder for the audio data.
    fn decoder(&self) -> Box<dyn ReadSamples<'static, Format = PcmS16Le> + '_> {
        match self {
            Self::Music(hps) => hps.decoder(),
            Self::Sfx { bank, index } => bank.decoder(*index),
        }
    }
}

/// Exports Audacity labels alongside a sound file.
fn export_labels(cues: Vec<Cue>, sample_rate: u32, sound_path: &Path) -> Result<()> {
    if !cues.is_empty() {
        let label_path = sound_path.with_extension(LABELS_EXT);
        debug!("Writing label track to {}", label_path.display());
        let labels = BufWriter::new(File::create(label_path)?);
        audacity::write_labels(labels, cues, sample_rate)?;
    }
    Ok(())
}

/// The `audio info` CLI command.
fn command_info(ctx: Context, opt: AudioInfoOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut cache = AudioCache::new();
    let resource = AudioResource::find(&mut ctx, &opt.name)?;
    let name = resource.name();
    let file = AudioFileId::get(&mut ctx, &mut cache, &resource)?;
    let audio = AudioReader::open(&mut ctx, &mut cache, &file)?;
    let duration = audio.duration();
    let cues = audio.decoder().cues().collect::<Vec<_>>();
    let looping = cues.iter().any(|c| c.is_loop());
    let num_cues = cues.iter().filter(|c| !c.is_loop()).count();
    match &audio {
        AudioReader::Music(_) => print!("{}: Program stream", name),
        AudioReader::Sfx { bank, index } => {
            print!("{}: Sound sample {} in {}", name, index, bank.tag().name);
        }
    }
    match resource.id() {
        Some(id) => println!(" (ID 0x{:08x})", id.value()),
        None => println!(),
    }
    println!("Duration: {}", format_duration(duration));
    println!("Channels: {}", audio.channels());
    println!("Sample Rate: {} Hz", audio.sample_rate());
    println!("Looping: {}", if looping { "Yes" } else { "No" });
    println!("Cues: {}", num_cues);
    Ok(())
}

/// The `audio export` CLI command.
fn command_export(ctx: Context, opt: AudioExportOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut cache = AudioCache::new();
    if opt.names.is_empty() {
        bail!("Nothing to export");
    }
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), opt.names.len() > 1);
    fs::create_dir_all(out_dir)?;
    for name in &opt.names {
        let resource = AudioResource::find(&mut ctx, name)?;
        let default_name = format!("{}.wav", resource.name());
        let filename = out_name.as_ref().unwrap_or(&default_name);
        info!("Exporting {}", filename);
        let file = AudioFileId::get(&mut ctx, &mut cache, &resource)?;
        let audio = AudioReader::open(&mut ctx, &mut cache, &file)?;
        let output = out_dir.join(filename);
        export(&audio, &opt.settings, &output)?;
    }
    Ok(())
}

/// The `audio export-bank` CLI command.
fn command_export_bank(ctx: Context, opt: AudioExportBankOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let file = find_bank(&mut ctx, &opt.name)?;
    export_bank_impl(&mut ctx, &opt.settings, &file, &opt.output, "")?;
    Ok(())
}

/// The `audio export-all` CLI command.
fn command_export_all(ctx: Context, opt: AudioExportAllOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;

    // Export registered banks
    for group in SfxGroup::iter() {
        let file = ctx.disc_file_at(&group.path())?;
        export_bank_subdir(&mut ctx, &opt.settings, &file, &opt.output)?;
    }

    // Export sfx_hori, which is not a registered bank because it has bogus sound IDs
    let hori = ctx.disc_file_at(SFX_HORI_PATH)?;
    export_bank_subdir(&mut ctx, &opt.settings, &hori, &opt.output)?;

    // Export music
    let mut cache = AudioCache::new();
    // skip(1) to skip None
    for music in Music::iter().skip(1) {
        info!("Exporting {}.wav", music.name());
        let resource = AudioResource::Music(music);
        let file = AudioFileId::get(&mut ctx, &mut cache, &resource)?;
        let audio = AudioReader::open(&mut ctx, &mut cache, &file)?;
        let output = opt.output.join(format!("{}.wav", music.name()));
        export(&audio, &opt.settings, &output)?;
    }
    Ok(())
}

fn export(audio: &AudioReader<'_>, settings: &AudioExportSettings, path: &Path) -> Result<()> {
    let progress = progress_bar(1);
    if !progress.is_hidden() {
        let out_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        progress.set_message(out_name);
    }

    let out = BufWriter::new(File::create(path)?);
    let decoder = audio.decoder();
    let cues = decoder.cues().collect::<Vec<_>>();
    WavWriter::new(decoder).on_progress(|p| update_audio_progress(&progress, p)).write_to(out)?;
    progress.finish_using_style();

    if settings.labels {
        export_labels(cues, audio.sample_rate(), path)?;
    }
    Ok(())
}

/// Reads a sound bank from `reader` named `name` and exports WAV files to a subdirectory of `dir`
/// named after the bank.
fn export_bank_subdir<T: ReadSeek>(
    ctx: &mut OpenContext<T>,
    settings: &AudioExportSettings,
    file: &FileId,
    dir: &Path,
) -> Result<()> {
    let name = ctx.query_file(file)?.name;
    let name_prefix = name.split('.').next().unwrap_or(&name); // Strip extension
    let dir = dir.join(name_prefix);
    let display_prefix = format!("{}/", name_prefix);
    export_bank_impl(ctx, settings, file, &dir, &display_prefix)
}

fn export_bank_impl<T: ReadSeek>(
    ctx: &mut OpenContext<T>,
    settings: &AudioExportSettings,
    file: &FileId,
    dir: &Path,
    display_prefix: &str,
) -> Result<()> {
    let name = ctx.query_file(file)?.name;
    info!("Exporting from {}", name);
    let bank = ctx.read_bank_file(file)?;
    // Omit names for unusable banks (sfx_hori.ssm)
    let have_names = have_sample_names(&bank);
    fs::create_dir_all(&dir)?;
    let progress = progress_bar(bank.len() as u64);
    for (i, _) in bank.samples().enumerate() {
        let name = sfx_name(&bank, i, have_names);
        let filename = format!("{}.wav", name);
        if progress.is_hidden() {
            info!("Writing {}{}", display_prefix, filename);
        } else {
            progress.set_message(format!("{}{}", display_prefix, filename));
        }
        let out_path = dir.join(&filename);
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

/// The `audio import` CLI command.
fn command_import(ctx: Context, opt: AudioImportOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    let resource = AudioResource::find(&mut ctx, &opt.name)?;
    info!("Opening {}", resource.name());
    let mut cache = AudioCache::new();
    let file = AudioFileId::get(&mut ctx, &mut cache, &resource)?;
    match file {
        AudioFileId::Music(file) => import_music(&mut ctx, opt, file),
        AudioFileId::Sfx { file, index } => import_sfx(&mut ctx, opt, file, index),
    }
}

fn import_music<T: ReadWriteSeek>(
    ctx: &mut OpenContext<T>,
    opt: AudioImportOpt,
    file: FileId,
) -> Result<()> {
    let name = ctx.query_file(&file)?.name;
    let original_loop = ctx.open_music_file(&file)?.loop_start();

    let audio = open_sound_file(&opt.path, &opt.settings, MAX_MUSIC_SAMPLE_RATE)?;
    info!("Analyzing audio waveform");
    let progress = progress_bar(1);
    progress.set_message(audio.tag().name.clone());
    let encoder =
        PcmHpsWriter::new(audio).on_progress(|p| update_audio_progress(&progress, p)).prepare()?;
    progress.finish_using_style();

    info!("Encoding audio to GameCube format");
    let progress = progress_bar(1);
    progress.set_message(name);
    // Copy the loop setting from the original HPS
    let looping = if original_loop.is_some() { Looping::Enabled } else { Looping::Disabled };
    let mut writer = Cursor::new(vec![]);
    encoder
        .looping(looping)
        .on_progress(|p| update_audio_progress(&progress, p))
        .write_to(&mut writer)?;
    progress.finish_using_style();

    info!("Updating game files");
    writer.seek(SeekFrom::Start(0))?;
    ctx.begin_update().write_file(&file, writer).commit()?;
    Ok(())
}

fn import_sfx<T: ReadWriteSeek>(
    ctx: &mut OpenContext<T>,
    opt: AudioImportOpt,
    file: FileId,
    index: usize,
) -> Result<()> {
    let name = ctx.query_file(&file)?.name;
    let mut bank = ctx.read_bank_file(&file)?;

    let mut audio = open_sound_file(&opt.path, &opt.settings, MAX_SFX_SAMPLE_RATE)?;
    info!("Encoding audio to GameCube format");
    let mut new_sample = BankSample::from_pcm(&mut audio)?;
    let old_sample = bank.sample(index);
    if old_sample.channels[0].address.looping && !new_sample.channels[0].address.looping {
        warn!("Setting loop point at the start because none was defined");
        for channel in &mut new_sample.channels {
            channel.address.looping = true;
        }
    }

    info!("Rebuilding {}", name);
    bank.replace_sample(index, new_sample);
    let mut writer = Cursor::new(vec![]);
    bank.write_to(&mut writer)?;

    info!("Updating game files");
    writer.seek(SeekFrom::Start(0))?;
    ctx.begin_update().write_file(&file, writer).commit()?;
    Ok(())
}

/// The `audio play` subcommand.
fn command_play(ctx: Context, opt: AudioPlayOpt) -> Result<()> {
    let ctx = Box::leak(Box::new(ctx.open_read()?));
    let mut cache = AudioCache::new();
    let resource = AudioResource::find(ctx, &opt.name)?;
    let file = AudioFileId::get(ctx, &mut cache, &resource)?;
    let audio = Box::leak(Box::new(AudioReader::open(ctx, &mut cache, &file)?));
    let decoder = audio.decoder();
    let source = PlaybackSource::new(decoder)?.with_volume(opt.volume);

    info!("Checking system audio configuration");
    let mut device = PlaybackDevice::open_default(source.sample_rate())?;

    info!("Starting playback");
    playback::play(&mut device, source, resource.name().to_owned());

    info!("Playback finished");
    Ok(())
}

/// The `audio` CLI command.
pub fn command(ctx: Context, opt: AudioCommand) -> Result<()> {
    match opt {
        AudioCommand::Info(opt) => command_info(ctx, opt),
        AudioCommand::Export(opt) => command_export(ctx, opt),
        AudioCommand::ExportBank(opt) => command_export_bank(ctx, opt),
        AudioCommand::ExportAll(opt) => command_export_all(ctx, opt),
        AudioCommand::Import(opt) => command_import(ctx, opt),
        AudioCommand::Play(opt) => command_play(ctx, opt),
    }
}

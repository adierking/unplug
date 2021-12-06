use crate::common::*;
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::*;
use anyhow::{bail, Result};
use log::{debug, info, warn};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;
use unicase::UniCase;
use unplug::audio::format::PcmS16Le;
use unplug::audio::transport::hps::{HpsStream, PcmHpsBuilder};
use unplug::audio::transport::{FlacReader, Mp3Reader, OggReader, SoundBank, WavReader, WavWriter};
use unplug::audio::ReadSamples;
use unplug::common::io::{copy_buffered, BUFFER_SIZE};
use unplug::common::{ReadSeek, WriteTo};
use unplug::data::atc::ATCS;
use unplug::data::item::{ItemFlags, ITEMS};
use unplug::data::object::Object;
use unplug::data::sound::{Sound, SoundDefinition};
use unplug::data::sound_bank::SOUND_BANKS;
use unplug::data::stage::STAGES;
use unplug::dvd::{ArchiveReader, DiscStream, Entry, FileEntry, FileTree, OpenFile};
use unplug::event::{Block, Script};
use unplug::globals::Libs;
use unplug::stage::Stage;

const UNKNOWN_ID_PREFIX: &str = "unk";

/// The highest sample rate that imported music can have. Music sampled higher than this will be
/// downsampled.
const MAX_MUSIC_SAMPLE_RATE: u32 = 44100;

const SFX_HORI_NAME: &str = "sfx_hori.ssm";
const SFX_HORI_PATH: &str = "qp/sfx_hori.ssm";

fn list_files(tree: &FileTree, opt: ListOpt) -> Result<()> {
    let mut files: Vec<(String, &FileEntry)> =
        tree.recurse().filter_map(|(p, e)| tree[e].file().map(|f| (p, f))).collect();
    if opt.by_offset {
        files.sort_unstable_by_key(|(_, f)| f.offset);
    } else if opt.by_size {
        files.sort_unstable_by_key(|(_, f)| f.size);
    } else {
        files.sort_unstable_by(|(p1, _), (p2, _)| UniCase::new(p1).cmp(&UniCase::new(p2)));
    }
    if opt.reverse {
        files.reverse();
    }
    for (path, file) in files {
        if opt.long {
            println!("{:<8x} {:<8x} {}", file.offset, file.size, path);
        } else {
            println!("{}", path);
        }
    }
    Ok(())
}

pub fn list_archive(opt: ListArchiveOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let archive = ArchiveReader::open(file)?;
    list_files(&archive.files, opt.settings)
}

pub fn list_iso(opt: ListIsoOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let iso = DiscStream::open(file)?;
    println!("Game ID: {}", iso.game_id());
    list_files(&iso.files, opt.settings)
}

fn sort_ids<I: IdString + Ord>(ids: &mut [I], settings: &ListIdsOpt) {
    if settings.by_id {
        ids.sort_unstable();
    } else {
        ids.sort_unstable_by_key(|i| i.to_id());
    }
    if settings.reverse {
        ids.reverse();
    }
}

pub fn list_items(opt: ListItemsOpt) -> Result<()> {
    let mut items: Vec<_> = if opt.show_unknown {
        ITEMS.iter().map(|i| i.id).collect()
    } else {
        ITEMS.iter().filter(|i| !i.flags.contains(ItemFlags::UNUSED)).map(|i| i.id).collect()
    };
    sort_ids(&mut items, &opt.settings);
    for item in items {
        println!("[{:>3}] {}", i16::from(item), item.to_id());
    }
    Ok(())
}

pub fn list_equipment(opt: ListEquipmentOpt) -> Result<()> {
    let mut atcs: Vec<_> = ATCS.iter().map(|a| a.id).collect();
    sort_ids(&mut atcs, &opt.settings);
    for atc in atcs {
        let name = atc.to_id();
        if opt.show_unknown || !name.starts_with(UNKNOWN_ID_PREFIX) {
            println!("[{:>1}] {}", i16::from(atc), atc.to_id());
        }
    }
    Ok(())
}

pub fn list_stages(opt: ListStagesOpt) -> Result<()> {
    let mut stages: Vec<_> = STAGES.iter().map(|s| s.id).collect();
    sort_ids(&mut stages, &opt.settings);
    for stage in stages {
        let name = stage.to_id();
        println!("[{:>3}] {}", i32::from(stage), name);
    }
    Ok(())
}

fn extract_files(
    mut reader: (impl Read + Seek),
    tree: &FileTree,
    output: &Path,
    iobuf: &mut [u8],
) -> Result<()> {
    fs::create_dir_all(&output)?;
    for (path, id) in tree.recurse() {
        let out_path = Path::new(output).join(&path);
        match &tree[id] {
            Entry::File(file) => {
                info!("Extracting {}", path);
                let mut file_writer = File::create(out_path)?;
                let mut file_reader = file.open(&mut reader)?;
                copy_buffered(&mut file_reader, &mut file_writer, iobuf)?;
            }
            Entry::Directory(_) => {
                fs::create_dir_all(out_path)?;
            }
        }
    }
    Ok(())
}

pub fn extract_archive(opt: ExtractArchiveOpt) -> Result<()> {
    let mut iso = open_iso_optional(opt.iso.as_ref())?;
    let mut qp = ArchiveReader::open(match &mut iso {
        Some(iso) => iso.open_file_at(opt.path.to_str().unwrap())?,
        None => Box::new(File::open(opt.path)?),
    })?;

    let mut buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let start_time = Instant::now();
    extract_files(&mut qp.reader, &qp.files, &opt.output, &mut buf)?;

    debug!("Extraction finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn extract_iso(opt: ExtractIsoOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let mut iso = DiscStream::open(file)?;

    let mut buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let start_time = Instant::now();
    extract_files(&mut iso.stream, &iso.files, &opt.output, &mut buf)?;

    info!("Extracting main.dol");
    let dol_path = Path::new(&opt.output).join("main.dol");
    let mut dol_writer = File::create(dol_path)?;
    let (_, mut dol_reader) = iso.open_dol()?;
    copy_buffered(&mut dol_reader, &mut dol_writer, &mut buf)?;

    debug!("Extraction finished in {:?}", start_time.elapsed());
    Ok(())
}

fn dump_script(mut out: impl Write, script: &Script) -> Result<()> {
    info!("Dumping script");

    write!(out, "\nDATA\n\n")?;
    writeln!(out, "off   id   value")?;
    for (location, block) in script.blocks_ordered() {
        if let Block::Data(data) = block {
            writeln!(out, "{:<5x} {:<4} {:?}", location.offset, location.id.index(), data)?;
        }
    }

    write!(out, "\nCODE\n\n")?;
    writeln!(out, "off   id   command")?;
    for (location, command) in script.commands_ordered() {
        let block = location.block;
        writeln!(out, "{:<5x} {:<4} {:?}", block.offset, block.id.index(), command)?;
    }
    Ok(())
}

fn do_dump_stage(stage: &Stage, flags: &DumpStageFlags, mut out: impl Write) -> Result<()> {
    for (i, object) in stage.objects.iter().enumerate() {
        writeln!(out, "obj[{}]: {:?}", i, object)?;
    }
    writeln!(out)?;

    if flags.dump_unknown {
        writeln!(out, "settings: {:?}", stage.settings)?;
        writeln!(out)?;
        for (i, unk) in stage.unk_28.iter().enumerate() {
            writeln!(out, "unk28[{}]: {:?}", i, unk)?;
        }
        writeln!(out)?;
        for (i, unk) in stage.unk_2c.iter().enumerate() {
            writeln!(out, "unk2C[{}]: {:?}", i, unk)?;
        }
        writeln!(out)?;
        for (i, unk) in stage.unk_30.iter().enumerate() {
            writeln!(out, "unk30[{}]: {:?}", i, unk)?;
        }
        writeln!(out)?;
    }

    writeln!(out, "on_prologue: {:?}", stage.on_prologue)?;
    writeln!(out, "on_startup: {:?}", stage.on_startup)?;
    writeln!(out, "on_dead: {:?}", stage.on_dead)?;
    writeln!(out, "on_pose: {:?}", stage.on_pose)?;
    writeln!(out, "on_time_cycle: {:?}", stage.on_time_cycle)?;
    writeln!(out, "on_time_up: {:?}", stage.on_time_up)?;

    dump_script(out, &stage.script)?;
    Ok(())
}

pub fn dump_stage(opt: DumpStageOpt) -> Result<()> {
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_optional(iso.as_mut(), &opt.container)?;

    info!("Reading script globals");
    let libs = {
        let mut globals = read_globals_qp_or_file(qp.as_mut(), opt.globals.path)?;
        globals.read_libs()?
    };

    info!("Reading stage file");
    let stage = read_stage_qp_or_file(qp.as_mut(), opt.stage.name, &libs)?;
    do_dump_stage(&stage, &opt.flags, out)?;
    Ok(())
}

fn do_dump_libs(libs: &Libs, mut out: impl Write) -> Result<()> {
    for (i, id) in libs.entry_points.iter().enumerate() {
        writeln!(out, "lib[{}]: {:?}", i, id)?;
    }
    dump_script(out, &libs.script)?;
    Ok(())
}

pub fn dump_libs(opt: DumpLibsOpt) -> Result<()> {
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_optional(iso.as_mut(), &opt.container)?;

    info!("Reading script globals");
    let mut globals = read_globals_qp_or_file(qp.as_mut(), opt.globals.path)?;
    let libs = globals.read_libs()?;
    do_dump_libs(&libs, out)
}

pub fn dump_all_stages(opt: DumpAllStagesOpt) -> Result<()> {
    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_required(iso.as_mut(), &opt.container)?;

    let start_time = Instant::now();
    info!("Reading script globals");
    let libs = {
        let mut globals = read_globals_qp(&mut qp)?;
        globals.read_libs()?
    };
    fs::create_dir_all(&opt.output)?;
    let libs_out = File::create(Path::join(&opt.output, "globals.txt"))?;
    do_dump_libs(&libs, BufWriter::new(libs_out))?;

    for stage_def in STAGES {
        info!("Reading {}.bin", stage_def.name);
        let stage = read_stage_qp(&mut qp, stage_def.name, &libs)?;
        let stage_out = File::create(Path::join(&opt.output, format!("{}.txt", stage_def.name)))?;
        do_dump_stage(&stage, &opt.flags, BufWriter::new(stage_out))?;
    }

    info!("Dumping finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn dump_colliders(opt: DumpCollidersOpt) -> Result<()> {
    let mut out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_optional(iso.as_mut(), &opt.container)?;

    info!("Reading collider globals");
    let colliders = {
        let mut globals = read_globals_qp_or_file(qp.as_mut(), opt.globals.path)?;
        globals.read_colliders()?
    };

    info!("Dumping colliders");
    for (obj, list) in colliders.objects.iter().enumerate() {
        writeln!(out, "Object {:?} ({}):", Object::try_from(obj as i32)?, obj)?;
        for (i, collider) in list.iter().enumerate() {
            writeln!(out, "{:>2} {:?}", i, collider)?;
        }
        writeln!(out)?;
    }

    Ok(())
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

    info!("Opening audio file");
    let file = File::open(&opt.path)?;
    let name = opt.path.file_name().map(|p| p.to_str().unwrap()).unwrap_or_default().to_owned();
    let ext = opt.path.extension().map(|p| p.to_str().unwrap().to_lowercase()).unwrap_or_default();
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
    let rate = cached.front().expect("no audio packets").rate;
    if rate > MAX_MUSIC_SAMPLE_RATE {
        warn!("The audio file has a high sample rate ({} Hz)!", rate);
        warn!("It will be automatically resampled to {} Hz.", MAX_MUSIC_SAMPLE_RATE);
        audio = Box::from(cached.resample(MAX_MUSIC_SAMPLE_RATE));
    } else {
        audio = Box::from(cached);
    }

    info!("Analyzing audio waveform");
    let progress = progress_bar(1);
    progress.set_message(name);
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

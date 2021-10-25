use crate::common::*;
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::*;
use anyhow::Result;
use log::{debug, info};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::Path;
use std::time::Instant;
use unicase::UniCase;
use unplug::audio::transport::{HpsStream, SoundBank, WavBuilder};
use unplug::common::io::{copy_buffered, BUFFER_SIZE};
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
        info!("Reading {}.bin", stage_def.name());
        let stage = read_stage_qp(&mut qp, stage_def.name(), &libs)?;
        let stage_out = File::create(Path::join(&opt.output, format!("{}.txt", stage_def.name())))?;
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
    let out = BufWriter::new(File::create(&opt.output)?);
    WavBuilder::new()
        .channels(hps.channels.len())
        .sample_rate(hps.sample_rate)
        .samples(hps.decoder())
        .write_to(out)?;

    info!("Export finished in {:?}", start_time.elapsed());
    Ok(())
}

fn export_bank_sounds(bank: &SoundBank, dir: &Path, subdir: Option<&str>) -> Result<()> {
    // Omit names for unusable banks (sfx_hori.ssm)
    let have_names = SOUND_BANKS.iter().any(|b| b.sound_base == bank.base_index);
    fs::create_dir_all(dir)?;
    for (i, sound) in bank.sounds.iter().enumerate() {
        let id = bank.base_index + i as u32;
        let filename = if have_names {
            match Sound::try_from(id) {
                Ok(s) => format!("{}.wav", SoundDefinition::get(s).name),
                Err(_) => format!("{:>04}.wav", id),
            }
        } else {
            format!("{:>04}.wav", id)
        };
        if let Some(subdir) = subdir {
            info!("Writing {}/{}", subdir, filename);
        } else {
            info!("Writing {}", filename);
        }
        let out_path = dir.join(filename);
        let out = BufWriter::new(File::create(&out_path)?);
        WavBuilder::new()
            .channels(sound.channels.len())
            .sample_rate(sound.sample_rate)
            .samples(bank.decoder(i))
            .write_to(out)?;
    }
    Ok(())
}

pub fn export_sounds(opt: ExportSoundsOpt) -> Result<()> {
    let start_time = Instant::now();

    let mut iso = open_iso_optional(opt.iso.as_ref())?;
    if let Some(bank_path) = opt.path {
        // Export single bank
        let mut reader = BufReader::new(open_iso_entry_or_file(iso.as_mut(), &bank_path)?);
        let name = bank_path.file_name().unwrap().to_string_lossy();
        let bank = SoundBank::open(&mut reader, name)?;
        export_bank_sounds(&bank, &opt.output, None)?;
    } else {
        // Export everything
        let mut iso = iso.expect("no iso path or bank path");
        for bank_def in SOUND_BANKS {
            let bank_name = bank_def.path.rsplit(|c| c == '.' || c == '/').nth(1).unwrap();
            info!("Reading {}.ssm", bank_name);
            let mut reader = BufReader::new(iso.open_file_at(bank_def.path)?);
            let bank = SoundBank::open(&mut reader, format!("{}.ssm", bank_name))?;
            let dir = opt.output.join(bank_name);
            export_bank_sounds(&bank, &dir, Some(bank_name))?;
        }
    }

    info!("Export finished in {:?}", start_time.elapsed());
    Ok(())
}

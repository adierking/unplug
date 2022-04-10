#![allow(trivial_numeric_casts)]

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use structopt::clap::ArgGroup;
use structopt::StructOpt;

/// The minimum accepted volume level for playback.
const MIN_VOLUME: i32 = 0;
/// The maximum accepted volume level for playback.
const MAX_VOLUME: i32 = 100;

#[derive(StructOpt)]
#[structopt(name = "Unplug")]
#[structopt(about = "Chibi-Robo! Plug Into Adventure! Modding Toolkit")]
pub struct Opt {
    /// Enables debug logging
    ///
    /// Use -vv in non-distribution builds to enable trace logging
    #[structopt(short, long, parse(from_occurrences), global(true))]
    pub verbose: u64,

    /// Capture inferno trace data to a file (for developers)
    #[cfg(feature = "trace")]
    #[structopt(long, value_name("PATH"), parse(from_os_str), global(true))]
    pub trace: Option<PathBuf>,

    #[structopt(subcommand)]
    pub command: Subcommand,
}

#[derive(StructOpt)]
pub enum Subcommand {
    /// Lists files in a U8 archive (e.g. qp.bin)
    ListArchive(ListArchiveOpt),

    /// Lists files in an ISO
    ListIso(ListIsoOpt),

    /// Lists known item IDs
    ListItems(ListItemsOpt),

    /// Lists known equipment (ATC) IDs
    ListEquipment(ListEquipmentOpt),

    /// Lists known stages
    ListStages(ListStagesOpt),

    /// Extracts a U8 archive (e.g. qp.bin) into a directory
    ExtractArchive(ExtractArchiveOpt),

    /// Extracts a GameCube ISO into a directory
    ExtractIso(ExtractIsoOpt),

    /// Dumps the data from a stage file as text
    DumpStage(DumpStageOpt),

    /// Dumps the library functions from a globals.bin file
    DumpLibs(DumpLibsOpt),

    /// Dumps the data from each stage into a directory
    DumpAllStages(DumpAllStagesOpt),

    /// Dumps the collision data from globals.bin
    DumpColliders(DumpCollidersOpt),

    /// Exports messages to an XML file
    ExportMessages(ExportMessagesOpt),

    /// Imports messages from an XML file
    ImportMessages(ImportMessagesOpt),

    /// Exports global metadata to a JSON file
    ExportGlobals(ExportGlobalsOpt),

    /// Imports global metadata from a JSON file
    ImportGlobals(ImportGlobalsOpt),

    /// Exports shop data to a JSON file
    ExportShop(ExportShopOpt),

    /// Imports shop data from a JSON file
    ImportShop(ImportShopOpt),

    /// Exports a HPS music file to a WAV file
    ExportMusic(ExportMusicOpt),

    /// Imports an audio file, replacing an existing HPS music file
    ImportMusic(ImportMusicOpt),

    /// Plays an HPS music file
    PlayMusic(PlayMusicOpt),

    /// Exports sound effects to WAV files
    ExportSounds(ExportSoundsOpt),

    /// Imports a sound effect from an audio file
    ImportSound(ImportSoundOpt),

    /// Plays a sound effect
    PlaySound(PlaySoundOpt),
}

#[derive(StructOpt)]
#[structopt(group = ArgGroup::with_name("container"))]
pub struct OptionalContainerOpt {
    /// Run within a Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str), group = "container")]
    pub iso: Option<PathBuf>,

    /// Run within a qp.bin archive
    #[structopt(long, value_name("PATH"), parse(from_os_str), group = "container")]
    pub qp: Option<PathBuf>,
}

#[derive(StructOpt)]
#[structopt(group = ArgGroup::with_name("container").required(true))]
pub struct RequiredContainerOpt {
    /// Run within a Chibi-Robo! ISO (required if no --qp)
    #[structopt(long, value_name("PATH"), parse(from_os_str), group = "container")]
    pub iso: Option<PathBuf>,

    /// Run within a qp.bin archive (required if no --iso)
    #[structopt(long, value_name("PATH"), parse(from_os_str), group = "container")]
    pub qp: Option<PathBuf>,
}

#[derive(StructOpt)]
pub struct StageOpt {
    /// The stage name/path
    ///
    /// If the stage is being opened from an ISO or qp.bin, this is the stage
    /// name without any directory or extension, e.g. "stage01". If the stage is
    /// being opened from the local filesystem, this is the path to the file.
    #[structopt(parse(from_os_str))]
    pub name: PathBuf,
}

#[derive(StructOpt)]
pub struct GlobalsOpt {
    /// Path to globals.bin (only if no ISO or qp.bin is provided)
    #[structopt(long("globals"), value_name("PATH"), parse(from_os_str), required_unless_one(&["archive", "iso"]))]
    pub path: Option<PathBuf>,
}

#[derive(StructOpt)]
pub struct ListOpt {
    /// Lists file offsets and sizes
    #[structopt(short, long)]
    pub long: bool,

    /// Sorts files by name (default)
    #[structopt(long, overrides_with_all(&["by-offset", "by-size"]))]
    pub by_name: bool,

    /// Sorts files by offset
    #[structopt(long, overrides_with_all(&["by-name", "by-size"]))]
    pub by_offset: bool,

    /// Sorts files by size
    #[structopt(long, overrides_with_all(&["by-name", "by-offset"]))]
    pub by_size: bool,

    /// Reverses the sorting order
    #[structopt(long)]
    pub reverse: bool,
}

#[derive(StructOpt)]
pub struct ListArchiveOpt {
    #[structopt(flatten)]
    pub settings: ListOpt,

    /// Path to the archive to read
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,
}

#[derive(StructOpt)]
pub struct ListIsoOpt {
    #[structopt(flatten)]
    pub settings: ListOpt,

    /// Path to the ISO to read
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,
}

#[derive(StructOpt)]
pub struct ListIdsOpt {
    /// Sorts by name (default)
    #[structopt(long, overrides_with_all(&["by-id"]))]
    pub by_name: bool,

    /// Sorts by ID number
    #[structopt(long, overrides_with_all(&["by-name"]))]
    pub by_id: bool,

    /// Reverses the sorting order
    #[structopt(long)]
    pub reverse: bool,
}

#[derive(StructOpt)]
pub struct ListItemsOpt {
    #[structopt(flatten)]
    pub settings: ListIdsOpt,

    /// Includes items without names
    #[structopt(long)]
    pub show_unknown: bool,
}

#[derive(StructOpt)]
pub struct ListEquipmentOpt {
    #[structopt(flatten)]
    pub settings: ListIdsOpt,

    /// Includes equipment without names
    #[structopt(long)]
    pub show_unknown: bool,
}

#[derive(StructOpt)]
pub struct ListStagesOpt {
    #[structopt(flatten)]
    pub settings: ListIdsOpt,
}

#[derive(StructOpt)]
pub struct ExtractArchiveOpt {
    /// Run within a Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: Option<PathBuf>,

    /// Path to the archive to read
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,

    /// Directory to extract files to (will be created if necessary)
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: PathBuf,
}

#[derive(StructOpt)]
pub struct ExtractIsoOpt {
    /// Path to the ISO to read
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,

    /// Directory to extract files to (will be created if necessary)
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: PathBuf,
}

#[derive(StructOpt)]
pub struct DumpStageFlags {
    /// Dumps unknown structs
    #[structopt(long)]
    pub dump_unknown: bool,
}

#[derive(StructOpt)]
pub struct DumpStageOpt {
    #[structopt(flatten)]
    pub container: OptionalContainerOpt,

    #[structopt(flatten)]
    pub globals: GlobalsOpt,

    #[structopt(flatten)]
    pub stage: StageOpt,

    /// Redirects output to a file instead of stdout
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: Option<PathBuf>,

    #[structopt(flatten)]
    pub flags: DumpStageFlags,
}

#[derive(StructOpt)]
pub struct DumpLibsOpt {
    #[structopt(flatten)]
    pub container: OptionalContainerOpt,

    #[structopt(flatten)]
    pub globals: GlobalsOpt,

    /// Redirects output to a file instead of stdout
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: Option<PathBuf>,
}

#[derive(StructOpt)]
pub struct DumpAllStagesOpt {
    #[structopt(flatten)]
    pub container: RequiredContainerOpt,

    /// Path to the output directory
    #[structopt(short, value_name("PATH"))]
    pub output: PathBuf,

    #[structopt(flatten)]
    pub flags: DumpStageFlags,
}

#[derive(StructOpt)]
pub struct ExportMessagesOpt {
    #[structopt(flatten)]
    pub container: RequiredContainerOpt,

    /// Path to the output XML file
    #[structopt(short, value_name("PATH"))]
    pub output: PathBuf,
}

#[derive(StructOpt)]
pub struct DumpCollidersOpt {
    #[structopt(flatten)]
    pub container: OptionalContainerOpt,

    #[structopt(flatten)]
    pub globals: GlobalsOpt,

    /// Redirects output to a file instead of stdout
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: Option<PathBuf>,
}

#[derive(StructOpt)]
pub struct ImportMessagesOpt {
    #[structopt(flatten)]
    pub container: RequiredContainerOpt,

    /// Path to the input XML file
    #[structopt(value_name("PATH"))]
    pub input: PathBuf,
}

#[derive(StructOpt)]
pub struct ExportGlobalsOpt {
    #[structopt(flatten)]
    pub container: OptionalContainerOpt,

    #[structopt(flatten)]
    pub globals: GlobalsOpt,

    /// Don't output unnecessary whitespace
    #[structopt(short, long)]
    pub compact: bool,

    /// Redirects output to a file instead of stdout
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: Option<PathBuf>,
}

#[derive(StructOpt)]
pub struct ImportGlobalsOpt {
    #[structopt(flatten)]
    pub container: OptionalContainerOpt,

    #[structopt(flatten)]
    pub globals: GlobalsOpt,

    /// Path to the input JSON file
    #[structopt(value_name("PATH"))]
    pub input: PathBuf,
}

#[derive(StructOpt)]
pub struct ExportShopOpt {
    #[structopt(flatten)]
    pub container: RequiredContainerOpt,

    /// Don't output unnecessary whitespace
    #[structopt(short, long)]
    pub compact: bool,

    /// Redirects output to a file instead of stdout
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: Option<PathBuf>,
}

#[derive(StructOpt)]
pub struct ImportShopOpt {
    #[structopt(flatten)]
    pub container: RequiredContainerOpt,

    /// Path to the input JSON file
    #[structopt(value_name("PATH"))]
    pub input: PathBuf,
}

#[derive(StructOpt)]
pub struct SoundExportOpt {
    /// If an audio file has cue points, exports a .labels.txt file alongside it which defines the
    /// cues using Audacity's label track format.
    #[structopt(long)]
    pub labels: bool,
}

#[derive(StructOpt)]
pub struct ExportMusicOpt {
    /// Run within a Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: Option<PathBuf>,

    #[structopt(flatten)]
    pub settings: SoundExportOpt,

    /// Path to the HPS file to export
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,

    /// Path to the output WAV file
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: PathBuf,
}

#[derive(StructOpt)]
pub struct ImportMusicOpt {
    /// Path to the Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: PathBuf,

    /// Imports an Audacity label track from a file and uses it in place of the audio file's
    /// original cues.
    #[structopt(long)]
    pub labels: Option<PathBuf>,

    /// Path to the HPS file to replace
    #[structopt(value_name("PATH"), parse(from_os_str))]
    pub hps: PathBuf,

    /// Path to the audio file (WAV, FLAC, MP3, OGG)
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,
}

#[derive(StructOpt)]
pub struct ExportSoundsOpt {
    /// Run within a Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: Option<PathBuf>,

    #[structopt(flatten)]
    pub settings: SoundExportOpt,

    /// Path to the SSM file to export. If --iso is provided, this can be omitted to export all sounds.
    #[structopt(value_name("SSM"), parse(from_os_str), required_unless("iso"))]
    pub path: Option<PathBuf>,

    /// Path to the output directory
    #[structopt(short, long("out"), value_name("PATH"))]
    pub output: PathBuf,
}

#[derive(StructOpt)]
pub struct ImportSoundOpt {
    /// Path to the Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: PathBuf,

    /// Imports an Audacity label track from a file and uses it in place of the audio file's
    /// original cues.
    #[structopt(long)]
    pub labels: Option<PathBuf>,

    /// Name of the sound effect to replace
    #[structopt(value_name("NAME"))]
    pub sound: String,

    /// Path to the audio file (WAV, FLAC, MP3, OGG)
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,
}

/// `try_from_str` parser for parsing a playback volume
fn parse_volume(s: &str) -> Result<f64> {
    let volume = s.parse::<i32>()?;
    if (MIN_VOLUME..=MAX_VOLUME).contains(&volume) {
        Ok(f64::from(volume) / 100.0)
    } else {
        Err(anyhow!("volume must be between {} and {}", MIN_VOLUME, MAX_VOLUME))
    }
}

#[derive(StructOpt)]
pub struct PlaybackOpt {
    /// Volume level as a percentage (0-100, default 90)
    #[structopt(long, default_value = "90", parse(try_from_str = parse_volume))]
    pub volume: f64,
}

#[derive(StructOpt)]
pub struct PlayMusicOpt {
    /// Run within a Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: Option<PathBuf>,

    /// Name or path of the music to play
    #[structopt(value_name("NAME"), parse(from_os_str))]
    pub name: PathBuf,

    #[structopt(flatten)]
    pub playback: PlaybackOpt,
}

#[derive(StructOpt)]
pub struct PlaySoundOpt {
    /// Path to the Chibi-Robo! ISO
    #[structopt(long, value_name("PATH"), parse(from_os_str))]
    pub iso: PathBuf,

    /// Name of the sound effect to play
    #[structopt(value_name("NAME"))]
    pub sound: String,

    #[structopt(flatten)]
    pub playback: PlaybackOpt,
}

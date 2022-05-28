#![allow(trivial_numeric_casts, variant_size_differences)]

use anyhow::{anyhow, Result};
use clap::{Args, Parser};
use std::path::PathBuf;

/// The minimum accepted volume level for playback.
const MIN_VOLUME: i32 = 0;
/// The maximum accepted volume level for playback.
const MAX_VOLUME: i32 = 100;

#[derive(Parser)]
#[clap(name = "Unplug")]
#[clap(about = "Chibi-Robo! Plug Into Adventure! Modding Toolkit")]
pub struct Opt {
    /// Enables debug logging
    ///
    /// Use -vv in non-distribution builds to enable trace logging
    #[clap(short, long, parse(from_occurrences), global(true))]
    pub verbose: u64,

    /// Capture inferno trace data to a file (for developers)
    #[cfg(feature = "trace")]
    #[clap(long, value_name("PATH"), parse(from_os_str), global(true))]
    pub trace: Option<PathBuf>,

    #[clap(flatten)]
    pub config: ConfigOpt,

    #[clap(flatten)]
    pub context: ContextOpt,

    #[clap(subcommand)]
    pub command: Subcommand,
}

#[derive(Args)]
pub struct ConfigOpt {
    /// Path to the config file to use. If it does not exist, it will be created.
    #[clap(long, value_name("PATH"), parse(from_os_str), global(true))]
    pub config: Option<PathBuf>,

    /// Do not load or create a config file and use default settings instead.
    #[clap(long, global(true), conflicts_with("config"))]
    pub no_config: bool,
}

#[derive(Args)]
pub struct ContextOpt {
    #[clap(long, value_name("PATH"), parse(from_os_str), global(true))]
    pub iso: Option<PathBuf>,

    /// Opens a project instead of the current one.
    #[clap(long, value_name("NAME"), global(true))]
    pub project: Option<String>,

    /// Do not open any project.
    #[clap(long, global(true), conflicts_with("project"))]
    pub no_project: bool,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Manage Unplug configuration options
    #[clap(subcommand)]
    Config(ConfigCommand),

    /// Manage Unplug projects
    #[clap(subcommand)]
    Project(ProjectCommand),

    /// Commands for working with U8 archives
    #[clap(subcommand)]
    Archive(ArchiveCommand),

    /// Commands for working with audio resources
    #[clap(subcommand)]
    Audio(AudioCommand),

    /// Commands for working with global metadata
    #[clap(subcommand)]
    Globals(GlobalsCommand),

    /// Commands for working with ISO files
    #[clap(subcommand)]
    Iso(IsoCommand),

    /// Commands for listing known game assets
    #[clap(subcommand)]
    List(ListCommand),

    /// Commands for editing cutscene messages
    #[clap(subcommand)]
    Messages(MessagesCommand),

    /// Commands for working with qp.bin (alias for `archive dvd:qp.bin`)
    #[clap(subcommand)]
    Qp(QpCommand),

    /// Commands for working with event scripts
    #[clap(subcommand)]
    Script(ScriptCommand),

    /// Exports shop data to a JSON file
    ExportShop(ExportShopOpt),

    /// Imports shop data from a JSON file
    ImportShop(ImportShopOpt),

    /// Runs Dolphin with the current project/ISO.
    Dolphin(DolphinOpt),

    /// Debugging commands (development builds only)
    #[cfg(feature = "debug")]
    #[clap(subcommand)]
    Debug(DebugCommand),
}

#[derive(clap::Subcommand)]
pub enum ConfigCommand {
    /// Resets all configuration options to their default values.
    Clear,
    /// Prints the absolute path to the config file.
    Path,
    /// Prints the value of a setting.
    #[clap(subcommand)]
    Get(GetSetting),
    /// Sets the value of a setting.
    #[clap(subcommand)]
    Set(SetSetting),
}

#[derive(clap::Subcommand)]
pub enum GetSetting {
    /// A path to an ISO to load by default.
    DefaultIso,
    /// The path to the Dolphin executable (or macOS app bundle) to run projects with.
    DolphinPath,
}

#[derive(clap::Subcommand)]
pub enum SetSetting {
    /// A path to an ISO to load by default.
    DefaultIso { path: Option<String> },
    /// The path to the Dolphin executable (or macOS app bundle) to run projects with.
    DolphinPath { path: Option<String> },
}

#[derive(clap::Subcommand)]
pub enum ProjectCommand {
    /// Displays info about a project (or the current one).
    Info { name: Option<String> },
    /// Lists defined projects.
    List,
    /// Registers an existing project.
    Add {
        /// Path to the project file(s).
        #[clap(parse(from_os_str))]
        path: PathBuf,
        /// Sets the project name (defaults to the filename)
        #[clap(short, long)]
        name: Option<String>,
    },
    /// Unregisters a project without deleting any of its files.
    Forget { name: String },
    /// Opens a project to be automatically used for future Unplug commands.
    Open { name: String },
    /// Closes the currently-open project.
    Close,
}

#[derive(Args)]
pub struct ListOpt {
    /// Lists file offsets and sizes
    #[clap(short, long)]
    pub long: bool,

    /// Sorts files by name (default)
    #[clap(long, overrides_with_all(&["by-offset", "by-size"]))]
    pub by_name: bool,

    /// Sorts files by offset
    #[clap(long, overrides_with_all(&["by-name", "by-size"]))]
    pub by_offset: bool,

    /// Sorts files by size
    #[clap(long, overrides_with_all(&["by-name", "by-offset"]))]
    pub by_size: bool,

    /// Reverses the sorting order
    #[clap(long)]
    pub reverse: bool,
}

#[derive(Args)]
pub struct ListIdsOpt {
    /// Sorts by name (default)
    #[clap(long, overrides_with_all(&["by-id"]))]
    pub by_name: bool,

    /// Sorts by ID number
    #[clap(long, overrides_with_all(&["by-name"]))]
    pub by_id: bool,

    /// Reverses the sorting order
    #[clap(long)]
    pub reverse: bool,
}

#[derive(clap::Subcommand)]
pub enum ListCommand {
    /// Lists each item.
    Items(ListItemsOpt),
    /// Lists each type of equipment.
    Equipment(ListEquipmentOpt),
    /// Lists each stage.
    Stages(ListStagesOpt),
}

#[derive(Args)]
pub struct ListItemsOpt {
    #[clap(flatten)]
    pub settings: ListIdsOpt,

    /// Includes items without names
    #[clap(long)]
    pub show_unknown: bool,
}

#[derive(Args)]
pub struct ListEquipmentOpt {
    #[clap(flatten)]
    pub settings: ListIdsOpt,

    /// Includes equipment without names
    #[clap(long)]
    pub show_unknown: bool,
}

#[derive(Args)]
pub struct ListStagesOpt {
    #[clap(flatten)]
    pub settings: ListIdsOpt,
}

#[derive(clap::Subcommand)]
pub enum ScriptCommand {
    /// Dumps the script data from a single stage
    Dump(ScriptDumpOpt),
    /// Dumps all script data
    DumpAll(ScriptDumpAllOpt),
}

#[derive(Args)]
pub struct ScriptDumpFlags {
    /// Dumps unknown structs
    #[clap(long)]
    pub dump_unknown: bool,

    /// Do not show file offsets
    #[clap(long)]
    pub no_offsets: bool,
}

#[derive(Args)]
pub struct ScriptDumpOpt {
    /// Name of the stage to dump, or "globals" to dump globals
    pub stage: String,

    /// Redirects output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,

    #[clap(flatten)]
    pub flags: ScriptDumpFlags,
}

#[derive(Args)]
pub struct ScriptDumpAllOpt {
    /// Path to the output directory
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: PathBuf,

    #[clap(flatten)]
    pub flags: ScriptDumpFlags,
}

#[derive(clap::Subcommand)]
pub enum MessagesCommand {
    /// Exports messages to an XML file
    Export(MessagesExportOpt),
    /// Imports messages from an XML file
    Import(MessagesImportOpt),
}

#[derive(Args)]
pub struct MessagesExportOpt {
    /// Path to the output XML file
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: PathBuf,
}

#[derive(Args)]
pub struct MessagesImportOpt {
    /// Path to the input XML file
    #[clap(parse(from_os_str))]
    pub input: PathBuf,
}

#[derive(clap::Subcommand)]
pub enum GlobalsCommand {
    /// Exports global metadata to a JSON file
    Export(GlobalsExportOpt),
    /// Imports global metadata from a JSON file
    Import(GlobalsImportOpt),
    /// Dumps collision data to a text file
    DumpColliders(GlobalsDumpCollidersOpt),
}

#[derive(Args)]
pub struct GlobalsExportOpt {
    /// Don't output unnecessary whitespace
    #[clap(short, long)]
    pub compact: bool,

    /// Redirects output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct GlobalsImportOpt {
    /// Path to the input JSON file
    #[clap(parse(from_os_str))]
    pub input: PathBuf,
}

#[derive(Args)]
pub struct GlobalsDumpCollidersOpt {
    /// Redirects output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ExportShopOpt {
    /// Don't output unnecessary whitespace
    #[clap(short, long)]
    pub compact: bool,

    /// Redirects output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ImportShopOpt {
    /// Path to the input JSON file
    #[clap(parse(from_os_str))]
    pub input: PathBuf,
}

#[derive(clap::Subcommand)]
pub enum AudioCommand {
    /// Displays info about an audio resource
    Info(AudioInfoOpt),
    /// Exports one or more audio resources to wav files
    Export(AudioExportOpt),
    /// Exports an entire sample bank to a directory
    ExportBank(AudioExportBankOpt),
    /// Exports all audio resources to a directory
    ExportAll(AudioExportAllOpt),
    /// Imports an audio resource from an audio file
    Import(AudioImportOpt),
    /// Plays an audio resource
    Play(AudioPlayOpt),
}

#[derive(Args)]
pub struct AudioExportSettings {
    /// If an audio file has cue points, exports a .labels.txt file which defines the cues using
    /// Audacity's label track format
    #[clap(long)]
    pub labels: bool,
}

#[derive(Args)]
pub struct AudioImportSettings {
    /// If an audio file has a .labels.txt file alongside it, import Audacity labels from it.
    #[clap(long)]
    pub labels: bool,
}

#[derive(Args)]
pub struct AudioInfoOpt {
    /// The name or path of the audio resource.
    pub name: String,
}

#[derive(Args)]
pub struct AudioExportOpt {
    /// If extracting one audio resource, the path of the .wav file to write, otherwise the
    /// directory to write the audio files to
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,

    #[clap(flatten)]
    pub settings: AudioExportSettings,

    /// Names or paths of the audio resources to export
    pub names: Vec<String>,
}

#[derive(Args)]
pub struct AudioExportBankOpt {
    /// The directory to write the bank's .wav files to (defaults to the bank name)
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: PathBuf,

    #[clap(flatten)]
    pub settings: AudioExportSettings,

    /// Name or path of the sample bank to export
    pub name: String,
}

#[derive(Args)]
pub struct AudioExportAllOpt {
    /// The directory to write files to
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: PathBuf,

    #[clap(flatten)]
    pub settings: AudioExportSettings,
}

#[derive(Args)]
pub struct AudioImportOpt {
    /// Name or path of the sound resource to import
    pub name: String,

    #[clap(flatten)]
    pub settings: AudioImportSettings,

    /// Path to the audio file to import (WAV, FLAC, MP3, OGG)
    #[clap(parse(from_os_str))]
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

#[derive(Args)]
pub struct AudioPlayOpt {
    /// Name or path of the audio resource to play
    pub name: String,

    /// Volume level as a percentage (0-100, default 80)
    #[clap(long, default_value = "80", parse(try_from_str = parse_volume))]
    pub volume: f64,
}

#[derive(Args)]
pub struct DolphinOpt {
    /// Do not wait for Dolphin to exit
    #[clap(short, long)]
    pub no_wait: bool,

    /// Do not capture Dolphin's console output
    #[clap(long)]
    pub no_capture: bool,

    /// Show Dolphin's UI
    #[clap(long)]
    pub ui: bool,
}

#[derive(clap::Subcommand)]
pub enum IsoCommand {
    /// Shows information about the ISO.
    Info,
    /// Lists files in the ISO.
    List(IsoListOpt),
    /// Extracts files from the ISO.
    Extract(IsoExtractOpt),
    /// Extracts all files from the ISO.
    ExtractAll(IsoExtractAllOpt),
    /// Replaces a file in the ISO.
    Replace(IsoReplaceOpt),
}

#[derive(Args)]
pub struct IsoListOpt {
    #[clap(flatten)]
    pub settings: ListOpt,

    /// Paths to list (globbing is supported)
    pub paths: Vec<String>,
}

#[derive(Args)]
pub struct IsoExtractOpt {
    /// If extracting one file, the path of the output file, otherwise the
    /// directory to extract files to
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,

    pub paths: Vec<String>,
}

#[derive(Args)]
pub struct IsoExtractAllOpt {
    /// The directory to extract files to
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct IsoReplaceOpt {
    /// Path of the file in the ISO to replace
    #[clap(value_name("dest"))]
    pub dest_path: String,

    /// Path to the file to replace it with
    #[clap(value_name("src"), parse(from_os_str))]
    pub src_path: PathBuf,
}

#[derive(clap::Subcommand)]
pub enum ArchiveCommand {
    /// Shows information about the archive.
    Info {
        /// Path to the U8 archive
        path: String,
    },
    /// Lists files in the archive.
    List {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveListOpt,
    },
    /// Extracts files from the archive.
    Extract {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveExtractOpt,
    },
    /// Extracts all files from the archive.
    ExtractAll {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveExtractAllOpt,
    },
    /// Replaces a file in the archive.
    Replace {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveReplaceOpt,
    },
}

#[derive(Args)]
pub struct ArchiveListOpt {
    #[clap(flatten)]
    pub settings: ListOpt,

    /// Paths to list (globbing is supported)
    pub paths: Vec<String>,
}

#[derive(Args)]
pub struct ArchiveExtractOpt {
    /// If extracting one file, the path of the output file, otherwise the
    /// directory to extract files to
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,

    pub paths: Vec<String>,
}

#[derive(Args)]
pub struct ArchiveExtractAllOpt {
    /// The directory to extract files to
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ArchiveReplaceOpt {
    /// Path of the file in the archive to replace
    #[clap(value_name("dest"))]
    pub dest_path: String,

    /// Path to the file to replace it with
    #[clap(value_name("src"), parse(from_os_str))]
    pub src_path: PathBuf,
}

#[cfg(feature = "debug")]
#[derive(clap::Subcommand)]
pub enum DebugCommand {
    /// Read and rewrite script data
    RebuildScripts,
}

#[derive(clap::Subcommand)]
pub enum QpCommand {
    /// Shows information about qp.bin.
    Info,
    /// Lists files in qp.bin.
    List(ArchiveListOpt),
    /// Extracts files from qp.bin.
    Extract(ArchiveExtractOpt),
    /// Extracts all files from qp.bin.
    ExtractAll(ArchiveExtractAllOpt),
    /// Replaces a file in qp.bin.
    Replace(ArchiveReplaceOpt),
}

#![allow(trivial_numeric_casts, variant_size_differences)]

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// The minimum accepted volume level for playback.
const MIN_VOLUME: i32 = 0;
/// The maximum accepted volume level for playback.
const MAX_VOLUME: i32 = 100;

#[derive(Parser)]
#[clap(name = "Unplug", author, version)]
#[clap(about = "Chibi-Robo! Plug Into Adventure! Modding Toolkit")]
#[clap(help_expected = true, infer_subcommands = true)]
pub struct Opt {
    /// Show debug logs
    ///
    /// Use -vv in non-distribution builds to show trace logs as well
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
    pub command: Command,
}

#[derive(Args)]
pub struct ConfigOpt {
    /// Path to the config file to use (will be created if necessary)
    #[clap(long, value_name("PATH"), parse(from_os_str), global(true))]
    pub config: Option<PathBuf>,

    /// Ignore the config file and use default settings instead
    #[clap(long, global(true), conflicts_with("config"))]
    pub no_config: bool,
}

#[derive(Args)]
pub struct ContextOpt {
    /// Run the command on an ISO
    #[clap(long, value_name("PATH"), parse(from_os_str), global(true))]
    pub iso: Option<PathBuf>,

    /// Use a project instead of the current one
    #[clap(short, long, value_name("NAME"), global(true))]
    pub project: Option<String>,

    /// Ignore the current project
    #[clap(long, global(true), conflicts_with("project"))]
    pub no_project: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// View, edit, or extract U8 archives
    #[clap(subcommand)]
    Archive(ArchiveCommand),

    /// Export, import, or play audio resources
    #[clap(subcommand)]
    Audio(AudioCommand),

    /// Edit global metadata
    #[clap(subcommand)]
    Globals(GlobalsCommand),

    /// View, edit, or extract the game ISO
    #[clap(subcommand)]
    Iso(IsoCommand),

    /// List information about game assets
    #[clap(subcommand)]
    List(ListCommand),

    /// Edit cutscene messages
    #[clap(subcommand)]
    Messages(MessagesCommand),

    /// View, edit, or extract qp.bin
    ///
    /// This is an alias for `archive dvd:qp.bin`.
    #[clap(subcommand)]
    Qp(QpCommand),

    /// Dump event scripts
    #[clap(subcommand)]
    Script(ScriptCommand),

    /// Edit the in-game shop
    #[clap(subcommand)]
    Shop(ShopCommand),

    /// Edit stage data
    #[clap(subcommand)]
    Stage(StageCommand),

    /// Edit Unplug configuration options
    #[clap(subcommand, display_order = 1000)]
    Config(ConfigCommand),

    #[cfg(feature = "debug")]
    #[clap(subcommand, display_order = 1001)]
    /// Debugging commands (development builds only)
    Debug(DebugCommand),

    /// Run Dolphin with the current project/ISO
    #[clap(display_order = 1002)]
    Dolphin(DolphinOpt),

    /// Manage Unplug projects
    #[clap(subcommand, display_order = 1003)]
    Project(ProjectCommand),
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Reset all configuration options to their default values
    Clear,
    /// Print the absolute path to the config file
    Path,
    /// Print the value of a setting
    #[clap(subcommand)]
    Get(GetSetting),
    /// Set the value of a setting
    #[clap(subcommand)]
    Set(SetSetting),
}

#[derive(Subcommand)]
pub enum GetSetting {
    /// A path to an ISO to load by default
    DefaultIso,
    /// The path to the Dolphin executable (or macOS app bundle) to run projects with
    DolphinPath,
}

#[derive(Subcommand)]
pub enum SetSetting {
    /// A path to an ISO to load by default
    DefaultIso {
        /// The new path
        path: Option<String>,
    },
    /// The path to the Dolphin executable (or macOS app bundle) to run projects with
    DolphinPath {
        /// The new path
        path: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Show information about a project (or the current one)
    Info(ProjectInfoOpt),
    /// List defined projects
    List,
    /// Register an existing project
    Add(ProjectAddOpt),
    /// Unregister a project without deleting any of its files
    Forget(ProjectForgetOpt),
    /// Open a project to be automatically used for future Unplug commands
    Open(ProjectOpenOpt),
    /// Close the currently-open project
    Close,
}

#[derive(Args)]
pub struct ProjectInfoOpt {
    /// Name of the project to show
    pub name: Option<String>,
}

#[derive(Args)]
pub struct ProjectAddOpt {
    /// Path to the project file(s)
    #[clap(parse(from_os_str))]
    pub path: PathBuf,

    /// The project name (defaults to the filename)
    #[clap(short, long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct ProjectForgetOpt {
    /// Name of the project to forget
    pub name: String,
}

#[derive(Args)]
pub struct ProjectOpenOpt {
    /// Name of the project to open
    pub name: String,
}

#[derive(Args)]
pub struct ListOpt {
    /// List file offsets and sizes
    #[clap(short, long)]
    pub long: bool,

    /// Sort files by name (default)
    #[clap(long, overrides_with_all(&["by-offset", "by-size"]))]
    pub by_name: bool,

    /// Sort files by offset
    #[clap(long, overrides_with_all(&["by-name", "by-size"]))]
    pub by_offset: bool,

    /// Sort files by size
    #[clap(long, overrides_with_all(&["by-name", "by-offset"]))]
    pub by_size: bool,

    /// Sort in reverse order
    #[clap(long)]
    pub reverse: bool,
}

#[derive(Args)]
pub struct ListIdsOpt {
    /// Sort by name (default)
    #[clap(long, overrides_with_all(&["by-id"]))]
    pub by_name: bool,

    /// Sort by ID number
    #[clap(long, overrides_with_all(&["by-name"]))]
    pub by_id: bool,

    /// Sort in reverse order
    #[clap(long)]
    pub reverse: bool,
}

#[derive(Subcommand)]
pub enum ListCommand {
    /// List each item
    Items(ListItemsOpt),
    /// List each type of equipment
    Equipment(ListEquipmentOpt),
    /// List each stage
    Stages(ListStagesOpt),
}

#[derive(Args)]
pub struct ListItemsOpt {
    #[clap(flatten)]
    pub settings: ListIdsOpt,

    /// Include items without names
    #[clap(long)]
    pub show_unknown: bool,
}

#[derive(Args)]
pub struct ListEquipmentOpt {
    #[clap(flatten)]
    pub settings: ListIdsOpt,

    /// Include equipment without names
    #[clap(long)]
    pub show_unknown: bool,
}

#[derive(Args)]
pub struct ListStagesOpt {
    #[clap(flatten)]
    pub settings: ListIdsOpt,
}

#[derive(Subcommand)]
pub enum ScriptCommand {
    /// Dump the script data from a single stage
    Dump(ScriptDumpOpt),
    /// Dump all script data
    DumpAll(ScriptDumpAllOpt),
}

#[derive(Args)]
pub struct ScriptDumpFlags {
    /// Dump unknown structs
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

    /// Redirect output to a file instead of stdout
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

#[derive(Subcommand)]
pub enum MessagesCommand {
    /// Export messages to an XML file
    Export(MessagesExportOpt),
    /// Import messages from an XML file
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

#[derive(Subcommand)]
pub enum GlobalsCommand {
    /// Export global metadata to a JSON file
    Export(GlobalsExportOpt),
    /// Import global metadata from a JSON file
    Import(GlobalsImportOpt),
    /// Dump collision data to a text file
    DumpColliders(GlobalsDumpCollidersOpt),
}

#[derive(Args)]
pub struct GlobalsExportOpt {
    /// Don't output unnecessary whitespace
    #[clap(short, long)]
    pub compact: bool,

    /// Redirect output to a file instead of stdout
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
    /// Redirect output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum ShopCommand {
    /// Export shop data to a JSON file
    Export(ShopExportOpt),
    /// Import shop data from a JSON file
    Import(ShopImportOpt),
}

#[derive(Args)]
pub struct ShopExportOpt {
    /// Don't output unnecessary whitespace
    #[clap(short, long)]
    pub compact: bool,

    /// Redirect output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ShopImportOpt {
    /// Path to the input JSON file
    #[clap(parse(from_os_str))]
    pub input: PathBuf,
}

#[derive(Subcommand)]
pub enum AudioCommand {
    /// Show information about an audio resource
    Info(AudioInfoOpt),
    /// Export one or more audio resources to wav files
    Export(AudioExportOpt),
    /// Export an entire sample bank to a directory
    ExportBank(AudioExportBankOpt),
    /// Export all audio resources to a directory
    ExportAll(AudioExportAllOpt),
    /// Import an audio resource from an audio file
    Import(AudioImportOpt),
    /// Play an audio resource
    Play(AudioPlayOpt),
}

#[derive(Args)]
pub struct AudioExportSettings {
    /// If an audio file has cue points, export a .labels.txt file which defines the cues using
    /// Audacity's label track format
    #[clap(long)]
    pub labels: bool,
}

#[derive(Args)]
pub struct AudioImportSettings {
    /// If an audio file has a .labels.txt file alongside it, import Audacity labels from it
    #[clap(long)]
    pub labels: bool,
}

#[derive(Args)]
pub struct AudioInfoOpt {
    /// The name or path of the audio resource
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
    /// Wait for Dolphin to exit and capture console output
    #[clap(short, long)]
    pub wait: bool,

    /// Show Dolphin's UI
    #[clap(long)]
    pub ui: bool,
}

#[derive(Subcommand)]
pub enum IsoCommand {
    /// Show information about the ISO
    Info,
    /// List files in the ISO
    List(IsoListOpt),
    /// Extract files from the ISO
    Extract(IsoExtractOpt),
    /// Extract all files from the ISO
    ExtractAll(IsoExtractAllOpt),
    /// Replace a file in the ISO
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

    /// Paths of files to extract
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

#[derive(Subcommand)]
pub enum ArchiveCommand {
    /// Show information about the archive
    Info {
        /// Path to the U8 archive
        path: String,
    },
    /// List files in the archive
    List {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveListOpt,
    },
    /// Extract files from the archive
    Extract {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveExtractOpt,
    },
    /// Extract all files from the archive
    ExtractAll {
        /// Path to the U8 archive
        path: String,
        #[clap(flatten)]
        opt: ArchiveExtractAllOpt,
    },
    /// Replace a file in the archive
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

    /// Paths of files to extract
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
#[derive(Subcommand)]
pub enum DebugCommand {
    /// Read and rewrite script data
    RebuildScripts,
}

#[derive(Subcommand)]
pub enum QpCommand {
    /// Show information about qp.bin
    Info,
    /// List files in qp.bin
    List(ArchiveListOpt),
    /// Extract files from qp.bin
    Extract(ArchiveExtractOpt),
    /// Extract all files from qp.bin
    ExtractAll(ArchiveExtractAllOpt),
    /// Replace a file in qp.bin
    Replace(ArchiveReplaceOpt),
}

#[derive(Subcommand)]
pub enum StageCommand {
    /// Export stage data to a JSON file
    Export(StageExportOpt),
    /// Export data for all stages to JSON files
    ExportAll(StageExportAllOpt),
    /// Import stage data from a JSON file
    Import(StageImportOpt),
    /// Import all stages from JSON files
    ImportAll(StageImportAllOpt),
}

#[derive(Args)]
pub struct StageExportOpt {
    /// Name of the stage to export
    pub stage: String,

    /// Redirect output to a file instead of stdout
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct StageExportAllOpt {
    /// Path to the output directory
    #[clap(short, value_name("PATH"), parse(from_os_str))]
    pub output: PathBuf,
}

#[derive(Args)]
pub struct StageImportOpt {
    /// Name of the stage to import
    pub stage: String,

    /// Path to the input JSON file
    #[clap(parse(from_os_str))]
    pub input: PathBuf,
}

#[derive(Args)]
pub struct StageImportAllOpt {
    /// Path to the input directory
    #[clap(parse(from_os_str))]
    pub input: PathBuf,

    /// Always import a stage even if it hasn't changed
    #[clap(short, long)]
    pub force: bool,
}

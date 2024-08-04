#![allow(trivial_numeric_casts, variant_size_differences)]

use anyhow::{anyhow, Result};
use clap::{ArgAction, Args, Parser, Subcommand};
use std::path::PathBuf;

/// The minimum accepted volume level for playback.
const MIN_VOLUME: i32 = 0;
/// The maximum accepted volume level for playback.
const MAX_VOLUME: i32 = 100;

#[derive(Parser)]
#[clap(name = "Unplug", version)]
#[clap(about = "Chibi-Robo! Plug Into Adventure! Modding Toolkit")]
#[clap(help_expected = true, infer_subcommands = true)]
pub struct CliArgs {
    /// Show debug logs
    ///
    /// Use -vv in non-distribution builds to show trace logs as well
    #[clap(short, long, action = ArgAction::Count, global(true))]
    pub verbose: u8,

    /// Capture inferno trace data to a file (for developers)
    #[cfg(feature = "trace")]
    #[clap(long, value_name("PATH"), global(true))]
    pub trace: Option<PathBuf>,

    #[clap(flatten)]
    pub config: GlobalConfigArgs,

    #[clap(flatten)]
    pub context: GlobalContextArgs,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Args)]
pub struct GlobalConfigArgs {
    /// Path to the config file to use (will be created if necessary)
    #[clap(long, value_name("PATH"), global(true))]
    pub config: Option<PathBuf>,

    /// Ignore the config file and use default settings instead
    #[clap(long, global(true), conflicts_with("config"))]
    pub no_config: bool,
}

#[derive(Args)]
pub struct GlobalContextArgs {
    /// Run the command on an ISO
    #[clap(long, value_name("PATH"), global(true), group("context"))]
    pub iso: Option<PathBuf>,

    /// Run the command on the default ISO
    #[clap(long, global(true), group("context"))]
    pub default_iso: bool,

    /// Use a project instead of the current one
    #[clap(short, long, value_name("NAME"), global(true), group("context"))]
    pub project: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// View, edit, or extract U8 archives
    #[clap(subcommand)]
    Archive(archive::Subcommand),

    /// Export, import, or play audio resources
    #[clap(subcommand)]
    Audio(audio::Subcommand),

    /// Edit Unplug configuration options
    #[clap(subcommand)]
    Config(config::Subcommand),

    /// Run Dolphin with the current project/ISO
    Dolphin(dolphin::Args),

    /// Edit global metadata
    #[clap(subcommand)]
    Globals(globals::Subcommand),

    /// View, edit, or extract the game ISO
    #[clap(subcommand)]
    Iso(iso::Subcommand),

    /// List information about game assets
    #[clap(subcommand)]
    List(list::Subcommand),

    /// Edit cutscene messages
    #[clap(subcommand)]
    Messages(messages::Subcommand),

    /// Manage Unplug projects
    #[clap(subcommand)]
    Project(project::Subcommand),

    /// View, edit, or extract qp.bin
    ///
    /// This is an alias for `archive dvd:qp.bin`.
    #[clap(subcommand)]
    Qp(archive::QpSubcommand),

    /// Dump event scripts
    #[clap(subcommand)]
    Script(script::Subcommand),

    /// Edit the in-game shop
    #[clap(subcommand)]
    Shop(shop::Subcommand),

    /// Edit stage data
    #[clap(subcommand)]
    Stage(stage::Subcommand),

    #[cfg(feature = "debug")]
    #[clap(subcommand)]
    /// Debugging commands (development builds only)
    Debug(debug::Subcommand),

    #[cfg(test)]
    Test,
}

pub mod config {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
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
}

pub mod project {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Show information about a project (or the current one)
        Info(InfoArgs),
        /// List defined projects
        List,
        /// Create a new project by copying the default ISO
        New(NewArgs),
        /// Delete a project's files and unregister it
        Wipe(WipeArgs),
        /// Register an existing project
        Add(AddArgs),
        /// Unregister a project without deleting any of its files
        #[clap(alias = "forget")]
        Remove(RemoveArgs),
        /// Open a project to be automatically used for future Unplug commands
        Open(OpenArgs),
        /// Close the currently-open project
        Close,
    }

    #[derive(Args)]
    pub struct InfoArgs {
        /// Name of the project to show
        pub name: Option<String>,
    }

    #[derive(Args)]
    pub struct AddArgs {
        /// Path to the project file(s)
        pub path: PathBuf,

        /// The project name (defaults to the filename)
        #[clap(short, long)]
        pub name: Option<String>,
    }

    #[derive(Args)]
    pub struct RemoveArgs {
        /// Name of the project to remove
        pub name: String,
    }

    #[derive(Args)]
    pub struct OpenArgs {
        /// Name of the project to open
        pub name: String,
    }

    #[derive(Args)]
    pub struct NewArgs {
        /// Name of the new project
        pub name: String,

        /// Path to the ISO to copy from
        #[clap(short, value_name("PATH"))]
        pub source: Option<PathBuf>,

        /// Path of the new ISO
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,

        /// Allow overwriting existing projects/files
        #[clap(short, long)]
        pub force: bool,

        /// Do not open the new project
        #[clap(long)]
        pub no_open: bool,
    }

    #[derive(Args)]
    pub struct WipeArgs {
        /// Name of the project to wipe
        pub name: String,

        /// Do not prompt for confirmation
        #[clap(short, long)]
        pub force: bool,
    }
}

pub mod list {
    use super::*;

    #[derive(Args)]
    pub struct Options {
        /// List file offsets and sizes
        #[clap(short, long)]
        pub long: bool,

        /// Sort files by name (default)
        #[clap(long, overrides_with_all(&["by_offset", "by_size"]))]
        pub by_name: bool,

        /// Sort files by offset
        #[clap(long, overrides_with_all(&["by_name", "by_size"]))]
        pub by_offset: bool,

        /// Sort files by size
        #[clap(long, overrides_with_all(&["by_name", "by_offset"]))]
        pub by_size: bool,

        /// Sort in reverse order
        #[clap(long)]
        pub reverse: bool,
    }

    #[derive(Args)]
    pub struct IdArgs {
        /// Sort by name (default)
        #[clap(long, overrides_with_all(&["by_id"]))]
        pub by_name: bool,

        /// Sort by ID number
        #[clap(long, overrides_with_all(&["by_name"]))]
        pub by_id: bool,

        /// Sort in reverse order
        #[clap(long)]
        pub reverse: bool,
    }

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// List each item
        Items(ItemsArgs),
        /// List each type of equipment
        Equipment(EquipmentArgs),
        /// List each stage
        Stages(IdArgs),
        /// List each object
        Objects(IdArgs),
        /// List each music file
        Music(IdArgs),
        /// List each sound effect
        Sounds(IdArgs),
    }

    #[derive(Args)]
    pub struct ItemsArgs {
        #[clap(flatten)]
        pub settings: IdArgs,

        /// Include items without names
        #[clap(long)]
        pub show_unknown: bool,
    }

    #[derive(Args)]
    pub struct EquipmentArgs {
        #[clap(flatten)]
        pub settings: IdArgs,

        /// Include equipment without names
        #[clap(long)]
        pub show_unknown: bool,
    }
}

pub mod script {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Dump the script data from a single stage
        Dump(DumpArgs),
        /// Dump all script data
        DumpAll(DumpAllArgs),
        /// Disassemble a single stage's script
        Disassemble(DisassembleArgs),
        /// Disassemble all scripts
        DisassembleAll(DisassembleAllArgs),
        /// Assemble a single stage's script
        Assemble(AssembleArgs),
    }

    #[derive(Args)]
    pub struct DumpFlags {
        /// Dump unknown structs
        #[clap(long)]
        pub dump_unknown: bool,

        /// Do not show file offsets
        #[clap(long)]
        pub no_offsets: bool,
    }

    #[derive(Args)]
    pub struct DumpArgs {
        /// Name of the stage to dump, or "globals" to dump globals
        pub stage: String,

        /// Redirect output to a file instead of stdout
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,

        #[clap(flatten)]
        pub flags: DumpFlags,
    }

    #[derive(Args)]
    pub struct DumpAllArgs {
        /// Path to the output directory
        #[clap(short, value_name("PATH"))]
        pub output: PathBuf,

        #[clap(flatten)]
        pub flags: DumpFlags,
    }

    #[derive(Args)]
    pub struct DisassembleArgs {
        /// Name of the stage to dump
        pub stage: String,

        /// Path to the output file
        #[clap(short, value_name("PATH"))]
        pub output: PathBuf,
    }

    #[derive(Args)]
    pub struct DisassembleAllArgs {
        /// Path to the output directory
        #[clap(short, value_name("PATH"))]
        pub output: PathBuf,
    }

    #[derive(Args)]
    pub struct AssembleArgs {
        /// Path to the assembly source
        #[clap(value_name("PATH"))]
        pub path: PathBuf,
    }
}

pub mod messages {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Export messages to an XML file
        Export(ExportArgs),
        /// Import messages from an XML file
        Import(ImportArgs),
    }

    #[derive(Args)]
    pub struct ExportArgs {
        /// Path to the output XML file
        #[clap(short, value_name("PATH"))]
        pub output: PathBuf,
    }

    #[derive(Args)]
    pub struct ImportArgs {
        /// Path to the input XML file
        pub input: PathBuf,
    }
}

pub mod globals {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Export global metadata to a JSON file
        Export(ExportArgs),
        /// Import global metadata from a JSON file
        Import(ImportArgs),
        /// Dump collision data to a text file
        DumpColliders(DumpCollidersArgs),
    }

    #[derive(Args)]
    pub struct ExportArgs {
        /// Don't output unnecessary whitespace
        #[clap(short, long)]
        pub compact: bool,

        /// Redirect output to a file instead of stdout
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,
    }

    #[derive(Args)]
    pub struct ImportArgs {
        /// Path to the input JSON file
        pub input: PathBuf,
    }

    #[derive(Args)]
    pub struct DumpCollidersArgs {
        /// Redirect output to a file instead of stdout
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,
    }
}

pub mod shop {
    use super::*;
    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Export shop data to a JSON file
        Export(ExportArgs),
        /// Import shop data from a JSON file
        Import(ImportArgs),
    }

    #[derive(Args)]
    pub struct ExportArgs {
        /// Don't output unnecessary whitespace
        #[clap(short, long)]
        pub compact: bool,

        /// Redirect output to a file instead of stdout
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,
    }

    #[derive(Args)]
    pub struct ImportArgs {
        /// Path to the input JSON file
        pub input: PathBuf,
    }
}

pub mod audio {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Show information about an audio resource
        Info(InfoArgs),
        /// Export one or more audio resources to wav files
        Export(ExportArgs),
        /// Export an entire sample bank to a directory
        ExportBank(ExportBankArgs),
        /// Export all audio resources to a directory
        ExportAll(ExportAllArgs),
        /// Import an audio resource from an audio file
        Import(ImportArgs),
        /// Play an audio resource
        Play(PlayArgs),
    }

    #[derive(Args)]
    pub struct ExportSettings {
        /// If an audio file has cue points, export a .labels.txt file which defines the cues using
        /// Audacity's label track format
        #[clap(long)]
        pub labels: bool,
    }

    #[derive(Args)]
    pub struct ImportSettings {
        /// If an audio file has a .labels.txt file alongside it, import Audacity labels from it
        #[clap(long)]
        pub labels: bool,
    }

    #[derive(Args)]
    pub struct InfoArgs {
        /// The name or path of the audio resource
        pub name: String,
    }

    #[derive(Args)]
    pub struct ExportArgs {
        /// If extracting one audio resource, the path of the .wav file to write, otherwise the
        /// directory to write the audio files to
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,

        #[clap(flatten)]
        pub settings: ExportSettings,

        /// Names or paths of the audio resources to export
        #[clap(required = true)]
        pub names: Vec<String>,
    }

    #[derive(Args)]
    pub struct ExportBankArgs {
        /// The directory to write the bank's .wav files to (defaults to the bank name)
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,

        #[clap(flatten)]
        pub settings: ExportSettings,

        /// Name or path of the sample bank to export
        pub name: String,
    }

    #[derive(Args)]
    pub struct ExportAllArgs {
        /// The directory to write files to
        #[clap(short, value_name("PATH"))]
        pub output: PathBuf,

        #[clap(flatten)]
        pub settings: ExportSettings,
    }

    #[derive(Args)]
    pub struct ImportArgs {
        /// Name or path of the sound resource to import
        pub name: String,

        #[clap(flatten)]
        pub settings: ImportSettings,

        /// Path to the audio file to import (WAV, FLAC, MP3, OGG)
        pub path: PathBuf,
    }

    /// Clap value parser for parsing a playback volume
    fn parse_volume(s: &str) -> Result<f64> {
        let volume = s.parse::<i32>()?;
        if (MIN_VOLUME..=MAX_VOLUME).contains(&volume) {
            Ok(f64::from(volume) / 100.0)
        } else {
            Err(anyhow!("volume must be between {} and {}", MIN_VOLUME, MAX_VOLUME))
        }
    }

    #[derive(Args)]
    pub struct PlayArgs {
        /// Name or path of the audio resource to play
        pub name: String,

        /// Volume level as a percentage (0-100, default 80)
        #[clap(long, default_value = "80", allow_hyphen_values = true, value_parser = parse_volume)]
        pub volume: f64,
    }
}

pub mod dolphin {
    use super::*;

    #[derive(Args)]
    pub struct Args {
        /// Wait for Dolphin to exit and capture console output
        #[clap(short, long)]
        pub wait: bool,

        /// Show Dolphin's UI
        #[clap(long)]
        pub ui: bool,
    }
}

pub mod iso {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Show information about the ISO
        Info,
        /// List files in the ISO
        List(ListArgs),
        /// Extract files from the ISO
        Extract(ExtractArgs),
        /// Extract all files from the ISO
        ExtractAll(ExtractAllArgs),
        /// Replace a file in the ISO
        Replace(ReplaceArgs),
        /// Change properties of the ISO
        #[clap(subcommand)]
        Set(SetCommand),
    }

    #[derive(Subcommand)]
    pub enum SetCommand {
        /// The maker display name (max 63 bytes)
        Maker {
            /// The new maker name
            name: String,
        },
        /// The title display name (max 63 bytes)
        Name {
            /// The new title name
            name: String,
        },
    }

    #[derive(Args)]
    pub struct ListArgs {
        #[clap(flatten)]
        pub settings: list::Options,

        /// Paths to list (globbing is supported)
        pub paths: Vec<String>,
    }

    #[derive(Args)]
    pub struct ExtractArgs {
        /// If extracting one file, the path of the output file, otherwise the
        /// directory to extract files to
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,

        /// Paths of files to extract
        pub paths: Vec<String>,
    }

    #[derive(Args)]
    pub struct ExtractAllArgs {
        /// The directory to extract files to
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,
    }

    #[derive(Args)]
    pub struct ReplaceArgs {
        /// Path of the file in the ISO to replace
        #[clap(value_name("dest"))]
        pub dest_path: String,

        /// Path to the file to replace it with
        #[clap(value_name("src"))]
        pub src_path: PathBuf,
    }
}

pub mod archive {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
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
            opt: ListArgs,
        },
        /// Extract files from the archive
        Extract {
            /// Path to the U8 archive
            path: String,
            #[clap(flatten)]
            opt: ExtractArgs,
        },
        /// Extract all files from the archive
        ExtractAll {
            /// Path to the U8 archive
            path: String,
            #[clap(flatten)]
            opt: ExtractAllArgs,
        },
        /// Replace a file in the archive
        Replace {
            /// Path to the U8 archive
            path: String,
            #[clap(flatten)]
            opt: ReplaceArgs,
        },
    }

    #[derive(Args)]
    pub struct ListArgs {
        #[clap(flatten)]
        pub settings: list::Options,

        /// Paths to list (globbing is supported)
        pub paths: Vec<String>,
    }

    #[derive(Args)]
    pub struct ExtractArgs {
        /// If extracting one file, the path of the output file, otherwise the
        /// directory to extract files to
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,

        /// Paths of files to extract
        pub paths: Vec<String>,
    }

    #[derive(Args)]
    pub struct ExtractAllArgs {
        /// The directory to extract files to
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,
    }

    #[derive(Args)]
    pub struct ReplaceArgs {
        /// Path of the file in the archive to replace
        #[clap(value_name("dest"))]
        pub dest_path: String,

        /// Path to the file to replace it with
        #[clap(value_name("src"))]
        pub src_path: PathBuf,
    }

    #[derive(Subcommand)]
    pub enum QpSubcommand {
        /// Show information about qp.bin
        Info,
        /// List files in qp.bin
        List(ListArgs),
        /// Extract files from qp.bin
        Extract(ExtractArgs),
        /// Extract all files from qp.bin
        ExtractAll(ExtractAllArgs),
        /// Replace a file in qp.bin
        Replace(ReplaceArgs),
    }
}

#[cfg(feature = "debug")]
pub mod debug {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Read and rewrite script data
        RebuildScripts,
    }
}

pub mod stage {
    use super::*;

    #[derive(Subcommand)]
    pub enum Subcommand {
        /// Export stage data to a JSON file
        Export(ExportArgs),
        /// Export data for all stages to JSON files
        ExportAll(ExportAllArgs),
        /// Import stage data from a JSON file
        Import(ImportArgs),
        /// Import all stages from JSON files
        ImportAll(ImportAllArgs),
    }

    #[derive(Args)]
    pub struct ExportArgs {
        /// Name of the stage to export
        pub stage: String,

        /// Redirect output to a file instead of stdout
        #[clap(short, value_name("PATH"))]
        pub output: Option<PathBuf>,
    }

    #[derive(Args)]
    pub struct ExportAllArgs {
        /// Path to the output directory
        #[clap(short, value_name("PATH"))]
        pub output: PathBuf,
    }

    #[derive(Args)]
    pub struct ImportArgs {
        /// Name of the stage to import
        pub stage: String,

        /// Path to the input JSON file
        pub input: PathBuf,
    }

    #[derive(Args)]
    pub struct ImportAllArgs {
        /// Path to the input directory
        pub input: PathBuf,

        /// Always import a stage even if it hasn't changed
        #[clap(short, long)]
        pub force: bool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use float_cmp::approx_eq;
    use std::ffi::OsString;
    use std::iter;
    use std::path::Path;

    /// Generates a mapping function which pattern-matches a parsed command
    macro_rules! mapper {
        ($p:pat => $out:expr) => {
            |a: CliArgs| {
                let $p = a.command else { panic!() };
                $out
            }
        };
    }

    /// Parses `args` using an argument parser, maps the results using `mapper`, and passes the
    /// final value to `predicate`.
    fn parse<O, S, I, M, A, F>(args: I, mapper: M, predicate: F)
    where
        O: Parser,
        S: Into<OsString> + Clone,
        I: IntoIterator<Item = S>,
        M: FnOnce(O) -> A,
        F: FnOnce(A),
    {
        let opt = O::try_parse_from(
            iter::once(OsString::new()).chain(args.into_iter().map(|a| a.into())),
        )
        .unwrap();
        predicate(mapper(opt));
    }

    /// Parses each list of arguments in `argsets` using an argument parser, maps each result using
    /// `mapper`, and calls `predicate` for each value.
    fn multiparse<O, S, I, J, M, A, F>(argsets: J, mapper: M, predicate: F)
    where
        O: Parser,
        S: Into<OsString> + Clone,
        I: IntoIterator<Item = S>,
        J: IntoIterator<Item = I>,
        M: Fn(O) -> A,
        F: Fn(A),
    {
        for args in argsets {
            let opt = O::try_parse_from(
                iter::once(OsString::new()).chain(args.into_iter().map(|a| a.into())),
            )
            .unwrap();
            predicate(mapper(opt));
        }
    }

    fn error(args: impl IntoIterator<Item = &'static str>) -> ErrorKind {
        CliArgs::try_parse_from(iter::once("unplug").chain(args)).err().expect("error").kind()
    }

    #[test]
    fn test_cli_empty() {
        assert_eq!(error([]), ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand);
    }

    #[test]
    fn test_cli_global_options() {
        let map = std::convert::identity::<CliArgs>;
        parse(["-v", "test"], map, |opt| {
            assert_eq!(opt.verbose, 1);
        });
        parse(["-vv", "test"], map, |opt| {
            assert_eq!(opt.verbose, 2);
        });
        parse(["--config", "foo", "test"], map, |opt| {
            assert_eq!(opt.config.config.as_deref(), Some(Path::new("foo")));
        });
        parse(["--no-config", "test"], map, |opt| {
            assert!(opt.config.no_config);
        });
        assert_eq!(error(["--config", "foo", "--no-config", "test"]), ErrorKind::ArgumentConflict);
        parse(["--iso", "foo", "test"], map, |opt| {
            assert_eq!(opt.context.iso.as_deref(), Some(Path::new("foo")));
        });
        parse(["--default-iso", "test"], map, |opt| {
            assert!(opt.context.default_iso);
        });
        multiparse([["-p", "foo", "test"], ["--project", "foo", "test"]], map, |opt| {
            assert_eq!(opt.context.project.as_deref(), Some("foo"));
        });
        assert_eq!(error(["--iso", "foo", "--default-iso", "test"]), ErrorKind::ArgumentConflict);
        assert_eq!(error(["--iso", "foo", "-p", "bar", "test"]), ErrorKind::ArgumentConflict);
        assert_eq!(error(["--default-iso", "-p", "bar", "test"]), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn test_cli_list_options() {
        use list::*;
        #[derive(Parser)]
        struct ListOptionsParser {
            #[clap(flatten)]
            inner: Options,
        }
        let map = |o: ListOptionsParser| o.inner;
        parse(["--reverse"], map, |opt| {
            assert!(opt.reverse);
        });
        multiparse([["-l"], ["--long"]], map, |opt| {
            assert!(opt.long);
        });
        multiparse(
            [&["--by-name"][..], &["--by-offset", "--by-name"], &["--by-size", "--by-name"]],
            map,
            |opt| {
                assert!(opt.by_name);
                assert!(!opt.by_offset);
                assert!(!opt.by_size);
            },
        );
        multiparse(
            [&["--by-offset"][..], &["--by-name", "--by-offset"], &["--by-size", "--by-offset"]],
            map,
            |opt| {
                assert!(!opt.by_name);
                assert!(opt.by_offset);
                assert!(!opt.by_size);
            },
        );
        multiparse(
            [&["--by-size"][..], &["--by-name", "--by-size"], &["--by-offset", "--by-size"]],
            map,
            |opt| {
                assert!(!opt.by_name);
                assert!(!opt.by_offset);
                assert!(opt.by_size);
            },
        );
    }

    #[test]
    fn test_cli_list_ids_args() {
        use list::*;
        #[derive(Parser)]
        struct ListIdArgsParser {
            #[clap(flatten)]
            inner: IdArgs,
        }
        let map = |o: ListIdArgsParser| o.inner;
        parse(["--reverse"], map, |opt| {
            assert!(opt.reverse);
        });
        multiparse([&["--by-name"][..], &["--by-id", "--by-name"]], map, |opt| {
            assert!(opt.by_name);
            assert!(!opt.by_id);
        });
        multiparse([&["--by-id"][..], &["--by-name", "--by-id"]], map, |opt| {
            assert!(!opt.by_name);
            assert!(opt.by_id);
        });
    }

    #[test]
    fn test_cli_archive_info() {
        use archive::*;
        let map = mapper!(Command::Archive(Subcommand::Info { path }) => path);
        parse(["archive", "info", "foo"], map, |path| {
            assert_eq!(path, "foo");
        });
        assert_eq!(error(["archive", "info"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_archive_list() {
        use archive::*;
        let map = mapper!(Command::Archive(Subcommand::List { path, opt }) => (path, opt));
        parse(["archive", "list", "qp.bin"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert!(opt.paths.is_empty());
        });
        parse(["archive", "list", "qp.bin", "foo"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.paths, ["foo"]);
        });
        parse(["archive", "list", "qp.bin", "foo", "bar", "--long"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.paths, ["foo", "bar"]);
            assert!(opt.settings.long);
        });
        assert_eq!(error(["archive", "list"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_archive_extract() {
        use archive::*;
        let map = mapper!(Command::Archive(Subcommand::Extract { path, opt }) => (path, opt));
        parse(["archive", "extract", "qp.bin"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.output, None);
            assert!(opt.paths.is_empty());
        });
        parse(["archive", "extract", "qp.bin", "-o", "out", "foo", "bar"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.output, Some("out".into()));
            assert_eq!(opt.paths, ["foo", "bar"]);
        });
        assert_eq!(error(["archive", "extract"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_archive_extract_all() {
        use archive::*;
        let map = mapper!(Command::Archive(Subcommand::ExtractAll { path, opt }) => (path, opt));
        parse(["archive", "extract-all", "qp.bin"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.output, None);
        });
        parse(["archive", "extract-all", "qp.bin", "-o", "out"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.output, Some("out".into()));
        });
        assert_eq!(error(["archive", "extract-all"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_archive_replace() {
        use archive::*;
        let map = mapper!(Command::Archive(Subcommand::Replace { path, opt }) => (path, opt));
        parse(["archive", "replace", "qp.bin", "foo", "bar"], map, |(path, opt)| {
            assert_eq!(path, "qp.bin");
            assert_eq!(opt.dest_path, "foo");
            assert_eq!(opt.src_path, Path::new("bar"));
        });
        assert_eq!(error(["archive", "replace"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["archive", "replace", "qp.bin"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            error(["archive", "replace", "qp.bin", "foo"]),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn test_cli_audio_info() {
        use audio::*;
        let map = mapper!(Command::Audio(Subcommand::Info(opt)) => opt);
        parse(["audio", "info", "foo"], map, |opt| {
            assert_eq!(opt.name, "foo");
        });
        assert_eq!(error(["audio", "info"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_audio_export() {
        use audio::*;
        let map = mapper!(Command::Audio(Subcommand::Export(opt)) => opt);
        parse(["audio", "export", "foo"], map, |opt| {
            assert_eq!(opt.output, None);
            assert_eq!(opt.names, ["foo"]);
        });
        parse(["audio", "export", "foo", "bar", "baz"], map, |opt| {
            assert_eq!(opt.output, None);
            assert_eq!(opt.names, ["foo", "bar", "baz"]);
        });
        parse(["audio", "export", "-o", "out", "--labels", "foo"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
            assert!(opt.settings.labels);
            assert_eq!(opt.names, ["foo"]);
        });
        assert_eq!(error(["audio", "export"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_audio_export_bank() {
        use audio::*;
        let map = mapper!(Command::Audio(Subcommand::ExportBank(opt)) => opt);
        parse(["audio", "export-bank", "foo"], map, |opt| {
            assert_eq!(opt.output, None);
            assert_eq!(opt.name, "foo");
        });
        parse(["audio", "export-bank", "-o", "out", "--labels", "foo"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
            assert!(opt.settings.labels);
            assert_eq!(opt.name, "foo");
        });
        assert_eq!(error(["audio", "export-bank"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["audio", "export-bank", "foo", "bar"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_audio_export_all() {
        use audio::*;
        let map = mapper!(Command::Audio(Subcommand::ExportAll(opt)) => opt);
        parse(["audio", "export-all", "-o", "out"], map, |opt| {
            assert_eq!(opt.output, Path::new("out"));
            assert!(!opt.settings.labels);
        });
        parse(["audio", "export-all", "-o", "out", "--labels"], map, |opt| {
            assert_eq!(opt.output, Path::new("out"));
            assert!(opt.settings.labels);
        });
        assert_eq!(error(["audio", "export-all"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_audio_import() {
        use audio::*;
        let map = mapper!(Command::Audio(Subcommand::Import(opt)) => opt);
        parse(["audio", "import", "foo", "bar"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert_eq!(opt.path, Path::new("bar"));
            assert!(!opt.settings.labels);
        });
        parse(["audio", "import", "foo", "bar", "--labels"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert_eq!(opt.path, Path::new("bar"));
            assert!(opt.settings.labels);
        });
        assert_eq!(error(["audio", "import"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["audio", "import", "foo"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["audio", "import", "foo", "bar", "baz"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_audio_play() {
        use audio::*;
        let map = mapper!(Command::Audio(Subcommand::Play(opt)) => opt);
        parse(["audio", "play", "foo"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert!(approx_eq!(f64, opt.volume, 0.8));
        });
        parse(["audio", "play", "foo", "--volume", "0"], map, |opt| {
            assert!(approx_eq!(f64, opt.volume, 0.0));
        });
        parse(["audio", "play", "foo", "--volume", "50"], map, |opt| {
            assert!(approx_eq!(f64, opt.volume, 0.5));
        });
        parse(["audio", "play", "foo", "--volume", "100"], map, |opt| {
            assert!(approx_eq!(f64, opt.volume, 1.0));
        });
        assert_eq!(error(["audio", "play"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["audio", "play", "foo", "bar"]), ErrorKind::UnknownArgument);
        assert_eq!(error(["audio", "play", "foo", "--volume", "-1"]), ErrorKind::ValueValidation);
        assert_eq!(error(["audio", "play", "foo", "--volume", "101"]), ErrorKind::ValueValidation);
    }

    #[test]
    fn test_cli_config() {
        use config::*;
        let map = std::convert::identity;
        parse(["config", "clear"], map, |a: CliArgs| {
            assert!(matches!(a.command, Command::Config(Subcommand::Clear)));
        });
        parse(["config", "path"], map, |a: CliArgs| {
            assert!(matches!(a.command, Command::Config(Subcommand::Path)));
        });
        parse(["config", "get", "default-iso"], map, |a: CliArgs| {
            let Command::Config(Subcommand::Get(opt)) = a.command else { panic!() };
            assert!(matches!(opt, GetSetting::DefaultIso));
        });
        parse(["config", "get", "dolphin-path"], map, |a: CliArgs| {
            let Command::Config(Subcommand::Get(opt)) = a.command else { panic!() };
            assert!(matches!(opt, GetSetting::DolphinPath));
        });
        parse(["config", "set", "default-iso"], map, |a: CliArgs| {
            let Command::Config(Subcommand::Set(SetSetting::DefaultIso { path })) = a.command
            else {
                panic!()
            };
            assert_eq!(path, None);
        });
        parse(["config", "set", "default-iso", "foo"], map, |a: CliArgs| {
            let Command::Config(Subcommand::Set(SetSetting::DefaultIso { path })) = a.command
            else {
                panic!()
            };
            assert_eq!(path.as_deref(), Some("foo"));
        });
        parse(["config", "set", "dolphin-path"], map, |a: CliArgs| {
            let Command::Config(Subcommand::Set(SetSetting::DolphinPath { path })) = a.command
            else {
                panic!()
            };
            assert_eq!(path, None);
        });
        parse(["config", "set", "dolphin-path", "foo"], map, |a: CliArgs| {
            let Command::Config(Subcommand::Set(SetSetting::DolphinPath { path })) = a.command
            else {
                panic!()
            };
            assert_eq!(path.as_deref(), Some("foo"));
        });
        assert_eq!(error(["config", "get"]), ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand);
        assert_eq!(error(["config", "set"]), ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand);
    }

    #[test]
    fn test_cli_dolphin() {
        let map = mapper!(Command::Dolphin(opt) => opt);
        parse(["dolphin"], map, |opt| {
            assert!(!opt.wait);
            assert!(!opt.ui);
        });
        parse(["dolphin", "--ui"], map, |opt| {
            assert!(opt.ui);
        });
        multiparse([["dolphin", "-w"], ["dolphin", "--wait"]], map, |opt| {
            assert!(opt.wait);
        });
    }

    #[test]
    fn test_cli_globals_export() {
        use globals::*;
        let map = mapper!(Command::Globals(Subcommand::Export(opt)) => opt);
        parse(["globals", "export"], map, |opt| {
            assert!(!opt.compact);
            assert_eq!(opt.output, None);
        });
        multiparse([["globals", "export", "-c"], ["globals", "export", "--compact"]], map, |opt| {
            assert!(opt.compact);
        });
        parse(["globals", "export", "-o", "out"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
    }

    #[test]
    fn test_cli_globals_import() {
        use globals::*;
        let map = mapper!(Command::Globals(Subcommand::Import(opt)) => opt);
        parse(["globals", "import", "foo"], map, |opt| {
            assert_eq!(opt.input, Path::new("foo"));
        });
        assert_eq!(error(["globals", "import"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_globals_dump_colliders() {
        use globals::*;
        let map = mapper!(Command::Globals(Subcommand::DumpColliders(opt)) => opt);
        parse(["globals", "dump-colliders"], map, |opt| {
            assert_eq!(opt.output, None);
        });
        parse(["globals", "dump-colliders", "-o", "out"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
    }

    #[test]
    fn test_cli_iso_info() {
        use iso::*;
        let map = mapper!(Command::Iso(c) => c);
        parse(["iso", "info"], map, |c| {
            assert!(matches!(c, Subcommand::Info));
        });
    }

    #[test]
    fn test_cli_iso_list() {
        use iso::*;
        let map = mapper!(Command::Iso(Subcommand::List(opt)) => opt);
        parse(["iso", "list"], map, |opt| {
            assert!(opt.paths.is_empty());
        });
        parse(["iso", "list", "foo", "bar", "--long"], map, |opt| {
            assert_eq!(opt.paths, ["foo", "bar"]);
            assert!(opt.settings.long);
        });
    }

    #[test]
    fn test_cli_iso_extract() {
        use iso::*;
        let map = mapper!(Command::Iso(Subcommand::Extract(opt)) => opt);
        parse(["iso", "extract"], map, |opt| {
            assert_eq!(opt.output, None);
            assert!(opt.paths.is_empty());
        });
        parse(["iso", "extract", "foo", "bar", "-o", "out"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
            assert_eq!(opt.paths, ["foo", "bar"]);
        });
    }

    #[test]
    fn test_cli_iso_extract_all() {
        use iso::*;
        let map = mapper!(Command::Iso(Subcommand::ExtractAll(opt)) => opt);
        parse(["iso", "extract-all"], map, |opt| {
            assert_eq!(opt.output, None);
        });
        parse(["iso", "extract-all", "-o", "out"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
    }

    #[test]
    fn test_cli_iso_replace() {
        use iso::*;
        let map = mapper!(Command::Iso(Subcommand::Replace(opt)) => opt);
        parse(["iso", "replace", "foo", "bar"], map, |opt| {
            assert_eq!(opt.dest_path, "foo");
            assert_eq!(opt.src_path, Path::new("bar"));
        });
        assert_eq!(error(["iso", "replace"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["iso", "replace", "foo"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["iso", "replace", "foo", "bar", "baz"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_iso_set() {
        use iso::*;
        let map = mapper!(Command::Iso(Subcommand::Set(command)) => command);
        parse(["iso", "set", "maker", "The"], map, |command| {
            let SetCommand::Maker { name } = command else { panic!() };
            assert_eq!(name, "The");
        });
        parse(["iso", "set", "name", "This"], map, |command| {
            let SetCommand::Name { name } = command else { panic!() };
            assert_eq!(name, "This");
        });
    }

    #[test]
    fn test_cli_list() {
        use list::*;
        let map = mapper!(Command::List(c) => c);
        parse(["list", "items"], map, |c| {
            let Subcommand::Items(opt) = c else { panic!() };
            assert!(!opt.show_unknown);
        });
        parse(["list", "items", "--show-unknown"], map, |c| {
            let Subcommand::Items(opt) = c else { panic!() };
            assert!(opt.show_unknown);
        });
        parse(["list", "equipment"], map, |c| {
            let Subcommand::Equipment(opt) = c else { panic!() };
            assert!(!opt.show_unknown);
        });
        parse(["list", "equipment", "--show-unknown"], map, |c| {
            let Subcommand::Equipment(opt) = c else { panic!() };
            assert!(opt.show_unknown);
        });
        parse(["list", "stages"], map, |c| {
            assert!(matches!(c, Subcommand::Stages(_)));
        });
        parse(["list", "objects"], map, |c| {
            assert!(matches!(c, Subcommand::Objects(_)));
        });
        parse(["list", "music"], map, |c| {
            assert!(matches!(c, Subcommand::Music(_)));
        });
        parse(["list", "sounds"], map, |c| {
            assert!(matches!(c, Subcommand::Sounds(_)));
        });
    }

    #[test]
    fn test_cli_messages() {
        use messages::*;
        let map = mapper!(Command::Messages(c) => c);
        parse(["messages", "export", "-o", "out"], map, |c| {
            let Subcommand::Export(opt) = c else { panic!() };
            assert_eq!(opt.output, Path::new("out"));
        });
        parse(["messages", "import", "foo"], map, |c| {
            let Subcommand::Import(opt) = c else { panic!() };
            assert_eq!(opt.input, Path::new("foo"));
        });
    }

    #[test]
    fn test_cli_project_info() {
        use project::*;
        let map = mapper!(Command::Project(Subcommand::Info(opt)) => opt);
        parse(["project", "info"], map, |opt| {
            assert_eq!(opt.name, None);
        });
        parse(["project", "info", "foo"], map, |opt| {
            assert_eq!(opt.name.as_deref(), Some("foo"));
        });
    }

    #[test]
    fn test_cli_project_list() {
        use project::*;
        let map = std::convert::identity;
        parse(["project", "list"], map, |opt: CliArgs| {
            assert!(matches!(opt.command, Command::Project(Subcommand::List)));
        });
        assert_eq!(error(["project", "list", "foo"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_project_new() {
        use project::*;
        let map = mapper!(Command::Project(Subcommand::New(opt)) => opt);
        parse(["project", "new", "foo"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert_eq!(opt.source, None);
            assert_eq!(opt.output, None);
            assert!(!opt.force);
            assert!(!opt.no_open);
        });
        parse(["project", "new", "foo", "-s", "src"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert_eq!(opt.source.as_deref(), Some(Path::new("src")));
        });
        parse(["project", "new", "foo", "-o", "out"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
        parse(["project", "new", "foo", "--no-open"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert!(opt.no_open);
        });
        multiparse(
            [["project", "new", "foo", "-f"], ["project", "new", "foo", "--force"]],
            map,
            |opt| {
                assert_eq!(opt.name, "foo");
                assert!(opt.force);
            },
        );
        assert_eq!(error(["project", "new"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_project_wipe() {
        use project::*;
        let map = mapper!(Command::Project(Subcommand::Wipe(opt)) => opt);
        parse(["project", "wipe", "foo"], map, |opt| {
            assert_eq!(opt.name, "foo");
            assert!(!opt.force);
        });
        multiparse(
            [["project", "wipe", "foo", "-f"], ["project", "wipe", "foo", "--force"]],
            map,
            |opt| {
                assert_eq!(opt.name, "foo");
                assert!(opt.force);
            },
        );
        assert_eq!(error(["project", "wipe"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_project_add() {
        use project::*;
        let map = mapper!(Command::Project(Subcommand::Add(opt)) => opt);
        parse(["project", "add", "foo"], map, |opt| {
            assert_eq!(opt.path, Path::new("foo"));
            assert_eq!(opt.name, None);
        });
        multiparse(
            [["project", "add", "foo", "-n", "bar"], ["project", "add", "foo", "--name", "bar"]],
            map,
            |opt| {
                assert_eq!(opt.path, Path::new("foo"));
                assert_eq!(opt.name.as_deref(), Some("bar"));
            },
        );
        assert_eq!(error(["project", "add"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_project_remove() {
        use project::*;
        let map = mapper!(Command::Project(Subcommand::Remove(opt)) => opt);
        multiparse([["project", "remove", "foo"], ["project", "forget", "foo"]], map, |opt| {
            assert_eq!(opt.name, "foo");
        });
        assert_eq!(error(["project", "remove"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["project", "remove", "foo", "bar"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_project_open() {
        use project::*;
        let map = mapper!(Command::Project(Subcommand::Open(opt)) => opt);
        parse(["project", "open", "foo"], map, |opt| {
            assert_eq!(opt.name, "foo");
        });
        assert_eq!(error(["project", "open"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["project", "open", "foo", "bar"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_project_close() {
        use project::*;
        let map = mapper!(Command::Project(c) => c);
        parse(["project", "close"], map, |c| {
            assert!(matches!(c, Subcommand::Close));
        });
        assert_eq!(error(["project", "close", "foo"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_qp() {
        use archive::*;
        let map = mapper!(Command::Qp(c) => c);
        parse(["qp", "info"], map, |c| {
            assert!(matches!(c, QpSubcommand::Info));
        });
        parse(["qp", "list"], map, |c| {
            assert!(matches!(c, QpSubcommand::List(_)));
        });
        parse(["qp", "extract"], map, |c| {
            assert!(matches!(c, QpSubcommand::Extract(_)));
        });
        parse(["qp", "extract-all"], map, |c| {
            assert!(matches!(c, QpSubcommand::ExtractAll(_)));
        });
        parse(["qp", "replace", "foo", "bar"], map, |c| {
            assert!(matches!(c, QpSubcommand::Replace(_)));
        });
    }

    #[test]
    fn test_cli_script_dump() {
        use script::*;
        let map = mapper!(Command::Script(Subcommand::Dump(opt)) => opt);
        parse(["script", "dump", "foo"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.output, None);
            assert!(!opt.flags.dump_unknown);
            assert!(!opt.flags.no_offsets);
        });
        parse(["script", "dump", "foo", "-o", "out"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
        parse(["script", "dump", "foo", "--dump-unknown", "--no-offsets"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.output, None);
            assert!(opt.flags.dump_unknown);
            assert!(opt.flags.no_offsets);
        });
    }

    #[test]
    fn test_cli_script_dump_all() {
        use script::*;
        let map = mapper!(Command::Script(Subcommand::DumpAll(opt)) => opt);
        parse(["script", "dump-all", "-o", "out"], map, |opt| {
            assert_eq!(opt.output, Path::new("out"));
        });
        parse(["script", "dump-all", "-o", "out", "--dump-unknown", "--no-offsets"], map, |opt| {
            assert_eq!(opt.output, Path::new("out"));
            assert!(opt.flags.dump_unknown);
            assert!(opt.flags.no_offsets);
        });
        assert_eq!(error(["script", "dump-all"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_script_disassemble() {
        use script::*;
        let map = mapper!(Command::Script(Subcommand::Disassemble(opt)) => opt);
        parse(["script", "disassemble", "foo", "-o", "out"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.output, Path::new("out"));
        });
    }

    #[test]
    fn test_cli_script_disassemble_all() {
        use script::*;
        let map = mapper!(Command::Script(Subcommand::DisassembleAll(opt)) => opt);
        parse(["script", "disassemble-all", "-o", "out"], map, |opt| {
            assert_eq!(opt.output, Path::new("out"));
        });
    }

    #[test]
    fn test_cli_script_assemble() {
        use script::*;
        let map = mapper!(Command::Script(Subcommand::Assemble(opt)) => opt);
        parse(["script", "assemble", "foo"], map, |opt| {
            assert_eq!(opt.path, Path::new("foo"));
        });
    }

    #[test]
    fn test_cli_shop_export() {
        use shop::*;
        let map = mapper!(Command::Shop(Subcommand::Export(opt)) => opt);
        parse(["shop", "export"], map, |opt| {
            assert!(!opt.compact);
            assert_eq!(opt.output, None);
        });
        parse(["shop", "export", "-o", "out"], map, |opt| {
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
        multiparse([["shop", "export", "-c"], ["shop", "export", "--compact"]], map, |opt| {
            assert!(opt.compact);
        });
    }

    #[test]
    fn test_cli_shop_import() {
        use shop::*;
        let map = mapper!(Command::Shop(Subcommand::Import(opt)) => opt);
        parse(["shop", "import", "foo"], map, |opt| {
            assert_eq!(opt.input, Path::new("foo"));
        });
        assert_eq!(error(["shop", "import"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_stage_export() {
        use stage::*;
        let map = mapper!(Command::Stage(Subcommand::Export(opt)) => opt);
        parse(["stage", "export", "foo"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.output, None);
        });
        parse(["stage", "export", "foo", "-o", "out"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.output.as_deref(), Some(Path::new("out")));
        });
        assert_eq!(error(["stage", "export"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_stage_export_all() {
        use stage::*;
        let map = mapper!(Command::Stage(Subcommand::ExportAll(opt)) => opt);
        parse(["stage", "export-all", "-o", "out"], map, |opt| {
            assert_eq!(opt.output, Path::new("out"));
        });
        assert_eq!(error(["stage", "export-all"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_stage_import() {
        use stage::*;
        let map = mapper!(Command::Stage(Subcommand::Import(opt)) => opt);
        parse(["stage", "import", "foo", "bar"], map, |opt| {
            assert_eq!(opt.stage, "foo");
            assert_eq!(opt.input, Path::new("bar"));
        });
        assert_eq!(error(["stage", "import"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(error(["stage", "import", "foo"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn test_cli_stage_import_all() {
        use stage::*;
        let map = mapper!(Command::Stage(Subcommand::ImportAll(opt)) => opt);
        parse(["stage", "import-all", "foo"], map, |opt| {
            assert_eq!(opt.input, Path::new("foo"));
            assert!(!opt.force);
        });
        multiparse(
            [["stage", "import-all", "foo", "-f"], ["stage", "import-all", "foo", "--force"]],
            map,
            |opt| {
                assert_eq!(opt.input, Path::new("foo"));
                assert!(opt.force);
            },
        );
        assert_eq!(error(["stage", "import-all"]), ErrorKind::MissingRequiredArgument);
    }

    #[cfg(feature = "debug")]
    #[test]
    fn test_debug() {
        use debug::*;
        let map = mapper!(Command::Debug(c) => c);
        parse(["debug", "rebuild-scripts"], map, |c| {
            assert!(matches!(c, Subcommand::RebuildScripts));
        });
    }
}

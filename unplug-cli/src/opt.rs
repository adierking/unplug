#![allow(trivial_numeric_casts)]

use std::path::PathBuf;
use structopt::clap::ArgGroup;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "Unplug")]
#[structopt(about = "Chibi-Robo! Plug Into Adventure! Modding Toolkit")]
pub struct Opt {
    /// Enables debug logging
    ///
    /// Use -vv in debug builds to enable trace logging
    #[structopt(short, long, parse(from_occurrences), global(true))]
    pub verbose: u64,

    #[structopt(subcommand)]
    pub command: Subcommand,
}

#[derive(StructOpt)]
pub enum Subcommand {
    /// Lists files in a U8 archive (e.g. qp.bin)
    ListArchive(ListArchiveOpt),

    /// Lists files in an ISO
    ListIso(ListIsoOpt),

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
    ExportMetadata(ExportMetadataOpt),

    /// Imports global metadata from a JSON file
    ImportMetadata(ImportMetadataOpt),
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
    #[allow(dead_code)]
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
pub struct ExportMetadataOpt {
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
pub struct ImportMetadataOpt {
    #[structopt(flatten)]
    pub container: OptionalContainerOpt,

    #[structopt(flatten)]
    pub globals: GlobalsOpt,

    /// Path to the input JSON file
    #[structopt(value_name("PATH"))]
    pub input: PathBuf,
}

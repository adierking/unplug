use crate::common::find_stage_file;
use crate::context::Context;
use crate::io::OutputRedirect;
use crate::json::MaxIndentJsonFormatter;
use crate::opt::{
    StageCommand, StageExportAllOpt, StageExportOpt, StageImportAllOpt, StageImportOpt,
};
use crate::serde_list_wrapper;
use anyhow::{anyhow, bail, Error, Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::ser::Serializer;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::num::NonZeroI32;
use std::path::Path;
use unplug::data::stage::{Stage as StageId, StageDefinition, STAGES};
use unplug::data::Object;
use unplug::event::BlockId;
use unplug::stage::{ObjectFlags, ObjectPlacement, Stage};

/// Maximum JSON indentation
const MAX_INDENT: usize = 3;

/// Serialize/Deserialize implementation for Object
mod object {
    use super::*;
    use serde::de::Error;
    use serde::{Deserializer, Serializer};
    use unplug::data::object::ObjectDefinition;

    pub(super) fn serialize<S: Serializer>(
        object: &Object,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        ObjectDefinition::get(*object).name.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Object, D::Error> {
        let name = String::deserialize(deserializer)?;
        match ObjectDefinition::find(&name) {
            Some(obj) => Ok(obj.id),
            None => Err(D::Error::custom(format!("invalid object name: \"{}\"", name))),
        }
    }
}

/// Serialize/Deserialize implementation for ObjectFlags
mod object_flags {
    use super::*;
    use serde::{Deserializer, Serializer};
    use std::collections::HashSet;

    const FLAGS: &[(ObjectFlags, &str)] = &[
        (ObjectFlags::SPAWN, "spawn"),
        (ObjectFlags::OPAQUE, "opaque"),
        (ObjectFlags::BLASTTHRU, "blastthru"),
        (ObjectFlags::RADAR, "radar"),
        (ObjectFlags::INTANGIBLE, "intangible"),
        (ObjectFlags::INVISIBLE, "invisible"),
        (ObjectFlags::TOON, "toon"),
        (ObjectFlags::FLASH, "flash"),
        (ObjectFlags::UNLIT, "unlit"),
        (ObjectFlags::BOTCAM, "botcam"),
        (ObjectFlags::EXPLODE, "explode"),
        (ObjectFlags::PUSHTHRU, "pushthru"),
        (ObjectFlags::LOWPRI, "lowpri"),
        (ObjectFlags::REFLECT, "reflect"),
        (ObjectFlags::PUSHBLOCK, "pushblock"),
        (ObjectFlags::CULL, "cull"),
        (ObjectFlags::LIFT, "lift"),
        (ObjectFlags::CLIMB, "climb"),
        (ObjectFlags::CLAMBER, "clamber"),
        (ObjectFlags::LADDER, "ladder"),
        (ObjectFlags::ROPE, "rope"),
        (ObjectFlags::STAIRS, "stairs"),
        (ObjectFlags::FALL, "fall"),
        (ObjectFlags::GRAB, "grab"),
        (ObjectFlags::INTERACT, "interact"),
        (ObjectFlags::TOUCH, "touch"),
        (ObjectFlags::ATC, "atc"),
        (ObjectFlags::PROJECTILE, "projectile"),
        (ObjectFlags::UNK_28, "unk28"),
        (ObjectFlags::MIRROR, "mirror"),
        (ObjectFlags::UNK_30, "unk30"),
        (ObjectFlags::DISABLED, "disabled"),
    ];

    pub(super) fn serialize<S: Serializer>(
        flags: &ObjectFlags,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut names = vec![];
        for &(flag, name) in FLAGS {
            if flags.contains(flag) {
                names.push(name.to_owned());
            }
        }
        names.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ObjectFlags, D::Error> {
        let names = Vec::<String>::deserialize(deserializer)?.into_iter().collect::<HashSet<_>>();
        let mut flags = ObjectFlags::empty();
        for &(flag, name) in FLAGS {
            if names.contains(name) {
                flags.insert(flag);
            }
        }
        Ok(flags)
    }
}

/// Serialize/Deserialize implementation for `Option<BlockId>`
mod script {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        script: &Option<BlockId>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        script.map(|id| id.index()).serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<BlockId>, D::Error> {
        let val = Option::<usize>::deserialize(deserializer)?;
        Ok(val.map(|id| BlockId::new(id as u32)))
    }
}

/// Serializable 3D vector which stores integers.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Vec3I {
    x: i32,
    y: i32,
    z: i32,
}

impl Vec3I {
    fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

/// Serializable 3D vector which stores floats converted to/from hundredths.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
struct Vec3F {
    x: f64,
    y: f64,
    z: f64,
}

impl From<Vec3I> for Vec3F {
    fn from(v: Vec3I) -> Self {
        Self { x: f64::from(v.x) / 100.0, y: f64::from(v.y) / 100.0, z: f64::from(v.z) / 100.0 }
    }
}

impl From<Vec3F> for Vec3I {
    fn from(v: Vec3F) -> Self {
        Self {
            x: (v.x * 100.0).round() as _,
            y: (v.y * 100.0).round() as _,
            z: (v.z * 100.0).round() as _,
        }
    }
}

/// JSON structure for object placements.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObjectPlacementDef {
    #[serde(with = "object")]
    object: Object,
    position: Vec3F,
    rotation: Vec3I,
    scale: Vec3F,
    data: i32,
    spawn_flag: Option<NonZeroI32>,
    variant: i32,
    #[serde(with = "object_flags")]
    flags: ObjectFlags,
    #[serde(with = "script")]
    script: Option<BlockId>,
}

serde_list_wrapper!(ObjectPlacementWrapper, ObjectPlacementDef);

impl From<ObjectPlacement> for ObjectPlacementDef {
    fn from(obj: ObjectPlacement) -> Self {
        Self {
            object: obj.id,
            position: Vec3I::new(obj.x, obj.y, obj.z).into(),
            rotation: Vec3I::new(obj.rotate_x, obj.rotate_y, obj.rotate_z),
            scale: Vec3I::new(obj.scale_x, obj.scale_y, obj.scale_z).into(),
            data: obj.data,
            spawn_flag: obj.spawn_flag,
            variant: obj.variant,
            flags: obj.flags,
            script: obj.script,
        }
    }
}

impl From<ObjectPlacementDef> for ObjectPlacement {
    fn from(obj: ObjectPlacementDef) -> Self {
        let position = Vec3I::from(obj.position);
        let scale = Vec3I::from(obj.scale);
        Self {
            id: obj.object,
            x: position.x,
            y: position.y,
            z: position.z,
            rotate_x: obj.rotation.x,
            rotate_y: obj.rotation.y,
            rotate_z: obj.rotation.z,
            scale_x: scale.x,
            scale_y: scale.y,
            scale_z: scale.z,
            data: obj.data,
            spawn_flag: obj.spawn_flag,
            variant: obj.variant,
            flags: obj.flags,
            script: obj.script,
        }
    }
}

/// Root JSON structure.
#[derive(Serialize, Deserialize)]
struct StageDef {
    objects: Vec<ObjectPlacementWrapper>,
}

struct ImportedStage {
    objects: Vec<ObjectPlacement>,
}

fn read_stage(path: &Path) -> Result<ImportedStage> {
    let reader = BufReader::new(File::open(path)?);
    let root: StageDef = serde_json::from_reader(reader)?;
    Ok(ImportedStage { objects: ObjectPlacementWrapper::unwrap(root.objects)? })
}

fn write_stage(stage: Stage, out: impl Write) -> Result<()> {
    let root = StageDef { objects: ObjectPlacementWrapper::wrap(stage.objects) };
    let formatter = MaxIndentJsonFormatter::new(MAX_INDENT);
    let mut serializer = Serializer::with_formatter(out, formatter);
    root.serialize(&mut serializer)?;
    Ok(())
}

/// The `stage` CLI command.
pub fn command(ctx: Context, opt: StageCommand) -> Result<()> {
    match opt {
        StageCommand::Export(opt) => command_export(ctx, opt),
        StageCommand::ExportAll(opt) => command_export_all(ctx, opt),
        StageCommand::Import(opt) => command_import(ctx, opt),
        StageCommand::ImportAll(opt) => command_import_all(ctx, opt),
    }
}

/// The `stage export` CLI command.
fn command_export(ctx: Context, opt: StageExportOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;

    let out = BufWriter::new(OutputRedirect::new(opt.output.as_deref())?);
    let filename = opt
        .output
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .unwrap_or_else(|| "JSON".to_owned());
    info!("Exporting {}", filename);

    let libs = ctx.read_globals()?.read_libs()?;
    let file = find_stage_file(&mut ctx, &opt.stage)?;
    let stage = ctx.read_stage_file(&libs, &file)?;
    write_stage(stage, out)?;
    Ok(())
}

/// The `stage export-all` CLI command.
pub fn command_export_all(ctx: Context, opt: StageExportAllOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    fs::create_dir_all(&opt.output)?;
    let libs = ctx.read_globals()?.read_libs()?;
    for stage_def in STAGES {
        let filename = format!("{}.json", stage_def.name);
        info!("Exporting {}", filename);
        let stage = ctx.read_stage(&libs, stage_def.id)?;
        let path = opt.output.join(filename);
        let out = BufWriter::new(File::create(path)?);
        write_stage(stage, out)?;
    }
    Ok(())
}

fn is_script_modified(old: &[ObjectPlacement], new: &[ObjectPlacement]) -> bool {
    let num_objects = old.len().max(new.len());
    for i in 0..num_objects {
        if i < old.len() && i < new.len() {
            // Script differs between two objects
            if old[i].script != new[i].script {
                return true;
            }
        } else if i < old.len() {
            // Script in old but not new
            if old[i].script.is_some() {
                return true;
            }
        } else if new[i].script.is_some() {
            // Script in new but not old
            return true;
        }
    }
    false
}

fn script_modified_error() -> Error {
    anyhow!("Editing object scripts is not supported yet")
}

/// The `stage import` CLI command.
fn command_import(ctx: Context, opt: StageImportOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading input JSON");
    let imported = read_stage(&opt.input)?;

    let file = find_stage_file(&mut ctx, &opt.stage)?;
    let info = ctx.query_file(&file)?;
    info!("Rebuilding {}", info.name);
    let libs = ctx.read_globals()?.read_libs()?;
    let mut stage = ctx.read_stage_file(&libs, &file)?;

    // The issue with supporting script swapping is that it means block IDs would change after the
    // stage is written. This would make it impossible to import over an already-edited stage, which
    // is necessary to make it easy for modders to iterate on things. A potential solution is to
    // store the script data in the JSON somehow.
    if is_script_modified(&stage.objects, &imported.objects) {
        return Err(script_modified_error());
    }

    stage.objects = imported.objects;
    ctx.begin_update().write_stage_file(&file, &stage)?.commit()?;
    Ok(())
}

/// The `stage import-all` CLI command.
pub fn command_import_all(ctx: Context, opt: StageImportAllOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading input JSON");
    let mut jsons: Vec<(StageId, ImportedStage)> = vec![];
    for stage_def in STAGES {
        let filename = format!("{}.json", stage_def.name);
        debug!("Reading {}", filename);
        let path = opt.input.join(&filename);
        if opt.force || path.exists() {
            let imported = match read_stage(&path) {
                Ok(x) => x,
                Err(e) => bail!("Error reading {}: {:#}", filename, e),
            };
            jsons.push((stage_def.id, imported));
        }
    }
    if jsons.is_empty() {
        bail!("No JSON files were found");
    }

    info!("Checking stage data");
    let libs = ctx.read_globals()?.read_libs()?;
    let mut updated: Vec<(StageId, Box<Stage>)> = vec![];
    for (id, imported) in jsons {
        let stage_def = StageDefinition::get(id);
        let mut stage = ctx.read_stage(&libs, id)?;
        if !opt.force && stage.objects == imported.objects {
            continue;
        }
        if is_script_modified(&stage.objects, &imported.objects) {
            // See command_import()
            error!("{}.bin's script does not match", stage_def.name);
            return Err(script_modified_error());
        }
        stage.objects = imported.objects;
        updated.push((id, Box::from(stage)));
        info!("{}.bin will be imported", stage_def.name);
    }
    if updated.is_empty() {
        info!("No stages were changed");
        return Ok(());
    }

    info!("Updating game files");
    let mut update = ctx.begin_update();
    for (id, stage) in updated {
        update = update.write_stage(id, &*stage)?;
    }
    update.commit()?;
    Ok(())
}

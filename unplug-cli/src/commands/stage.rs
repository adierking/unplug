use crate::opt::stage::*;

use crate::common::find_stage_file;
use crate::context::Context;
use crate::io::OutputRedirect;
use crate::json::MaxIndentJsonFormatter;
use crate::serde_list_wrapper;
use anyhow::{bail, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::ser::Serializer;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::num::NonZeroI32;
use std::path::Path;
use unplug::data::{Item, Object, ObjectFlags, Resource, Stage as StageId};
use unplug::event::{BlockId, Command, Expr, Pointer, Script};
use unplug::stage::{ObjectPlacement, Stage};

/// Maximum JSON indentation
const MAX_INDENT: usize = 3;

/// lib() function for collecting an item
const LIB_COLLECT_ITEM: i16 = 77;

/// Serialize/Deserialize implementation for Object
mod object {
    use super::*;
    use serde::de::Error;
    use serde::{Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        object: &Object,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        object.name().serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Object, D::Error> {
        let name = String::deserialize(deserializer)?;
        match Object::find(&name) {
            Some(obj) => Ok(obj),
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
            script: None,
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
pub fn command(ctx: Context, opt: Subcommand) -> Result<()> {
    match opt {
        Subcommand::Export(opt) => command_export(ctx, opt),
        Subcommand::ExportAll(opt) => command_export_all(ctx, opt),
        Subcommand::Import(opt) => command_import(ctx, opt),
        Subcommand::ImportAll(opt) => command_import_all(ctx, opt),
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
    for id in StageId::iter() {
        let filename = format!("{}.json", id.name());
        info!("Exporting {}", filename);
        let stage = ctx.read_stage(&libs, id)?;
        let path = opt.output.join(filename);
        let out = BufWriter::new(File::create(path)?);
        write_stage(stage, out)?;
    }
    Ok(())
}

/// Patches an item script to change the item ID and returns true if a patch was made.
fn patch_item_script(
    script: &mut Script,
    entry_point: BlockId,
    item: Item,
    visited: &mut HashSet<BlockId>,
) -> bool {
    if !visited.insert(entry_point) {
        return false;
    }

    // Recursively scan for any calls to the "collect item" function and patch the item argument
    let code = script.block_mut(entry_point).code_mut().unwrap();
    let mut success = false;
    for i in 1..code.commands.len() {
        if code.commands[i] == Command::Lib(LIB_COLLECT_ITEM) {
            if let Command::SetSp(_) = code.commands[i - 1] {
                code.commands[i - 1] = Command::SetSp(Expr::Imm16(item.into()).into());
                success = true;
            }
        }
    }

    let (next_block, else_block) = (code.next_block, code.else_block);
    if let Some(Pointer::Block(next_block)) = next_block {
        success = patch_item_script(script, next_block, item, visited) || success;
    }
    if let Some(Pointer::Block(else_block)) = else_block {
        success = patch_item_script(script, else_block, item, visited) || success;
    }
    success
}

/// Compares object tables and autopatches scripts to match changes.
fn patch_scripts(
    script: &mut Script,
    old_objects: &[ObjectPlacement],
    new_objects: &mut [ObjectPlacement],
) {
    for (i, (old, new)) in old_objects.iter().zip(new_objects).enumerate() {
        new.script = old.script;
        let Some(obj_script) = old.script else { continue };
        let (old_item, new_item) = match (Item::try_from(old.id), Item::try_from(new.id)) {
            (Ok(o), Ok(n)) => (o, n),
            _ => continue,
        };
        if new_item != old_item {
            if patch_item_script(script, obj_script, new_item, &mut HashSet::new()) {
                info!("Patched item script for object {} ({:?})", i, new.id);
            } else {
                warn!(
                    "Item script for object {} ({:?}) is nonstandard and could not be patched",
                    i, new.id
                );
            }
        }
    }
}

/// The `stage import` CLI command.
fn command_import(ctx: Context, opt: StageImportOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading input JSON");
    let mut imported = read_stage(&opt.input)?;

    let file = find_stage_file(&mut ctx, &opt.stage)?;
    let info = ctx.query_file(&file)?;
    info!("Patching {}", info.name);
    let libs = ctx.read_globals()?.read_libs()?;
    let mut stage = ctx.read_stage_file(&libs, &file)?;

    patch_scripts(&mut stage.script, &stage.objects, &mut imported.objects);
    stage.objects = imported.objects;
    ctx.begin_update().write_stage_file(&file, &stage)?.commit()?;
    Ok(())
}

/// The `stage import-all` CLI command.
pub fn command_import_all(ctx: Context, opt: StageImportAllOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading input JSON");
    let mut jsons: Vec<(StageId, ImportedStage)> = vec![];
    for id in StageId::iter() {
        let filename = format!("{}.json", id.name());
        debug!("Reading {}", filename);
        let path = opt.input.join(&filename);
        if opt.force || path.exists() {
            let imported = match read_stage(&path) {
                Ok(x) => x,
                Err(e) => bail!("Error reading {}: {:#}", filename, e),
            };
            jsons.push((id, imported));
        }
    }
    if jsons.is_empty() {
        bail!("No JSON files were found");
    }

    info!("Checking stage data");
    let libs = ctx.read_globals()?.read_libs()?;
    let mut updated: Vec<(StageId, Box<Stage>)> = vec![];
    for (id, mut imported) in jsons {
        let mut stage = ctx.read_stage(&libs, id)?;
        if !opt.force && stage.objects == imported.objects {
            continue;
        }

        info!("Patching {}", id.file_name());
        patch_scripts(&mut stage.script, &stage.objects, &mut imported.objects);
        stage.objects = imported.objects;
        updated.push((id, Box::from(stage)));
    }
    if updated.is_empty() {
        info!("No stages were changed");
        return Ok(());
    }

    info!("Updating game files");
    let mut update = ctx.begin_update();
    for (id, stage) in updated {
        update = update.write_stage(id, &stage)?;
    }
    update.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use unplug::event::command::IfArgs;
    use unplug::event::{Block, CodeBlock};

    #[test]
    fn test_patch_item_script_basic() {
        let blocks = vec![Block::Code(CodeBlock {
            commands: vec![
                Command::PushBp,
                Command::SetSp(Expr::Imm16(Item::DogBone.into()).into()),
                Command::Lib(77),
                Command::PopBp,
                Command::Return,
            ],
            next_block: None,
            else_block: None,
        })];

        let mut script = Script::with_blocks(blocks);
        assert!(patch_item_script(
            &mut script,
            BlockId::new(0),
            Item::Wastepaper,
            &mut HashSet::new()
        ));

        let expected = vec![
            Command::PushBp,
            Command::SetSp(Expr::Imm16(Item::Wastepaper.into()).into()),
            Command::Lib(77),
            Command::PopBp,
            Command::Return,
        ];
        let block = script.block(BlockId::new(0)).code().unwrap();
        assert_eq!(block.commands, expected);
    }

    #[test]
    fn test_patch_item_script_complex() {
        // This script is nonsense, but the test effectively checks a couple different scenarios:
        // 1. The item collection call appears more than once and both are patched
        // 2. The patcher can handle loops in a script
        let blocks = vec![
            // 0
            Block::Code(CodeBlock {
                commands: vec![Command::While(Box::from(IfArgs {
                    condition: Expr::Imm16(1),
                    else_target: BlockId::new(5).into(),
                }))],
                next_block: Some(BlockId::new(1).into()),
                else_block: Some(BlockId::new(5).into()),
            }),
            // 1
            Block::Code(CodeBlock {
                commands: vec![Command::If(Box::from(IfArgs {
                    condition: Expr::Imm16(1),
                    else_target: BlockId::new(3).into(),
                }))],
                next_block: Some(BlockId::new(2).into()),
                else_block: Some(BlockId::new(3).into()),
            }),
            // 2
            Block::Code(CodeBlock {
                commands: vec![
                    Command::PushBp,
                    Command::SetSp(Expr::Imm16(Item::DogBone.into()).into()),
                    Command::Lib(77),
                    Command::PopBp,
                    Command::EndIf(BlockId::new(4).into()),
                ],
                next_block: Some(BlockId::new(4).into()),
                else_block: None,
            }),
            // 3
            Block::Code(CodeBlock {
                commands: vec![
                    Command::PushBp,
                    Command::SetSp(Expr::Imm16(Item::DogBone.into()).into()),
                    Command::Lib(77),
                    Command::PopBp,
                ],
                next_block: Some(BlockId::new(4).into()),
                else_block: None,
            }),
            // 4
            Block::Code(CodeBlock {
                commands: vec![Command::Goto(BlockId::new(0).into())],
                next_block: Some(BlockId::new(0).into()),
                else_block: None,
            }),
            // 5
            Block::Code(CodeBlock {
                commands: vec![Command::Return],
                next_block: None,
                else_block: None,
            }),
        ];

        let mut script = Script::with_blocks(blocks.clone());
        assert!(patch_item_script(
            &mut script,
            BlockId::new(0),
            Item::Wastepaper,
            &mut HashSet::new()
        ));

        assert_eq!(*script.block(BlockId::new(0)), blocks[0]);
        assert_eq!(*script.block(BlockId::new(1)), blocks[1]);
        assert_eq!(*script.block(BlockId::new(4)), blocks[4]);
        assert_eq!(*script.block(BlockId::new(5)), blocks[5]);

        let expected2 = vec![
            Command::PushBp,
            Command::SetSp(Expr::Imm16(Item::Wastepaper.into()).into()),
            Command::Lib(77),
            Command::PopBp,
            Command::EndIf(BlockId::new(4).into()),
        ];
        let block2 = script.block(BlockId::new(2)).code().unwrap();
        assert_eq!(block2.commands, expected2);

        let expected3 = vec![
            Command::PushBp,
            Command::SetSp(Expr::Imm16(Item::Wastepaper.into()).into()),
            Command::Lib(77),
            Command::PopBp,
        ];
        let block3 = script.block(BlockId::new(3)).code().unwrap();
        assert_eq!(block3.commands, expected3);
    }

    #[test]
    fn test_patch_item_script_nonstandard() {
        let blocks = vec![Block::Code(CodeBlock {
            commands: vec![Command::Return],
            next_block: None,
            else_block: None,
        })];

        let mut script = Script::with_blocks(blocks.clone());
        assert!(!patch_item_script(
            &mut script,
            BlockId::new(0),
            Item::Wastepaper,
            &mut HashSet::new()
        ));

        assert_eq!(*script.block(BlockId::new(0)), blocks[0]);
    }
}

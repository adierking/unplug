use crate::opt::globals::*;

use crate::context::Context;
use crate::io::OutputRedirect;
use crate::serde_list_wrapper;
use anyhow::{bail, Result};
use log::info;
use serde::de::Error;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use unplug::common::CText;
use unplug::data::{Object, Resource, Sound};
use unplug::globals::metadata::*;
use unplug::globals::GlobalsBuilder;

/// Serialize/Deserialize implementation for CText
mod text {
    use serde::de::{self, Deserialize, Deserializer};
    use serde::ser::{self, Serializer};
    use unplug::common::CText;

    pub(super) fn serialize<S: Serializer>(text: &CText, serializer: S) -> Result<S::Ok, S::Error> {
        let decoded = text.decode().map_err(ser::Error::custom)?;
        serializer.serialize_str(&decoded)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<CText, D::Error> {
        let string = String::deserialize(deserializer)?;
        CText::encode(&string).map_err(de::Error::custom)
    }
}

/// Serialize/Deserialize implementation for ItemFlags
mod item_flags {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use unplug::globals::metadata::ItemFlags;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ItemFlagsDef {
        junk: bool,
        chibi_vision: bool,
        inventory: bool,
    }

    pub(super) fn serialize<S: Serializer>(
        flags: &ItemFlags,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        ItemFlagsDef {
            junk: flags.contains(ItemFlags::JUNK),
            chibi_vision: flags.contains(ItemFlags::CHIBI_VISION),
            inventory: flags.contains(ItemFlags::INVENTORY),
        }
        .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ItemFlags, D::Error> {
        let def = ItemFlagsDef::deserialize(deserializer)?;
        let mut flags = ItemFlags::empty();
        flags.set(ItemFlags::JUNK, def.junk);
        flags.set(ItemFlags::CHIBI_VISION, def.chibi_vision);
        flags.set(ItemFlags::INVENTORY, def.inventory);
        Ok(flags)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "BatteryGlobals", rename_all = "camelCase")]
struct BatteryGlobalsDef {
    idle: i32,
    idle_anim: i32,
    walk: i32,
    jog: i32,
    run: i32,
    slide: i32,
    equip: i32,
    lift: i32,
    drop: i32,
    leticker: i32,
    ledge_grab: i32,
    ledge_slide: i32,
    ledge_climb: i32,
    ledge_drop: i32,
    ledge_teeter: i32,
    jump: i32,
    fall: i32,
    ladder_grab: i32,
    ladder_ascend: i32,
    ladder_descend: i32,
    ladder_top: i32,
    ladder_bottom: i32,
    rope_grab: i32,
    rope_ascend: i32,
    rope_descend: i32,
    rope_top: i32,
    rope_bottom: i32,
    push: i32,
    copter_hover: i32,
    copter_descend: i32,
    popper_shoot: i32,
    popper_shoot_charged: i32,
    radar_scan: i32,
    radar_follow: i32,
    brush: i32,
    spoon: i32,
    mug: i32,
    squirter_suck: i32,
    squirter_spray: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PopperGlobals", rename_all = "camelCase")]
struct PopperGlobalsDef {
    range_default: i32,
    range_upgraded: i32,
    projectile_speed: i32,
    max_projectiles: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CopterGlobals", rename_all = "camelCase")]
struct CopterGlobalsDef {
    hover_duration: i32,
    gravity: i32,
    terminal_velocity: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "RadarGlobals", rename_all = "camelCase")]
struct RadarGlobalsDef {
    red_range: i32,
    yellow_range: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlayerGlobals", rename_all = "camelCase")]
struct PlayerGlobalsDef {
    climb_duration: i32,
    climb_rate: i32,
    gentle_climb_percent: i32,
    auto_plug_pickup_time: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "DefaultAtcs", rename_all = "camelCase")]
struct DefaultAtcsDef {
    copter: bool,
    popper: bool,
    radar: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CoinValues", rename_all = "camelCase")]
struct CoinValuesDef {
    coin_g: u32,
    coin_s: u32,
    coin_c: u32,
    junk_a: u32,
    junk_b: u32,
    junk_c: u32,
    junk_unko: u32,
    energyb: u32,
    happy_heart: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Item", rename_all = "camelCase")]
struct ItemDef {
    #[serde(with = "text")]
    name: CText,
    #[serde(with = "text")]
    description: CText,
    #[serde(with = "item_flags")]
    flags: ItemFlags,
    pickup_delay: i16,
    price: i16,
    junk_exp: i16,
    junk_money: i16,
    pickup_sound: i8,
    collect_sound: i8,
}

serde_list_wrapper!(ItemWrapper, Item, "ItemDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Actor", rename_all = "camelCase")]
struct ActorDef {
    #[serde(with = "text")]
    name: CText,
}

serde_list_wrapper!(ActorWrapper, Actor, "ActorDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Atc", rename_all = "camelCase")]
struct AtcDef {
    #[serde(with = "text")]
    name: CText,
    #[serde(with = "text")]
    description: CText,
    price: i16,
}

serde_list_wrapper!(AtcWrapper, Atc, "AtcDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Suit", rename_all = "camelCase")]
struct SuitDef {
    #[serde(with = "text")]
    name: CText,
}

serde_list_wrapper!(SuitWrapper, Suit, "SuitDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Stage", rename_all = "camelCase")]
struct StageDef {
    #[serde(with = "text")]
    name: CText,
    #[serde(with = "text")]
    description: CText,
}

serde_list_wrapper!(StageWrapper, Stage, "StageDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Leticker", rename_all = "camelCase")]
struct LetickerDef {
    #[serde(with = "text")]
    name: CText,
    #[serde(with = "text")]
    description: CText,
    price: i16,
}

serde_list_wrapper!(LetickerWrapper, Leticker, "LetickerDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Sticker", rename_all = "camelCase")]
struct StickerDef {
    #[serde(with = "text")]
    name: CText,
    #[serde(with = "text")]
    description: CText,
    flag_index: u32,
}

serde_list_wrapper!(StickerWrapper, Sticker, "StickerDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Stat", rename_all = "camelCase")]
struct StatDef {
    #[serde(with = "text")]
    name: CText,
    #[serde(with = "text")]
    description: CText,
}

serde_list_wrapper!(StatWrapper, Stat, "StatDef");

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct SoundDef(Sound);

impl Serialize for SoundDef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.0 {
            Sound::None => serializer.serialize_none(),
            Sound::Music(music) => serializer.serialize_str(music.name()),
            Sound::Sfx(sfx) => serializer.serialize_str(sfx.name()),
        }
    }
}

impl<'de> Deserialize<'de> for SoundDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name = match Option::<String>::deserialize(deserializer)? {
            Some(name) => name,
            None => return Ok(SoundDef(Sound::None)),
        };
        Sound::find(&name)
            .map(SoundDef)
            .ok_or_else(|| D::Error::custom(format!("invalid sound name: \"{}\"", name)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataDef {
    #[serde(with = "BatteryGlobalsDef")]
    battery_globals: BatteryGlobals,
    #[serde(with = "PopperGlobalsDef")]
    popper_globals: PopperGlobals,
    #[serde(with = "CopterGlobalsDef")]
    copter_globals: CopterGlobals,
    #[serde(with = "RadarGlobalsDef")]
    radar_globals: RadarGlobals,
    #[serde(with = "PlayerGlobalsDef")]
    player_globals: PlayerGlobals,
    #[serde(with = "DefaultAtcsDef")]
    default_atcs: DefaultAtcs,
    #[serde(with = "CoinValuesDef")]
    coin_values: CoinValues,
    pickup_sounds: Vec<SoundDef>,
    collect_sounds: Vec<SoundDef>,
    items: Vec<ItemWrapper>,
    actors: Vec<ActorWrapper>,
    atcs: Vec<AtcWrapper>,
    suits: Vec<SuitWrapper>,
    stages: Vec<StageWrapper>,
    letickers: Vec<LetickerWrapper>,
    stickers: Vec<StickerWrapper>,
    stats: Vec<StatWrapper>,
}

impl From<Metadata> for MetadataDef {
    fn from(metadata: Metadata) -> Self {
        Self {
            battery_globals: metadata.battery_globals,
            popper_globals: metadata.popper_globals,
            copter_globals: metadata.copter_globals,
            radar_globals: metadata.radar_globals,
            player_globals: metadata.player_globals,
            default_atcs: metadata.default_atcs,
            coin_values: metadata.coin_values,
            pickup_sounds: metadata.pickup_sounds.into_iter().map(SoundDef).collect(),
            collect_sounds: metadata.collect_sounds.into_iter().map(SoundDef).collect(),
            items: ItemWrapper::wrap(Vec::from(metadata.items)),
            actors: ActorWrapper::wrap(Vec::from(metadata.actors)),
            atcs: AtcWrapper::wrap(Vec::from(metadata.atcs)),
            suits: SuitWrapper::wrap(Vec::from(metadata.suits)),
            stages: StageWrapper::wrap(Vec::from(metadata.stages)),
            letickers: LetickerWrapper::wrap(Vec::from(metadata.letickers)),
            stickers: StickerWrapper::wrap(Vec::from(metadata.stickers)),
            stats: StatWrapper::wrap(Vec::from(metadata.stats)),
        }
    }
}

/// The `globals export` CLI command.
pub fn command_export(ctx: Context, args: ExportArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(args.output)?);

    info!("Dumping global metadata");
    let metadata = ctx.read_globals()?.read_metadata()?;
    let root = MetadataDef::from(metadata);
    if args.compact {
        serde_json::to_writer(out, &root)?;
    } else {
        serde_json::to_writer_pretty(out, &root)?;
    }
    Ok(())
}

/// The `globals import` CLI command.
pub fn command_import(ctx: Context, args: ImportArgs) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    info!("Reading input JSON");
    let json = BufReader::new(File::open(args.input)?);
    let root: MetadataDef = serde_json::from_reader(json)?;

    info!("Reading global metadata");
    let mut globals = ctx.read_globals()?;
    let mut metadata = globals.read_metadata()?;

    info!("Rebuilding globals.bin");
    metadata.battery_globals = root.battery_globals;
    metadata.popper_globals = root.popper_globals;
    metadata.copter_globals = root.copter_globals;
    metadata.radar_globals = root.radar_globals;
    metadata.player_globals = root.player_globals;
    metadata.default_atcs = root.default_atcs;
    metadata.coin_values = root.coin_values;

    if root.pickup_sounds.len() != metadata.pickup_sounds.len() {
        bail!("expected exactly {} pickupSounds", metadata.pickup_sounds.len());
    }
    if root.collect_sounds.len() != metadata.collect_sounds.len() {
        bail!("expected exactly {} collectSounds", metadata.collect_sounds.len());
    }
    let pickup_sounds = root.pickup_sounds.into_iter().map(|s| s.0).collect::<Vec<_>>();
    let collect_sounds = root.collect_sounds.into_iter().map(|s| s.0).collect::<Vec<_>>();
    metadata.pickup_sounds.copy_from_slice(&pickup_sounds);
    metadata.collect_sounds.copy_from_slice(&collect_sounds);

    ItemWrapper::unwrap_into(root.items, &mut metadata.items)?;
    ActorWrapper::unwrap_into(root.actors, &mut metadata.actors)?;
    AtcWrapper::unwrap_into(root.atcs, &mut metadata.atcs)?;
    SuitWrapper::unwrap_into(root.suits, &mut metadata.suits)?;
    StageWrapper::unwrap_into(root.stages, &mut metadata.stages)?;
    LetickerWrapper::unwrap_into(root.letickers, &mut metadata.letickers)?;
    StickerWrapper::unwrap_into(root.stickers, &mut metadata.stickers)?;
    StatWrapper::unwrap_into(root.stats, &mut metadata.stats)?;
    ctx.begin_update()
        .write_globals(GlobalsBuilder::new().base(&mut globals).metadata(&metadata))?
        .commit()?;
    Ok(())
}

/// The `globals dump-colliders` CLI command.
fn command_dump_colliders(ctx: Context, args: DumpCollidersArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut out = BufWriter::new(OutputRedirect::new(args.output)?);
    info!("Dumping collider globals");
    let colliders = ctx.read_globals()?.read_colliders()?;
    for (obj, list) in colliders.objects.iter().enumerate() {
        writeln!(out, "Object {:?} ({}):", Object::try_from(obj as i32)?, obj)?;
        for (i, collider) in list.iter().enumerate() {
            writeln!(out, "{:>2} {:?}", i, collider)?;
        }
        writeln!(out)?;
    }
    Ok(())
}

/// The `globals` CLI command.
pub fn command(ctx: Context, command: Subcommand) -> Result<()> {
    match command {
        Subcommand::Export(args) => command_export(ctx, args),
        Subcommand::Import(args) => command_import(ctx, args),
        Subcommand::DumpColliders(args) => command_dump_colliders(ctx, args),
    }
}

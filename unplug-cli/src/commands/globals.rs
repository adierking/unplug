use crate::context::Context;
use crate::io::OutputRedirect;
use crate::opt::{GlobalsCommand, GlobalsDumpCollidersOpt, GlobalsExportOpt, GlobalsImportOpt};
use anyhow::{bail, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use unplug::common::Text;
use unplug::data::Object;
use unplug::globals::metadata::*;
use unplug::globals::GlobalsBuilder;

/// Generates a serializable wrapper type for list elements.
macro_rules! wrapper {
    ($wrapper:ident, $wrapped:ty, $def:literal) => {
        #[derive(Serialize, Deserialize)]
        struct $wrapper {
            id: usize,
            #[serde(flatten, with = $def)]
            inner: $wrapped,
        }

        impl $wrapper {
            fn wrap_boxed_slice(s: Box<[$wrapped]>) -> Vec<Self> {
                Vec::from(s).into_iter().enumerate().map(|(id, inner)| Self { id, inner }).collect()
            }

            fn update_metadata(wrappers: Vec<Self>, metadata: &mut [$wrapped]) -> Result<()> {
                for wrapper in wrappers {
                    if wrapper.id >= metadata.len() {
                        bail!("invalid {} ID: {}", stringify!($wrapped), wrapper.id);
                    }
                    metadata[wrapper.id] = wrapper.inner;
                }
                Ok(())
            }
        }
    };
}

/// Serialize/Deserialize implementation for Text
mod text {
    use serde::de::{self, Deserialize, Deserializer};
    use serde::ser::{self, Serializer};
    use unplug::common::Text;

    pub(super) fn serialize<S: Serializer>(text: &Text, serializer: S) -> Result<S::Ok, S::Error> {
        let decoded = text.decode().map_err(ser::Error::custom)?;
        serializer.serialize_str(&decoded)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Text, D::Error> {
        let string = String::deserialize(deserializer)?;
        Text::encode(&string).map_err(de::Error::custom)
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
    name: Text,
    #[serde(with = "text")]
    description: Text,
    #[serde(with = "item_flags")]
    flags: ItemFlags,
    pickup_delay: i16,
    price: i16,
    junk_exp: i16,
    junk_money: i16,
    pickup_sound: i8,
    collect_sound: i8,
}

wrapper!(ItemWrapper, Item, "ItemDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Actor", rename_all = "camelCase")]
struct ActorDef {
    #[serde(with = "text")]
    name: Text,
}

wrapper!(ActorWrapper, Actor, "ActorDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Atc", rename_all = "camelCase")]
struct AtcDef {
    #[serde(with = "text")]
    name: Text,
    #[serde(with = "text")]
    description: Text,
    price: i16,
}

wrapper!(AtcWrapper, Atc, "AtcDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Suit", rename_all = "camelCase")]
struct SuitDef {
    #[serde(with = "text")]
    name: Text,
}

wrapper!(SuitWrapper, Suit, "SuitDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Stage", rename_all = "camelCase")]
struct StageDef {
    #[serde(with = "text")]
    name: Text,
    #[serde(with = "text")]
    description: Text,
}

wrapper!(StageWrapper, Stage, "StageDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Leticker", rename_all = "camelCase")]
struct LetickerDef {
    #[serde(with = "text")]
    name: Text,
    #[serde(with = "text")]
    description: Text,
    price: i16,
}

wrapper!(LetickerWrapper, Leticker, "LetickerDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Sticker", rename_all = "camelCase")]
struct StickerDef {
    #[serde(with = "text")]
    name: Text,
    #[serde(with = "text")]
    description: Text,
    flag_index: u32,
}

wrapper!(StickerWrapper, Sticker, "StickerDef");

#[derive(Serialize, Deserialize)]
#[serde(remote = "Stat", rename_all = "camelCase")]
struct StatDef {
    #[serde(with = "text")]
    name: Text,
    #[serde(with = "text")]
    description: Text,
}

wrapper!(StatWrapper, Stat, "StatDef");

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
    pickup_sounds: Vec<u32>,
    collect_sounds: Vec<u32>,
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
            pickup_sounds: metadata.pickup_sounds.to_vec(),
            collect_sounds: metadata.collect_sounds.to_vec(),
            items: ItemWrapper::wrap_boxed_slice(metadata.items),
            actors: ActorWrapper::wrap_boxed_slice(metadata.actors),
            atcs: AtcWrapper::wrap_boxed_slice(metadata.atcs),
            suits: SuitWrapper::wrap_boxed_slice(metadata.suits),
            stages: StageWrapper::wrap_boxed_slice(metadata.stages),
            letickers: LetickerWrapper::wrap_boxed_slice(metadata.letickers),
            stickers: StickerWrapper::wrap_boxed_slice(metadata.stickers),
            stats: StatWrapper::wrap_boxed_slice(metadata.stats),
        }
    }
}

/// The `globals export` CLI command.
pub fn command_export(ctx: Context, opt: GlobalsExportOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    info!("Dumping global metadata");
    let metadata = ctx.read_globals()?.read_metadata()?;
    let root = MetadataDef::from(metadata);
    if opt.compact {
        serde_json::to_writer(out, &root)?;
    } else {
        serde_json::to_writer_pretty(out, &root)?;
    }
    Ok(())
}

/// The `globals import` CLI command.
pub fn command_import(ctx: Context, opt: GlobalsImportOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    info!("Reading input JSON");
    let json = BufReader::new(File::open(opt.input)?);
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
    metadata.pickup_sounds.copy_from_slice(&root.pickup_sounds);
    metadata.collect_sounds.copy_from_slice(&root.collect_sounds);
    ItemWrapper::update_metadata(root.items, &mut metadata.items)?;
    ActorWrapper::update_metadata(root.actors, &mut metadata.actors)?;
    AtcWrapper::update_metadata(root.atcs, &mut metadata.atcs)?;
    SuitWrapper::update_metadata(root.suits, &mut metadata.suits)?;
    StageWrapper::update_metadata(root.stages, &mut metadata.stages)?;
    LetickerWrapper::update_metadata(root.letickers, &mut metadata.letickers)?;
    StickerWrapper::update_metadata(root.stickers, &mut metadata.stickers)?;
    StatWrapper::update_metadata(root.stats, &mut metadata.stats)?;
    ctx.begin_update()
        .write_globals(GlobalsBuilder::new().base(&mut globals).metadata(&metadata))?
        .commit()?;
    Ok(())
}

/// The `globals dump-colliders` CLI command.
fn command_dump_colliders(ctx: Context, opt: GlobalsDumpCollidersOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut out = BufWriter::new(OutputRedirect::new(opt.output)?);
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
pub fn command(ctx: Context, opt: GlobalsCommand) -> Result<()> {
    match opt {
        GlobalsCommand::Export(opt) => command_export(ctx, opt),
        GlobalsCommand::Import(opt) => command_import(ctx, opt),
        GlobalsCommand::DumpColliders(opt) => command_dump_colliders(ctx, opt),
    }
}

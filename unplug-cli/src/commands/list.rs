use crate::args::list::*;

use crate::context::Context;
use anyhow::Result;
use unicase::Ascii;
use unplug::data::{Atc, Item, ItemFlags, Music, Object, Resource, Sfx, Stage};

const UNKNOWN_ID_PREFIX: &str = "unk";

fn sort_ids<I: Copy + Ord>(ids: &mut [(I, &str)], settings: &IdArgs) {
    if settings.by_id {
        ids.sort_unstable_by_key(|i| i.0);
    } else {
        ids.sort_unstable_by(|a, b| Ascii::new(a.1).cmp(&Ascii::new(b.1)));
    }
    if settings.reverse {
        ids.reverse();
    }
}

/// The `list items` CLI command.
fn command_items(args: ItemsArgs) -> Result<()> {
    let mut items: Vec<_> = if args.show_unknown {
        Item::iter().map(|i| (i, i.name())).collect()
    } else {
        Item::iter()
            .filter(|i| !i.flags().contains(ItemFlags::UNUSED))
            .map(|i| (i, i.name()))
            .collect()
    };
    sort_ids(&mut items, &args.settings);
    for (id, name) in items {
        println!("[{:>3}] {}", i16::from(id), name);
    }
    Ok(())
}

/// The `list equipment` CLI command.
fn command_equipment(args: EquipmentArgs) -> Result<()> {
    let mut atcs: Vec<_> = Atc::iter().map(|a| (a, a.name())).collect();
    sort_ids(&mut atcs, &args.settings);
    for (id, name) in atcs {
        if args.show_unknown || !name.starts_with(UNKNOWN_ID_PREFIX) {
            println!("[{:>1}] {}", i16::from(id), name);
        }
    }
    Ok(())
}

/// The `list stages` CLI command.
fn command_stages(args: IdArgs) -> Result<()> {
    let mut stages: Vec<_> = Stage::iter().map(|s| (s, s.title())).collect();
    sort_ids(&mut stages, &args);
    for (id, title) in stages {
        println!("[{:>3}] {}", i32::from(id), title);
    }
    Ok(())
}

/// The `list objects` CLI command.
fn command_objects(args: IdArgs) -> Result<()> {
    let mut objects: Vec<_> = Object::iter().map(|o| (o, o.name())).collect();
    sort_ids(&mut objects, &args);
    for (id, name) in objects {
        println!("[{:>5}] {}", i32::from(id), name);
    }
    Ok(())
}

/// The `list music` CLI command.
fn command_music(args: IdArgs) -> Result<()> {
    let mut music: Vec<_> = Music::iter().map(|m| (m, m.name())).collect();
    sort_ids(&mut music, &args);
    for (id, name) in music {
        println!("[{:>3}] {}", u8::from(id), name);
    }
    Ok(())
}

/// The `list sounds` CLI command.
fn command_sounds(args: IdArgs) -> Result<()> {
    let mut sfx: Vec<_> = Sfx::iter().map(|s| (s, s.name())).collect();
    sort_ids(&mut sfx, &args);
    for (id, name) in sfx {
        println!("[{:>08x}] {}", u32::from(id), name);
    }
    Ok(())
}

/// The `list` CLI command.
pub fn command(_ctx: Context, command: Subcommand) -> Result<()> {
    match command {
        Subcommand::Items(args) => command_items(args),
        Subcommand::Equipment(args) => command_equipment(args),
        Subcommand::Stages(args) => command_stages(args),
        Subcommand::Objects(args) => command_objects(args),
        Subcommand::Music(args) => command_music(args),
        Subcommand::Sounds(args) => command_sounds(args),
    }
}

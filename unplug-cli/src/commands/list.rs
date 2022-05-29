use crate::context::Context;
use crate::id::IdString;
use crate::opt::{ListCommand, ListEquipmentOpt, ListIdsOpt, ListItemsOpt, ListStagesOpt};
use anyhow::Result;
use unplug::data::atc::ATCS;
use unplug::data::item::{ItemFlags, ITEMS};
use unplug::data::stage::STAGES;

const UNKNOWN_ID_PREFIX: &str = "unk";

fn sort_ids<I: IdString + Ord>(ids: &mut [I], settings: &ListIdsOpt) {
    if settings.by_id {
        ids.sort_unstable();
    } else {
        ids.sort_unstable_by_key(|i| i.to_id());
    }
    if settings.reverse {
        ids.reverse();
    }
}

/// The `list items` CLI command.
fn command_items(opt: ListItemsOpt) -> Result<()> {
    let mut items: Vec<_> = if opt.show_unknown {
        ITEMS.iter().map(|i| i.id).collect()
    } else {
        ITEMS.iter().filter(|i| !i.flags.contains(ItemFlags::UNUSED)).map(|i| i.id).collect()
    };
    sort_ids(&mut items, &opt.settings);
    for item in items {
        println!("[{:>3}] {}", i16::from(item), item.to_id());
    }
    Ok(())
}

/// The `list equipment` CLI command.
fn command_equipment(opt: ListEquipmentOpt) -> Result<()> {
    let mut atcs: Vec<_> = ATCS.iter().map(|a| a.id).collect();
    sort_ids(&mut atcs, &opt.settings);
    for atc in atcs {
        let name = atc.to_id();
        if opt.show_unknown || !name.starts_with(UNKNOWN_ID_PREFIX) {
            println!("[{:>1}] {}", i16::from(atc), atc.to_id());
        }
    }
    Ok(())
}

/// The `list stages` CLI command.
fn command_stages(opt: ListStagesOpt) -> Result<()> {
    let mut stages: Vec<_> = STAGES.iter().map(|s| s.id).collect();
    sort_ids(&mut stages, &opt.settings);
    for stage in stages {
        let name = stage.to_id();
        println!("[{:>3}] {}", i32::from(stage), name);
    }
    Ok(())
}

/// The `list` CLI command.
pub fn command(_ctx: Context, opt: ListCommand) -> Result<()> {
    match opt {
        ListCommand::Items(opt) => command_items(opt),
        ListCommand::Equipment(opt) => command_equipment(opt),
        ListCommand::Stages(opt) => command_stages(opt),
    }
}

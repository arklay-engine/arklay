//! Writing a planned game as a pack.
//!
//! A generated pack is its base pack with four kinds of change, all data:
//! the planned pickups written into the room scripts' `item_aot_set`s
//! ([`patch_room`]), the planned door locks written into their
//! `door_aot_set`s ([`super::locks::patch_doors`]), a manifest naming the
//! seed, the flags and the new game's setup ([`manifest`]), and the request
//! and spoiler entries. The room-script changes a randomized game needs
//! (documents, maps and MO disc readers turned into pickups, Jill's missing
//! pickups, the scripted locks and Rebecca's refusals) are in the shipped
//! scripts already, gated on `ITEM_RANDOMIZER`, `LOCK_RANDOMIZER` and
//! `BR_LOCK_FULL_BYTE`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail, ensure};
use arklay_biorand::items::Pickup;
use arklay_biorand::plan::{Game, InventorySizes, Plan, system_flags};

use super::{Request, character_of};
use crate::manifest::{self, GameSetup, Manifest};
use crate::pack::{Pack, PackWriter};
use crate::scd::ir::{Decoded, Insn, Scripts};

/// `item_aot_set`, the instruction that declares a pickup.
pub(super) const ITEM_AOT_SET: u8 = 0x18;
/// The instructions that re-arm or remove an AOT by id, with its SCE (the
/// handler kind) at byte 2: `aot_reset`, `aot_delete`, `aot_on`.
pub(super) const AOT_BY_ID: [u8; 3] = [0x12, 0x13, 0x24];
/// The byte offsets of an `item_aot_set`'s fields.
pub(super) const ITEM_ID: usize = 1;
pub(super) const ITEM_TYPE: usize = 10;
pub(super) const ITEM_AMOUNT: usize = 11;
pub(super) const ITEM_FLAG: usize = 22;
/// The byte offset of the SCE in [`AOT_BY_ID`] instructions.
pub(super) const AOT_SCE: usize = 2;
/// The pickup SCEs: an item, a document, a map.
pub(super) const PICKUP_SCES: [u8; 3] = [0x04, 0x0D, 0x0F];

/// The system flags of options this milestone leaves to later ones, which a
/// pack must not raise: the scripts would act on doors and enemies the pack
/// does not change.
const DEFERRED_FLAGS: [&str; 2] = [
    system_flags::DOOR_RANDOMIZER,
    system_flags::ENEMY_RANDOMIZER,
];

/// The system flags a pack of `game` raises: the plan's, less the deferred
/// options' ([`DEFERRED_FLAGS`]).
pub(super) fn applied_flags(game: &Game) -> Vec<String> {
    game.system_flags
        .iter()
        .filter(|flag| !DEFERRED_FLAGS.contains(flag))
        .map(|flag| (*flag).to_string())
        .collect()
}

/// What a new game of `game` sets up: the scenario flags, the player's start
/// inventory and any slot count other than the original's.
pub(super) fn setup(game: &Game) -> Result<GameSetup> {
    let sizes = &game.inventory_sizes;
    ensure!(
        sizes.rebecca == InventorySizes::ORIGINAL.rebecca,
        "Rebecca's inventory has {} slots; the engine keeps her six",
        sizes.rebecca
    );
    let slots = |count: u8, original: u8| -> Result<Option<u8>> {
        ensure!(
            (1..=manifest::MAX_SLOTS).contains(&count),
            "an inventory of {count} slots"
        );
        Ok((count != original).then_some(count))
    };
    let held = usize::from(sizes.of(game.player));
    let start_items = game
        .inventory
        .slots
        .iter()
        .filter(|slot| slot.item != 0)
        .take(held)
        .map(|slot| {
            let amount = u8::try_from(slot.amount)
                .with_context(|| format!("item {} starts with {}", slot.item, slot.amount))?;
            Ok((slot.item, amount))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(GameSetup {
        scenario_flags: game
            .scenario_flags
            .iter()
            .map(|flag| (flag.flag, flag.raised))
            .collect(),
        start_items: Some(start_items),
        chris_slots: slots(sizes.chris, InventorySizes::ORIGINAL.chris)?,
        jill_slots: slots(sizes.jill, InventorySizes::ORIGINAL.jill)?,
    })
}

/// The manifest of `game`'s pack `id`, from its base pack's.
pub(super) fn manifest(base: &Manifest, id: &str, seed: u32, game: &Game) -> Result<Manifest> {
    let player = match game.player {
        arklay_biorand::map::Player::Chris => "Chris",
        arklay_biorand::map::Player::Jill => "Jill",
    };
    let mut manifest = base.clone();
    manifest.id = id.to_string();
    manifest.name = Some(format!("BioRand {seed:06} ({player})"));
    manifest.description = Some(format!(
        "{} with its items randomized by BioRand, seed {seed:06}.",
        base.name.as_deref().unwrap_or("Resident Evil")
    ));
    manifest.seed = Some(seed);
    manifest.generator_version = Some(super::GENERATOR_VERSION);
    manifest.plan_version = Some(arklay_biorand::plan::GENERATOR_VERSION);
    manifest.character = Some(character_of(game.player));
    manifest.biorand_flags = Some(applied_flags(game));
    manifest.setup = setup(game)?;
    Ok(manifest)
}

/// What every planned pickup flag holds, as `(item, amount)`: the slots
/// `game` changes, whichever room declares them. Staging rewrites every
/// `item_aot_set` by its flag, so two slots sharing a flag hold the same.
pub(super) fn planned_pickups(game: &Game) -> Result<BTreeMap<u8, (u8, u8)>> {
    let mut pickups = BTreeMap::new();
    for (dir, slots) in &game.items {
        for (id, slot) in slots {
            let planned = match slot.pickup {
                Pickup::Original => continue,
                Pickup::Empty => (0, 0),
                Pickup::Item { item, amount, .. } => {
                    let amount = u8::try_from(amount).with_context(|| {
                        format!("room {dir} AOT {id} holds {amount} of item {item}")
                    })?;
                    (item, amount)
                }
            };
            let flag = u8::try_from(slot.flag)
                .with_context(|| format!("room {dir} AOT {id} has pickup flag {}", slot.flag))?;
            if let Some(&other) = pickups.get(&flag)
                && other != planned
            {
                bail!("pickup flag {flag} is planned to hold both {other:?} and {planned:?}");
            }
            pickups.insert(flag, planned);
        }
    }
    Ok(pickups)
}

/// The planned flags of `game`'s slots in room `dir` that change, which the
/// room's script must declare.
pub(super) fn planned_flags(game: &Game, dir: &str) -> BTreeSet<u8> {
    game.items
        .get(dir)
        .into_iter()
        .flat_map(|slots| slots.values())
        .filter(|slot| slot.pickup != Pickup::Original)
        .filter_map(|slot| u8::try_from(slot.flag).ok())
        .collect()
}

/// Every command instruction of a room's scripts.
pub(super) fn commands(scripts: &Scripts) -> impl Iterator<Item = &Insn> {
    scripts
        .init
        .iter()
        .chain(&scripts.main)
        .flat_map(|block| &block.insns)
        .chain(scripts.events.iter().flat_map(|stream| &stream.insns))
        .filter(|insn| matches!(insn.decoded, Decoded::Command(_)))
}

/// The SCE of a pickup of `item`: what its `aot_reset`/`aot_on` must name.
/// The engine picks an `item_aot_set`'s handler the same way.
pub(super) fn pickup_sce(item: u8) -> u8 {
    crate::game::item_handler(item)
}

/// A room script with `pickups` written in: every `item_aot_set` whose
/// pickup flag is planned gets the planned item and amount, and every
/// `aot_reset`, `aot_delete` and `aot_on` re-arming such an AOT with a pickup
/// SCE gets the SCE of its new item. The bytes are patched where they are,
/// so nothing else of the script moves. Returns the flags written.
pub(super) fn patch_room(
    scd: &[u8],
    pickups: &BTreeMap<u8, (u8, u8)>,
) -> Result<(Vec<u8>, BTreeSet<u8>)> {
    let scripts = crate::scd::reader::parse(scd)?;
    let mut out = scd.to_vec();
    let mut written = BTreeSet::new();
    // The AOT ids each planned item_aot_set declares, with its new item.
    let mut aots: BTreeMap<u8, u8> = BTreeMap::new();
    for insn in commands(&scripts).filter(|insn| insn.op == ITEM_AOT_SET) {
        ensure!(
            insn.bytes.len() > ITEM_FLAG,
            "a short item_aot_set at {:#x}",
            insn.offset
        );
        let flag = insn.bytes[ITEM_FLAG];
        let Some(&(item, amount)) = pickups.get(&flag) else {
            continue;
        };
        out[insn.offset + ITEM_TYPE] = item;
        out[insn.offset + ITEM_AMOUNT] = amount;
        written.insert(flag);
        let id = insn.bytes[ITEM_ID] & 0x7F;
        if let Some(&other) = aots.get(&id)
            && pickup_sce(other) != pickup_sce(item)
        {
            bail!("AOT {id} is declared twice with pickups of different kinds");
        }
        aots.insert(id, item);
    }
    for insn in commands(&scripts).filter(|insn| AOT_BY_ID.contains(&insn.op)) {
        ensure!(
            insn.bytes.len() > AOT_SCE,
            "a short AOT instruction at {:#x}",
            insn.offset
        );
        let Some(&item) = aots.get(&(insn.bytes[ITEM_ID] & 0x7F)) else {
            continue;
        };
        if PICKUP_SCES.contains(&insn.bytes[AOT_SCE]) {
            out[insn.offset + AOT_SCE] = pickup_sce(item);
        }
    }
    Ok((out, written))
}

/// The entries a generated pack adds beside its base's.
pub(super) fn extra_entries(request: &Request, game: &Game) -> Vec<(&'static str, Vec<u8>)> {
    let mut spoiler_json =
        serde_json::to_string_pretty(&game.spoiler.to_json()).expect("JSON values serialize");
    spoiler_json.push('\n');
    vec![
        (super::REQUEST_ENTRY, request.to_json().into_bytes()),
        (super::SPOILER_JSON_ENTRY, spoiler_json.into_bytes()),
        (
            super::SPOILER_TEXT_ENTRY,
            game.spoiler.to_text().into_bytes(),
        ),
    ]
}

/// Whether `path` is an entry the generator writes rather than copies.
pub(super) fn is_generated_entry(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path == manifest::ENTRY
        || [
            super::REQUEST_ENTRY,
            super::SPOILER_JSON_ENTRY,
            super::SPOILER_TEXT_ENTRY,
        ]
        .contains(&path.as_str())
}

/// The room directory of a room script entry `room/<dir>/room.scd`.
pub(super) fn room_script_dir(path: &str) -> Option<String> {
    path.strip_prefix("room/")?
        .strip_suffix("/room.scd")
        .filter(|dir| !dir.contains('/'))
        .map(str::to_ascii_lowercase)
}

/// `game`'s pack `id` of `plan`, from the `source` base pack, ready to write.
pub(super) fn pack(
    source: &Pack,
    id: &str,
    request: &Request,
    plan: &Plan,
    game: &Game,
) -> Result<PackWriter> {
    let base = source.manifest().context("the base pack has no manifest")?;
    let seed = u32::try_from(plan.seed).context("the plan's seed")?;
    let manifest = manifest(base, id, seed, game)?;
    let pickups = planned_pickups(game)?;

    let mut writer = PackWriter::new();
    writer.add(manifest::ENTRY, manifest.render().into_bytes())?;
    let mut rooms = BTreeSet::new();
    for path in source.own_paths() {
        if is_generated_entry(path) {
            continue;
        }
        if let Some(dir) = room_script_dir(path) {
            let scd = source.read(path)?;
            let (patched, written) =
                patch_room(&scd, &pickups).with_context(|| format!("failed to patch {path}"))?;
            let patched = super::locks::patch_doors(&patched, &dir, game)
                .with_context(|| format!("failed to write the door locks of {path}"))?;
            let missing: Vec<_> = planned_flags(game, &dir)
                .difference(&written)
                .copied()
                .collect();
            ensure!(
                missing.is_empty(),
                "{path} declares no pickup with flag {missing:?}"
            );
            rooms.insert(dir);
            if patched != scd.as_ref() {
                writer.add(path, patched)?;
                continue;
            }
        }
        let (data, zstd) = source.read_stored(path)?;
        writer.add_stored(path, data.into_owned(), zstd)?;
    }
    for dir in game.items.keys() {
        ensure!(
            rooms.contains(dir) || planned_flags(game, dir).is_empty(),
            "the plan places pickups in room {dir}, which the base pack has no script for"
        );
    }
    for (path, data) in extra_entries(request, game) {
        writer.add(path, data)?;
    }
    Ok(writer)
}

/// Compress and write `writer` to `out` atomically, reporting progress.
pub(super) fn write(
    mut writer: PackWriter,
    out: &Path,
    jobs: usize,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<()> {
    let total = writer.len() as u64;
    let done = AtomicU64::new(0);
    progress(0, total);
    writer.compress_entries_reporting(jobs, crate::convert::is_precompressed, |_| {
        progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
    })?;
    writer
        .write(out)
        .with_context(|| format!("failed to write {}", out.display()))
}

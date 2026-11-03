//! Staging's `InventoryRandomizer` (`InventoryRandomizer.cs`): a player's
//! random starting inventory. It fills up to a random number of slots with the
//! knife, the primary and secondary weapons with their spare ammunition, and
//! then with health and ink stacks.
//!
//! The stage has no capacity of its own. Staging's `Full` counts the weapons
//! and their ammunition too, and an overfull inventory is kept as it is. The
//! consumer pads it to the player's slot count and drops what does not fit,
//! as staging's `RandomInventory.WithSize` does (`RandomInventory.cs:20-26`).

use crate::config::Config;
use crate::map::{ItemDef, Map, Player};
use crate::rng::Rng;

/// One filled slot: an item and how many of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub item: u8,
    pub amount: u16,
}

/// A player's starting inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartInventory {
    pub player: Player,
    /// The filled slots in order. There may be more than the player's slot
    /// count; the consumer drops the ones that do not fit.
    pub slots: Vec<Slot>,
    /// The weapons among `slots`, in the order they were given: the knife,
    /// then the primary weapon, then the secondary.
    pub weapons: Vec<u8>,
}

/// The starting inventory of `player`, drawn from `rng` and read from the
/// `inventory/*` options of `config` as staging reads them.
pub fn randomize(map: &Map, config: &Config, player: Player, rng: &mut Rng) -> StartInventory {
    // InventoryRandomizer.cs:15-17. Staging's Next(min, max + 1) is inclusive,
    // and gives min when the range is empty.
    let min = config.get_int("inventory/min", 0) as i32;
    let max = config.get_int("inventory/max", 8) as i32;
    let fill = usize::try_from(rng.next_in(min, max + 1)).unwrap_or(0);

    let mut slots = Vec::new();
    let mut weapons = Vec::new();
    // :20-28: the knife, at its catalogue maximum.
    let knife_on = knife_enabled(config, rng);
    let knife = map
        .items
        .iter()
        .find(|(_, def)| knife_on && def.kind == "weapon/knife");
    if let Some((&id, def)) = knife {
        slots.push(Slot {
            item: id,
            amount: def.max,
        });
        weapons.push(id);
    }
    // :30-40: the primary, then a secondary outside the primary's group.
    let primary = random_weapon(map, config, "inventory/primary", None, rng);
    let exclude = primary.as_ref().map(|weapon| weapon.group);
    let secondary = random_weapon(map, config, "inventory/secondary", exclude, rng);
    for weapon in primary.iter().chain(secondary.iter()) {
        weapons.push(weapon.id);
        add_weapon(&mut slots, weapon);
    }
    // :42-48: health and ink stacks, until the slots are full.
    let mut pool = Pool::new(vec![
        stacks(map, config, rng, "health/"),
        stacks(map, config, rng, "ink"),
    ]);
    while slots.len() < fill {
        let Some(slot) = pool.take(map, rng) else {
            break;
        };
        slots.push(slot);
    }
    StartInventory {
        player,
        slots,
        weapons,
    }
}

/// Staging's `GetRandomEnabled` (:52-60): `Always` and `Never` are as set,
/// and anything else is a coin toss.
fn knife_enabled(config: &Config, rng: &mut Rng) -> bool {
    match config.get_str("inventory/weapon/knife", "Always").as_str() {
        "Always" => true,
        "Never" => false,
        _ => rng.index(2) == 1,
    }
}

/// A weapon for one slot, with the ammunition that does not fit in it.
struct Weapon<'a> {
    id: u8,
    /// The kind after `weapon/`: `handgun` for `weapon/handgun`.
    group: &'a str,
    amount: u16,
    /// The ammunition beyond the weapon, given as stacks of `spare_item` of
    /// at most `spare_stack` each.
    spare: u16,
    spare_item: u8,
    spare_stack: u16,
}

/// Staging's `GetRandomWeapon` (:62-101): one weapon of the slot, or none. The
/// pool holds the enabled weapons, plus `none` when it is enabled, and one is
/// picked at random. `exclude` is the group the weapon may not share.
fn random_weapon<'a>(
    map: &'a Map,
    config: &Config,
    prefix: &str,
    exclude: Option<&str>,
    rng: &mut Rng,
) -> Option<Weapon<'a>> {
    let mut pool: Vec<Option<(u8, &'a ItemDef)>> = Vec::new();
    if config.get_bool(&format!("{prefix}/none"), false) {
        pool.push(None);
    }
    for (&id, def) in &map.items {
        // :72-77: only weapons, and none of the excluded group.
        if !def.kind.starts_with("weapon/") || exclude == Some(weapon_group(&def.kind)) {
            continue;
        }
        if config.get_bool(&format!("{prefix}/{}", def.kind), false) {
            pool.push(Some((id, def)));
        }
    }
    if pool.is_empty() {
        return None;
    }
    let (id, def) = (*rng.pick(&pool))?;

    // :89-92: the ammunition is a fraction of the weapon's maximum, and the
    // weapon holds at most its maximum. Staging draws the total with
    // NextDouble(min, max) (Rng.cs:30-33), which does not check that the range
    // is not empty, so this does not either.
    let max = i64::from(def.max);
    let low = config.get_f64(&format!("{prefix}/ammo/min"), 0.0) * max as f64;
    let high = config.get_f64(&format!("{prefix}/ammo/max"), 0.0) * max as f64;
    let total = (low + rng.next_f64() * (high - low)).round_ties_even() as i64;
    let amount = total.min(max);

    // :93-98: the rest is spare ammunition, of a random type the weapon takes.
    // A type with no catalogue entry or no capacity is left out: staging would
    // fail on the first and loop forever on the second.
    let ammo = usable_ammo(map, &def.ammo);
    let (spare, spare_item, spare_stack) = if ammo.is_empty() {
        (0, 0, 0)
    } else {
        let item = *rng.pick(&ammo);
        (total - amount, item, i64::from(max_of(map, item)))
    };
    Some(Weapon {
        id,
        group: weapon_group(&def.kind),
        amount: clamp_u16(amount),
        spare: clamp_u16(spare),
        spare_item,
        spare_stack: clamp_u16(spare_stack),
    })
}

/// The group of a weapon kind: `handgun` for `weapon/handgun` (:153).
fn weapon_group(kind: &str) -> &str {
    kind.split('/').nth(1).unwrap_or_default()
}

/// The ammunition types of `ids` that are in the catalogue with a capacity.
fn usable_ammo(map: &Map, ids: &[u8]) -> Vec<u8> {
    ids.iter()
        .copied()
        .filter(|&id| max_of(map, id) > 0)
        .collect()
}

/// Staging's `InventoryBuilder.Add(WeaponSwag)` (:130-141): the weapon, then
/// its spare ammunition in stacks. Like staging, it does not check capacity.
fn add_weapon(slots: &mut Vec<Slot>, weapon: &Weapon) {
    slots.push(Slot {
        item: weapon.id,
        amount: weapon.amount,
    });
    let mut spare = weapon.spare;
    while spare > 0 {
        let take = spare.min(weapon.spare_stack);
        slots.push(Slot {
            item: weapon.spare_item,
            amount: take,
        });
        spare -= take;
    }
}

/// Staging's `GetRandomItems` (:202-222): the stacks of each item whose kind
/// starts with `prefix`. The amount is drawn from the item's configured range,
/// and an amount of 0 is no stack.
fn stacks(map: &Map, config: &Config, rng: &mut Rng, prefix: &str) -> Vec<(u8, u16)> {
    let mut list = Vec::new();
    for (&id, def) in &map.items {
        // An item that holds nothing would fill slots with empty stacks.
        if !def.kind.starts_with(prefix) || def.max == 0 {
            continue;
        }
        let min = config.get_int(&format!("inventory/{}/min", def.kind), 0) as i32;
        let max = config.get_int(&format!("inventory/{}/max", def.kind), 0) as i32;
        // :215: Next(min, max + 1), so the amount runs from min to max.
        let amount = rng.next_in(min, max + 1);
        if amount > 0 {
            list.push((id, amount as u16));
        }
    }
    list
}

/// Staging's `ItemPool` (:156-200): the stacks of each group, handed out one
/// at a time, each time from the next group in turn.
struct Pool {
    groups: Vec<Vec<(u8, u16)>>,
    /// The group the next stack comes from. `None` before the first stack, and
    /// after a reshuffle the next group is `Some(0)`.
    next: Option<usize>,
}

impl Pool {
    /// Empty groups are left out, as staging's `AddGroup` does (:163-165).
    fn new(groups: Vec<Vec<(u8, u16)>>) -> Self {
        Self {
            groups: groups
                .into_iter()
                .filter(|group| !group.is_empty())
                .collect(),
            next: None,
        }
    }

    /// Staging's `TakeStack` (:168-200). A stack is at most its item's
    /// maximum, and the rest of the amount stays in the group.
    fn take(&mut self, map: &Map, rng: &mut Rng) -> Option<Slot> {
        if self.next.is_none_or(|next| next >= self.groups.len()) {
            self.groups = rng.shuffled(&self.groups);
            self.next = Some(0);
        }
        if self.groups.is_empty() {
            return None;
        }
        let index = self.next.unwrap_or_default();
        let group = &mut self.groups[index];
        let at = rng.index(group.len());
        let (item, amount) = group[at];
        let take = amount.min(max_of(map, item));
        if amount > take {
            group[at].1 = amount - take;
        } else {
            group.remove(at);
        }
        if self.groups[index].is_empty() {
            self.groups.remove(index);
        }
        self.next = Some(index + 1);
        Some(Slot { item, amount: take })
    }
}

/// The catalogue maximum of an item, or 0 for one it does not have.
fn max_of(map: &Map, item: u8) -> u16 {
    map.items.get(&item).map_or(0, |def| def.max)
}

fn clamp_u16(value: i64) -> u16 {
    value.clamp(0, i64::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, max: u16, ammo: &[u8]) -> ItemDef {
        ItemDef {
            name: kind.into(),
            kind: kind.into(),
            ammo: ammo.to_vec(),
            max,
            ..ItemDef::default()
        }
    }

    /// The knife (1), a handgun (2) and a shotgun (3) with their ammunition
    /// (11 and 12), two herbs (5 and 6) and ink (47).
    fn catalogue() -> Map {
        let mut map = Map::default();
        map.items.insert(1, item("weapon/knife", 1, &[]));
        map.items.insert(2, item("weapon/handgun", 15, &[11]));
        map.items.insert(3, item("weapon/shotgun", 10, &[12]));
        map.items.insert(11, item("ammo/handgun", 10, &[]));
        map.items.insert(12, item("ammo/shotgun", 8, &[]));
        map.items.insert(5, item("health/g", 1, &[]));
        map.items.insert(6, item("health/r", 1, &[]));
        map.items.insert(47, item("ink", 4, &[]));
        map
    }

    /// A configuration with no knife, no weapons and no pickups.
    fn quiet() -> Config {
        let mut config = Config::new();
        config.set("inventory/weapon/knife", "Never");
        config.set("inventory/min", 0);
        config.set("inventory/max", 0);
        config
    }

    #[test]
    fn the_knife_follows_its_setting() {
        let map = catalogue();
        let mut config = quiet();
        config.set("inventory/weapon/knife", "Always");
        let inventory = randomize(&map, &config, Player::Chris, &mut Rng::new(1));
        assert_eq!(inventory.slots, [Slot { item: 1, amount: 1 }]);
        assert_eq!(inventory.weapons, [1]);

        let inventory = randomize(&map, &quiet(), Player::Chris, &mut Rng::new(1));
        assert!(inventory.slots.is_empty());
    }

    #[test]
    fn a_weapon_holds_what_it_can_and_the_rest_comes_as_stacks() {
        let map = catalogue();
        let mut config = quiet();
        config.set("inventory/primary/none", false);
        config.set("inventory/primary/weapon/handgun", true);
        config.set("inventory/primary/ammo/min", 2.0);
        config.set("inventory/primary/ammo/max", 2.0);
        let inventory = randomize(&map, &config, Player::Jill, &mut Rng::new(1));
        // Twice the handgun's 15 rounds: 15 in the gun, and 15 spare in stacks
        // of the 10 that fit in one.
        assert_eq!(
            inventory.slots,
            [
                Slot {
                    item: 2,
                    amount: 15
                },
                Slot {
                    item: 11,
                    amount: 10
                },
                Slot {
                    item: 11,
                    amount: 5
                },
            ]
        );
        assert_eq!(inventory.weapons, [2]);
        assert_eq!(inventory.player, Player::Jill);
    }

    #[test]
    fn a_secondary_is_not_of_the_primary_group() {
        let map = catalogue();
        let mut config = quiet();
        config.set("inventory/primary/none", false);
        config.set("inventory/primary/weapon/handgun", true);
        config.set("inventory/secondary/none", false);
        config.set("inventory/secondary/weapon/handgun", true);
        let inventory = randomize(&map, &config, Player::Chris, &mut Rng::new(1));
        assert_eq!(inventory.weapons, [2], "the only secondary is a handgun");

        config.set("inventory/secondary/weapon/shotgun", true);
        let inventory = randomize(&map, &config, Player::Chris, &mut Rng::new(1));
        assert_eq!(inventory.weapons, [2, 3]);
    }

    #[test]
    fn a_stack_is_at_most_the_items_maximum() {
        let map = catalogue();
        let mut config = quiet();
        config.set("inventory/min", 8);
        config.set("inventory/max", 8);
        config.set("inventory/ink/min", 10);
        config.set("inventory/ink/max", 10);
        let inventory = randomize(&map, &config, Player::Chris, &mut Rng::new(1));
        // Ink holds 4, so 10 units come as stacks of 4, 4 and 2.
        assert_eq!(
            inventory.slots,
            [4, 4, 2].map(|amount| Slot { item: 47, amount })
        );
    }

    #[test]
    fn pickups_fill_to_a_random_count_within_the_range() {
        let map = catalogue();
        let mut config = quiet();
        config.set("inventory/min", 2);
        config.set("inventory/max", 4);
        config.set("inventory/health/g/min", 5);
        config.set("inventory/health/g/max", 5);
        for seed in 0..40u64 {
            let inventory = randomize(&map, &config, Player::Chris, &mut Rng::new(seed));
            let count = inventory.slots.len();
            assert!((2..=4).contains(&count), "seed {seed}: {count} slots");
            assert!(
                inventory
                    .slots
                    .iter()
                    .all(|slot| slot.item == 5 && slot.amount == 1),
                "seed {seed}: five single herbs to choose from"
            );
        }
    }

    #[test]
    fn the_health_and_ink_groups_take_turns() {
        let map = catalogue();
        let mut config = quiet();
        config.set("inventory/min", 2);
        config.set("inventory/max", 2);
        config.set("inventory/health/g/min", 3);
        config.set("inventory/health/g/max", 3);
        config.set("inventory/ink/min", 4);
        config.set("inventory/ink/max", 4);
        for seed in 0..20u64 {
            let inventory = randomize(&map, &config, Player::Chris, &mut Rng::new(seed));
            // All the health items are one group, and ink is the other, so the
            // two slots take one stack from each (InventoryRandomizer.cs:42-48).
            let herbs = inventory
                .slots
                .iter()
                .filter(|slot| slot.item == 5 || slot.item == 6)
                .count();
            let ink: Vec<&Slot> = inventory
                .slots
                .iter()
                .filter(|slot| slot.item == 47)
                .collect();
            assert_eq!((herbs, ink.len()), (1, 1), "seed {seed}");
            assert_eq!(ink[0].amount, 4, "seed {seed}");
        }
    }
}

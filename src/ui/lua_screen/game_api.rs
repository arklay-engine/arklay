//! The game-rule calls of the pause menu: the player's health, the
//! equipped item, and the USE/EQUIP/MOVE actions that change the
//! inventory. The rules stay in [`GameState`]; the menu script decides
//! when to run them. The changing calls need the game state the engine
//! lends with [`super::LuaScreen::update_with_game`] and friends.

use mlua::{IntoLuaMulti, UserDataMethods};

use crate::game::{CombineResult, GameState, STATE_BYTE_CHARACTER, UseResult, platform_rand};
use crate::item_catalog::ItemKind;

use super::Api;

/// A file page's sprites (`files::page_draw`) as a sequence of tables:
/// `chunk` (of the file) or `sheet` (from texture 0x12), `u`, `v`, `w`, `h`,
/// `x`, `y` and `colour`.
fn page_sprites(
    lua: &mlua::Lua,
    sprites: &[crate::game::status2::files::PageSprite],
) -> mlua::Result<mlua::Table> {
    use crate::game::status2::files::PageTexture;
    let list = lua.create_table()?;
    for (index, sprite) in sprites.iter().enumerate() {
        let entry = lua.create_table()?;
        match sprite.texture {
            PageTexture::Chunk(chunk) => entry.set("chunk", chunk)?,
            PageTexture::Sheet(sheet) => entry.set("sheet", sheet)?,
        }
        entry.set("u", sprite.u)?;
        entry.set("v", sprite.v)?;
        entry.set("w", sprite.w)?;
        entry.set("h", sprite.h)?;
        entry.set("x", sprite.x)?;
        entry.set("y", sprite.y)?;
        entry.set("colour", sprite.colour)?;
        list.set(index + 1, entry)?;
    }
    Ok(list)
}

pub(super) fn add_methods<'a, M: UserDataMethods<Api<'a>>>(methods: &mut M) {
    // ---- queries -------------------------------------------------------
    // `player_health()`: the player's health, maximum health and status
    // byte (the poison flags are `0x22`); `0, 0, 0` outside a session.
    methods.add_method("player_health", |_lua, api, ()| {
        Ok(api.game().map_or((0, 0, 0), |game| {
            (
                i64::from(game.entities[0].health),
                i64::from(game.max_health),
                i64::from(game.health_status),
            )
        }))
    });

    // `character()`: the character byte's low two bits (`0` Chris, `1`
    // Jill, `2`/`3` the alternates), which pick the portrait.
    methods.add_method("character", |_lua, api, ()| {
        Ok(api.game().map_or(0, |game| {
            game.state_bytes[usize::from(STATE_BYTE_CHARACTER)] & 3
        }))
    });

    // `has_radio()`: whether the radio tab is live.
    methods.add_method("has_radio", |_lua, api, ()| {
        Ok(api.game().is_some_and(GameState::has_radio))
    });

    // `authentic()`: whether the run was started with `--authentic`, the
    // original's item rules. A paused session's game state answers; a screen
    // with no session reads the run's copy on its context.
    methods.add_method("authentic", |_lua, api, ()| {
        Ok(api
            .game()
            .map_or(api.cx.authentic, |game| game.player_options.authentic))
    });

    // `equipped()`: the equipped item, `0` for none.
    methods.add_method("equipped", |_lua, api, ()| {
        Ok(api.game().and_then(|game| game.equipped).unwrap_or(0))
    });

    // `item_count(item)`: how many `item` the inventory holds in total.
    methods.add_method("item_count", |_lua, api, item: u8| {
        Ok(api.game().map_or(0, |game| game.item_count(item)))
    });

    // `re2_items()`: RE2's status screen as a table (`page`, `sub`,
    // `command`, `step`, `count`, `cursor`, `target`, `tab`,
    // `command_cursor`, `result`, `follow`, the condition panel's
    // `panel_mode`, `panel_sweep`, `panel_cure` and `panel_done`, whether
    // the player is `poisoned`, the screen's shared `blink` and
    // `blink_falling`, FILE's `binder`, `slot`, `page_index`, `zoom`,
    // `view_zoom`, `spin`, `file_x`, the `shown_page`, the 24 `files` held,
    // 0xFF for none, the `file` in the binder's slot and, in the reader's
    // subs 0xD to 0x12, the `file_page`'s sprites), or nil when it isn't up. The engine runs it; the script only
    // draws it.
    methods.add_method("re2_items", |lua, api, ()| {
        let Some((page, poisoned, work, held)) = api.game().and_then(|game| {
            Some((
                game.re2_items?,
                game.re2.player.poisoned,
                game.re2_file,
                game.re2.files,
            ))
        }) else {
            return Ok(mlua::Value::Nil);
        };
        let table = lua.create_table()?;
        table.set("count", page.count)?;
        table.set("result", page.result)?;
        table.set("follow", page.follow)?;
        table.set("panel_mode", page.panel.mode)?;
        table.set("panel_sweep", page.panel.sweep)?;
        table.set("panel_cure", page.panel.cure)?;
        table.set("panel_done", page.panel.done)?;
        table.set("poisoned", poisoned)?;
        table.set("page", page.page)?;
        table.set("sub", page.sub)?;
        table.set("command", page.command)?;
        table.set("step", page.step)?;
        table.set("cursor", page.cursor)?;
        table.set("target", page.target)?;
        table.set("tab", page.tab)?;
        table.set("command_cursor", page.command_cursor)?;
        table.set("blink", work.blink.level)?;
        table.set("blink_falling", work.blink.falling)?;
        table.set("binder", work.binder)?;
        table.set("slot", work.slot)?;
        table.set("page_index", work.page)?;
        table.set("zoom", work.zoom)?;
        table.set("view_zoom", work.view_zoom)?;
        table.set("spin", work.spin)?;
        table.set("file_x", work.x)?;
        table.set("shown_page", work.shown_page)?;
        table.set("files", lua.create_sequence_from(held)?)?;
        let file = crate::game::status2::files::file_at(&held, work.binder, work.slot);
        table.set("file", file)?;
        if page.page == crate::game::status2::PAGE_FILE
            && (crate::game::status2::FILE_PAGE..=crate::game::status2::FILE_PREV_IN)
                .contains(&page.sub)
        {
            let sprites = crate::game::status2::files::page_draw(
                &work,
                file,
                crate::game::status2::files::PageLit::reader(page.sub),
            );
            table.set("file_page", page_sprites(lua, &sprites)?)?;
        }
        Ok(mlua::Value::Table(table))
    });

    // `re2_item_get()`: RE2's get-item window while it is up: its `item`
    // and, on the ticks it flies to its slot, the picture's `fly` (`x`, `y`,
    // `w`, `h`, from the window's origin), whether the player is
    // `poisoned`, its `phase`, and for a file `St_get_file`'s `sub`, the
    // `file` and, while the page draws, the `file_page`'s sprites; nil when
    // it isn't up.
    methods.add_method("re2_item_get", |lua, api, ()| {
        let Some((get, fly, poisoned, file, file_page)) = api.game().and_then(|game| {
            Some((
                game.re2_item_get?,
                game.re2_item_get_fly(),
                game.re2.player.poisoned,
                game.re2_item_get_file(),
                game.re2_item_get_page(),
            ))
        }) else {
            return Ok(mlua::Value::Nil);
        };
        use crate::game::Re2ItemGetPhase as Phase;
        let table = lua.create_table()?;
        table.set("item", get.action.params[0])?;
        table.set("poisoned", poisoned)?;
        table.set(
            "phase",
            match get.phase {
                Phase::Open => "open",
                Phase::Ask => "ask",
                Phase::Fly => "fly",
                Phase::Refuse => "refuse",
                Phase::Shown => "shown",
                Phase::Close => "close",
                Phase::FileFade => "file_fade",
                Phase::FileRead => "file_read",
            },
        )?;
        if let Some(page) = get.file {
            table.set("sub", page.sub())?;
        }
        if let Some(file) = file {
            table.set("file", file)?;
        }
        if let Some(sprites) = file_page {
            table.set("file_page", page_sprites(lua, &sprites)?)?;
        }
        if let Some(fly) = fly {
            let entry = lua.create_table()?;
            entry.set("x", fly.x)?;
            entry.set("y", fly.y)?;
            entry.set("w", fly.w)?;
            entry.set("h", fly.h)?;
            table.set("fly", entry)?;
        }
        Ok(mlua::Value::Table(table))
    });

    // `re2_box()`: RE2's item box page while its screen is up: its
    // `routine`, `sub`, `count`, `cursor`, `top`, `scroll`, `pulse` and
    // whether `St_init01` has `started` it; nil otherwise.
    methods.add_method("re2_box", |lua, api, ()| {
        let Some(page) = api.game().and_then(|game| game.re2_box) else {
            return Ok(mlua::Value::Nil);
        };
        let table = lua.create_table()?;
        table.set("routine", page.routine)?;
        table.set("sub", page.sub)?;
        table.set("count", page.count)?;
        table.set("cursor", page.cursor)?;
        table.set("top", page.top)?;
        table.set("scroll", page.scroll)?;
        table.set("pulse", page.pulse)?;
        table.set("started", page.started)?;
        table.set("arrows", page.arrows)?;
        Ok(mlua::Value::Table(table))
    });

    // `re2_box_slot(slot)`: one of RE2's item box slots, `id, count, mark`.
    methods.add_method("re2_box_slot", |_lua, api, slot: usize| {
        let [id, count, mark] = api
            .game()
            .and_then(|game| {
                game.re2
                    .item_box
                    .get(slot % crate::game::itembox2::BOX_SLOTS)
                    .copied()
            })
            .unwrap_or_default();
        Ok((id, count, mark))
    });

    // `re2_map()`: what RE2's MAP page draws on the player's floor: its
    // `map` picture's number, its `rooms` in the draw order (each `x`, `y`,
    // `w`, `h`, an `outline` and maybe a translucent `fill`, both a
    // `texture` slot, `u`, `v` and `clut`) and the player's `marker` (`x`,
    // `y`, `u`, `v`), or nil without a game.
    methods.add_method("re2_map", |lua, api, ()| {
        let Some(view) = api.game().map(|game| game.re2_map_view()) else {
            return Ok(mlua::Value::Nil);
        };
        let sprite = |sprite: crate::game::status2::map::MapSprite| -> mlua::Result<mlua::Table> {
            let table = lua.create_table()?;
            table.set("texture", sprite.texture)?;
            table.set("u", sprite.u)?;
            table.set("v", sprite.v)?;
            table.set("clut", sprite.clut)?;
            Ok(table)
        };
        let table = lua.create_table()?;
        table.set("floor", view.floor)?;
        table.set("map", view.map)?;
        let rooms = lua.create_table()?;
        for (index, room) in view.rooms.iter().enumerate() {
            let entry = lua.create_table()?;
            entry.set("x", room.x)?;
            entry.set("y", room.y)?;
            entry.set("w", room.w)?;
            entry.set("h", room.h)?;
            entry.set("outline", sprite(room.outline)?)?;
            if let Some(fill) = room.fill {
                entry.set("fill", sprite(fill)?)?;
            }
            rooms.set(index + 1, entry)?;
        }
        table.set("rooms", rooms)?;
        if let Some(marker) = view.marker {
            let entry = lua.create_table()?;
            entry.set("x", marker.x)?;
            entry.set("y", marker.y)?;
            entry.set("u", marker.u)?;
            entry.set("v", marker.v)?;
            table.set("marker", entry)?;
        }
        Ok(mlua::Value::Table(table))
    });

    // `re2_computer()`: what room 200's computer draws this tick: the
    // `frame` `Wframe_set` drew (`{x0, y0, x1, y1}`, inclusive), whether
    // the console `drawn`, its glyph `cells` (`{x, y, u, v}` in record
    // order) and the `cursor` (`{x, y}`), or nil while no computer runs.
    methods.add_method("re2_computer", |lua, api, ()| {
        let Some(view) = api
            .game()
            .and_then(|game| game.re2_computer.as_ref())
            .map(crate::game::computer2::Computer::view)
        else {
            return Ok(mlua::Value::Nil);
        };
        let table = lua.create_table()?;
        if let Some(frame) = view.frame {
            table.set("frame", frame)?;
        }
        table.set("drawn", view.drawn)?;
        table.set("cells", view.cells)?;
        if let Some(cursor) = view.cursor {
            table.set("cursor", cursor)?;
        }
        Ok(mlua::Value::Table(table))
    });

    // `re2_slot(slot)`: the id and count of RE2's `Item_work` slot `slot`;
    // 10 is the personal item.
    methods.add_method("re2_slot", |_lua, api, slot: u8| {
        let [id, count] = api.game().map_or([0, 0], |game| game.re2_slot(slot));
        Ok((id, count))
    });

    // ---- changes -------------------------------------------------------
    // `rand()`: the next draw of the game's shared `rand()` stream (the
    // one the room scripts use), `0..=0x7FFF`.
    methods.add_method_mut("rand", |_lua, api, ()| {
        let game = api.game_mut("rand")?;
        Ok(platform_rand(&mut game.rand_state))
    });

    // `select_item(item)`: mirror the item under the cursor into the state
    // byte the scripts test; `0` (or nil) for none.
    methods.add_method_mut("select_item", |_lua, api, item: Option<u8>| {
        let game = api.game_mut("select_item")?;
        game.select_item(item.filter(|&item| item != 0));
        Ok(())
    });

    // `equip(item)`: equip `item`, or unequip with `0` (or nil).
    methods.add_method_mut("equip", |_lua, api, item: Option<u8>| {
        let game = api.game_mut("equip")?;
        game.set_equipped(item.filter(|&item| item != 0));
        Ok(())
    });

    // `use_item(slot)`: USE the item in inventory slot `slot`. On success
    // the item is recorded as used and one is consumed, and it returns
    // `true, healed, cured, closes` (`closes`: a key, special, chemical or
    // book use that exits the whole menu). A refusal returns `false,
    // category` (the use-category index the refusal message adds to
    // `0xF7`); an empty slot returns nothing.
    methods.add_method_mut("use_item", |lua, api, slot: usize| {
        let game = api.game_mut("use_item")?;
        let item = game.inventory.get(slot).map_or(0, |stack| stack.id);
        if item == 0 {
            return Ok(mlua::MultiValue::new());
        }
        let kind = game.item_defs.get(item).kind;
        let values = match game.use_item(item) {
            UseResult::Used { healed, cured } => {
                game.record_used_item(item);
                consume_slot(game, slot);
                (true, healed, cured, closes_inventory(kind)).into_lua_multi(lua)?
            }
            UseResult::Unusable => (false, kind.dispatch_index()).into_lua_multi(lua)?,
        };
        Ok(values)
    });

    // `combine(slot, target)`: MOVE the item in `slot` onto `target`.
    // Returns `"applied", herb`, `"drug_store"` (chemicals need the drug
    // store), `"herb_no_recipe"` or `"none"` (no recipe, or `target` past
    // the held items).
    methods.add_method_mut("combine", |_lua, api, (slot, target): (usize, usize)| {
        let game = api.game_mut("combine")?;
        if target >= game.inventory.len() {
            return Ok(("none", false));
        }
        Ok(match game.combine_slots(slot, target) {
            CombineResult::Applied { herb, .. } => ("applied", herb),
            CombineResult::NeedsDrugStore => ("drug_store", false),
            CombineResult::HerbNoRecipe => ("herb_no_recipe", false),
            CombineResult::NoRecipe => ("none", false),
        })
    });

    // `combine_check(slot, target)`: the original's `menu_item_check_combine`.
    // Ordinary recipes are applied immediately and return `"applied"`; a
    // valid herb recipe is deferred for the 0xF5 yes/no prompt and returns
    // `"herb"`; `"drug_store"`, `"herb_no_recipe"` and `"none"` are the
    // refusals. The parser maps the enum to those strings.
    methods.add_method_mut(
        "combine_check",
        |_lua, api, (slot, target): (usize, usize)| {
            let game = api.game_mut("combine_check")?;
            if target >= game.inventory.len() {
                return Ok("none");
            }
            Ok(match game.combine_check(slot, target) {
                crate::game::CombineCheck::Applied => "applied",
                crate::game::CombineCheck::Herb { .. } => "herb",
                crate::game::CombineCheck::NeedsDrugStore => "drug_store",
                crate::game::CombineCheck::HerbNoRecipe => "herb_no_recipe",
                crate::game::CombineCheck::NoRecipe => "none",
            })
        },
    );

    // `show_message(id)`: open the game's message window on global message
    // `id`; refused while one is already up.
    methods.add_method_mut("show_message", |_lua, api, id: u8| {
        let game = api.game_mut("show_message")?;
        game.show_message(id, 0);
        Ok(())
    });

    // `message_choice()`: the game's `g_menu_choice_id` byte. Bit `0x80` is
    // set while a message owns the frame; bit 0 is the yes/no answer once it
    // has been confirmed (0 = Yes, 1 = No), exactly the byte the reference's
    // scripts read with `cmpb 5`.
    methods.add_method_mut("message_choice", |_lua, api, ()| {
        let game = api.game_mut("message_choice")?;
        Ok(i64::from(game.message.menu_choice_id()))
    });
}

/// Whether a successful use of an item of `kind` closes the inventory
/// afterwards: the key, special, chemical and book dispatch slots set the
/// original's `DAT_00ae9f13` bit 0x80, which the main menu's case-1 test
/// turns into a full menu exit. (Books only reach here as the red book, the
/// only usable one.)
fn closes_inventory(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Chemical | ItemKind::Special | ItemKind::Key | ItemKind::Book
    )
}

/// Remove one item from `slot`, compacting the inventory.
fn consume_slot(game: &mut GameState, slot: usize) {
    // A stack picked up with quantity 0 (the square and hex cranks, the
    // lighter, the lockpick) is never used up: like `remove_item`, only a
    // counted stack is decremented and emptied. The square crank turns
    // 301's pool twice, and there is one crank.
    if let Some(stack) = game.inventory.get_mut(slot)
        && stack.quantity > 0
    {
        stack.quantity -= 1;
        if stack.quantity == 0 {
            stack.id = 0;
        }
    }
    game.rebuild_slots();
    if game.equipped.is_some_and(|item| !game.has_item(item)) {
        game.set_equipped(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_inventory_follows_the_use_dispatch() {
        // Key, special, chemical and book slots set the close bit; heal,
        // ammo and weapons do not.
        let defs = crate::item_catalog::ItemCatalog::embedded();
        let closes = |item: u8| closes_inventory(defs.get(item).kind);
        assert!(closes(0x33)); // sword key
        assert!(closes(0x1D)); // crank (special)
        assert!(closes(0x15)); // UMB No.2 (chemical)
        assert!(closes(0x3E)); // red book
        assert!(!closes(0x44)); // green herb
        assert!(!closes(0x02)); // beretta
        assert!(!closes(0x0B)); // handgun ammo
    }

    #[test]
    fn consume_slot_keeps_an_uncounted_stack() {
        // The square crank (picked up with quantity 0) survives its use;
        // a counted stack is decremented and emptied at zero.
        let mut game = GameState::default();
        game.add_item(0x1D, 0);
        game.add_item(0x3D, 1);
        consume_slot(&mut game, 0);
        assert!(game.has_item(0x1D));
        consume_slot(&mut game, 1);
        assert!(!game.has_item(0x3D));
    }
}

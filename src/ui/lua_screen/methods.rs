//! The Lua method registrations, split out of the parent module (#266).
//! mlua turns each registered closure into generic glue that is costly to
//! compile, and moving these closures' spans recompiles all of it. In their
//! own file, edits to the parent module no longer move them.

use super::*;

impl UserData for Api<'_> {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ---- queries -------------------------------------------------
        methods.add_method("mode", |_lua, api, ()| {
            Ok(match api.host.mode {
                Mode::Load => "load",
                Mode::Save { .. } => "save",
                Mode::Screen { .. } => "screen",
            })
        });

        methods.add_method("save_slots", |lua, api, ()| {
            let table = lua.create_table()?;
            for (index, slot) in save::scan_slots(&api.host.save_dir).iter().enumerate() {
                table.set(index + 1, slot.is_some())?;
            }
            Ok(table)
        });

        methods.add_method("ticks", |_lua, api, ()| Ok(api.host.ticks));
        // The eight numbered slots: `false` for an empty one, else
        // `{character, saves, stage, room}`.
        methods.add_method("slot_info", |lua, api, ()| {
            let table = lua.create_table()?;
            for (index, slot) in save::scan_slots(&api.host.save_dir).iter().enumerate() {
                match slot {
                    Some(info) => table.set(index + 1, slot_table(lua, info)?)?,
                    None => table.set(index + 1, false)?,
                }
            }
            Ok(table)
        });

        // `flag(bank, bit)`: one bit of the paused session's flag banks;
        // false outside a session.
        methods.add_method("flag", |_lua, api, (bank, bit): (usize, u8)| {
            Ok(api
                .game()
                .and_then(|game| game.flags.get(bank))
                .is_some_and(|flags| flags.bit(bit)))
        });

        // `room()`: the paused session's stage, room, camera cut and player
        // digit (0 Chris, 1 Jill); nil outside a session.
        methods.add_method("room", |_lua, api, ()| {
            Ok(match api.game() {
                Some(game) => (
                    Some(game.id.stage),
                    Some(game.id.room),
                    Some(game.camera.current_cut as i64),
                    Some(game.id.player_flag),
                ),
                None => (None, None, None, None),
            })
        });

        // `pack_bytes(entry)`: a pack entry's raw bytes, nil when absent.
        methods.add_method("pack_bytes", |lua, api, entry: String| {
            match api.cx.pack.read(&entry) {
                Ok(bytes) => {
                    budget::check_len(bytes.len(), budget::MAX_SCREEN_IMAGE_BYTES, "pack entry")
                        .map_err(mlua::Error::external)?;
                    Ok(Some(lua.create_string(&bytes)?))
                }
                Err(_) => Ok(None),
            }
        });

        methods.add_method("has_font", |_lua, api, ()| Ok(api.cx.font.is_some()));
        methods.add_method("has_text", |_lua, api, ()| Ok(api.cx.text.is_some()));
        // The session's selected language tag (`en-US`, `ja-JP`, ...); nil
        // without a text table.
        methods.add_method("language", |_lua, api, ()| {
            Ok(api.cx.text.map(|text| text.selected_language().to_string()))
        });

        // The font-encoded bytes of the text table's `key`; nil without a
        // text table. Draw them with `api:glyphs`.
        methods.add_method("ui_bytes", |lua, api, key: String| {
            api.cx
                .text
                .map(|text| lua.create_string(text.ui(&key)))
                .transpose()
        });

        // The font-encoded name of item `id`; nil without a text table or
        // a name.
        // `item_name_bytes(id, examined)`: with `examined`, the menu's name:
        // an unchecked item whose class the session has not examined yet
        // keeps its unchecked name.
        methods.add_method(
            "item_name_bytes",
            |lua, api, (id, examined): (u16, Option<bool>)| {
                let Some(text) = api.cx.text else {
                    return Ok(None);
                };
                let name = match (examined.unwrap_or(false), u8::try_from(id), api.game()) {
                    (true, Ok(item), Some(game)) => {
                        crate::message::menu_item_name(text, item, &game.examined_flags())
                    }
                    _ => text.item_name(id),
                };
                name.map(|name| lua.create_string(name)).transpose()
            },
        );

        // `font_margin()`: the session font's left text margin.
        methods.add_method("font_margin", |_lua, api, ()| {
            Ok(api.cx.font.map_or(0, |font| font.metrics.left_margin))
        });

        // `glyph_width()`: the session font's fixed glyph advance, the cell
        // the original's text grid steps by: 14 for the Japanese `FONT.TIM`,
        // 8 for the Western sheet (the USA `fontus.tim` pitch). Nil without
        // a font. Screens with per-release column tables pick theirs by it,
        // as the reference's `GetAssetVersion()` picks font and columns
        // together.
        methods.add_method("glyph_width", |_lua, api, ()| {
            Ok(api.cx.font.map(|font| font.metrics.glyph_w))
        });

        // `message_active()`: whether the game's message window is up.
        methods.add_method("message_active", |_lua, api, ()| {
            Ok(api.game().is_some_and(|game| game.message.active))
        });

        // `examine_spin(pitch, yaw, roll, sx, sy, sz)`: the turntable pose
        // after pre-multiplying the spin, re-extracted the reference way.
        methods.add_method(
            "examine_spin",
            |_lua, _api, (pitch, yaw, roll, sx, sy, sz): (i32, i32, i32, i32, i32, i32)| {
                let [pitch, yaw, roll] = crate::examine::spin([pitch, yaw, roll], [sx, sy, sz]);
                Ok((pitch, yaw, roll))
            },
        );

        // `examine_check(item, pitch, yaw, roll, zoom_spin, quantity)`: the
        // examine check, as `kind, id, mark, select`: `"description"`,
        // `"zoom"` or `"message"` with its global message id, whether it
        // marks the item examined and the item `\i` names.
        methods.add_method(
            "examine_check",
            |_lua, api, (item, pitch, yaw, roll, zoom_spin, quantity): (u8, i32, i32, i32, i32, u8)| {
                let item13_message = api.game().is_some_and(|game| {
                    game.flag_test(crate::game::BANK_SCENARIO, 0x13, false)
                        || game.flag_test(crate::game::BANK_SCENARIO, 0x00, false)
                });
                let context = crate::examine::ExamineContext {
                    slot_quantity: quantity,
                    item13_message,
                };
                Ok(
                    match crate::examine::examine_check(item, [pitch, yaw, roll], zoom_spin, context) {
                        crate::examine::ConfirmOutcome::Description => {
                            ("description", None, false, None)
                        }
                        crate::examine::ConfirmOutcome::Zoom => ("zoom", None, false, None),
                        crate::examine::ConfirmOutcome::Message {
                            id,
                            mark_examined,
                            select,
                        } => ("message", Some(id), mark_examined, select),
                    },
                )
            },
        );

        // `class_message(item)`: the global message the item's examine
        // class shows; nil when it has none.
        methods.add_method("class_message", |_lua, _api, item: u8| {
            Ok(crate::examine::class_message(item))
        });

        // `inventory(slot)`: the id and quantity of inventory slot `slot`;
        // `0, 0` past the held run or outside a session.
        methods.add_method("inventory", |_lua, api, slot: usize| {
            let stack = api
                .game()
                .and_then(|game| game.inventory.get(slot).copied())
                .unwrap_or_default();
            Ok((stack.id, stack.quantity))
        });

        // `inventory_capacity()`: the current character's inventory slots;
        // Chris's six outside a session.
        methods.add_method("inventory_capacity", |_lua, api, ()| {
            Ok(api
                .game()
                .map_or(crate::game::INVENTORY_SLOTS_CHRIS, |game| {
                    game.inventory_capacity()
                }))
        });

        // `item_box(slot)`: the id and quantity of item-box slot `slot`;
        // `0, 0` for an out-of-range slot or outside a session.
        methods.add_method("item_box", |_lua, api, slot: usize| {
            let stack = api
                .game()
                .and_then(|game| game.item_box.get(slot).copied())
                .unwrap_or_default();
            Ok((stack.id, stack.quantity))
        });

        // The font-encoded shorthand location label of a save's room.
        methods.add_method("location_bytes", |lua, api, (stage, room): (u8, u8)| {
            api.cx
                .text
                .map(|text| lua.create_string(text.save().location(stage, room)))
                .transpose()
        });

        methods.add_method("saves", |lua, api, ()| {
            let table = lua.create_table()?;
            for (index, entry) in api.host.list.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("path", entry.path.to_string_lossy().to_string())?;
                row.set("name", entry.name.clone())?;
                row.set("base", entry.base.clone())?;
                row.set("loadable", entry.loadable())?;
                row.set("timestamp", entry.timestamp)?;
                row.set("autosave", entry.autosave)?;
                if let Some(info) = entry.info {
                    row.set("character", info.character)?;
                    row.set("saves", info.saves)?;
                    row.set("stage", info.stage)?;
                    row.set("room", info.room)?;
                    if let Some(text) = api.cx.text {
                        let save = text.save();
                        row.set(
                            "character_name",
                            text.decode(save.character_name(usize::from(info.character & 1))),
                        )?;
                        row.set(
                            "location",
                            text.decode(save.location(info.stage, info.room)),
                        )?;
                    }
                }
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_method(
            "string",
            |_lua, api, (key, tag): (String, Option<String>)| {
                Ok(api.cx.text.and_then(|text| match &tag {
                    Some(tag) => text.text_in(tag, &key),
                    None => text.text(&key),
                }))
            },
        );

        // The block a save screen would write, as `{character, saves, stage,
        // room}`; `nil` outside save mode.
        methods.add_method("pending", |lua, api, ()| {
            let Mode::Save { file, .. } = &api.host.mode else {
                return Ok(None);
            };
            slot_table(lua, &save::SaveSlotInfo::from_file(file)).map(Some)
        });

        // The local wall-clock time as `{year, month, day, hour, minute,
        // second, millisecond}`; `nil` when the platform cannot say.
        methods.add_method("local_time", |lua, _api, ()| {
            let Some(time) = crate::local_time::now() else {
                return Ok(None);
            };
            let table = lua.create_table()?;
            table.set("year", time.year)?;
            table.set("month", time.month)?;
            table.set("day", time.day)?;
            table.set("hour", time.hour)?;
            table.set("minute", time.minute)?;
            table.set("second", time.second)?;
            table.set("millisecond", time.millisecond)?;
            Ok(Some(table))
        });

        methods.add_method("character_name", |_lua, api, index: usize| {
            Ok(api
                .cx
                .text
                .map(|text| text.decode(text.save().character_name(index & 1))))
        });

        methods.add_method_mut(
            "text_width",
            |_lua, api, (text, tight): (String, Option<bool>)| {
                let Some(font) = api.cx.font else {
                    api.host.note_missing_font();
                    return Ok(0);
                };
                let mut bytes = Vec::new();
                charset::encode(&text, api.host.script, &mut bytes);
                Ok(font.advance(tight.unwrap_or(api.host.tight), &bytes))
            },
        );

        methods.add_method_mut("classic_rebirth_width", |_lua, api, text: String| {
            let Some(font) = api.cx.font else {
                api.host.note_missing_font();
                return Ok(0);
            };
            let mut bytes = Vec::new();
            charset::encode(&text, api.host.script, &mut bytes);
            Ok(font.classic_rebirth_width(&bytes))
        });

        // ---- actions -------------------------------------------------
        methods.add_method("sound", |_lua, api, name: String| {
            let Some(cue) = cue_of(&name) else {
                eprintln!("[lua] screen: unknown sound `{name}`");
                return Ok(());
            };
            api.cx.play_cue(cue);
            Ok(())
        });

        // A current-room bank sound (`play_sfx(2, slot)`); the session
        // resolves the slot through the loaded room's sound table.
        methods.add_method("room_sound", |_lua, api, slot: u8| {
            api.cx.play_cue(UiCue::RoomSound(slot));
            Ok(())
        });

        methods.add_method_mut("save", |_lua, api, name: String| {
            let Some(path) = save::file_path(&api.host.save_dir, &name) else {
                return Ok((
                    false,
                    Some(format!("`{name}` is not a valid save file name")),
                ));
            };
            let Mode::Save { file, ink_ribbon } = &api.host.mode else {
                return Ok((false, Some("the screen is not in save mode".to_string())));
            };
            let mut file = file.clone();
            if *ink_ribbon {
                consume_snapshot_ribbon(&mut file);
            }
            match save::save_to_path(&path, &file) {
                Ok(()) => {
                    api.host.outcome = Some(SaveOutcome {
                        slot: None,
                        ink_ribbon: *ink_ribbon,
                    });
                    api.host.rescan();
                    Ok((true, None))
                }
                Err(err) => Ok((false, Some(format!("{err:#}")))),
            }
        });

        // Write the pending block to numbered slot `slot` (0-7), consuming
        // an ink ribbon when the opener asked: `true, info` with the
        // written slot's `{character, saves, stage, room}`, or `false, err`.
        methods.add_method_mut("save_slot", |lua, api, slot: usize| {
            let Mode::Save { file, ink_ribbon } = &api.host.mode else {
                return Ok((
                    false,
                    Value::String(lua.create_string("the screen is not in save mode")?),
                ));
            };
            if slot >= save::SAVE_SLOT_COUNT {
                return Ok((
                    false,
                    Value::String(lua.create_string(format!("no save slot {slot}"))?),
                ));
            }
            let mut file = file.clone();
            if *ink_ribbon {
                consume_snapshot_ribbon(&mut file);
            }
            match save::save(&api.host.save_dir, slot, &file) {
                Ok(()) => {
                    api.host.outcome = Some(SaveOutcome {
                        slot: Some(slot),
                        ink_ribbon: *ink_ribbon,
                    });
                    api.host.rescan();
                    let info = save::SaveSlotInfo::from_file(&file);
                    Ok((true, Value::Table(slot_table(lua, &info)?)))
                }
                Err(err) => Ok((false, Value::String(lua.create_string(format!("{err:#}"))?))),
            }
        });

        methods.add_method_mut("remove", |_lua, api, name: String| {
            let Some(path) = save::file_path(&api.host.save_dir, &name) else {
                return Ok((
                    false,
                    Some(format!("`{name}` is not a valid save file name")),
                ));
            };
            match save::remove_file(&path) {
                Ok(()) => {
                    api.host.rescan();
                    Ok((true, None))
                }
                Err(err) => Ok((false, Some(format!("{err:#}")))),
            }
        });

        methods.add_method_mut("load", |_lua, api, name: String| {
            if !matches!(api.host.mode, Mode::Load) {
                return Ok((false, Some("the screen is not in load mode".to_string())));
            }
            let Some(path) = save::file_path(&api.host.save_dir, &name) else {
                return Ok((
                    false,
                    Some(format!("`{name}` is not a valid save file name")),
                ));
            };
            if save::load_file(&path).is_err() {
                return Ok((false, Some(format!("`{name}` is not a readable save file"))));
            }
            api.host.load = Some(path);
            Ok((true, None))
        });

        methods.add_method_mut(
            "exit",
            |_lua, api, (action, arg): (Option<String>, Option<i64>)| {
                if let Some(name) = action {
                    api.host.action = Some(action_of(&name, arg)?);
                }
                api.host.exited = true;
                Ok(())
            },
        );

        methods.add_method_mut("set", |_lua, api, (key, value): (String, Value)| {
            let value = match value {
                Value::Nil => {
                    api.host.values.remove(&key);
                    return Ok(());
                }
                Value::Boolean(flag) => i64::from(flag),
                Value::Integer(value) => value,
                other => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "api:set({key}) takes an integer or boolean, got {}",
                        other.type_name()
                    )));
                }
            };
            // Overwriting a published key never grows the table.
            if !api.host.values.contains_key(&key) {
                budget::check_len(api.host.values.len() + 1, 64, "screen values")
                    .map_err(|err| mlua::Error::RuntimeError(format!("{err:#}")))?;
            }
            api.host.values.insert(key, value);
            Ok(())
        });

        methods.add_method_mut(
            "overlay",
            |_lua, api, (colour, alpha): (Option<String>, Option<i64>)| {
                let alpha = alpha.unwrap_or(0).clamp(0, 255) as u8;
                let overlay = match colour.as_deref() {
                    None => None,
                    Some("black") => Some((FadeColor::Black, alpha)),
                    Some("white") => Some((FadeColor::White, alpha)),
                    Some(other) => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "unknown overlay colour `{other}` (black, white or nil)"
                        )));
                    }
                };
                api.host.fade = match overlay {
                    Some((FadeColor::Black, alpha)) => alpha,
                    _ => 0,
                };
                api.host.overlay = Some(overlay);
                Ok(())
            },
        );

        methods.add_method("log", |_lua, _api, text: mlua::String| {
            eprintln!("[lua] {}", text.to_string_lossy());
            Ok(())
        });

        // ---- drawing -------------------------------------------------
        methods.add_method_mut(
            "background",
            |_lua, api, (entry, opts): (String, Option<Table>)| {
                let opts = options(api.host, opts.as_ref())?;
                let Api {
                    host,
                    cx,
                    framebuffer,
                    ..
                } = api;
                let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                let (image, hd) = host
                    .entry_art(cx.pack, &entry, framebuffer, &opts)
                    .map_err(mlua::Error::external)?;
                let (width, height) = image.size();
                let full = [0, 0, framebuffer.width as i32, framebuffer.height as i32];
                if let Decoded::Rgba(image) = image {
                    let blit = width == framebuffer.width
                        && height == framebuffer.height
                        && opts.tint == Tint::White
                        && opts.brightness == 2;
                    let tint = if blit { Tint::White } else { opts.tint };
                    let draw = |framebuffer: &mut Framebuffer| {
                        if blit {
                            framebuffer.blit(image);
                        } else {
                            framebuffer.draw_background(image, [0, 0], tint.rgb());
                        }
                    };
                    match hd {
                        Some(hd) => {
                            // The blit copies the image one to one; the
                            // stretch covers the logical quad.
                            let (corners, uv) = if blit {
                                draw::sprite_quad(&hd, full, full, [false; 2])
                            } else {
                                draw::background_quad(&hd, image, framebuffer, [0, 0])
                            };
                            let colour = tint.rgb().map(|channel| f32::from(channel) / 255.0);
                            // A background ignores the clip, so its quad must
                            // too; the blit punches no hole of its own.
                            let clip = framebuffer.clip.take();
                            framebuffer.draw_replaced(hd, corners, uv, colour, |framebuffer| {
                                draw(framebuffer);
                                if blit {
                                    framebuffer.punch_rect(full);
                                }
                            });
                            framebuffer.clip = clip;
                        }
                        None => draw(framebuffer),
                    }
                    return Ok(());
                }
                draw_image(framebuffer, image, hd, None, full, &opts);
                Ok(())
            },
        );

        methods.add_method_mut(
            "rect",
            |_lua, api, (x, y, w, h, colour, alpha): (i32, i32, i32, i32, Value, Option<i64>)| {
                let (x, y) = (coord(x), coord(y));
                let (w, h) = (coord(w), coord(h));
                let rgb = colour_rgb(&colour)?;
                let alpha = alpha.unwrap_or(255).clamp(0, 255) as u8;
                let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                if alpha == 255 {
                    framebuffer.fill_rect([x, y, w, h], [rgb[0], rgb[1], rgb[2], 255]);
                } else {
                    framebuffer.blend_rect([x, y, w, h], rgb, alpha);
                }
                Ok(())
            },
        );

        methods.add_method_mut(
            "text",
            |_lua, api, (x, y, text, opts): (i32, i32, String, Option<Table>)| {
                let (x, y) = (coord(x), coord(y));
                let opts = options(api.host, opts.as_ref())?;
                // The framebuffer is taken first so a text call outside
                // `draw` is the error the API documents.
                let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                // A pack without a font draws no text anywhere in the
                // engine; the screen keeps running rather than dying.
                let Some(font) = api.cx.font else {
                    api.host.note_missing_font();
                    return Ok(());
                };
                let mut bytes = Vec::new();
                charset::encode(&text, api.host.script, &mut bytes);
                font.draw_text(
                    framebuffer,
                    x,
                    y,
                    opts.tint,
                    opts.brightness,
                    &bytes,
                    opts.tight,
                );
                Ok(())
            },
        );

        methods.add_method_mut(
            "sprite",
            |_lua, api, (entry, x, y, opts): (String, i32, i32, Option<Table>)| {
                let (x, y) = (coord(x), coord(y));
                let opts = options(api.host, opts.as_ref())?;
                let Api {
                    host,
                    cx,
                    framebuffer,
                    ..
                } = api;
                let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                let (image, hd) = host
                    .entry_art(cx.pack, &entry, framebuffer, &opts)
                    .map_err(mlua::Error::external)?;
                let (width, height) = image.size();
                draw_image(
                    framebuffer,
                    image,
                    hd,
                    None,
                    [x, y, width as i32, height as i32],
                    &opts,
                );
                Ok(())
            },
        );

        #[allow(clippy::too_many_arguments)]
        methods.add_method_mut(
            "sprite_region",
            |_lua,
             api,
             (entry, sx, sy, sw, sh, dx, dy, dw, dh, opts): (
                String,
                i32,
                i32,
                i32,
                i32,
                i32,
                i32,
                i32,
                i32,
                Option<Table>,
            )| {
                let (sx, sy, sw, sh) = (coord(sx), coord(sy), coord(sw), coord(sh));
                let (dx, dy, dw, dh) = (coord(dx), coord(dy), coord(dw), coord(dh));
                let opts = options(api.host, opts.as_ref())?;
                let Api {
                    host,
                    cx,
                    framebuffer,
                    ..
                } = api;
                let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                let (image, hd) = host
                    .entry_art(cx.pack, &entry, framebuffer, &opts)
                    .map_err(mlua::Error::external)?;
                draw_image(
                    framebuffer,
                    image,
                    hd,
                    Some([sx, sy, sw, sh]),
                    [dx, dy, dw, dh],
                    &opts,
                );
                Ok(())
            },
        );

        // `wrap(bytes, max_width, opts)`: font-encoded `bytes` broken at
        // spaces into rows no wider than `max_width` pixels (`opts.tight`
        // picks the metrics). Without a font the text is one row.
        methods.add_method(
            "wrap",
            |lua, api, (bytes, max_width, opts): (mlua::String, i32, Option<Table>)| {
                let opts = options(api.host, opts.as_ref())?;
                let bytes = bytes.as_bytes();
                let table = lua.create_table()?;
                match api.cx.font {
                    Some(font) => {
                        for (index, row) in
                            crate::font::wrap_pixels(&bytes, max_width, font, opts.tight)
                                .into_iter()
                                .enumerate()
                        {
                            table.set(index + 1, lua.create_string(row)?)?;
                        }
                    }
                    None => table.set(1, lua.create_string(&*bytes)?)?,
                }
                Ok(table)
            },
        );

        // `glyphs(x, y, bytes, opts)`: draw already font-encoded bytes (from
        // `api:ui_bytes`, `api:location_bytes` or assembled by the script).
        methods.add_method_mut(
            "glyphs",
            |_lua, api, (x, y, bytes, opts_table): (i32, i32, mlua::String, Option<Table>)| {
                let (x, y) = (coord(x), coord(y));
                let opts = options(api.host, opts_table.as_ref())?;
                let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                let Some(font) = api.cx.font else {
                    api.host.note_missing_font();
                    return Ok(());
                };
                // `fit` condenses a row wider than that many pixels.
                let fit = match &opts_table {
                    Some(table) => table.get::<Option<i32>>("fit")?.unwrap_or(0),
                    None => 0,
                };
                font.draw_text_fit(
                    framebuffer,
                    x,
                    y,
                    opts.tint,
                    opts.brightness,
                    &bytes.as_bytes(),
                    opts.tight,
                    fit,
                );
                Ok(())
            },
        );

        // `glyphs_width(bytes[, opts])`: the pixel advance `api:glyphs`
        // draws already font-encoded bytes at with the same options (the
        // session's metrics unless `opts.tight` says otherwise). 0 without
        // a font.
        methods.add_method_mut(
            "glyphs_width",
            |_lua, api, (bytes, opts_table): (mlua::String, Option<Table>)| {
                let opts = options(api.host, opts_table.as_ref())?;
                let Some(font) = api.cx.font else {
                    api.host.note_missing_font();
                    return Ok(0);
                };
                Ok(font.advance(opts.tight, &bytes.as_bytes()))
            },
        );

        // `text_8x14(x, y, bytes[, opts])`: raw bytes through the original's
        // unshadowed `PrintText8x14` grid, in `opts.colour`.
        methods.add_method_mut(
            "text_8x14",
            |_lua, api, (x, y, bytes, opts): (i32, i32, mlua::String, Option<Table>)| {
                let (x, y) = (coord(x), coord(y));
                let opts = options(api.host, opts.as_ref())?;
                let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
                let Some(font) = api.cx.font else {
                    api.host.note_missing_font();
                    return Ok(());
                };
                font.draw_text_8x14(framebuffer, x, y, opts.tint, &bytes.as_bytes());
                Ok(())
            },
        );

        methods.add_method_mut("image_size", |_lua, api, entry: String| {
            let image = api
                .host
                .image(api.cx.pack, &entry)
                .map_err(mlua::Error::external)?;
            Ok(image.size())
        });

        methods.add_method_mut("fade", |_lua, api, alpha: i64| {
            api.host.fade = alpha.clamp(0, 255) as u8;
            api.host.overlay = None;
            Ok(())
        });

        draw::add_methods(methods);
        game_api::add_methods(methods);
    }
}

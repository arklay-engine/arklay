//! The player bot's part of the driver API: `bot_start`, `bot_run` and the
//! calls that read the attached bot.

use super::*;

/// What `bot_start` and `bot_run` take to attach the bot.
struct BotStart {
    risk: f64,
    goal_room: Option<String>,
    seed: Option<u32>,
    params: Vec<(String, String)>,
    invincible: bool,
    log: bool,
    /// A memory file an earlier run wrote with `bot_memory`, to start from.
    memory: Option<String>,
    /// Notes to put in the bot's memory before its first tick.
    remember: Vec<(String, String)>,
}

impl BotStart {
    /// Read the attach options of `function`'s table; the caller reads its
    /// own options and then calls [`Options::finish`].
    fn read(options: &mut Options<'_>) -> mlua::Result<Self> {
        let function = options.function;
        let risk: Option<f64> = options.get("risk")?;
        let goal_room: Option<String> = options.get("goal_room")?;
        let seed: Option<u32> = options.get("seed")?;
        let params: Option<Table> = options.get("params")?;
        let invincible: Option<bool> = options.get("invincible")?;
        let log: Option<bool> = options.get("log")?;
        let memory: Option<String> = options.get("memory")?;
        let remember: Option<Table> = options.get("remember")?;
        let risk = risk.unwrap_or(DEFAULT_RISK);
        if !(0.0..=1.0).contains(&risk) {
            return Err(mlua::Error::RuntimeError(format!(
                "{function}: risk {risk} is not within 0..=1"
            )));
        }
        // Sorted by name, so the run does not depend on the table's order;
        // the pack's parameter file checks the names and values.
        let strings = |option: &str, table: Option<Table>| -> mlua::Result<Vec<(String, String)>> {
            let mut pairs = Vec::new();
            let Some(table) = table else {
                return Ok(pairs);
            };
            for pair in table.pairs::<String, Value>() {
                let (name, value) = pair.map_err(|err| {
                    mlua::Error::RuntimeError(format!("{function}: option `{option}`: {err}"))
                })?;
                let value = match value {
                    Value::String(text) => text.to_string_lossy(),
                    Value::Boolean(on) => if on { "yes" } else { "no" }.to_string(),
                    Value::Integer(number) => number.to_string(),
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "{function}: `{name}` of `{option}` is a {}, not a string",
                            other.type_name()
                        )));
                    }
                };
                pairs.push((name, value));
            }
            pairs.sort();
            Ok(pairs)
        };
        Ok(Self {
            risk,
            goal_room,
            seed,
            params: strings("params", params)?,
            invincible: invincible.unwrap_or(false),
            log: log.unwrap_or(true),
            memory,
            remember: strings("remember", remember)?,
        })
    }
}

impl Driver {
    /// Attach the open pack's bot to the running session (`bot_start`).
    fn bot_start(&mut self, start: &BotStart) -> mlua::Result<()> {
        let app = self.app()?;
        if !matches!(app.mode, Mode::Play(_)) {
            return Err(mlua::Error::RuntimeError(
                "bot_start: no game session is running: call new_game, load_room or load_save \
                 first"
                    .into(),
            ));
        }
        bot::set_log(start.log);
        // The bot skips scenes with the buttons a player would, and a scene
        // it skips runs to its end in one update, so where control returns
        // never depends on the machine's load.
        app.enable_skips();
        app.attach_bot(
            start.risk,
            start.goal_room.as_deref(),
            start.seed,
            &start.params,
            start.invincible,
        )
        .map_err(engine_error)?;
        if let Some(path) = &start.memory {
            let json = std::fs::read_to_string(path)
                .with_context(|| format!("reading the bot memory {path}"))
                .and_then(|text| {
                    serde_json::from_str(&text)
                        .with_context(|| format!("parsing the bot memory {path}"))
                })
                .map_err(engine_error)?;
            if let Some(mut bot) = app.bot.take() {
                bot.runtime = bot.runtime.with_memory(json).map_err(engine_error)?;
                app.bot = Some(bot);
            }
        }
        if let Some(runtime) = app.bot() {
            for (key, value) in &start.remember {
                runtime.remember(key, value);
            }
        }
        app.unbounded_cutscene_skip = true;
        if let Mode::Play(session) = &mut app.mode {
            session.cutscene_skip_burst = None;
        }
        self.bot_started = Some(Instant::now());
        Ok(())
    }

    /// The attached bot.
    fn bot(&mut self) -> mlua::Result<&bot::BotRuntime> {
        self.app()?.bot().ok_or_else(|| {
            mlua::Error::RuntimeError("no bot is attached: call bot_start or bot_run first".into())
        })
    }

    /// The attached bot's report (`bot_stats`), as `bot_run`'s `stats` writes it.
    fn bot_report(&mut self, outcome: &str) -> mlua::Result<serde_json::Value> {
        let elapsed = self
            .bot_started
            .map_or(0.0, |started| started.elapsed().as_secs_f64());
        let facts = self.app()?.bot_run_facts();
        Ok(self.bot()?.stats_json(outcome, elapsed, facts))
    }

    /// How the attached bot's run stands: `running`, `finished` (the bot
    /// ended it, with its reason) or `death` (the player died with no save
    /// to load).
    fn bot_outcome(&mut self) -> mlua::Result<&'static str> {
        self.bot()?;
        Ok(match self.app()?.bot_run_state() {
            bot::RunState::Running => "running",
            bot::RunState::Finished => "finished",
            bot::RunState::Stranded => "death",
        })
    }

    /// Tick until the attached bot's run ends or `max_ticks` more ticks have
    /// run (`bot_run`, [`bot::play`]): the outcome, `max_ticks` when the
    /// budget ran out. A game that quits fails the run ([`Self::tick`]).
    fn bot_play(&mut self, max_ticks: u64) -> mlua::Result<&'static str> {
        self.bot()?;
        let outcome = bot::play(
            self,
            max_ticks,
            |driver| Ok(driver.app()?.bot_run_state()),
            |driver| driver.tick().map(|()| AppFlow::Continue),
        )?;
        Ok(outcome.name())
    }
}

/// Register the bot functions.
pub(super) fn install(lua: &Lua, driver: &Shared) -> mlua::Result<()> {
    let globals = lua.globals();
    let shared = Rc::clone(driver);
    globals.set(
        "bot_start",
        lua.create_function(move |_lua, table: Option<Table>| {
            with(&shared, |driver| {
                let mut options = Options::new("bot_start", table);
                let start = BotStart::read(&mut options)?;
                options.finish()?;
                driver.bot_start(&start)
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_finished",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| {
                let outcome = driver.bot_outcome()?;
                let reason = match outcome {
                    "running" => None,
                    "death" => Some("dead, no save".to_string()),
                    _ => driver.bot()?.finish_reason(),
                };
                Ok((outcome != "running", reason))
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_stats",
        lua.create_function(move |lua, ()| {
            let report = with(&shared, |driver| {
                let outcome = driver.bot_outcome()?;
                driver.bot_report(outcome)
            })?;
            from_json(lua, &report)
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_memory",
        lua.create_function(move |_lua, path: String| {
            with(&shared, |driver| {
                let json = driver.bot()?.memory_json().map_err(engine_error)?;
                let text = serde_json::to_string(&json).map_err(mlua::Error::external)?;
                std::fs::write(&path, text).map_err(|err| {
                    mlua::Error::RuntimeError(format!("bot_memory: writing {path}: {err}"))
                })
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_story",
        lua.create_function(move |_lua, (bank, bit): (usize, u8)| {
            with(&shared, |driver| Ok(driver.bot()?.story_seen(bank, bit)))
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_run",
        lua.create_function(move |lua, table: Option<Table>| {
            let report = with(&shared, |driver| {
                let mut options = Options::new("bot_run", table);
                let start = BotStart::read(&mut options)?;
                let max_ticks: Option<u64> = options.get("max_ticks")?;
                let stats: Option<String> = options.get("stats")?;
                options.finish()?;
                driver.bot_start(&start)?;
                let outcome = driver.bot_play(max_ticks.unwrap_or(DEFAULT_MAX_TICKS))?;
                let report = driver.bot_report(outcome)?;
                if let Some(path) = stats {
                    let text =
                        serde_json::to_string_pretty(&report).map_err(mlua::Error::external)?;
                    std::fs::write(&path, text).map_err(|err| {
                        mlua::Error::RuntimeError(format!("bot_run: writing {path}: {err}"))
                    })?;
                }
                Ok(report)
            })?;
            from_json(lua, &report)
        })?,
    )?;
    Ok(())
}

/// A JSON value as the Lua value [`to_json`] would encode back to it: an
/// array as a sequence, an object as a table, `null` as nil.
fn from_json(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(on) => Value::Boolean(*on),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => Value::Integer(integer),
            None => Value::Number(number.as_f64().unwrap_or(f64::NAN)),
        },
        serde_json::Value::String(text) => Value::String(lua.create_string(text)?),
        serde_json::Value::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(fields) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (key, item) in fields {
                table.raw_set(key.as_str(), from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
    })
}

//! MODS > AVAILABLE: the hosted mod catalog ([`crate::catalog`]) as a list
//! or a grid of pictures, filtered and sorted, with a details screen that
//! installs, updates and removes a mod.
//!
//! The index and the thumbnails are fetched on worker threads so the
//! launcher never waits on the network; a download runs as a job with the
//! progress screen, like an engine update, and lands in the primary data
//! root's `mods` folder, where discovery picks it up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::catalog::{self, Catalog, CatalogMod};
use crate::convert::Game;
use crate::gui::canvas::Picture;
use crate::gui::dom::Value;
use crate::gui::dom::value::ImageValue;
use crate::logging;
use crate::paths;

use super::{Launcher, ProgressState, Screen, mod_enabled, pack_for, set_mod_enabled};

/// How the catalog shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum View {
    #[default]
    Grid,
    List,
}

/// The catalog's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Sort {
    /// As published.
    #[default]
    Catalog,
    Name,
    Category,
    Size,
}

impl Sort {
    const ALL: [Self; 4] = [Self::Catalog, Self::Name, Self::Category, Self::Size];

    fn label(self) -> &'static str {
        match self {
            Self::Catalog => "Featured",
            Self::Name => "Name",
            Self::Category => "Category",
            Self::Size => "Size",
        }
    }
}

/// What a filter keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Filter {
    All,
    Game(String),
    Category(String),
}

impl Filter {
    fn label(&self) -> String {
        match self {
            Self::All => "All".to_string(),
            Self::Game(id) => game_title(id),
            Self::Category(category) => category.clone(),
        }
    }

    fn keeps(&self, entry: &CatalogMod) -> bool {
        match self {
            Self::All => true,
            Self::Game(id) => entry.game.eq_ignore_ascii_case(id),
            Self::Category(category) => entry.category.as_deref() == Some(category.as_str()),
        }
    }
}

/// Where fetching the catalog stands.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
enum Status {
    /// No catalog is configured.
    #[default]
    Unset,
    Loading,
    Ready,
    Failed(String),
}

/// The catalog as the launcher holds it.
#[derive(Default)]
pub(super) struct ModCatalog {
    status: Status,
    location: Option<String>,
    mods: Vec<CatalogMod>,
    fetch_rx: Option<mpsc::Receiver<Result<Catalog, String>>>,
    thumbnails: HashMap<String, Arc<Picture>>,
    thumb_rx: Option<mpsc::Receiver<(String, Option<Picture>)>>,
    pub(super) view: View,
    sort: Sort,
    filter: usize,
    /// The id the details screen shows.
    detail: Option<String>,
    /// When the catalog last loaded, so re-entering MODS refreshes a stale
    /// one.
    loaded: Option<Instant>,
    /// Where a new mod installs and where thumbnails are cached, when not
    /// the primary data root's (tests).
    install_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
}

/// A running mod download.
pub(super) struct ModJob {
    receiver: mpsc::Receiver<Result<(), String>>,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) state: Arc<Mutex<ProgressState>>,
    pub(super) entry: CatalogMod,
    /// Whether it replaces an installed version.
    pub(super) update: bool,
}

/// A catalog this old is fetched again when MODS opens.
const STALE: Duration = Duration::from_secs(15 * 60);

/// What is installed of a catalog mod.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Installed {
    No,
    /// This version, at the path.
    Current(PathBuf),
    /// Another version, at the path.
    Other(String, PathBuf),
    /// The id is installed from a mod folder, which a download must not
    /// shadow (two layers with one id are refused).
    Folder(PathBuf),
}

fn game_title(id: &str) -> String {
    Game::from_id(id).map_or_else(|| id.to_string(), |game| game.title().to_string())
}

/// `bytes` for people: `505 MB`, `1.5 GB`.
pub(super) fn human_size(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    let mb = bytes as f64 / MB;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0)
    } else if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

/// A thumbnail's key: one per mod version, so a refreshed catalog with a
/// new version fetches the new picture.
fn thumb_key(entry: &CatalogMod) -> String {
    format!("{}@{}", entry.id, entry.version)
}

/// A dark 4:3 picture standing in while a thumbnail loads, so the grid does
/// not reflow when it arrives.
pub(super) fn placeholder() -> Value {
    static PICTURE: std::sync::OnceLock<Arc<Picture>> = std::sync::OnceLock::new();
    let picture = PICTURE.get_or_init(|| {
        Arc::new(Picture {
            width: 4,
            height: 3,
            rgba: [0x18, 0x18, 0x18, 0xFF].repeat(12),
        })
    });
    Value::Image(ImageValue {
        colour: picture.clone(),
        grey: None,
    })
}

impl ModCatalog {
    /// The filters the catalog offers: everything, each game when there is
    /// more than one, then each category, in catalog order.
    fn filters(&self) -> Vec<Filter> {
        let mut filters = vec![Filter::All];
        let mut games: Vec<&str> = Vec::new();
        let mut categories: Vec<&str> = Vec::new();
        for entry in &self.mods {
            if !games.contains(&entry.game.as_str()) {
                games.push(&entry.game);
            }
            if let Some(category) = entry.category.as_deref()
                && !categories.contains(&category)
            {
                categories.push(category);
            }
        }
        if games.len() > 1 {
            filters.extend(games.into_iter().map(|game| Filter::Game(game.to_string())));
        }
        filters.extend(
            categories
                .into_iter()
                .map(|category| Filter::Category(category.to_string())),
        );
        filters
    }

    fn filter(&self) -> Filter {
        self.filters()
            .get(self.filter)
            .cloned()
            .unwrap_or(Filter::All)
    }

    /// The shown mods, filtered and sorted, as indices into `mods`.
    fn shown(&self) -> Vec<usize> {
        let filter = self.filter();
        let mut shown: Vec<usize> = (0..self.mods.len())
            .filter(|&index| filter.keeps(&self.mods[index]))
            .collect();
        let key = |index: &usize| &self.mods[*index];
        match self.sort {
            Sort::Catalog => {}
            Sort::Name => shown.sort_by_key(|index| key(index).name.to_lowercase()),
            Sort::Category => shown.sort_by(|a, b| {
                let (a, b) = (key(a), key(b));
                a.category
                    .cmp(&b.category)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }),
            Sort::Size => shown.sort_by_key(|index| std::cmp::Reverse(key(index).size)),
        }
        shown
    }

    fn by_id(&self, id: &str) -> Option<&CatalogMod> {
        self.mods.iter().find(|entry| entry.id == id)
    }

    fn picture(&self, entry: &CatalogMod) -> Value {
        match self.thumbnails.get(&thumb_key(entry)) {
            Some(picture) => Value::Image(ImageValue {
                colour: picture.clone(),
                grey: None,
            }),
            None => placeholder(),
        }
    }
}

impl Launcher {
    /// Fetch the catalog when it is not loaded (or is stale), unless a fetch
    /// is under way.
    pub(super) fn refresh_catalog(&mut self, force: bool) {
        let location = catalog::location(self.config.mods.catalog.as_deref());
        let state = &mut self.catalog;
        if state.fetch_rx.is_some() {
            return;
        }
        let fresh = state.status == Status::Ready
            && state.location == location
            && state.loaded.is_some_and(|loaded| loaded.elapsed() < STALE);
        if fresh && !force {
            return;
        }
        state.location = location.clone();
        let Some(location) = location else {
            state.status = Status::Unset;
            return;
        };
        if state.status != Status::Ready {
            state.status = Status::Loading;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(catalog::fetch(&location).map_err(|err| format!("{err:#}")));
        });
        state.fetch_rx = Some(receiver);
    }

    /// Collect a finished fetch and arrived thumbnails; returns whether
    /// anything changed.
    pub(super) fn poll_catalog(&mut self) -> bool {
        let mut changed = false;
        if let Some(receiver) = &self.catalog.fetch_rx {
            let received = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("the catalog fetch stopped".into())),
            };
            if let Some(result) = received {
                self.catalog.fetch_rx = None;
                changed = true;
                match result {
                    Ok(fetched) => {
                        self.catalog.mods = fetched.mods;
                        self.catalog.status = Status::Ready;
                        self.catalog.loaded = Some(Instant::now());
                        self.catalog.filter = self
                            .catalog
                            .filter
                            .min(self.catalog.filters().len().saturating_sub(1));
                        self.load_thumbnails();
                    }
                    Err(err) => {
                        logging::write(&format!("mod catalog: {err}"));
                        self.catalog.status = Status::Failed(err);
                    }
                }
            }
        }
        if let Some(receiver) = &self.catalog.thumb_rx {
            loop {
                match receiver.try_recv() {
                    Ok((id, Some(picture))) => {
                        self.catalog.thumbnails.insert(id, Arc::new(picture));
                        changed = true;
                    }
                    Ok((_, None)) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.catalog.thumb_rx = None;
                        break;
                    }
                }
            }
        }
        changed
    }

    /// Block until the catalog and its thumbnails have loaded, for at most
    /// `limit` (a headless capture has no event loop to wait in).
    pub(super) fn wait_for_catalog(&mut self, limit: Duration) {
        let deadline = Instant::now() + limit;
        while (self.catalog.fetch_rx.is_some() || self.catalog.thumb_rx.is_some())
            && Instant::now() < deadline
        {
            self.poll_catalog();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Fetch the thumbnails not yet loaded, one after another on a worker.
    fn load_thumbnails(&mut self) {
        let wanted: Vec<CatalogMod> = self
            .catalog
            .mods
            .iter()
            .filter(|entry| entry.thumbnail.is_some())
            .filter(|entry| !self.catalog.thumbnails.contains_key(&thumb_key(entry)))
            .cloned()
            .collect();
        if wanted.is_empty() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let dir = self
            .catalog
            .cache_dir
            .clone()
            .unwrap_or_else(catalog::cache_dir);
        std::thread::spawn(move || {
            for entry in wanted {
                let picture = catalog::thumbnail(&entry, &dir).unwrap_or_else(|err| {
                    logging::write(&format!("mod catalog: {err:#}"));
                    None
                });
                if sender.send((thumb_key(&entry), picture)).is_err() {
                    return;
                }
            }
        });
        self.catalog.thumb_rx = Some(receiver);
    }

    /// What is installed of `entry`, from the discovered mods.
    fn installed(&self, entry: &CatalogMod) -> Installed {
        let Some(row) = self
            .mods
            .iter()
            .find(|row| row.id.eq_ignore_ascii_case(&entry.id))
        else {
            return Installed::No;
        };
        if row.path.is_dir() {
            Installed::Folder(row.path.clone())
        } else if row.version == entry.version {
            Installed::Current(row.path.clone())
        } else {
            Installed::Other(row.version.clone(), row.path.clone())
        }
    }

    /// `entry`'s status line and whether it needs attention.
    fn mod_status(&self, entry: &CatalogMod) -> (String, bool) {
        if let Some(job) = &self.mod_job
            && job.entry.id == entry.id
        {
            let percent = job
                .state
                .lock()
                .map(|state| (state.overall * 100.0).round() as u32)
                .unwrap_or(0);
            return (format!("Downloading {percent}%"), true);
        }
        match self.installed(entry) {
            Installed::No => ("Not installed".to_string(), false),
            Installed::Current(_) => ("Installed".to_string(), false),
            Installed::Other(version, _) if version.is_empty() => {
                ("Update available".to_string(), true)
            }
            Installed::Other(version, _) => {
                (format!("Update available ({version} installed)"), true)
            }
            Installed::Folder(_) => ("Installed from a folder".to_string(), false),
        }
    }

    /// A short form of the status for a grid card.
    fn mod_badge(&self, entry: &CatalogMod) -> (String, bool) {
        if self
            .mod_job
            .as_ref()
            .is_some_and(|job| job.entry.id == entry.id)
        {
            return (self.mod_status(entry).0.replace("Downloading ", ""), true);
        }
        match self.installed(entry) {
            Installed::No => (human_size(entry.size), false),
            Installed::Current(_) | Installed::Folder(_) => ("Installed".to_string(), false),
            Installed::Other(..) => ("Update".to_string(), true),
        }
    }

    /// The installed version a catalog lists newer, for the INSTALLED tab.
    pub(super) fn catalog_update(&self, id: &str) -> Option<&str> {
        let entry = self.catalog.by_id(id)?;
        match self.installed(entry) {
            Installed::Other(..) => Some(&entry.version),
            _ => None,
        }
    }

    pub(super) fn publish_catalog(&mut self) {
        let status = match &self.catalog.status {
            Status::Unset => {
                "No mod catalog is configured: set [mods] catalog in arklay.toml.".to_string()
            }
            Status::Loading => "Loading the mod catalog...".to_string(),
            Status::Failed(err) => format!("The mod catalog could not be loaded: {err}"),
            Status::Ready if self.catalog.mods.is_empty() => "The catalog lists no mods.".into(),
            Status::Ready => String::new(),
        };
        let shown = self.catalog.shown();
        let rows: Vec<Value> = shown
            .iter()
            .enumerate()
            .map(|(index, &at)| {
                let entry = &self.catalog.mods[at];
                let (status, warn) = self.mod_status(entry);
                let (badge, badge_warn) = self.mod_badge(entry);
                let tone = |warn: bool| if warn { "warn" } else { "value-dim" };
                Value::map([
                    ("index", Value::from(index)),
                    (
                        "title",
                        Value::from(format!("{} ({})", entry.name, short_game(&entry.game))),
                    ),
                    ("name", Value::from(entry.name.as_str())),
                    ("game", Value::from(game_title(&entry.game))),
                    (
                        "category",
                        Value::from(entry.category.as_deref().unwrap_or("")),
                    ),
                    ("version", Value::from(entry.version.as_str())),
                    ("size", Value::from(human_size(entry.size))),
                    ("status", Value::from(status)),
                    ("tone", Value::from(tone(warn))),
                    ("badge", Value::from(badge)),
                    ("badge_tone", Value::from(tone(badge_warn))),
                    ("thumbnail", self.catalog.picture(entry)),
                ])
            })
            .collect();
        let has_rows = !rows.is_empty();
        let status = if status.is_empty() && !has_rows {
            format!("No mods in {}.", self.catalog.filter().label())
        } else {
            status
        };
        let view = self.catalog.view;
        self.ui.set("catalog.rows", Value::list(rows));
        self.ui.set("catalog.status", status);
        self.ui.set("catalog.list", has_rows && view == View::List);
        self.ui.set("catalog.grid", has_rows && view == View::Grid);
        let ready = self.catalog.status == Status::Ready && !self.catalog.mods.is_empty();
        self.ui.set(
            "catalog.view_hint",
            match (ready, view) {
                (false, _) => "",
                (true, View::Grid) => "List view",
                (true, View::List) => "Grid view",
            },
        );
        self.ui.set(
            "catalog.filter_hint",
            if ready && self.catalog.filters().len() > 1 {
                format!("Show: {}", self.catalog.filter().label())
            } else {
                String::new()
            },
        );
        self.ui.set(
            "catalog.sort_hint",
            if ready {
                format!("Sort: {}", self.catalog.sort.label())
            } else {
                String::new()
            },
        );
        self.publish_mod_info();
    }

    /// The details screen's fields for the shown mod.
    fn publish_mod_info(&mut self) {
        let Some(entry) = self
            .catalog
            .detail
            .as_deref()
            .and_then(|id| self.catalog.by_id(id))
            .cloned()
        else {
            return;
        };
        let (status, warn) = self.mod_status(&entry);
        let installed = self.installed(&entry);
        let busy = self.mod_job.is_some() || self.job.is_some() || self.update_job.is_some();
        let action = match &installed {
            _ if busy => "",
            Installed::No => "Install",
            Installed::Other(..) => "Update",
            Installed::Current(_) => "Reinstall",
            Installed::Folder(_) => "",
        };
        let removable = matches!(installed, Installed::Current(_) | Installed::Other(..)) && !busy;
        let game = game_title(&entry.game);
        let game_installed =
            Game::from_id(&entry.game).is_some_and(|game| pack_for(&self.packs, game).is_some());
        self.ui.set("mod.name", entry.name.to_uppercase());
        self.ui.set(
            "mod.game",
            if game_installed {
                game.clone()
            } else {
                format!("{game} (not installed)")
            },
        );
        self.ui.set("mod.thumbnail", self.catalog.picture(&entry));
        self.ui.set(
            "mod.description",
            entry.description.clone().unwrap_or_default(),
        );
        self.ui.set("mod.version", entry.version.as_str());
        self.ui
            .set("mod.category", entry.category.clone().unwrap_or_default());
        self.ui.set("mod.size", human_size(entry.size));
        self.ui.set("mod.status", status);
        self.ui
            .set("mod.status_tone", if warn { "warn" } else { "value" });
        self.ui.set(
            "mod.authors",
            if entry.authors.is_empty() {
                "Unknown".to_string()
            } else {
                entry.authors.join(", ")
            },
        );
        self.ui.set("mod.credits", entry.credits.join("\n"));
        self.ui.set("mod.action_label", action);
        self.ui.set("mod.installed", removable);
        self.ui
            .set("mod.remove_question", format!("Remove {}?", entry.name));
        self.ui.set("mod.has_homepage", entry.homepage.is_some());
    }

    /// Carry out a catalog action; returns whether `name` was one.
    pub(super) fn catalog_action(&mut self, name: &str, arg: &Value) -> bool {
        match name {
            "catalog_view" => {
                self.catalog.view = match self.catalog.view {
                    View::Grid => View::List,
                    View::List => View::Grid,
                };
                self.publish();
                self.ui.set_focus_index(0);
            }
            "catalog_filter" => {
                let count = self.catalog.filters().len().max(1);
                self.catalog.filter = (self.catalog.filter + 1) % count;
                self.publish();
                self.ui.set_focus_index(0);
            }
            "catalog_sort" => {
                let at = Sort::ALL
                    .iter()
                    .position(|&sort| sort == self.catalog.sort)
                    .unwrap_or(0);
                self.catalog.sort = Sort::ALL[(at + 1) % Sort::ALL.len()];
                self.publish();
                self.ui.set_focus_index(0);
            }
            "catalog_refresh" => self.refresh_catalog(true),
            "mod_info" => {
                let index = arg.as_int().unwrap_or(0).max(0) as usize;
                let shown = self.catalog.shown();
                if let Some(&at) = shown.get(index) {
                    self.catalog.detail = Some(self.catalog.mods[at].id.clone());
                    self.enter(Screen::ModInfo);
                }
            }
            "mod_install" => self.install_mod(),
            "mod_remove" => self.remove_mod(),
            "mod_homepage" => {
                if let Some(url) = self.detail().and_then(|entry| entry.homepage.clone())
                    && (url.starts_with("https://") || url.starts_with("http://"))
                {
                    open_link(&url);
                }
            }
            _ => return false,
        }
        true
    }

    fn detail(&self) -> Option<&CatalogMod> {
        self.catalog
            .detail
            .as_deref()
            .and_then(|id| self.catalog.by_id(id))
    }

    /// Download and install the shown mod.
    fn install_mod(&mut self) {
        if self.mod_job.is_some() || self.job.is_some() || self.update_job.is_some() {
            return;
        }
        let Some(entry) = self.detail().cloned() else {
            return;
        };
        let installed = self.installed(&entry);
        let dest = match &installed {
            Installed::Folder(path) => {
                self.show_message(
                    &entry.name.to_uppercase(),
                    &[
                        "This mod is installed from a folder:",
                        &path.display().to_string(),
                        "Remove the folder to install the hosted pack.",
                    ],
                    Screen::ModInfo,
                );
                return;
            }
            Installed::Current(path) | Installed::Other(_, path) => path.clone(),
            Installed::No => self
                .catalog
                .install_dir
                .clone()
                .unwrap_or_else(|| paths::mods_dir(&paths::primary_data_dir()))
                .join(entry.file_name()),
        };
        let (sender, receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(ProgressState {
            label: "download".to_string(),
            item: entry.name.clone(),
            ..ProgressState::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_cancel = Arc::clone(&cancel);
        let thread_entry = entry.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let result = catalog::download(&thread_entry, &dest, &thread_cancel, |done, total| {
                if let Ok(mut state) = thread_state.lock() {
                    state.item = format!("{} of {}", human_size(done), human_size(total));
                    state.overall = if total > 0 {
                        done.min(total) as f64 / total as f64
                    } else {
                        0.0
                    };
                    state.elapsed = start.elapsed();
                }
            });
            let _ = sender.send(result.map_err(|err| format!("{err:#}")));
        });
        logging::write(&format!("mods: downloading {} {}", entry.id, entry.version));
        self.mod_job = Some(ModJob {
            receiver,
            cancel,
            state,
            update: matches!(installed, Installed::Other(..)),
            entry,
        });
        self.publish_screen(Screen::Progress);
        self.open(Screen::Progress);
    }

    /// Collect a finished download.
    pub(super) fn poll_mod_job(&mut self) -> bool {
        let Some(job) = &self.mod_job else {
            return false;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err("the download stopped".to_string()),
        };
        let job = self.mod_job.take().expect("a mod job");
        let name = job.entry.name.clone();
        // A cancel that came too late (the pack was already in place) is
        // reported as the install it became.
        if result.is_err() && job.cancel.load(Ordering::Relaxed) {
            self.show_message(
                "DOWNLOAD CANCELLED",
                &[&format!("{name} was not changed.")],
                Screen::ModInfo,
            );
            return true;
        }
        match result {
            Ok(()) => {
                logging::write(&format!(
                    "mods: installed {} {}",
                    job.entry.id, job.entry.version
                ));
                self.refresh_mods();
                // A newly installed mod applies, even under an `enabled` list.
                if !job.update && !mod_enabled(&self.config.mods, &job.entry.id) {
                    set_mod_enabled(&mut self.config.mods, &job.entry.id, true);
                    self.save_mods();
                    self.refresh_mods();
                }
                let verb = if job.update {
                    "updated to"
                } else {
                    "installed:"
                };
                self.show_message(
                    if job.update {
                        "MOD UPDATED"
                    } else {
                        "MOD INSTALLED"
                    },
                    &[
                        &format!("{name} {verb} {}", job.entry.version),
                        "It applies the next time you play. Turn it off in Installed.",
                    ],
                    Screen::ModInfo,
                );
            }
            Err(err) => {
                logging::write(&format!("mods: {} failed: {err}", job.entry.id));
                self.show_message(
                    "DOWNLOAD FAILED",
                    &[&err, "Nothing was changed."],
                    Screen::ModInfo,
                );
            }
        }
        true
    }

    /// Delete the shown mod's pack.
    fn remove_mod(&mut self) {
        let Some(entry) = self.detail().cloned() else {
            return;
        };
        let path = match self.installed(&entry) {
            Installed::Current(path) | Installed::Other(_, path) => path,
            _ => return,
        };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                logging::write(&format!("mods: removed {}", path.display()));
                self.refresh_mods();
                self.publish();
            }
            Err(err) => self.show_message(
                "REMOVE FAILED",
                &[&format!("{}: {err}", path.display())],
                Screen::ModInfo,
            ),
        }
    }
}

/// Open `url` in the default browser.
fn open_link(url: &str) {
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    if let Err(err) = command.arg(url).spawn() {
        logging::write(&format!("warning: could not open {url}: {err}"));
    }
}

/// A game's short tag for a row: `RE1`.
fn short_game(id: &str) -> String {
    id.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, game: &str, category: &str, size: u64) -> CatalogMod {
        CatalogMod {
            id: id.to_string(),
            name: id.to_uppercase(),
            version: "1.0".to_string(),
            game: game.to_string(),
            kind: "mod".to_string(),
            category: Some(category.to_string()),
            description: None,
            authors: Vec::new(),
            credits: Vec::new(),
            homepage: None,
            url: String::new(),
            size,
            sha256: String::new(),
            thumbnail: None,
        }
    }

    #[test]
    fn filters_cover_games_only_when_there_are_several() {
        let mut catalog = ModCatalog {
            mods: vec![
                entry("b", "re1", "High Definition", 5),
                entry("a", "re1", "Audio", 50),
                entry("c", "re1", "High Definition", 20),
            ],
            ..ModCatalog::default()
        };
        assert_eq!(
            catalog.filters(),
            [
                Filter::All,
                Filter::Category("High Definition".into()),
                Filter::Category("Audio".into())
            ]
        );
        assert_eq!(catalog.shown(), [0, 1, 2]);
        catalog.sort = Sort::Name;
        assert_eq!(catalog.shown(), [1, 0, 2]);
        catalog.sort = Sort::Size;
        assert_eq!(catalog.shown(), [1, 2, 0]);
        catalog.filter = 1;
        assert_eq!(catalog.shown(), [2, 0]);

        catalog.mods.push(entry("d", "re2", "Audio", 1));
        assert_eq!(catalog.filters()[1], Filter::Game("re1".into()));
        assert_eq!(catalog.filters()[2], Filter::Game("re2".into()));
    }

    /// Run `poll` until it reports a change, for at most five seconds.
    fn wait(launcher: &mut Launcher, mut poll: impl FnMut(&mut Launcher) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !poll(launcher) {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_hosted_mod_installs_updates_and_removes_from_a_local_catalog() {
        use sha2::Digest;
        let root = std::env::temp_dir().join(format!("arklay-mod-manager-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let hosted = root.join("hosted");
        std::fs::create_dir_all(hosted.join("hd/1.0")).unwrap();
        let mut writer = crate::pack::PackWriter::new();
        let manifest = "[pack]\nformat = 1\nid = \"hd\"\nname = \"HD\"\nversion = \"1.0\"\n\
                        kind = \"mod\"\nbase = \"re1\"\n";
        writer
            .add(crate::manifest::ENTRY, manifest.as_bytes().to_vec())
            .unwrap();
        writer.add("hires/bgd/0259741A.webp", vec![7; 100]).unwrap();
        let pack = hosted.join("hd/1.0/hd.akpak");
        writer.write(&pack).unwrap();
        let bytes = std::fs::read(&pack).unwrap();
        let index = format!(
            r#"{{"format": 1, "mods": [{{"id": "hd", "name": "HD", "version": "1.0",
                "game": "re1", "category": "High Definition", "authors": ["Team"],
                "credits": ["Thanks"], "url": "hd/1.0/hd.akpak", "size": {},
                "sha256": "{:x}"}}]}}"#,
            bytes.len(),
            sha2::Sha256::digest(&bytes)
        );
        std::fs::write(hosted.join("index.json"), index).unwrap();

        let mut config = crate::config::Config::default();
        config.mods.catalog = Some(hosted.join("index.json").to_string_lossy().into_owned());
        let packs = vec![super::super::PackRow {
            id: "re1".into(),
            name: "re1".into(),
            file: "re1.akpak".into(),
            version: String::new(),
            source: "LOCAL".into(),
            path: PathBuf::from("/packs/re1.akpak"),
            stale: false,
            error: None,
            thumbnail: None,
            official: false,
        }];
        let mut launcher = Launcher::with(&config, packs, HashMap::new(), Vec::new()).unwrap();
        let installed = root.join("installed");
        launcher.mod_dirs = vec![installed.clone()];
        launcher.catalog.install_dir = Some(installed.clone());
        launcher.catalog.cache_dir = Some(root.join("cache"));
        if std::env::var(catalog::URL_ENV).is_ok() {
            return;
        }

        launcher.enter(Screen::Mods);
        wait(&mut launcher, Launcher::poll_catalog);
        launcher.publish();
        let rows = launcher.ui.get("catalog.rows");
        assert_eq!(rows.as_list().len(), 1);
        assert_eq!(launcher.ui.get("catalog.status"), Value::from(""));
        assert_eq!(launcher.ui.get("catalog.grid"), Value::Bool(true));

        assert!(launcher.catalog_action("mod_info", &Value::Int(0)));
        assert_eq!(launcher.screen(), Screen::ModInfo);
        assert_eq!(launcher.ui.get("mod.action_label"), Value::from("Install"));
        assert_eq!(launcher.ui.get("mod.authors"), Value::from("Team"));

        launcher.catalog_action("mod_install", &Value::Nil);
        assert_eq!(launcher.screen(), Screen::Progress);
        wait(&mut launcher, Launcher::poll_mod_job);
        assert_eq!(launcher.screen(), Screen::Message);
        assert_eq!(std::fs::read(installed.join("hd.akpak")).unwrap(), bytes);
        assert_eq!(launcher.mods.len(), 1);
        launcher.goto(Screen::ModInfo);
        launcher.publish();
        assert_eq!(launcher.ui.get("mod.status"), Value::from("Installed"));
        assert_eq!(
            launcher.ui.get("mod.action_label"),
            Value::from("Reinstall")
        );

        // A newer listing offers the update, here and on INSTALLED.
        launcher.catalog.mods[0].version = "1.1".into();
        launcher.publish_screen(Screen::Mods);
        assert_eq!(
            launcher.ui.get("mod.status"),
            Value::from("Update available (1.0 installed)")
        );
        let installed_rows = launcher.ui.get("mods.rows");
        let Value::Map(row) = &installed_rows.as_list()[0] else {
            panic!("an installed row");
        };
        assert_eq!(row["version"], Value::from("1.0 > 1.1"));

        launcher.catalog_action("mod_remove", &Value::Nil);
        assert!(!installed.join("hd.akpak").exists());
        assert!(launcher.mods.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sizes_read_as_people_write_them() {
        assert_eq!(human_size(529_808_808), "530 MB");
        assert_eq!(human_size(1_605_392_957), "1.6 GB");
        assert_eq!(human_size(4_476), "0.0 MB");
        assert_eq!(human_size(2_500_000), "2.5 MB");
    }
}

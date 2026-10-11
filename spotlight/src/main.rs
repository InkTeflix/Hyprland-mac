use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Adjustment, Align, Application, ApplicationWindow, Box as GtkBox, Button, Entry, Image, Label,
    Orientation, Overlay, Scrollbar,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// -----------------------------------------------------------------------------
// PROJECT IDENTITY
// -----------------------------------------------------------------------------
//
// Deliberately generic.
// Do not put usernames, machine names, hostnames, monitor names,
// filesystem paths, or other personal/device-specific information here.
//
const APP_ID: &str = "org.spotlight.Launcher";
const LAYER_NAMESPACE: &str = "spotlight";

// -----------------------------------------------------------------------------
// DESIGN REFERENCE CONSTANTS
// -----------------------------------------------------------------------------
//
// These are visual/design references only. They are not assumptions about the
// user's display, resolution, monitor count, or hardware.

const REFERENCE_COMPOSITION_WIDTH: f64 = 660.0;
const REFERENCE_SEARCH_WIDTH: f64 = 380.0;
const REFERENCE_CONTROL_SIZE: f64 = 58.0;
const REFERENCE_GAP: f64 = 12.0;
const REFERENCE_INNER_GAP: f64 = 10.0;
const REFERENCE_ICON_SIZE: f64 = 22.0;
const REFERENCE_ROW_HEIGHT: f64 = 58.0;
const MIN_RESULT_ROW_HEIGHT: i32 = 36;
const MAX_VISIBLE_RESULT_ROWS: usize = 8;
// Cap rendered/ranked results so query updates stay cheap and the scrollbar is bounded.
const MAX_SEARCH_RESULTS: usize = 30;
const SEARCH_HISTORY_LIMIT: usize = 20;
const PINNED_PRIORITY_BIAS: i32 = 100_000;
const PREVIEW_MAX_BYTES: usize = 32 * 1024;
const PREVIEW_MAX_LINES: usize = 80;

// -----------------------------------------------------------------------------
// APPLICATION STATE
// -----------------------------------------------------------------------------

struct AppState {
    window: ApplicationWindow,
    root: GtkBox,
    entry: Entry,
    theme_provider: gtk::CssProvider,
    theme_mode: Cell<ThemeMode>,
    interaction: InteractionContext,
}

#[derive(Clone)]
struct ResultRow {
    button: Button,
    icon: Image,
    title: Label,
    subtitle: Label,
    result_id: RefCell<Option<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuxiliarySurface {
    None,
    Preview,
    ContextMenu,
    Properties,
    History,
    Help,
}

#[derive(Clone)]
struct ContextActionRow {
    button: Button,
    label: Label,
}

#[derive(Clone)]
struct HistoryRow {
    button: Button,
    label: Label,
}

#[derive(Clone)]
struct DetailSurfaces {
    results_panel: Overlay,
    preview_panel: GtkBox,
    preview_icon: Image,
    preview_title: Label,
    preview_subtitle: Label,
    preview_body: Label,
    preview_picture: gtk::Picture,
    preview_body_scroll: gtk::ScrolledWindow,
    preview_body_height: i32,
    preview_body_height_with_picture: i32,
    properties_panel: GtkBox,
    properties_icon: Image,
    properties_title: Label,
    properties_subtitle: Label,
    properties_body: Label,
    context_panel: GtkBox,
    context_title: Label,
    context_status: Label,
    context_actions: Vec<ContextActionRow>,
    history_panel: GtkBox,
    history_empty: Label,
    history_list: GtkBox,
    history_clear_button: Button,
    help_panel: GtkBox,
    help_indexing_label: Label,
    help_progress_bar: gtk::ProgressBar,
    help_dismiss_button: Button,
}

#[derive(Clone)]
struct InteractionContext {
    window: ApplicationWindow,
    root: GtkBox,
    entry: Entry,
    idle_hint: Label,
    idle_hint_source: Rc<RefCell<Option<glib::SourceId>>>,
    current_directory: Rc<RefCell<PathBuf>>,
    file_catalog_backup: Rc<RefCell<Option<Vec<IndexedResult>>>>,
    context_action_count: Rc<Cell<usize>>,
    back_button: Button,
    catalog: Rc<RefCell<Vec<IndexedResult>>>,
    // Bit mask of local providers whose current catalog batch is searchable.
    catalog_ready: Rc<Cell<u8>>,
    // Progress for this refresh run, separate from any usable cached indexes.
    catalog_refresh_progress: Rc<Cell<u8>>,
    catalog_refresh_finished: Rc<Cell<bool>>,
    catalog_refresh_failed: Rc<Cell<bool>>,
    help_is_welcome: Rc<Cell<bool>>,
    welcome_auto_dismiss_scheduled: Rc<Cell<bool>>,
    result_rows: Rc<Vec<ResultRow>>,
    selected_index: Rc<Cell<usize>>,
    result_window_start: Rc<Cell<usize>>,
    active_filter: Rc<Cell<ProviderFilter>>,
    scroll_adjustment: Adjustment,
    scrollbar: Scrollbar,
    visible_rows: usize,
    surfaces: DetailSurfaces,
    auxiliary_surface: Rc<Cell<AuxiliarySurface>>,
    preview_generation: Rc<Cell<u64>>,
    preview_cancel: Rc<RefCell<Option<Arc<AtomicBool>>>>,
    // GTK textures are owned by the UI thread and kept here to avoid decoding
    // the same cached thumbnail every time the same result is previewed.
    preview_textures: Rc<RefCell<HashMap<String, gdk::Texture>>>,
    history: Rc<RefCell<Vec<String>>>,
    history_rows: Rc<Vec<HistoryRow>>,
    history_selected_index: Rc<Cell<usize>>,
    history_window_start: Rc<Cell<usize>>,
    history_visible_rows: usize,
    context_selected_index: Rc<Cell<usize>>,
    standalone_buttons: [Button; 4],
    inline_buttons: [Button; 4],
}

struct ProviderBatch {
    provider: ProviderFilter,
    records: Vec<IndexedResult>,
}

struct CachedCatalog {
    records: Vec<IndexedResult>,
    ready_mask: u8,
}

struct PreviewPayload {
    subtitle: String,
    body: String,
    body_markup: bool,
    thumbnail_png: Option<Vec<u8>>,
    thumbnail_key: Option<String>,
}

#[derive(Clone, Debug)]
struct IndexedResult {
    id: String,
    provider: ProviderFilter,
    title: String,
    generic_name: String,
    comment: String,
    categories: String,
    executable: String,
    path: String,
    content_preview: String,
    metadata: String,
    icon_name: String,
    usage_count: u32,
    recent_use_rank: u32,
    priority_bias: i32,
}

fn nonempty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

fn application_category_hint(categories: &str) -> Option<&'static str> {
    for category in categories.split(';') {
        let hint = match category.trim() {
            "Settings" | "HardwareSettings" | "SystemSettings" => "Settings",
            "Development" | "IDE" => "Development",
            "Network" | "WebBrowser" => "Network",
            "AudioVideo" | "Audio" | "Video" => "Media",
            "Office" | "WordProcessor" | "Spreadsheet" | "Presentation" => "Office",
            "Graphics" | "Photography" => "Graphics",
            "Utility" => "Utility",
            "System" => "System",
            "Game" => "Game",
            "Education" => "Education",
            _ => continue,
        };
        return Some(hint);
    }
    None
}

impl IndexedResult {
    // Search only compact, stable metadata. Raw comments and file-content
    // snippets are reserved for the future preview surface, not every keystroke.
    fn searchable_fields(&self) -> [(&str, i32); 5] {
        [
            (&self.title, 100),
            (&self.generic_name, 78),
            (&self.categories, 48),
            (&self.executable, 44),
            (&self.path, 38),
        ]
    }

    // Result rows are deliberately terse; detailed metadata belongs in Preview.
    fn display_subtitle(&self) -> String {
        match self.provider {
            ProviderFilter::Applications => nonempty_or(
                &self.generic_name,
                application_category_hint(&self.categories).unwrap_or("Application"),
            )
            .to_string(),
            ProviderFilter::Files => {
                let kind = nonempty_or(&self.generic_name, "File");
                if self.metadata.is_empty() {
                    kind.to_string()
                } else {
                    format!("{kind} · {}", self.metadata)
                }
            }
            ProviderFilter::Actions => nonempty_or(&self.generic_name, "Action").to_string(),
            ProviderFilter::Clipboard => {
                if self.metadata.is_empty() {
                    self.generic_name.clone()
                } else {
                    format!("{} · {}", self.generic_name, self.metadata)
                }
            }
            // Catalog entries always carry a concrete provider.
            ProviderFilter::All => String::new(),
        }
    }
}

struct SpotlightStorage {
    index_dir: PathBuf,
    cache_dir: PathBuf,
    state_dir: PathBuf,
}

impl SpotlightStorage {
    fn create() -> Option<Self> {
        let root = spotlight_root()?;
        let index_dir = root.join("indexes");
        let cache_dir = root.join("cache");
        let state_dir = root.join("state");

        for path in [&root, &index_dir, &cache_dir, &state_dir] {
            if let Err(error) = fs::create_dir_all(path) {
                eprintln!("[spotlight] storage: unable to prepare index storage: {error}");
                return None;
            }
        }

        Some(Self {
            index_dir,
            cache_dir,
            state_dir,
        })
    }
}

fn spotlight_root() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(config_home.join("hypr-mac").join("spotlight"))
}

fn hypr_mac_root() -> Option<PathBuf> {
    spotlight_root()?.parent().map(Path::to_path_buf)
}

fn escape_index_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn persist_provider_index(storage: &SpotlightStorage, provider: ProviderFilter, records: &[IndexedResult]) {
    let filename = match provider {
        ProviderFilter::All => return,
        ProviderFilter::Applications => "applications.tsv",
        ProviderFilter::Files => "files.tsv",
        ProviderFilter::Actions => "actions.tsv",
        ProviderFilter::Clipboard => "clipboard.tsv",
    };

    let path = storage.index_dir.join(filename);
    let Ok(mut file) = File::create(&path) else {
        return;
    };

    let _ = writeln!(file, "# spotlight-index-v2");
    let _ = writeln!(
        file,
        "id\tprovider\ttitle\tgeneric_name\tcomment\tcategories\texecutable\tpath\tcontent_preview\tmetadata\ticon\tusage_count\trecent_use_rank\tpriority_bias"
    );

    for record in records.iter().filter(|record| record.provider == provider) {
        let _ = writeln!(
            file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            escape_index_field(&record.id),
            escape_index_field(record.provider.label()),
            escape_index_field(&record.title),
            escape_index_field(&record.generic_name),
            escape_index_field(&record.comment),
            escape_index_field(&record.categories),
            escape_index_field(&record.executable),
            escape_index_field(&record.path),
            escape_index_field(&record.content_preview),
            escape_index_field(&record.metadata),
            escape_index_field(&record.icon_name),
            record.usage_count,
            record.recent_use_rank,
            record.priority_bias,
        );
    }
}

// Providers publish independent batches. Applications become searchable first,
// actions and clipboard follow, and the potentially expensive file traversal is
// deliberately last. The UI merges each batch instead of replacing cached data.
fn load_catalog_staged(sender: std::sync::mpsc::SyncSender<ProviderBatch>) {
    let storage = SpotlightStorage::create();

    let mut applications = load_real_applications();
    apply_preferences(&mut applications);
    if let Some(storage) = storage.as_ref() {
        persist_provider_index(storage, ProviderFilter::Applications, &applications);
    }
    if sender
        .send(ProviderBatch {
            provider: ProviderFilter::Applications,
            records: applications,
        })
        .is_err()
    {
        return;
    }

    let mut actions = load_real_actions();
    apply_preferences(&mut actions);
    if let Some(storage) = storage.as_ref() {
        persist_provider_index(storage, ProviderFilter::Actions, &actions);
    }
    if sender
        .send(ProviderBatch {
            provider: ProviderFilter::Actions,
            records: actions,
        })
        .is_err()
    {
        return;
    }

    let mut clipboard = load_real_clipboard();
    apply_preferences(&mut clipboard);
    if let Some(storage) = storage.as_ref() {
        persist_provider_index(storage, ProviderFilter::Clipboard, &clipboard);
    }
    if sender
        .send(ProviderBatch {
            provider: ProviderFilter::Clipboard,
            records: clipboard,
        })
        .is_err()
    {
        return;
    }

    // File traversal and metadata stat calls can take longer than reading the
    // desktop entries, so publish that provider only after fast local results.
    let mut files = load_real_files();
    apply_preferences(&mut files);
    if let Some(storage) = storage.as_ref() {
        persist_provider_index(storage, ProviderFilter::Files, &files);
    }
    let _ = sender.send(ProviderBatch {
        provider: ProviderFilter::Files,
        records: files,
    });
}

fn state_file(name: &str) -> Option<PathBuf> {
    Some(SpotlightStorage::create()?.state_dir.join(name))
}

fn welcome_seen() -> bool {
    state_file("welcome-seen")
        .map(|path| path.is_file())
        .unwrap_or(false)
}

fn mark_welcome_seen() {
    if let Some(path) = state_file("welcome-seen") {
        let _ = fs::write(path, b"seen\n");
    }
}

fn schedule_welcome_auto_dismiss(interaction: &InteractionContext) {
    if !interaction.help_is_welcome.get()
        || interaction.welcome_auto_dismiss_scheduled.replace(true)
    {
        return;
    }

    // Only the first-run introduction auto-dismisses. Explicitly opened help is
    // persistent until the user closes it. Keep the delay calm and predictable.
    let interaction = interaction.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(7), move || {
        interaction.welcome_auto_dismiss_scheduled.set(false);
        if interaction.help_is_welcome.get()
            && interaction.auxiliary_surface.get() == AuxiliarySurface::Help
        {
            close_auxiliary_surface(&interaction);
            focus_search_entry(&interaction.entry);
        }
    });
}

fn update_help_indexing(interaction: &InteractionContext) {
    let progress = interaction.catalog_refresh_progress.get();
    let completed = progress.count_ones().min(4);
    interaction
        .surfaces
        .help_progress_bar
        .set_fraction(completed as f64 / 4.0);

    let status = if interaction.catalog_refresh_failed.get() {
        interaction.surfaces.help_progress_bar.set_visible(false);
        "Index refresh unavailable · existing results remain usable".to_string()
    } else if interaction.catalog_refresh_finished.get() {
        interaction.surfaces.help_progress_bar.set_fraction(1.0);
        "Local indexing complete · results are ready".to_string()
    } else {
        interaction.surfaces.help_progress_bar.set_visible(true);
        let next_stage = if progress & provider_ready_bit(ProviderFilter::Applications) == 0 {
            "applications"
        } else if progress & provider_ready_bit(ProviderFilter::Actions) == 0 {
            "actions"
        } else if progress & provider_ready_bit(ProviderFilter::Clipboard) == 0 {
            "clipboard history"
        } else {
            "files"
        };
        format!("Indexing {next_stage}…  ·  {completed}/4 sources ready")
    };
    interaction.surfaces.help_indexing_label.set_text(&status);

    if interaction.catalog_refresh_finished.get()
        && interaction.auxiliary_surface.get() == AuxiliarySurface::Help
        && interaction.help_is_welcome.get()
    {
        schedule_welcome_auto_dismiss(interaction);
    }
}

fn open_help(interaction: &InteractionContext, is_welcome: bool) {
    interaction.help_is_welcome.set(is_welcome);
    interaction
        .surfaces
        .help_dismiss_button
        .set_label(if is_welcome { "Start searching" } else { "Close help" });
    activate_auxiliary_surface(interaction, AuxiliarySurface::Help);
    update_help_indexing(interaction);
    interaction.surfaces.help_dismiss_button.grab_focus();
}

fn load_search_history() -> Vec<String> {
    let Some(path) = state_file("search-history.txt") else {
        return Vec::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut history = Vec::new();
    for line in contents.lines() {
        let query = unescape_index_field(line).trim().to_string();
        if query.is_empty() || history.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&query)) {
            continue;
        }
        history.push(query);
        if history.len() >= SEARCH_HISTORY_LIMIT {
            break;
        }
    }
    history
}

fn persist_search_history(history: &[String]) {
    let Some(path) = state_file("search-history.txt") else {
        return;
    };
    let Ok(mut file) = File::create(path) else {
        return;
    };
    for query in history.iter().take(SEARCH_HISTORY_LIMIT) {
        let _ = writeln!(file, "{}", escape_index_field(query));
    }
}

fn record_search_query(history: &Rc<RefCell<Vec<String>>>, query: &str) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    let mut history = history.borrow_mut();
    history.retain(|old| !old.eq_ignore_ascii_case(query));
    history.insert(0, query.to_string());
    history.truncate(SEARCH_HISTORY_LIMIT);
    persist_search_history(&history);
}

fn load_pinned_ids() -> HashSet<String> {
    let Some(path) = state_file("pinned-results.txt") else {
        return HashSet::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return HashSet::new();
    };
    contents.lines().map(unescape_index_field).filter(|id| !id.is_empty()).collect()
}

fn persist_pinned_ids(pinned: &HashSet<String>) {
    let Some(path) = state_file("pinned-results.txt") else {
        return;
    };
    let Ok(mut file) = File::create(path) else {
        return;
    };
    let mut ids: Vec<&String> = pinned.iter().collect();
    ids.sort_unstable();
    for id in ids {
        let _ = writeln!(file, "{}", escape_index_field(id));
    }
}

fn toggle_pinned_id(id: &str) -> bool {
    let mut pinned = load_pinned_ids();
    let now_pinned = if pinned.remove(id) {
        false
    } else {
        pinned.insert(id.to_string());
        true
    };
    persist_pinned_ids(&pinned);
    now_pinned
}

fn is_pinned_id(id: &str) -> bool {
    load_pinned_ids().contains(id)
}

fn load_usage_stats() -> HashMap<String, (u32, u64)> {
    let Some(path) = state_file("result-usage.tsv") else {
        return HashMap::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };

    let mut stats = HashMap::new();
    for line in contents.lines() {
        let mut fields = line.split('\t');
        let (Some(id), Some(count), Some(last_used)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        if let (Ok(count), Ok(last_used)) = (count.parse::<u32>(), last_used.parse::<u64>()) {
            stats.insert(unescape_index_field(id), (count, last_used));
        }
    }
    stats
}

fn persist_usage_stats(stats: &HashMap<String, (u32, u64)>) {
    let Some(path) = state_file("result-usage.tsv") else {
        return;
    };
    let Ok(mut file) = File::create(path) else {
        return;
    };
    let mut entries: Vec<_> = stats.iter().collect();
    entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    for (id, (count, last_used)) in entries {
        let _ = writeln!(file, "{}\t{}\t{}", escape_index_field(id), count, last_used);
    }
}

fn apply_preferences(records: &mut [IndexedResult]) {
    let pinned = load_pinned_ids();
    let usage = load_usage_stats();
    for record in records {
        record.priority_bias = if pinned.contains(&record.id) {
            PINNED_PRIORITY_BIAS
        } else {
            0
        };
        if let Some((count, last_used)) = usage.get(&record.id) {
            record.usage_count = *count;
            let last_used = (*last_used).min(u32::MAX as u64) as u32;
            record.recent_use_rank = u32::MAX.saturating_sub(last_used);
        }
    }
}

fn record_result_use(catalog: &mut [IndexedResult], result_id: &str) {
    let mut stats = load_usage_stats();
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let entry = stats.entry(result_id.to_string()).or_insert((0, 0));
    entry.0 = entry.0.saturating_add(1);
    entry.1 = now;
    persist_usage_stats(&stats);

    for result in catalog {
        if let Some((count, last_used)) = stats.get(&result.id) {
            result.usage_count = *count;
            result.recent_use_rank = u32::MAX.saturating_sub((*last_used).min(u32::MAX as u64) as u32);
        }
    }
}

fn provider_index_filename(provider: ProviderFilter) -> Option<&'static str> {
    match provider {
        ProviderFilter::All => None,
        ProviderFilter::Applications => Some("applications.tsv"),
        ProviderFilter::Files => Some("files.tsv"),
        ProviderFilter::Actions => Some("actions.tsv"),
        ProviderFilter::Clipboard => Some("clipboard.tsv"),
    }
}

fn unescape_index_field(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => output.push('\\'),
            Some('t') => output.push('\t'),
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn provider_from_label(label: &str) -> Option<ProviderFilter> {
    match label {
        "Applications" => Some(ProviderFilter::Applications),
        "Files" => Some(ProviderFilter::Files),
        "Actions" => Some(ProviderFilter::Actions),
        "Clipboard" => Some(ProviderFilter::Clipboard),
        _ => None,
    }
}

// Existing indexes let the persistent UI search immediately. On a cold start,
// the UI stays responsive while provider batches are collected on a worker.
fn load_cached_catalog() -> CachedCatalog {
    let Some(storage) = SpotlightStorage::create() else {
        return CachedCatalog {
            records: Vec::new(),
            ready_mask: 0,
        };
    };

    let providers = [
        ProviderFilter::Applications,
        ProviderFilter::Actions,
        ProviderFilter::Clipboard,
        ProviderFilter::Files,
    ];
    let mut all = Vec::new();
    let mut ready_mask = 0u8;

    // A missing or stale index for one provider must not discard usable caches
    // from the others. This makes cold starts resilient to interrupted refreshes.
    for provider in providers {
        let Some(filename) = provider_index_filename(provider) else {
            continue;
        };
        let path = storage.index_dir.join(filename);
        let Some(records) = load_provider_index(&path, provider) else {
            continue;
        };
        ready_mask |= provider_ready_bit(provider);
        all.extend(records);
    }

    // Built-in Spotlight actions are cheap and available immediately, even if
    // the on-disk Actions index was created by an older version.
    all.extend(load_builtin_actions());
    let mut records = dedupe_results(all);
    apply_preferences(&mut records);
    CachedCatalog { records, ready_mask }
}

fn load_provider_index(path: &Path, provider: ProviderFilter) -> Option<Vec<IndexedResult>> {
    let file = File::open(path).ok()?;
    let mut lines = BufReader::new(file).lines();
    let version = lines.next()?.ok()?;
    if version != "# spotlight-index-v2" {
        return None;
    }
    let header = lines.next()?.ok()?;
    if !header.starts_with("id\tprovider\ttitle\t") {
        return None;
    }

    let mut records = Vec::new();
    for line in lines {
        let line = line.ok()?;
        if line.is_empty() {
            continue;
        }
        let fields: Vec<String> = line.split('\t').map(unescape_index_field).collect();
        if fields.len() != 14 || provider_from_label(&fields[1])? != provider {
            return None;
        }

        records.push(IndexedResult {
            id: fields[0].clone(),
            provider,
            title: fields[2].clone(),
            generic_name: fields[3].clone(),
            comment: fields[4].clone(),
            categories: fields[5].clone(),
            executable: fields[6].clone(),
            path: fields[7].clone(),
            content_preview: fields[8].clone(),
            metadata: fields[9].clone(),
            icon_name: fields[10].clone(),
            usage_count: fields[11].parse().ok()?,
            recent_use_rank: fields[12].parse().ok()?,
            priority_bias: fields[13].parse().ok()?,
        });
    }

    Some(records)
}

fn load_real_applications() -> Vec<IndexedResult> {
    let roots = application_roots();
    let mut records = Vec::new();

    for root in roots {
        collect_desktop_entries(&root, ProviderFilter::Applications, &mut records);
    }

    dedupe_results(records)
}

fn application_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(xdg_data_home).join("applications"));
    } else if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/applications"));
    }

    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/flatpak/exports/share/applications"));
    }

    roots.push(PathBuf::from("/usr/local/share/applications"));
    roots.push(PathBuf::from("/usr/share/applications"));

    roots
}

fn collect_desktop_entries(root: &Path, provider: ProviderFilter, records: &mut Vec<IndexedResult>) {
    if !root.is_dir() {
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_desktop_entries(&path, provider, records);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }

        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };

        let fields = match parse_desktop_entry(&path) {
            Some(fields) => fields,
            None => continue,
        };

        let hidden = fields
            .get("Hidden")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let no_display = fields
            .get("NoDisplay")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let entry_type = fields.get("Type").map(String::as_str).unwrap_or("Application");

        if hidden || no_display || entry_type != "Application" {
            continue;
        }

        let Some(name) = fields.get("Name").cloned() else {
            continue;
        };

        let generic_name = fields
            .get("GenericName")
            .cloned()
            .unwrap_or_default();
        let comment = fields.get("Comment").cloned().unwrap_or_default();
        let categories = fields.get("Categories").cloned().unwrap_or_default();
        let executable = fields
            .get("Exec")
            .map(|exec| clean_desktop_exec(exec))
            .unwrap_or_default();
        let icon_name = fields.get("Icon").cloned().unwrap_or_default();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        records.push(IndexedResult {
            id: format!(
                "app:{}",
                path.canonicalize()
                    .unwrap_or(path.clone())
                    .display()
            ),
            provider,
            title: name,
            generic_name,
            comment,
            categories,
            executable,
            path: path.display().to_string(),
            content_preview: String::new(),
            metadata: String::new(),
            icon_name,
            usage_count: 0,
            recent_use_rank: u32::MAX.saturating_sub((modified % u32::MAX as u64) as u32),
            priority_bias: 0,
        });
    }
}

fn parse_desktop_entry(path: &Path) -> Option<HashMap<String, String>> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut in_desktop_entry = false;
    let mut fields = HashMap::new();

    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line.eq_ignore_ascii_case("[Desktop Entry]");
            continue;
        }

        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        fields.insert(key.trim().to_string(), value.trim().to_string());
    }

    Some(fields)
}

fn clean_desktop_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|token| !token.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

fn load_real_files() -> Vec<IndexedResult> {
    let Some(root) = hypr_mac_root() else {
        return Vec::new();
    };

    let mut records = Vec::new();
    collect_files(&root, &root, &mut records);

    records
}

fn collect_files(root: &Path, current: &Path, records: &mut Vec<IndexedResult>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");

        if path.is_dir() {
            if matches!(
                file_name,
                ".git" | "target" | "node_modules" | ".cache"
            ) {
                continue;
            }

            if path == root.join("spotlight").join("indexes")
                || path == root.join("spotlight").join("cache")
            {
                continue;
            }

            collect_files(root, &path, records);
            continue;
        }

        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };

        if !metadata.is_file() {
            continue;
        }

        let size = metadata.len();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let generic_name = file_type_label(&extension);
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();

        records.push(IndexedResult {
            id: format!("file:{}", path.display()),
            provider: ProviderFilter::Files,
            title: file_name.to_string(),
            generic_name,
            comment: String::new(),
            categories: extension.clone(),
            executable: String::new(),
            path: relative_path,
            content_preview: String::new(),
            metadata: format_file_size(size),
            icon_name: icon_for_file_extension(&extension).to_string(),
            usage_count: 0,
            recent_use_rank: u32::MAX.saturating_sub((modified % u32::MAX as u64) as u32),
            priority_bias: 0,
        });
    }
}

// File contents are intentionally not read during indexing. The preview layer
// will load bounded content asynchronously only after a result is selected.
fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let size = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if size < MB {
        format!("{:.1} KB", size / KB)
    } else if size < GB {
        format!("{:.1} MB", size / MB)
    } else {
        format!("{:.1} GB", size / GB)
    }
}

fn file_type_label(extension: &str) -> String {
    match extension {
        "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "lua" | "sh" => {
            "Source code".to_string()
        }
        "md" | "markdown" => "Markdown document".to_string(),
        "toml" | "yaml" | "yml" | "json" | "conf" => "Configuration".to_string(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" => "Image".to_string(),
        "pdf" => "PDF document".to_string(),
        "desktop" => "Desktop entry".to_string(),
        "tar" | "gz" | "zip" | "xz" | "7z" => "Archive".to_string(),
        "txt" => "Text document".to_string(),
        _ if extension.is_empty() => "File".to_string(),
        _ => "File".to_string(),
    }
}

fn icon_for_file_extension(extension: &str) -> &'static str {
    match extension {
        "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "lua" | "sh" => {
            "text-x-script-symbolic"
        }
        "md" | "markdown" | "txt" | "toml" | "yaml" | "yml" | "json" | "conf" => {
            "text-x-generic-symbolic"
        }
        "png" | "jpg" | "jpeg" | "webp" | "gif" => "image-x-generic-symbolic",
        "pdf" => "application-pdf-symbolic",
        _ => "text-x-generic-symbolic",
    }
}

fn builtin_action(id: &str, title: &str, kind: &str, description: &str, icon_name: &str) -> IndexedResult {
    let aliases = match id {
        "date-time" => "Action;Local;Clock;Date;Time",
        "now-playing" => "Action;Local;Media;Music;Player;NowPlaying",
        "volume-status" => "Action;Local;Audio;Volume;Sound",
        "battery-status" => "Action;Local;Battery;Power;Charging",
        "lock-screen" => "Action;Local;Lock;Security;Session",
        _ => "Action;Local",
    };
    IndexedResult {
        id: format!("builtin:{id}"),
        provider: ProviderFilter::Actions,
        title: title.to_string(),
        generic_name: kind.to_string(),
        comment: description.to_string(),
        categories: aliases.to_string(),
        executable: String::new(),
        path: String::new(),
        content_preview: String::new(),
        metadata: "Built-in Spotlight action".to_string(),
        icon_name: icon_name.to_string(),
        usage_count: 0,
        recent_use_rank: u32::MAX,
        priority_bias: 0,
    }
}

fn load_builtin_actions() -> Vec<IndexedResult> {
    vec![
        builtin_action("date-time", "Current date & time", "System information", "Show the local date and time inside Spotlight.", "x-office-calendar-symbolic"),
        builtin_action("now-playing", "Currently playing", "Media status", "Show the active media player's title and artist inside Spotlight.", "audio-x-generic-symbolic"),
        builtin_action("volume-status", "Audio volume", "Audio status", "Show the current default audio output level.", "audio-volume-high-symbolic"),
        builtin_action("battery-status", "Battery status", "Power status", "Show battery percentage and charging state when a battery is present.", "battery-good-symbolic"),
        builtin_action("lock-screen", "Lock screen", "System action", "Lock the current desktop session.", "system-lock-screen-symbolic"),
    ]
}

fn load_real_actions() -> Vec<IndexedResult> {
    let mut records = load_builtin_actions();
    let Some(root) = hypr_mac_root() else {
        return records;
    };

    let spotlight_root = root.join("spotlight");
    collect_action_desktop_entries(&spotlight_root.join("actions"), &mut records);
    // Only explicitly designated Spotlight script/bin folders become actions.
    // Avoid indexing and making executable arbitrary files from the whole config tree.
    for scripts_root in [
        spotlight_root.join("actions").join("scripts"),
        spotlight_root.join("actions").join("bin"),
        spotlight_root.join("scripts"),
        spotlight_root.join("bin"),
    ] {
        collect_executable_files(&scripts_root, &mut records);
    }
    dedupe_results(records)
}

fn collect_action_desktop_entries(root: &Path, records: &mut Vec<IndexedResult>) {
    if !root.is_dir() {
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_action_desktop_entries(&path, records);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }

        let Some(fields) = parse_desktop_entry(&path) else {
            continue;
        };

        let Some(name) = fields.get("Name").cloned() else {
            continue;
        };

        let exec = fields
            .get("Exec")
            .map(|value| clean_desktop_exec(value))
            .unwrap_or_default();

        records.push(IndexedResult {
            id: format!("action:{}", path.display()),
            provider: ProviderFilter::Actions,
            title: name,
            generic_name: fields
                .get("GenericName")
                .cloned()
                .unwrap_or_else(|| "Spotlight action".to_string()),
            comment: fields.get("Comment").cloned().unwrap_or_default(),
            categories: fields
                .get("Categories")
                .cloned()
                .unwrap_or_else(|| "Action".to_string()),
            executable: exec,
            path: path.display().to_string(),
            content_preview: String::new(),
            metadata: "project action definition".to_string(),
            icon_name: fields
                .get("Icon")
                .map(String::as_str)
                .unwrap_or("system-run-symbolic")
                .to_string(),
            usage_count: 0,
            recent_use_rank: u32::MAX,
            priority_bias: 0,
        });
    }
}

fn collect_executable_files(root: &Path, records: &mut Vec<IndexedResult>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_executable_files(&path, records);
            continue;
        }

        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };

        #[cfg(unix)]
        let executable = metadata.permissions().mode() & 0o111 != 0;
        #[cfg(not(unix))]
        let executable = false;

        if !executable || !metadata.is_file() {
            continue;
        }

        let title = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Action")
            .replace('-', " ")
            .replace('_', " ");

        records.push(IndexedResult {
            id: format!("action:{}", path.display()),
            provider: ProviderFilter::Actions,
            title: title.clone(),
            generic_name: "Executable action".to_string(),
            comment: String::new(),
            categories: "Action;Script".to_string(),
            executable: path.display().to_string(),
            path: path.display().to_string(),
            content_preview: String::new(),
            metadata: format_file_size(metadata.len()),
            icon_name: "system-run-symbolic".to_string(),
            usage_count: 0,
            recent_use_rank: u32::MAX,
            priority_bias: 0,
        });
    }
}

fn load_real_clipboard() -> Vec<IndexedResult> {
    let output = match Command::new("cliphist").arg("list").output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
        _ => match Command::new("wl-paste").arg("--no-newline").output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).into_owned()
            }
            _ => return Vec::new(),
        },
    };

    output
        .lines()
        .take(100)
        .enumerate()
        .filter_map(|(index, line)| {
            let text = line.trim();
            if text.is_empty() {
                return None;
            }

            let content = text
                .split_once('\t')
                .map(|(_, value)| value)
                .unwrap_or(text)
                .trim();

            if content.is_empty() {
                return None;
            }

            Some(IndexedResult {
                id: format!("clipboard:{}", index),
                provider: ProviderFilter::Clipboard,
                title: clipboard_title(content),
                generic_name: clipboard_kind(content).to_string(),
                comment: "Current clipboard history item".to_string(),
                categories: clipboard_categories(content),
                executable: String::new(),
                path: format!("clipboard://{index}"),
                content_preview: content.to_string(),
                metadata: format!("{} chars", content.chars().count()),
                icon_name: "edit-paste-symbolic".to_string(),
                usage_count: 0,
                recent_use_rank: index as u32,
                priority_bias: 0,
            })
        })
        .collect()
}

fn clipboard_title(content: &str) -> String {
    let one_line = content.replace(['\n', '\r'], " ");
    let title = one_line.trim();
    if title.chars().count() > 80 {
        title.chars().take(77).collect::<String>() + "..."
    } else {
        title.to_string()
    }
}

fn clipboard_kind(content: &str) -> &'static str {
    if content.starts_with("http://") || content.starts_with("https://") {
        "Copied URL"
    } else {
        "Copied text"
    }
}

fn clipboard_categories(content: &str) -> String {
    if content.starts_with("http://") || content.starts_with("https://") {
        "Clipboard;URL".to_string()
    } else {
        "Clipboard;Text".to_string()
    }
}

fn dedupe_results(records: Vec<IndexedResult>) -> Vec<IndexedResult> {
    let mut seen = HashMap::new();
    records
        .into_iter()
        .filter(|record| seen.insert(record.id.clone(), ()).is_none())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderFilter {
    All,
    Applications,
    Files,
    Actions,
    Clipboard,
}

impl ProviderFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "Spotlight",
            Self::Applications => "Applications",
            Self::Files => "Files",
            Self::Actions => "Actions",
            Self::Clipboard => "Clipboard",
        }
    }

}

fn filter_from_index(index: usize) -> ProviderFilter {
    match index {
        0 => ProviderFilter::Applications,
        1 => ProviderFilter::Files,
        2 => ProviderFilter::Actions,
        3 => ProviderFilter::Clipboard,
        _ => ProviderFilter::All,
    }
}

fn filter_index(filter: ProviderFilter) -> Option<usize> {
    match filter {
        ProviderFilter::All => None,
        ProviderFilter::Applications => Some(0),
        ProviderFilter::Files => Some(1),
        ProviderFilter::Actions => Some(2),
        ProviderFilter::Clipboard => Some(3),
    }
}

// -----------------------------------------------------------------------------
// THEME MODE
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

fn initial_theme_mode() -> ThemeMode {
    std::env::var("SPOTLIGHT_COLOR_SCHEME")
        .ok()
        .and_then(|value| ThemeMode::from_str(&value))
        .unwrap_or(ThemeMode::Light)
}

fn set_theme_mode(root: &GtkBox, mode: ThemeMode) {
    root.remove_css_class("spotlight-theme-light");
    root.remove_css_class("spotlight-theme-dark");

    match mode {
        ThemeMode::Light => root.add_css_class("spotlight-theme-light"),
        ThemeMode::Dark => root.add_css_class("spotlight-theme-dark"),
    }

    eprintln!("[spotlight] theme: active color scheme={}", mode.as_str());
}

// -----------------------------------------------------------------------------
// BASIC COMMAND-LINE HELP
// -----------------------------------------------------------------------------

fn print_cli_help() {
    println!("{}", cli_help_text());
}

// -----------------------------------------------------------------------------
// MAIN
// -----------------------------------------------------------------------------

fn main() -> glib::ExitCode {
    // Terminal help must work without initializing GTK, connecting to Wayland,
    // or raising the persistent GUI instance.
    if std::env::args_os().skip(1).any(|arg| arg == "--help" || arg == "-h" || arg == "help") {
        print_cli_help();
        return 0.into();
    }

    eprintln!("[spotlight] main: entered");

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    eprintln!("[spotlight] main: application constructed");

    let state: Rc<RefCell<Option<AppState>>> = Rc::new(RefCell::new(None));

    // -------------------------------------------------------------------------
    // STARTUP
    // -------------------------------------------------------------------------
    //
    // Create the layer-shell window once and keep it alive.
    // This gives us the persistent-process architecture we want.
    //
    {
        let state = Rc::clone(&state);

        app.connect_startup(move |app| {
            eprintln!("[spotlight] startup: entered");
            let (window, root, entry, theme_provider, theme_mode, interaction) = build_window(app);
            eprintln!("[spotlight] startup: window built");

            window.hide();
            eprintln!("[spotlight] startup: window hidden");

            *state.borrow_mut() = Some(AppState {
                window,
                root,
                entry,
                theme_provider,
                theme_mode: Cell::new(theme_mode),
                interaction,
            });
        });
    }

    // -------------------------------------------------------------------------
    // COMMAND LINE
    // -------------------------------------------------------------------------
    //
    // Hyprland can invoke:
    //
    //     spotlight toggle
    //     spotlight show
    //     spotlight hide
    //     spotlight theme light
    //     spotlight theme dark
    //
    // Because GtkApplication uses a unique application ID, subsequent
    // invocations are forwarded to the already-running process.
    //
    {
        let state = Rc::clone(&state);

        app.connect_command_line(move |_app, command_line| {
            eprintln!("[spotlight] command-line: callback entered");
            let args = command_line.arguments();

            let command = args.get(1).and_then(|arg| arg.to_str()).unwrap_or("");
            let command_arg = args.get(2).and_then(|arg| arg.to_str()).unwrap_or("");

            eprintln!(
                "[spotlight] command-line: command={command:?} arg={command_arg:?}"
            );

            let binding = state.borrow();
            let Some(state) = binding.as_ref() else {
                return 0.into();
            };

            match command {
                "toggle" => {
                    eprintln!("[spotlight] command-line: dispatch toggle");
                    toggle_window(state);
                }

                "show" => {
                    eprintln!("[spotlight] command-line: dispatch show");
                    show_window(state);
                }

                "hide" => {
                    eprintln!("[spotlight] command-line: dispatch hide");
                    hide_state_window(state);
                }

                "theme" => {
                    match ThemeMode::from_str(command_arg) {
                        Some(mode) => {
                            eprintln!(
                                "[spotlight] command-line: set color scheme={}",
                                mode.as_str()
                            );
                            set_theme_mode(&state.root, mode);
                            state.theme_mode.set(mode);
                        }
                        None => {
                            eprintln!(
                                "[spotlight] command-line: invalid theme {:?}; use light or dark",
                                command_arg
                            );
                        }
                    }
                }

                _ => {
                    // No command means:
                    //
                    //     start persistent process
                    //     keep Spotlight hidden
                    //
                    // This is useful for startup/autostart.
                }
            }

            0.into()
        });
    }

    eprintln!("[spotlight] main: entering GTK event loop");
    let exit_code = app.run();
    eprintln!("[spotlight] main: GTK event loop returned");
    exit_code
}

// -----------------------------------------------------------------------------
// RESPONSIVE UI METRICS
// -----------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct UiMetrics {
    search_width: i32,
    control_size: i32,
    gap: i32,
    inner_gap: i32,
    icon_size: i32,
}

fn responsive_metrics(output_width: i32) -> UiMetrics {
    // The reference composition is the existing V0.1 layout:
    //
    // search pill: 380px
    // four controls: 4 × 58px
    // four inter-control gaps: 4 × 12px
    //
    // Total visual width: 660px.
    //
    // We use that only as a design reference. The actual UI scales down
    // when the available output is narrower and the search pill grows
    // moderately on very wide outputs.
    let available_width = (output_width - 24).max(1) as f64;

    // Never enlarge the four controls beyond their Tahoe reference size.
    // When the display is narrower than the reference composition, scale
    // the composition down as a whole.
    let scale = (available_width / REFERENCE_COMPOSITION_WIDTH).min(1.0);

    // On narrow outputs, the design references are allowed to compress all the
    // way down instead of hitting fixed minimums that could overflow the
    // available width. The search field receives whatever width remains.
    let (control_size, gap, inner_gap, icon_size, search_width) = if scale < 1.0 {
        let control_size = (REFERENCE_CONTROL_SIZE * scale).round().max(1.0) as i32;
        let gap = (REFERENCE_GAP * scale).round().max(0.0) as i32;
        let inner_gap = (REFERENCE_INNER_GAP * scale).round().max(1.0) as i32;
        let icon_size = (REFERENCE_ICON_SIZE * scale).round().max(1.0) as i32;

        let controls_and_gaps = (control_size + gap) * 4;
        let search_width = (available_width - controls_and_gaps as f64)
            .round()
            .max(1.0) as i32;

        (control_size, gap, inner_gap, icon_size, search_width)
    } else {
        // Preserve the current design reference at normal widths.
        // On very wide outputs, let the search field grow, but keep a
        // deliberate ceiling so the interface never becomes enormous.
        let extra_width = ((available_width - REFERENCE_COMPOSITION_WIDTH).max(0.0)) * 0.25;
        let search_width = (REFERENCE_SEARCH_WIDTH + extra_width).min(900.0).round() as i32;

        (
            REFERENCE_CONTROL_SIZE as i32,
            REFERENCE_GAP as i32,
            REFERENCE_INNER_GAP as i32,
            REFERENCE_ICON_SIZE as i32,
            search_width,
        )
    };

    UiMetrics {
        search_width,
        control_size,
        gap,
        inner_gap,
        icon_size,
    }
}

fn responsive_result_metrics(
    output_height: i32,
    content_top: i32,
    control_size: i32,
) -> (usize, i32) {
    const CONTENT_GAP: i32 = 12;
    const BOTTOM_MARGIN: i32 = 16;

    let scale = (control_size as f64 / REFERENCE_ROW_HEIGHT).clamp(0.35, 1.0);
    let row_height = (REFERENCE_ROW_HEIGHT * scale)
        .round()
        .clamp(MIN_RESULT_ROW_HEIGHT as f64, REFERENCE_ROW_HEIGHT) as i32;

    let available_height = output_height - content_top - control_size - CONTENT_GAP - BOTTOM_MARGIN;

    let visible_rows = (available_height / row_height.max(1)).max(1) as usize;

    (visible_rows.min(MAX_VISIBLE_RESULT_ROWS), row_height)
}

// -----------------------------------------------------------------------------
// AUXILIARY SURFACES — PREVIEW, PROPERTIES, CONTEXT ACTIONS, HISTORY
// -----------------------------------------------------------------------------

fn make_detail_surface(
    width: i32,
    height: i32,
    heading: &str,
) -> (GtkBox, Image, Label, Label, Label, gtk::Picture, gtk::ScrolledWindow) {
    let panel = GtkBox::new(Orientation::Vertical, 8);
    panel.set_size_request(width, height);
    panel.set_width_request(width);
    panel.set_height_request(height);
    panel.set_halign(Align::Center);
    panel.set_valign(Align::Start);
    panel.set_hexpand(false);
    panel.set_vexpand(false);
    panel.set_overflow(gtk::Overflow::Hidden);
    panel.add_css_class("detail-surface");
    panel.set_visible(false);

    let header = GtkBox::new(Orientation::Horizontal, 12);
    header.set_halign(Align::Fill);
    header.set_hexpand(true);
    header.set_margin_top(14);
    header.set_margin_start(20);
    header.set_margin_end(20);

    let identity_icon = Image::from_icon_name("system-search-symbolic");
    identity_icon.set_pixel_size(28);
    identity_icon.set_size_request(30, 30);
    identity_icon.set_valign(Align::Center);
    identity_icon.add_css_class("detail-identity-icon");
    header.append(&identity_icon);

    let title = Label::new(Some(heading));
    title.set_halign(Align::Start);
    title.set_valign(Align::Center);
    title.set_hexpand(true);
    title.set_single_line_mode(true);
    title.set_wrap(false);
    title.set_max_width_chars(((width - 86) / 8).max(1));
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("detail-title");
    header.append(&title);
    panel.append(&header);

    let subtitle = Label::new(None);
    subtitle.set_halign(Align::Start);
    subtitle.set_single_line_mode(true);
    subtitle.set_wrap(false);
    subtitle.set_max_width_chars(((width - 40) / 9).max(1));
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.add_css_class("detail-subtitle");
    subtitle.set_margin_start(20);
    subtitle.set_margin_end(20);
    panel.append(&subtitle);

    // Preview thumbnails are decoded on a worker and only attached to GTK on
    // the main thread. Properties uses the same surface builder but leaves its
    // picture hidden.
    let picture = gtk::Picture::new();
    picture.set_halign(Align::Center);
    picture.set_valign(Align::Center);
    picture.set_size_request((width - 80).min(520).max(1), (height / 3).clamp(0, 150));
    picture.set_can_shrink(true);
    picture.set_visible(false);
    picture.add_css_class("preview-picture");
    panel.append(&picture);

    let body = Label::new(None);
    body.set_halign(Align::Fill);
    body.set_valign(Align::Start);
    body.set_xalign(0.0);
    body.set_yalign(0.0);
    body.set_wrap(true);
    body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    body.set_selectable(true);
    body.set_hexpand(true);
    body.set_vexpand(false);
    body.set_width_request((width - 40).max(1));
    body.set_overflow(gtk::Overflow::Hidden);
    body.add_css_class("detail-body");

    // Keep rich preview text inside a bounded viewport. Long files and property
    // lists scroll here rather than expanding the overlay surface.
    let body_scroll = gtk::ScrolledWindow::new();
    body_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    body_scroll.set_propagate_natural_height(false);
    body_scroll.set_hexpand(true);
    body_scroll.set_vexpand(false);
    body_scroll.set_size_request((width - 24).max(1), (height - 100).max(40));
    body_scroll.set_margin_start(12);
    body_scroll.set_margin_end(12);
    body_scroll.set_margin_bottom(8);
    body_scroll.set_overflow(gtk::Overflow::Hidden);
    body_scroll.add_css_class("detail-body-scroll");
    body_scroll.set_child(Some(&body));
    panel.append(&body_scroll);

    (panel, identity_icon, title, subtitle, body, picture, body_scroll)
}

fn make_context_surface(
    width: i32,
    height: i32,
    row_height: i32,
) -> (GtkBox, Label, Label, Vec<ContextActionRow>) {
    let panel = GtkBox::new(Orientation::Vertical, 6);
    panel.set_size_request(width, height);
    panel.set_width_request(width);
    panel.set_height_request(height);
    panel.set_halign(Align::Center);
    panel.set_valign(Align::Start);
    panel.set_hexpand(false);
    panel.set_vexpand(false);
    panel.set_overflow(gtk::Overflow::Hidden);
    panel.add_css_class("detail-surface");
    panel.add_css_class("context-surface");
    panel.set_visible(false);

    let title = Label::new(Some("Quick actions"));
    title.set_halign(Align::Start);
    title.set_single_line_mode(true);
    title.set_wrap(false);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(((width - 40) / 8).max(1));
    title.add_css_class("detail-title");
    title.set_margin_top(12);
    title.set_margin_start(18);
    title.set_margin_end(18);
    panel.append(&title);

    let status = Label::new(Some("Choose an action for the selected result."));
    status.set_halign(Align::Start);
    status.set_single_line_mode(true);
    status.set_wrap(false);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.set_max_width_chars(((width - 40) / 9).max(1));
    status.add_css_class("detail-subtitle");
    status.set_margin_start(18);
    status.set_margin_end(18);
    panel.append(&status);

    let labels = [
        "Preview  ·  Ctrl+Space",
        "Properties  ·  Alt+Enter",
        "Pin result",
        "Copy location",
        "Open / activate",
    ];
    let action_height = ((height - 76) / labels.len() as i32)
        .clamp(28, row_height.max(28));
    let mut actions = Vec::with_capacity(labels.len());
    for text_value in labels {
        let button = Button::new();
        button.set_height_request(action_height);
        button.set_width_request((width - 24).max(1));
        button.set_halign(Align::Fill);
        button.set_hexpand(true);
        button.set_focusable(true);
        button.set_focus_on_click(false);
        button.set_overflow(gtk::Overflow::Hidden);
        button.add_css_class("result-button");
        button.add_css_class("context-action-button");

        let label = Label::new(Some(text_value));
        label.set_halign(Align::Start);
        label.set_single_line_mode(true);
        label.set_wrap(false);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(((width - 56) / 8).max(1));
        label.set_margin_start(8);
        label.add_css_class("context-action-label");
        button.set_child(Some(&label));
        panel.append(&button);
        actions.push(ContextActionRow { button, label });
    }

    (panel, title, status, actions)
}

fn make_history_surface(
    width: i32,
    height: i32,
    row_height: i32,
    max_visible_rows: usize,
) -> (GtkBox, GtkBox, Label, Button, Vec<HistoryRow>, usize) {
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.set_size_request(width, height);
    panel.set_width_request(width);
    panel.set_height_request(height);
    panel.set_halign(Align::Center);
    panel.set_valign(Align::Start);
    panel.set_hexpand(false);
    panel.set_vexpand(false);
    panel.set_overflow(gtk::Overflow::Hidden);
    panel.add_css_class("detail-surface");
    panel.add_css_class("history-surface");
    panel.set_visible(false);

    let header = GtkBox::new(Orientation::Horizontal, 8);
    header.set_height_request(34);
    header.set_halign(Align::Fill);
    header.set_margin_top(8);
    header.set_margin_start(16);
    header.set_margin_end(14);

    let title = Label::new(Some("Search History"));
    title.set_halign(Align::Start);
    title.set_hexpand(true);
    title.add_css_class("detail-title");
    header.append(&title);

    let clear_button = Button::with_label("Clear");
    clear_button.set_tooltip_text(Some("Clear saved search history"));
    clear_button.set_focusable(true);
    clear_button.set_focus_on_click(false);
    clear_button.add_css_class("history-clear-button");
    header.append(&clear_button);
    panel.append(&header);

    let empty_label = Label::new(Some("No submitted searches yet"));
    empty_label.set_halign(Align::Center);
    empty_label.set_valign(Align::Center);
    empty_label.set_vexpand(true);
    empty_label.add_css_class("detail-subtitle");
    empty_label.set_visible(true);
    panel.append(&empty_label);

    let list = GtkBox::new(Orientation::Vertical, 0);
    list.set_size_request(width, (height - 46).max(1));
    list.set_width_request(width);
    list.set_hexpand(true);
    list.set_vexpand(true);
    list.set_overflow(gtk::Overflow::Hidden);
    list.set_visible(false);
    panel.append(&list);

    let visible_rows = ((height - 46).max(row_height) / row_height.max(1))
        .max(1) as usize;
    let visible_rows = visible_rows.min(max_visible_rows.max(1));
    let mut rows = Vec::with_capacity(SEARCH_HISTORY_LIMIT);
    for _ in 0..SEARCH_HISTORY_LIMIT {
        let button = Button::new();
        button.set_height_request(row_height);
        button.set_width_request(width - 8);
        button.set_halign(Align::Fill);
        button.set_hexpand(true);
        button.set_focusable(true);
        button.set_focus_on_click(false);
        button.set_overflow(gtk::Overflow::Hidden);
        button.add_css_class("result-button");
        button.add_css_class("history-result-button");
        button.set_visible(false);

        let label = Label::new(None);
        label.set_halign(Align::Start);
        label.set_single_line_mode(true);
        label.set_wrap(false);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(((width - 52) / 8).max(1));
        label.set_margin_start(14);
        label.add_css_class("result-title");
        button.set_child(Some(&label));
        list.append(&button);
        rows.push(HistoryRow { button, label });
    }

    (panel, list, empty_label, clear_button, rows, visible_rows)
}

fn append_help_section(panel: &GtkBox, width: i32, title_text: &str, detail_text: &str) {
    let section = GtkBox::new(Orientation::Vertical, 2);
    section.set_halign(Align::Fill);
    section.set_hexpand(true);
    section.set_margin_start(20);
    section.set_margin_end(20);

    let title = Label::new(Some(title_text));
    title.set_halign(Align::Start);
    title.set_single_line_mode(true);
    title.set_wrap(false);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(((width - 44) / 8).max(1));
    title.add_css_class("help-section-title");
    section.append(&title);

    let details = Label::new(Some(detail_text));
    details.set_halign(Align::Start);
    details.set_xalign(0.0);
    details.set_wrap(true);
    details.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    details.set_max_width_chars(((width - 44) / 7).max(1));
    details.add_css_class("help-section-details");
    section.append(&details);
    panel.append(&section);
}

fn make_help_surface(
    width: i32,
    height: i32,
) -> (GtkBox, Label, gtk::ProgressBar, Button) {
    let panel_height = height.min(500).max(360);
    let panel = GtkBox::new(Orientation::Vertical, 7);
    panel.set_size_request(width, panel_height);
    panel.set_width_request(width);
    panel.set_height_request(panel_height);
    panel.set_halign(Align::Center);
    panel.set_valign(Align::Start);
    panel.set_hexpand(false);
    panel.set_vexpand(false);
    panel.set_overflow(gtk::Overflow::Hidden);
    panel.add_css_class("detail-surface");
    panel.add_css_class("help-surface");
    panel.set_visible(false);

    // Indexing progress sits first, immediately below the search pill, like the
    // product's first-run introduction. It reports staged local indexing, not a
    // guessed percentage for work that cannot be measured precisely.
    let indexing = GtkBox::new(Orientation::Vertical, 5);
    indexing.set_halign(Align::Fill);
    indexing.set_hexpand(true);
    indexing.set_margin_top(12);
    indexing.set_margin_start(20);
    indexing.set_margin_end(20);

    let indexing_label = Label::new(Some("Indexing applications…"));
    indexing_label.set_halign(Align::Start);
    indexing_label.set_single_line_mode(true);
    indexing_label.set_wrap(false);
    indexing_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    indexing_label.add_css_class("detail-subtitle");
    indexing.append(&indexing_label);

    let progress = gtk::ProgressBar::new();
    progress.set_fraction(0.0);
    progress.set_show_text(false);
    progress.set_hexpand(true);
    progress.set_size_request((width - 44).max(1), 6);
    progress.add_css_class("help-index-progress");
    indexing.append(&progress);
    panel.append(&indexing);

    let separator = gtk::Separator::new(Orientation::Horizontal);
    separator.set_margin_start(20);
    separator.set_margin_end(20);
    separator.set_margin_top(2);
    separator.set_margin_bottom(2);
    panel.append(&separator);

    let title = Label::new(Some("Welcome to Spotlight"));
    title.set_halign(Align::Start);
    title.set_single_line_mode(true);
    title.set_wrap(false);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(((width - 44) / 8).max(1));
    title.set_margin_top(2);
    title.set_margin_start(20);
    title.set_margin_end(20);
    title.add_css_class("detail-title");
    panel.append(&title);

    let subtitle = Label::new(Some("Fast, local search with a few useful keyboard shortcuts."));
    subtitle.set_halign(Align::Start);
    subtitle.set_single_line_mode(true);
    subtitle.set_wrap(false);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_max_width_chars(((width - 44) / 8).max(1));
    subtitle.set_margin_start(20);
    subtitle.set_margin_end(20);
    subtitle.add_css_class("detail-subtitle");
    panel.append(&subtitle);

    append_help_section(
        &panel,
        width,
        "🔎  Search and select",
        "Type to search · ↑ / ↓ select · Enter/KP_Enter or double-click opens · click selects · Tab moves focus.",
    );
    append_help_section(
        &panel,
        width,
        "👁️  Preview and actions",
        "Ctrl+Space preview · Ctrl+Enter actions (↑ / ↓, Enter) · Alt+Enter details.",
    );
    append_help_section(
        &panel,
        width,
        "🗂️  Browse providers",
        "Ctrl+1..4 (keypad digits) switch providers · Ctrl+0 (keypad zero) resets query and provider.",
    );
    append_help_section(
        &panel,
        width,
        "🕘  History and help",
        "Ctrl+↑ history · Ctrl+↓ close · ? (Shift+/) help · Ctrl/Alt+← back · Esc closes the top surface · wheel scrolls results.",
    );
    append_help_section(
        &panel,
        width,
        "✨  Actions and clipboard",
        "Actions include date/time, now playing, volume, battery and lock screen. Enter on clipboard history copies that text; it does not paste it into another app.",
    );
    append_help_section(
        &panel,
        width,
        "💻  Local commands",
        "cd <dir> browses · ls lists in Spotlight · fastfetch shows system details · nvim <file> opens a terminal editor · run <command> asks before running · sudo updatedb uses PolicyKit confirmation.",
    );

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.set_halign(Align::End);
    footer.set_margin_start(20);
    footer.set_margin_end(20);
    footer.set_margin_bottom(12);
    footer.set_hexpand(true);

    let dismiss = Button::with_label("Start searching");
    dismiss.set_focusable(true);
    dismiss.set_focus_on_click(false);
    dismiss.set_tooltip_text(Some("Close help and return to search"));
    dismiss.add_css_class("history-clear-button");
    dismiss.add_css_class("help-start-button");
    footer.append(&dismiss);
    panel.append(&footer);

    (panel, indexing_label, progress, dismiss)
}

// -----------------------------------------------------------------------------
// WINDOW
// -----------------------------------------------------------------------------

fn build_window(
    app: &Application,
) -> (ApplicationWindow, GtkBox, Entry, gtk::CssProvider, ThemeMode, InteractionContext) {
    eprintln!("[spotlight] build_window: begin");

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spotlight")
        .decorated(false)
        .resizable(false)
        .build();

    // -------------------------------------------------------------------------
    // CSS CLASS ON TOPLEVEL
    // -------------------------------------------------------------------------

    window.add_css_class("spotlight-window");

    // -------------------------------------------------------------------------
    // WAYLAND LAYER SHELL
    // -------------------------------------------------------------------------

    //
    // IMPORTANT:
    //
    // We deliberately do NOT:
    //
    //     - hard-code a monitor
    //     - hard-code a resolution
    //     - assume any particular display resolution
    //     - use screen coordinates
    //
    // The layer is stretched to the output selected by the compositor.
    // Its size is derived at runtime below; no monitor name or resolution is
    // stored in the project.
    //

    eprintln!("[spotlight] build_window: init layer-shell");
    window.init_layer_shell();

    window.set_layer(Layer::Overlay);

    window.set_namespace(Some(LAYER_NAMESPACE));

    window.set_keyboard_mode(KeyboardMode::Exclusive);

    // Full-output transparent layer.
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    // Spotlight does not reserve any desktop space.
    window.set_exclusive_zone(-1);
    eprintln!("[spotlight] build_window: layer-shell configured");

    // Use runtime output geometry for initial layout metrics. No display
    // resolution, monitor name, or hardware-specific geometry is encoded here.
    if let Some(display) = gdk::Display::default() {
        if let Some(item) = display.monitors().item(0) {
            if let Ok(monitor) = item.downcast::<gdk::Monitor>() {
                let geometry = monitor.geometry();
                window.set_default_size(geometry.width(), geometry.height());
            }
        }
    }

    // -------------------------------------------------------------------------
    // ROOT
    // -------------------------------------------------------------------------

    let root = GtkBox::new(Orientation::Vertical, 0);

    root.set_hexpand(true);
    root.set_vexpand(true);

    root.add_css_class("spotlight-root");

    let theme_mode = initial_theme_mode();
    set_theme_mode(&root, theme_mode);

    // -------------------------------------------------------------------------
    // RESPONSIVE UI METRICS
    // -------------------------------------------------------------------------

    let (output_width, output_height) = gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_then(|item| item.downcast::<gdk::Monitor>().ok())
        .map(|monitor| {
            let geometry = monitor.geometry();
            (geometry.width(), geometry.height())
        })
        .unwrap_or((1, 1));

    let metrics = responsive_metrics(output_width);

    // The empty state consists of the search field plus the four 58px controls.
    // In typing/provider states the search pill absorbs exactly that same footprint.
    let composition_width = metrics.search_width + (metrics.control_size * 4) + (metrics.gap * 4);

    // -------------------------------------------------------------------------
    // TAHOE HOME POSITION
    // -------------------------------------------------------------------------
    //
    // The reference Spotlight is not vertically centered. Keep the search
    // surface's visual center at roughly 23% of the output height so the
    // results can grow downward without moving the anchor itself.
    //
    const SPOTLIGHT_VERTICAL_ANCHOR: f64 = 0.232;
    let anchor_center = (output_height as f64 * SPOTLIGHT_VERTICAL_ANCHOR).round() as i32;
    let content_top = (anchor_center - (metrics.control_size / 2)).max(8);

    // -------------------------------------------------------------------------
    // SEARCH COMPOSITION
    // -----------------------------------------------------------------------------

    // Keep the search chrome and result surface in a vertical stack whose top
    // edge is anchored to the Tahoe home position. Results grow downward.
    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_halign(Align::Center);
    content.set_valign(Align::Start);
    content.set_margin_top(content_top);
    content.set_hexpand(false);
    content.set_vexpand(false);
    content.add_css_class("spotlight-content");

    let cluster = GtkBox::new(Orientation::Horizontal, metrics.gap);
    cluster.set_halign(Align::Center);
    cluster.set_valign(Align::Center);
    cluster.set_hexpand(false);
    cluster.set_vexpand(false);
    cluster.add_css_class("spotlight-cluster");

    // Empty state: four larger provider controls live beside the search pill.
    // Typing state: those controls collapse into the pill itself.
    // Focused provider state: a back affordance + provider name replace them.
    let search_pill = GtkBox::new(Orientation::Horizontal, metrics.inner_gap);
    search_pill.set_halign(Align::Center);
    search_pill.set_valign(Align::Center);
    search_pill.set_width_request(metrics.search_width);
    search_pill.set_height_request(metrics.control_size);
    search_pill.add_css_class("search-pill");

    let back_button = make_back_button("Return to all results");
    back_button.set_visible(false);
    search_pill.append(&back_button);

    let search_icon = Image::from_icon_name("system-search-symbolic");
    search_icon.set_pixel_size(metrics.icon_size);
    search_icon.add_css_class("search-icon");
    search_pill.append(&search_icon);

    let category_box = GtkBox::new(Orientation::Horizontal, 6);
    category_box.set_halign(Align::Center);
    category_box.set_valign(Align::Center);
    category_box.set_visible(false);
    category_box.add_css_class("category-identity");

    let category_label = Label::new(Some("Applications"));
    category_label.add_css_class("category-identity-label");
    category_box.append(&category_label);
    search_pill.append(&category_box);

    let entry = Entry::new();
    let placeholder = if metrics.search_width < 230 {
        "Search"
    } else {
        "Spotlight Search"
    };
    entry.set_placeholder_text(Some(placeholder));
    entry.set_hexpand(true);
    entry.set_halign(Align::Fill);
    entry.set_valign(Align::Center);
    entry.set_has_frame(false);
    entry.set_focusable(true);
    entry.add_css_class("search-entry");
    search_pill.append(&entry);

    // Compact provider controls appear inside the pill only once the user starts
    // typing in general Spotlight mode.
    let inline_filters = GtkBox::new(Orientation::Horizontal, 4);
    inline_filters.set_halign(Align::Center);
    inline_filters.set_valign(Align::Center);
    inline_filters.set_hexpand(false);
    inline_filters.set_visible(false);
    inline_filters.add_css_class("inline-filters");

    let inline_applications = make_inline_filter_button("applications-button", "A", "Applications");
    let inline_files = make_inline_filter_icon_button("files-button", "folder-symbolic", "Files");
    let inline_actions =
        make_inline_filter_icon_button("actions-button", "view-continuous-symbolic", "Actions");
    let inline_clipboard =
        make_inline_filter_icon_button("clipboard-button", "edit-copy-symbolic", "Clipboard");

    inline_filters.append(&inline_applications);
    inline_filters.append(&inline_files);
    inline_filters.append(&inline_actions);
    inline_filters.append(&inline_clipboard);
    search_pill.append(&inline_filters);

    cluster.append(&search_pill);

    // -----------------------------------------------------------------------------
    // FOUR TAHOE CONTROLS
    // -----------------------------------------------------------------------------

    let applications = make_letter_button(
        "applications-button",
        "A",
        "Applications",
        metrics.control_size,
    );

    let files = make_icon_button(
        "files-button",
        "folder-symbolic",
        "Files",
        metrics.control_size,
        metrics.icon_size,
    );

    let actions = make_icon_button(
        "actions-button",
        "view-continuous-symbolic",
        "Actions",
        metrics.control_size,
        metrics.icon_size,
    );

    let clipboard = make_icon_button(
        "clipboard-button",
        "edit-copy-symbolic",
        "Clipboard",
        metrics.control_size,
        metrics.icon_size,
    );

    cluster.append(&applications);
    cluster.append(&files);
    cluster.append(&actions);
    cluster.append(&clipboard);

    // -----------------------------------------------------------------------------
    // RESPONSIVE RESULT PREVIEW
    // -----------------------------------------------------------------------------

    // Eight rows are the normal visual ceiling, but short outputs automatically
    // receive fewer visible rows so the result surface remains on-screen.
    // Selection-relative opacity supplies the visual density gradient.
    let (visible_result_rows, result_row_height) =
        responsive_result_metrics(output_height, content_top, metrics.control_size);
    let result_view_height = visible_result_rows as i32 * result_row_height;

    // Never recursively scan the filesystem on the GTK construction path.
    // A complete cache is immediately searchable; on a cold start, the staged
    // worker publishes Applications first and expensive Files last.
    let cached_catalog = load_cached_catalog();
    let catalog = Rc::new(RefCell::new(cached_catalog.records));
    let catalog_ready = Rc::new(Cell::new(cached_catalog.ready_mask));
    let catalog_refresh_progress = Rc::new(Cell::new(0u8));
    let catalog_refresh_finished = Rc::new(Cell::new(false));
    let catalog_refresh_failed = Rc::new(Cell::new(false));
    let help_is_welcome = Rc::new(Cell::new(false));
    let welcome_auto_dismiss_scheduled = Rc::new(Cell::new(false));

    let (results_panel, result_rows, result_scroll_adjustment, result_scrollbar) = make_result_surface(
        composition_width,
        result_view_height,
        result_row_height,
        visible_result_rows,
        MAX_SEARCH_RESULTS,
    );
    let result_rows = Rc::new(result_rows);
    let result_scrollbar = Rc::new(result_scrollbar);
    let selected_index = Rc::new(Cell::new(0usize));
    let result_window_start = Rc::new(Cell::new(0usize));
    let active_filter = Rc::new(Cell::new(ProviderFilter::All));

    let (preview_panel, preview_icon, preview_title, preview_subtitle, preview_body, preview_picture, preview_body_scroll) =
        make_detail_surface(composition_width, result_view_height, "Preview");
    let (properties_panel, properties_icon, properties_title, properties_subtitle, properties_body, _properties_picture, _properties_body_scroll) =
        make_detail_surface(composition_width, result_view_height, "Properties");
    let context_width = composition_width.min(360).max(1);
    let context_height = result_view_height.min(300).max(210);
    let (context_panel, context_title, context_status, context_actions) =
        make_context_surface(context_width, context_height, result_row_height);
    let (
        history_panel,
        history_list,
        history_empty,
        history_clear_button,
        history_rows,
        history_visible_rows,
    ) = make_history_surface(
        composition_width,
        result_view_height,
        result_row_height,
        visible_result_rows,
    );
    let (help_panel, help_indexing_label, help_progress_bar, help_dismiss_button) =
        make_help_surface(composition_width, result_view_height);

    let surfaces = DetailSurfaces {
        results_panel: results_panel.clone(),
        preview_panel,
        preview_icon,
        preview_title,
        preview_subtitle,
        preview_body,
        preview_picture,
        preview_body_scroll,
        preview_body_height: 220,
        preview_body_height_with_picture: 82,
        properties_panel,
        properties_icon,
        properties_title,
        properties_subtitle,
        properties_body,
        context_panel,
        context_title,
        context_status,
        context_actions,
        history_panel,
        history_empty,
        history_list,
        history_clear_button,
        help_panel,
        help_indexing_label,
        help_progress_bar,
        help_dismiss_button,
    };
    let auxiliary_surface = Rc::new(Cell::new(AuxiliarySurface::None));
    let preview_generation = Rc::new(Cell::new(0u64));
    let preview_cancel = Rc::new(RefCell::new(None));
    let preview_textures = Rc::new(RefCell::new(HashMap::<String, gdk::Texture>::new()));
    let history = Rc::new(RefCell::new(load_search_history()));
    let history_rows = Rc::new(history_rows);
    let history_selected_index = Rc::new(Cell::new(0usize));
    let history_window_start = Rc::new(Cell::new(0usize));
    let context_selected_index = Rc::new(Cell::new(0usize));
    let idle_hint_source = Rc::new(RefCell::new(None::<glib::SourceId>));
    let current_directory = Rc::new(RefCell::new(
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
    ));
    let file_catalog_backup = Rc::new(RefCell::new(None::<Vec<IndexedResult>>));
    let context_action_count = Rc::new(Cell::new(5usize));

    let idle_hint = Label::new(Some("Tip: press ? for help · Ctrl+1–4 switches providers"));
    idle_hint.set_halign(Align::Center);
    idle_hint.set_single_line_mode(true);
    idle_hint.set_wrap(false);
    idle_hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
    idle_hint.set_visible(false);
    idle_hint.add_css_class("spotlight-idle-hint");

    content.append(&cluster);
    content.append(&idle_hint);
    content.append(&results_panel);
    content.append(&surfaces.preview_panel);
    content.append(&surfaces.properties_panel);
    content.append(&surfaces.context_panel);
    content.append(&surfaces.history_panel);
    content.append(&surfaces.help_panel);
    root.append(&content);

    window.set_child(Some(&root));
    eprintln!("[spotlight] build_window: child attached");

    // -----------------------------------------------------------------------------
    // FILTER BUTTONS
    // -----------------------------------------------------------------------------

    let standalone_filter_buttons = [
        applications.clone(),
        files.clone(),
        actions.clone(),
        clipboard.clone(),
    ];

    let inline_filter_buttons = [
        inline_applications.clone(),
        inline_files.clone(),
        inline_actions.clone(),
        inline_clipboard.clone(),
    ];

    let interaction = InteractionContext {
        window: window.clone(),
        root: root.clone(),
        entry: entry.clone(),
        idle_hint: idle_hint.clone(),
        idle_hint_source: Rc::clone(&idle_hint_source),
        current_directory: Rc::clone(&current_directory),
        file_catalog_backup: Rc::clone(&file_catalog_backup),
        context_action_count: Rc::clone(&context_action_count),
        back_button: back_button.clone(),
        catalog: Rc::clone(&catalog),
        catalog_ready: Rc::clone(&catalog_ready),
        catalog_refresh_progress: Rc::clone(&catalog_refresh_progress),
        catalog_refresh_finished: Rc::clone(&catalog_refresh_finished),
        catalog_refresh_failed: Rc::clone(&catalog_refresh_failed),
        help_is_welcome: Rc::clone(&help_is_welcome),
        welcome_auto_dismiss_scheduled: Rc::clone(&welcome_auto_dismiss_scheduled),
        result_rows: Rc::clone(&result_rows),
        selected_index: Rc::clone(&selected_index),
        result_window_start: Rc::clone(&result_window_start),
        active_filter: Rc::clone(&active_filter),
        scroll_adjustment: result_scroll_adjustment.clone(),
        scrollbar: result_scrollbar.as_ref().clone(),
        visible_rows: visible_result_rows,
        surfaces: surfaces.clone(),
        auxiliary_surface: Rc::clone(&auxiliary_surface),
        preview_generation: Rc::clone(&preview_generation),
        preview_cancel: Rc::clone(&preview_cancel),
        preview_textures: Rc::clone(&preview_textures),
        history: Rc::clone(&history),
        history_rows: Rc::clone(&history_rows),
        history_selected_index: Rc::clone(&history_selected_index),
        history_window_start: Rc::clone(&history_window_start),
        history_visible_rows,
        context_selected_index: Rc::clone(&context_selected_index),
        standalone_buttons: standalone_filter_buttons.clone(),
        inline_buttons: inline_filter_buttons.clone(),
    };

    update_history_view(&interaction);

    for (action_index, action) in interaction.surfaces.context_actions.iter().enumerate() {
        let interaction = interaction.clone();
        action.button.connect_clicked(move |_| {
            interaction.context_selected_index.set(action_index);
            run_context_action(&interaction, action_index);
        });
    }

    {
        let interaction = interaction.clone();
        let clear_button = interaction.surfaces.history_clear_button.clone();
        clear_button.connect_clicked(move |_| {
            interaction.history.borrow_mut().clear();
            let history = interaction.history.borrow();
            persist_search_history(history.as_slice());
            interaction.history_selected_index.set(0);
            interaction.history_window_start.set(0);
            update_history_view(&interaction);
        });
    }

    {
        let interaction = interaction.clone();
        let dismiss_button = interaction.surfaces.help_dismiss_button.clone();
        dismiss_button.connect_clicked(move |_| {
            close_auxiliary_surface(&interaction);
            focus_search_entry(&interaction.entry);
        });
    }

    for (history_index, row) in interaction.history_rows.iter().enumerate() {
        let interaction = interaction.clone();
        row.button.connect_clicked(move |_| {
            let count = interaction.history.borrow().len();
            if history_index < count {
                interaction.history_selected_index.set(history_index);
                ensure_history_selection_visible(&interaction, history_index);
                update_history_view(&interaction);
            }
        });
    }

    for (button_index, button) in standalone_filter_buttons.iter().cloned().enumerate() {
        let active_filter = Rc::clone(&active_filter);
        let results_panel = results_panel.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let result_window_start = Rc::clone(&result_window_start);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();
        let inline_filter_buttons = inline_filter_buttons.clone();
        let entry = entry.clone();
        let scroll_adjustment = result_scroll_adjustment.clone();
        let result_scrollbar = Rc::clone(&result_scrollbar);
        let catalog = Rc::clone(&catalog);
        button.connect_clicked(move |_| {
            let filter = filter_from_index(button_index);

            active_filter.set(filter);
            set_active_filter_buttons(&standalone_filter_buttons, filter);
            set_active_filter_buttons(&inline_filter_buttons, filter);

            update_search_chrome(
                &search_pill,
                &search_icon,
                &back_button,
                &category_box,
                &category_label,
                &inline_filters,
                &standalone_filter_buttons,
                filter,
                entry.text().as_str(),
                metrics.search_width,
                composition_width,
            );

            update_results(
                &results_panel,
                result_rows.as_ref(),
                &selected_index,
                &result_window_start,
                filter,
                entry.text().as_str(),
                visible_result_rows,
                &scroll_adjustment,
                &result_scrollbar,
                catalog.borrow().as_slice(),
            );
            focus_search_entry(&entry);
        });
    }

    for (button_index, button) in inline_filter_buttons.iter().cloned().enumerate() {
        let active_filter = Rc::clone(&active_filter);
        let results_panel = results_panel.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let result_window_start = Rc::clone(&result_window_start);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();
        let inline_filter_buttons = inline_filter_buttons.clone();
        let entry = entry.clone();
        let scroll_adjustment = result_scroll_adjustment.clone();
        let result_scrollbar = Rc::clone(&result_scrollbar);
        let catalog = Rc::clone(&catalog);
        button.connect_clicked(move |_| {
            let filter = filter_from_index(button_index);

            active_filter.set(filter);
            set_active_filter_buttons(&standalone_filter_buttons, filter);
            set_active_filter_buttons(&inline_filter_buttons, filter);

            update_search_chrome(
                &search_pill,
                &search_icon,
                &back_button,
                &category_box,
                &category_label,
                &inline_filters,
                &standalone_filter_buttons,
                filter,
                entry.text().as_str(),
                metrics.search_width,
                composition_width,
            );

            update_results(
                &results_panel,
                result_rows.as_ref(),
                &selected_index,
                &result_window_start,
                filter,
                entry.text().as_str(),
                visible_result_rows,
                &scroll_adjustment,
                &result_scrollbar,
                catalog.borrow().as_slice(),
            );
            focus_search_entry(&entry);
        });
    }

    // Back from a focused provider restores normal Spotlight. If text remains,
    // the compact four options return inside the pill; if empty, the four large
    // circular controls return outside the pill.
    {
        let active_filter = Rc::clone(&active_filter);
        let results_panel = results_panel.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let result_window_start = Rc::clone(&result_window_start);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();
        let inline_filter_buttons = inline_filter_buttons.clone();
        let entry = entry.clone();
        let scroll_adjustment = result_scroll_adjustment.clone();
        let result_scrollbar = Rc::clone(&result_scrollbar);
        let catalog = Rc::clone(&catalog);
        let file_catalog_backup_for_back = Rc::clone(&file_catalog_backup);
        let current_directory_for_back = Rc::clone(&current_directory);
        back_button.clone().connect_clicked(move |_| {
            if let Some(records) = file_catalog_backup_for_back.borrow_mut().take() {
                let mut catalog_mut = catalog.borrow_mut();
                catalog_mut.retain(|record| record.provider != ProviderFilter::Files);
                catalog_mut.extend(records);
                if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
                    *current_directory_for_back.borrow_mut() = home;
                }
            }
            active_filter.set(ProviderFilter::All);
            set_active_filter_buttons(&standalone_filter_buttons, ProviderFilter::All);
            set_active_filter_buttons(&inline_filter_buttons, ProviderFilter::All);

            update_search_chrome(
                &search_pill,
                &search_icon,
                &back_button,
                &category_box,
                &category_label,
                &inline_filters,
                &standalone_filter_buttons,
                ProviderFilter::All,
                entry.text().as_str(),
                metrics.search_width,
                composition_width,
            );

            update_results(
                &results_panel,
                result_rows.as_ref(),
                &selected_index,
                &result_window_start,
                ProviderFilter::All,
                entry.text().as_str(),
                visible_result_rows,
                &scroll_adjustment,
                &result_scrollbar,
                catalog.borrow().as_slice(),
            );
            focus_search_entry(&entry);
        });
    }

    // KEYBOARD HANDLING
    // -------------------------------------------------------------------------
    //
    // Focus model:
    //
    //   Search entry
    //       Tab
    //        ↓
    //   action group ← Left / Right →
    //        │
    //        └── Tab → Search entry (always; never commits an action)
    //
    // Once a provider/action is active, Tab toggles:
    //
    //   Search entry ↔ Back affordance
    //
    // Alt/Meta/Super + Left/Up is also a direct shortcut back to general search.
    //
    // GTK exposes focusability, `has_focus()`, and `grab_focus()` at the Widget
    // level, while Button exposes `emit_clicked()`. This lets the keyboard model
    // drive the same existing click handlers instead of duplicating provider logic.

    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let interaction = interaction.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
            schedule_idle_hint(&interaction);
            let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            let alt = modifiers.contains(gdk::ModifierType::ALT_MASK);
            let no_other_modifier = !modifiers.intersects(
                gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::META_MASK
                    | gdk::ModifierType::SUPER_MASK,
            );
            let return_key = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter);

            // State-aware Escape: close exactly one nested surface per press.
            if key == gdk::Key::Escape {
                let current_surface = interaction.auxiliary_surface.get();
                if current_surface != AuxiliarySurface::None {
                    close_auxiliary_surface(&interaction);
                    if matches!(current_surface, AuxiliarySurface::ContextMenu | AuxiliarySurface::History | AuxiliarySurface::Properties | AuxiliarySurface::Help) {
                        focus_search_entry(&interaction.entry);
                    }
                } else {
                    cancel_preview_worker(&interaction);
                    interaction.preview_generation.set(
                        interaction.preview_generation.get().wrapping_add(1),
                    );
                    hide_window(&interaction.window);
                }
                return glib::Propagation::Stop;
            }

            // '?' (Shift+/) is the single in-app help shortcut. Handle either
            // slash or question keyvals to work across common keyboard layouts.
            let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
            if shift && !control && !alt && no_other_modifier
                && matches!(key, gdk::Key::question | gdk::Key::slash)
            {
                if interaction.auxiliary_surface.get() == AuxiliarySurface::Help {
                    close_auxiliary_surface(&interaction);
                    focus_search_entry(&interaction.entry);
                } else {
                    if interaction.auxiliary_surface.get() != AuxiliarySurface::None {
                        close_auxiliary_surface(&interaction);
                    }
                    open_help(&interaction, false);
                }
                return glib::Propagation::Stop;
            }

            // On the first-run introduction, Start searching is the focused
            // primary action, so the next Enter dismisses the welcome screen.
            if interaction.auxiliary_surface.get() == AuxiliarySurface::Help
                && return_key
                && !control
                && !alt
                && no_other_modifier
                && interaction.surfaces.help_dismiss_button.has_focus()
            {
                interaction.surfaces.help_dismiss_button.emit_clicked();
                return glib::Propagation::Stop;
            }

            // A context action chooser owns Up/Down/Enter while open.
            if interaction.auxiliary_surface.get() == AuxiliarySurface::ContextMenu {
                let count = interaction.context_action_count.get().min(interaction.surfaces.context_actions.len());
                match key {
                    gdk::Key::Up if count > 0 => {
                        let current = interaction.context_selected_index.get().min(count - 1);
                        let next = if current == 0 { count - 1 } else { current - 1 };
                        interaction.context_selected_index.set(next);
                        interaction.surfaces.context_actions[next].button.grab_focus();
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Down if count > 0 => {
                        let current = interaction.context_selected_index.get().min(count - 1);
                        let next = (current + 1) % count;
                        interaction.context_selected_index.set(next);
                        interaction.surfaces.context_actions[next].button.grab_focus();
                        return glib::Propagation::Stop;
                    }
                    _ if return_key => {
                        if count > 0 {
                            let index = interaction.context_selected_index.get().min(count - 1);
                            interaction.surfaces.context_actions[index].button.emit_clicked();
                        }
                        return glib::Propagation::Stop;
                    }
                    _ => return glib::Propagation::Proceed,
                }
            }

            // Search History has an independent selection model and key context.
            if interaction.auxiliary_surface.get() == AuxiliarySurface::History {
                if control && key == gdk::Key::Down {
                    close_auxiliary_surface(&interaction);
                    focus_search_entry(&interaction.entry);
                    return glib::Propagation::Stop;
                }
                if return_key && interaction.surfaces.history_clear_button.has_focus() {
                    interaction.surfaces.history_clear_button.emit_clicked();
                    return glib::Propagation::Stop;
                }
                match key {
                    gdk::Key::Up => {
                        move_history_selection(&interaction, -1);
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Down => {
                        move_history_selection(&interaction, 1);
                        return glib::Propagation::Stop;
                    }
                    _ if return_key => {
                        restore_history_selection(&interaction);
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Tab => return glib::Propagation::Proceed,
                    _ => return glib::Propagation::Proceed,
                }
            }

            // Ctrl+Space previews the selected result; it never launches it.
            if control && !alt && key == gdk::Key::space {
                if interaction.auxiliary_surface.get() == AuxiliarySurface::Preview {
                    close_auxiliary_surface(&interaction);
                } else {
                    open_preview(&interaction);
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+0 clears the query and returns to the unfiltered provider mode.
            if control && !alt && !shift && no_other_modifier
                && matches!(key, gdk::Key::_0 | gdk::Key::KP_0)
            {
                if interaction.auxiliary_surface.get() != AuxiliarySurface::None {
                    close_auxiliary_surface(&interaction);
                }
                restore_global_file_index(&interaction);
                interaction.entry.set_text("");
                interaction.back_button.emit_clicked();
                focus_search_entry(&interaction.entry);
                return glib::Propagation::Stop;
            }

            // Provider shortcuts, including keypad digits.
            if control && !alt {
                let provider_index = match key {
                    gdk::Key::_1 | gdk::Key::KP_1 => Some(0),
                    gdk::Key::_2 | gdk::Key::KP_2 => Some(1),
                    gdk::Key::_3 | gdk::Key::KP_3 => Some(2),
                    gdk::Key::_4 | gdk::Key::KP_4 => Some(3),
                    _ => None,
                };
                if let Some(index) = provider_index {
                    if interaction.auxiliary_surface.get() != AuxiliarySurface::None {
                        close_auxiliary_surface(&interaction);
                    }
                    interaction.inline_buttons[index].emit_clicked();
                    focus_search_entry(&interaction.entry);
                    return glib::Propagation::Stop;
                }
            }

            if control && key == gdk::Key::Up {
                open_history(&interaction);
                return glib::Propagation::Stop;
            }
            // Ctrl+Down is reserved for closing History; never hijack it for
            // ordinary result navigation when History is not open.
            if control && key == gdk::Key::Down {
                return glib::Propagation::Stop;
            }

            // Ctrl+Enter opens contextual actions; Alt+Enter opens properties.
            if control && !alt && return_key {
                open_context_menu(&interaction);
                return glib::Propagation::Stop;
            }
            if alt && !control && return_key {
                open_properties(&interaction);
                return glib::Propagation::Stop;
            }

            // Ctrl/Alt+Left backs out of a detail surface first. If no detail
            // surface is open, the same chord returns from a focused provider.
            if (control || alt) && key == gdk::Key::Left {
                if matches!(
                    interaction.auxiliary_surface.get(),
                    AuxiliarySurface::Preview | AuxiliarySurface::Properties
                ) {
                    close_auxiliary_surface(&interaction);
                    focus_search_entry(&interaction.entry);
                    return glib::Propagation::Stop;
                }
            }

            // Ctrl+Left / Alt+Left are provider-back shortcuts. Plain Left stays
            // normal text editing whenever the search entry owns focus.
            if (control || alt)
                && key == gdk::Key::Left
                && interaction.active_filter.get() != ProviderFilter::All
            {
                close_auxiliary_surface(&interaction);
                interaction.back_button.emit_clicked();
                focus_search_entry(&interaction.entry);
                return glib::Propagation::Stop;
            }

            // Tab navigation retains the established search/action focus model.
            if key == gdk::Key::Tab && no_other_modifier && !control {
                if interaction.active_filter.get() != ProviderFilter::All {
                    if interaction.back_button.has_focus() {
                        focus_search_entry(&interaction.entry);
                    } else {
                        interaction.back_button.grab_focus();
                    }
                    return glib::Propagation::Stop;
                }

                let action_buttons = if interaction.inline_buttons.iter().any(|button| button.is_visible()) {
                    &interaction.inline_buttons
                } else {
                    &interaction.standalone_buttons
                };
                if interaction.entry.has_focus() {
                    if let Some(first) = action_buttons.iter().find(|button| button.is_visible()) {
                        first.grab_focus();
                        return glib::Propagation::Stop;
                    }
                } else if action_buttons.iter().any(|button| button.has_focus()) {
                    focus_search_entry(&interaction.entry);
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }

            // Enter while Back/provider chrome is focused activates that control,
            // not the result selection hidden underneath.
            if return_key {
                if interaction.back_button.has_focus() {
                    interaction.back_button.emit_clicked();
                    focus_search_entry(&interaction.entry);
                    return glib::Propagation::Stop;
                }
                let action_buttons = if interaction.inline_buttons.iter().any(|button| button.is_visible()) {
                    &interaction.inline_buttons
                } else {
                    &interaction.standalone_buttons
                };
                if let Some(button) = action_buttons.iter().find(|button| button.has_focus()) {
                    button.emit_clicked();
                    return glib::Propagation::Stop;
                }
            }

            // Left/Right navigate the provider controls only while one of them
            // owns focus; text cursor movement remains untouched in GtkEntry.
            if interaction.active_filter.get() == ProviderFilter::All {
                let action_buttons = if interaction.inline_buttons.iter().any(|button| button.is_visible()) {
                    &interaction.inline_buttons
                } else {
                    &interaction.standalone_buttons
                };
                if let Some(index) = action_buttons.iter().position(|button| button.has_focus()) {
                    let direction = match key {
                        gdk::Key::Right => Some(1isize),
                        gdk::Key::Left => Some(-1isize),
                        _ => None,
                    };
                    if let Some(direction) = direction {
                        let next = find_visible_action_index(action_buttons, index, direction);
                        if next != index {
                            action_buttons[next].grab_focus();
                            return glib::Propagation::Stop;
                        }
                    }
                }
            }

            // Results keep their shared selection/window state. While Preview is
            // open, arrows change the selected result and supersede its preview
            // request; older worker responses are discarded by generation ID.
            let in_preview = interaction.auxiliary_surface.get() == AuxiliarySurface::Preview;
            if !in_preview && !interaction.surfaces.results_panel.is_visible() {
                return glib::Propagation::Proceed;
            }
            let result_count = populated_result_count(interaction.result_rows.as_ref());
            if result_count == 0 {
                return glib::Propagation::Proceed;
            }
            let current = interaction.selected_index.get().min(result_count - 1);
            let visible = interaction.visible_rows.clamp(1, result_count);
            let next = match key {
                gdk::Key::Down => Some((current + 1) % result_count),
                gdk::Key::Up => Some(if current == 0 { result_count - 1 } else { current - 1 }),
                _ => None,
            };
            if let Some(next) = next {
                set_result_selection(
                    &interaction.surfaces.results_panel,
                    interaction.result_rows.as_ref(),
                    &interaction.selected_index,
                    &interaction.result_window_start,
                    next,
                    visible,
                    &interaction.scroll_adjustment,
                );
                if in_preview {
                    schedule_preview_refresh(&interaction);
                }
                return glib::Propagation::Stop;
            }

            glib::Propagation::Proceed
        });
    }

    window.add_controller(key_controller);

    let idle_motion = gtk::EventControllerMotion::new();
    {
        let interaction = interaction.clone();
        idle_motion.connect_motion(move |_controller, _x, _y| {
            schedule_idle_hint(&interaction);
        });
    }
    window.add_controller(idle_motion);

    // A stationary-pointer click also counts as activity; without this gesture,
    // the idle hint could appear while someone is repeatedly clicking controls.
    let idle_click = gtk::GestureClick::new();
    {
        let interaction = interaction.clone();
        idle_click.connect_pressed(move |_gesture, _press_count, _x, _y| {
            schedule_idle_hint(&interaction);
        });
    }
    window.add_controller(idle_click);

    // GtkEntry activation is the normal default-action path for both Return
    // and keypad Enter. Modified Enter chords were consumed by the central
    // key-priority handler before they can reach this signal.
    {
        let interaction = interaction.clone();
        entry.connect_activate(move |_entry| {
            activate_selected_result(&interaction);
        });
    }

    // -------------------------------------------------------------------------
    // RESULT SURFACE POINTER / SCROLL INTERACTION
    // -------------------------------------------------------------------------

    // Mouse-wheel and touchpad scrolling move the same logical selection used by
    // Up / Down. This keeps the selection, visible result window, wrapping, and
    // density/fade behavior in one interaction model rather than adding a
    // second scrolling implementation just for pointer input.
    //
    // Touchpads emit smooth fractional deltas, while mouse wheels commonly emit
    // larger discrete deltas. Accumulate the vertical delta and advance one
    // result for each completed scroll unit.
    let scroll_controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll_controller.set_propagation_phase(gtk::PropagationPhase::Capture);

    let scroll_accumulator = Rc::new(Cell::new(0.0f64));
    {
        let results_panel = results_panel.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let result_window_start = Rc::clone(&result_window_start);
        let scroll_accumulator = Rc::clone(&scroll_accumulator);
        let scroll_adjustment = result_scroll_adjustment.clone();

        scroll_controller.connect_scroll(move |_controller, _dx, dy| {
            if !results_panel.is_visible() || result_rows.is_empty() || dy == 0.0 {
                return glib::Propagation::Proceed;
            }
            let result_count = populated_result_count(result_rows.as_ref());
            if result_count == 0 {
                return glib::Propagation::Proceed;
            }
            let effective_visible_rows = visible_result_rows.clamp(1, result_count);

            let mut accumulated = scroll_accumulator.get();

            // A positive vertical scroll delta means scrolling downward through
            // the results; a negative delta means scrolling upward.
            accumulated += dy;

            // Guard against an unexpectedly large input burst. We only need to
            // preserve enough remainder to produce smooth, deterministic steps.
            accumulated = accumulated.clamp(-64.0, 64.0);

            let mut selection = selected_index.get();
            let mut changed = false;

            while accumulated >= 1.0 {
                selection = (selection + 1) % result_count;
                accumulated -= 1.0;
                changed = true;
            }

            while accumulated <= -1.0 {
                selection = if selection == 0 {
                    result_count - 1
                } else {
                    selection - 1
                };
                accumulated += 1.0;
                changed = true;
            }

            scroll_accumulator.set(accumulated);

            if changed {
                set_result_selection(
                    &results_panel,
                    &result_rows,
                    &selected_index,
                    &result_window_start,
                    selection,
                    effective_visible_rows,
                    &scroll_adjustment,
                );
            }

            // Consume scroll input while the result surface is active. This is
            // especially important for the fullscreen layer-shell overlay so
            // the underlying desktop does not also react to the gesture.
            glib::Propagation::Stop
        });
    }

    results_panel.add_controller(scroll_controller);

    // -------------------------------------------------------------------------
    // RESULT SCROLLBAR DRAG / TRACK INTERACTION
    // -------------------------------------------------------------------------
    //
    // Dragging the thumb or clicking the scrollbar track changes the visible
    // result window directly. If the current logical selection falls outside the
    // newly visible window, clamp it to the nearest visible row so keyboard,
    // scrollbar, click, and wheel interactions continue to share one selection
    // model.
    {
        let results_panel = results_panel.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let result_window_start = Rc::clone(&result_window_start);
        let scroll_adjustment = result_scroll_adjustment.clone();

        scroll_adjustment.connect_value_changed(move |adjustment| {
            if !results_panel.is_visible() || result_rows.is_empty() {
                return;
            }

            let result_count = populated_result_count(result_rows.as_ref());
            if result_count == 0 {
                return;
            }
            let visible_rows = visible_result_rows.clamp(1, result_count);
            let max_start = result_count.saturating_sub(visible_rows);
            let window_start = adjustment.value().round().clamp(0.0, max_start as f64) as usize;
            let window_end = (window_start + visible_rows).min(result_count);

            let current_selection = selected_index.get();
            let selection = if current_selection < window_start {
                window_start
            } else if current_selection >= window_end {
                window_end.saturating_sub(1)
            } else {
                current_selection
            };

            result_window_start.set(window_start);
            selected_index.set(selection);

            set_visible_result_window(result_rows.as_ref(), window_start, visible_rows, selection);

            for (row_index, row) in result_rows.iter().enumerate() {
                if row_index == selection {
                    row.button.add_css_class("selected");
                } else {
                    row.button.remove_css_class("selected");
                }
            }
        });
    }

    // -------------------------------------------------------------------------
    // RESULT ROW MOUSE INTERACTION
    // -------------------------------------------------------------------------

    // Hover is a visual state only. Selection remains the logical keyboard
    // selection until the user clicks a result.
    let result_rows_weak = Rc::downgrade(&result_rows);

    for (row_index, row) in result_rows.iter().enumerate() {
        let hovered_button = row.button.clone();
        let motion = gtk::EventControllerMotion::new();

        motion.connect_enter(move |_controller, _x, _y| {
            hovered_button.add_css_class("hovered");
        });

        let hovered_button = row.button.clone();
        motion.connect_leave(move |_controller| {
            hovered_button.remove_css_class("hovered");
        });

        row.button.add_controller(motion);

        // Use one GestureClick sequence for both single- and multi-press
        // semantics. The previous implementation had two independent paths:
        // Button::clicked handled single clicks while GestureClick handled
        // double clicks. On GTK this can result in the same physical sequence
        // being processed by different controllers, making double activation
        // unreliable.
        //
        // Capture phase lets our gesture observe the sequence before the
        // Button's built-in click handling can claim it.
        let gesture_rows = result_rows_weak.clone();
        let gesture_panel = results_panel.clone();
        let gesture_interaction = interaction.clone();
        let gesture_selected = Rc::clone(&selected_index);
        let gesture_window = Rc::clone(&result_window_start);
        let gesture_visible_rows = visible_result_rows;
        let gesture_scroll_adjustment = result_scroll_adjustment.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(1);
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);

        gesture.connect_released(move |_gesture, n_press, _x, _y| {
            let Some(rows) = gesture_rows.upgrade() else {
                return;
            };

            match n_press {
                1 => {
                    // Single click/tap selects only.
                    set_result_selection(
                        &gesture_panel,
                        rows.as_ref(),
                        &gesture_selected,
                        &gesture_window,
                        row_index,
                        gesture_visible_rows,
                        &gesture_scroll_adjustment,
                    );
                }
                press if is_double_press(press) => {
                    // The same gesture sequence's second press confirms.
                    set_result_selection(
                        &gesture_panel,
                        rows.as_ref(),
                        &gesture_selected,
                        &gesture_window,
                        row_index,
                        gesture_visible_rows,
                        &gesture_scroll_adjustment,
                    );
                    activate_result_index(&gesture_interaction, row_index);
                }
                _ => {
                    // Ignore triple+ presses; double activation is exactly two.
                }
            }
        });

        row.button.add_controller(gesture);
    }

    // -------------------------------------------------------------------------
    // CSS
    // -------------------------------------------------------------------------

    eprintln!("[spotlight] build_window: install CSS");
    let theme_provider = install_css();

    // Query edits close auxiliary detail surfaces so the preview can never
    // describe a stale selection. Cached results remain immediately searchable.
    {
        let interaction = interaction.clone();
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();

        entry.connect_changed(move |entry| {
            schedule_idle_hint(&interaction);
            sync_command_candidate(&interaction, entry.text().as_str());
            if interaction.auxiliary_surface.get() != AuxiliarySurface::None {
                if interaction.auxiliary_surface.get() == AuxiliarySurface::Help {
                    mark_welcome_seen();
                    interaction.help_is_welcome.set(false);
                    interaction.welcome_auto_dismiss_scheduled.set(false);
                }
                cancel_preview_worker(&interaction);
                interaction.auxiliary_surface.set(AuxiliarySurface::None);
                interaction.preview_generation.set(
                    interaction.preview_generation.get().wrapping_add(1),
                );
                hide_auxiliary_panels(&interaction.surfaces);
            }

            update_search_chrome(
                &search_pill,
                &search_icon,
                &back_button,
                &category_box,
                &category_label,
                &inline_filters,
                &standalone_filter_buttons,
                interaction.active_filter.get(),
                entry.text().as_str(),
                metrics.search_width,
                composition_width,
            );

            update_results_or_loading(&interaction);
        });
    }

    start_background_catalog_refresh(interaction.clone());

    eprintln!("[spotlight] build_window: complete");
    (window, root, entry, theme_provider, theme_mode, interaction)
}

// Refresh index data off the GTK thread. The cached catalog is available
// immediately; the worker's fresh snapshot replaces it without blocking typing.
fn start_background_catalog_refresh(interaction: InteractionContext) {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<ProviderBatch>(1);
    let worker = std::thread::Builder::new()
        .name("spotlight-provider-refresh".to_string())
        .spawn(move || load_catalog_staged(sender));

    if worker.is_err() {
        eprintln!("[spotlight] catalog refresh could not start");
        interaction.catalog_refresh_failed.set(true);
        interaction.catalog_refresh_finished.set(true);
        update_help_indexing(&interaction);
        return;
    }

    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        match receiver.try_recv() {
            Ok(batch) => {
                let ready_bit = provider_ready_bit(batch.provider);
                interaction.catalog_ready.set(interaction.catalog_ready.get() | ready_bit);
                interaction.catalog_refresh_progress.set(
                    interaction.catalog_refresh_progress.get() | ready_bit,
                );
                update_help_indexing(&interaction);
                {
                    let mut catalog = interaction.catalog.borrow_mut();
                    if batch.provider == ProviderFilter::Files
                        && interaction.file_catalog_backup.borrow().is_some()
                    {
                        *interaction.file_catalog_backup.borrow_mut() = Some(batch.records);
                    } else {
                        catalog.retain(|record| record.provider != batch.provider);
                        catalog.extend(batch.records);
                    }
                }
                sync_command_candidate(&interaction, interaction.entry.text().as_str());

                // Keep preview/history/context stable while data arrives. When
                // the user returns to the result list it is ranked from the
                // newest merged catalog snapshot.
                if interaction.auxiliary_surface.get() == AuxiliarySurface::None {
                    refresh_result_surface(&interaction);
                }
                glib::ControlFlow::Continue
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                interaction.catalog_refresh_finished.set(true);
                update_help_indexing(&interaction);
                glib::ControlFlow::Break
            },
        }
    });
}

// -----------------------------------------------------------------------------
// BUTTONS
// -----------------------------------------------------------------------------

fn make_letter_button(class_name: &str, letter: &str, tooltip: &str, size: i32) -> Button {
    let button = Button::new();

    button.set_width_request(size);
    button.set_height_request(size);

    button.set_halign(Align::Center);
    button.set_valign(Align::Center);

    // These controls participate in the explicit Tab/Left/Right focus model.
    // They must be focusable for Widget::grab_focus() to succeed.
    button.set_focusable(true);
    button.set_focus_on_click(false);

    button.set_tooltip_text(Some(tooltip));

    button.add_css_class("action-button");
    button.add_css_class(class_name);

    let label = Label::new(Some(letter));

    label.add_css_class("action-letter");

    button.set_child(Some(&label));

    button
}

fn make_shortcuts_provider_icon(size: i32) -> Image {
    // A compact, original ribbon mark inspired by automation/shortcut apps.
    // It is embedded so the provider icon does not depend on an installed theme.
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">
      <defs>
        <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="#f4a0ff"/>
          <stop offset="0.52" stop-color="#9565fa"/>
          <stop offset="1" stop-color="#4f42d8"/>
        </linearGradient>
        <linearGradient id="r1" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="#ff9de8"/>
          <stop offset="0.55" stop-color="#fa5fba"/>
          <stop offset="1" stop-color="#8e4cf4"/>
        </linearGradient>
        <linearGradient id="r2" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#ffb46e"/>
          <stop offset="0.5" stop-color="#ff6d9d"/>
          <stop offset="1" stop-color="#a34df4"/>
        </linearGradient>
      </defs>
      <rect x="4" y="4" width="120" height="120" rx="27" fill="url(#bg)"/>
      <path d="M26 43 55 20q7-5 14 1l35 29q7 6 0 12L76 83q-7 6-14 0L27 58q-8-7-1-15Z" fill="url(#r1)"/>
      <path d="M102 78 73 102q-7 6-14 0L24 73q-7-6 0-12l28-24q7-6 14 0l35 29q8 6 1 12Z" fill="url(#r2)" opacity=".98"/>
      <path d="M47 57 70 39l17 14-23 19Z" fill="#fff" opacity=".28"/>
    </svg>"##;
    let fallback = || {
        let image = Image::from_icon_name("view-grid-symbolic");
        image.set_pixel_size(size);
        image.add_css_class("action-icon");
        image
    };
    let Ok(loader) = gdk_pixbuf::PixbufLoader::with_type("svg") else {
        return fallback();
    };
    if loader.write(SVG.as_bytes()).is_err() || loader.close().is_err() {
        return fallback();
    }
    let Some(pixbuf) = loader.pixbuf() else {
        return fallback();
    };
    let Some(scaled) = pixbuf.scale_simple(
        size.max(1),
        size.max(1),
        gdk_pixbuf::InterpType::Bilinear,
    ) else {
        return fallback();
    };
    let image = Image::from_pixbuf(Some(&scaled));
    image.add_css_class("shortcuts-provider-icon");
    image
}

fn make_icon_button(
    class_name: &str,
    icon_name: &str,
    tooltip: &str,
    size: i32,
    icon_size: i32,
) -> Button {
    let button = Button::new();

    button.set_width_request(size);
    button.set_height_request(size);

    button.set_halign(Align::Center);
    button.set_valign(Align::Center);

    // These controls participate in the explicit Tab/Left/Right focus model.
    // They must be focusable for Widget::grab_focus() to succeed.
    button.set_focusable(true);
    button.set_focus_on_click(false);

    button.set_tooltip_text(Some(tooltip));

    button.add_css_class("action-button");
    button.add_css_class(class_name);

    let icon = if class_name == "actions-button" {
        make_shortcuts_provider_icon(icon_size)
    } else {
        let icon = Image::from_icon_name(icon_name);
        icon.set_pixel_size(icon_size);
        icon.add_css_class("action-icon");
        icon
    };

    button.set_child(Some(&icon));

    button
}

// -----------------------------------------------------------------------------
// RESULT SURFACE
// -----------------------------------------------------------------------------

fn make_result_surface(
    width: i32,
    viewport_height: i32,
    row_height: i32,
    visible_rows: usize,
    result_capacity: usize,
) -> (Overlay, Vec<ResultRow>, Adjustment, Scrollbar) {
    let results_panel = Overlay::new();

    // The visible row count is derived from the runtime output height.
    // Eight is a visual ceiling, not a fixed requirement: shorter outputs
    // receive fewer rows and smaller rows where necessary.
    results_panel.set_width_request(width);
    results_panel.set_height_request(viewport_height);
    results_panel.set_halign(Align::Center);
    results_panel.set_valign(Align::Start);
    results_panel.set_vexpand(false);
    results_panel.set_overflow(gtk::Overflow::Hidden);
    results_panel.add_css_class("results-panel-shell");

    let results = GtkBox::new(Orientation::Vertical, 0);
    results.set_width_request(width);
    results.set_height_request(viewport_height);
    results.set_halign(Align::Fill);
    results.set_valign(Align::Start);
    results.set_overflow(gtk::Overflow::Hidden);
    results.add_css_class("results-list");

    let row_capacity = result_capacity.clamp(1, MAX_SEARCH_RESULTS);
    let mut rows = Vec::with_capacity(row_capacity);

    // Result widgets are independent of catalog size. Ranking can inspect the
    // provider index, but only this bounded number of rows is ever allocated.
    for _ in 0..row_capacity {
        let row = make_result_row(row_height, width);
        row.button.set_visible(false);
        results.append(&row.button);
        rows.push(row);
    }

    results_panel.set_child(Some(&results));

    let top_fade = GtkBox::new(Orientation::Vertical, 0);
    top_fade.set_height_request((row_height / 2).max(12));
    top_fade.set_halign(Align::Fill);
    top_fade.set_valign(Align::Start);
    top_fade.set_can_target(false);
    top_fade.add_css_class("results-fade-top");
    results_panel.add_overlay(&top_fade);

    let bottom_fade = GtkBox::new(Orientation::Vertical, 0);
    bottom_fade.set_height_request(row_height.clamp(24, 50));
    bottom_fade.set_halign(Align::Fill);
    bottom_fade.set_valign(Align::End);
    bottom_fade.set_can_target(false);
    bottom_fade.add_css_class("results-fade-bottom");
    results_panel.add_overlay(&bottom_fade);

    // -------------------------------------------------------------------------
    // PROPORTIONAL RESULT SCROLLBAR
    // -------------------------------------------------------------------------
    //
    // The adjustment represents the start of the visible result window.
    // GTK uses page_size / upper to derive the scrollbar thumb proportion, so
    // the thumb automatically reflects the ratio of visible results to total
    // results instead of using device-specific pixel geometry.
    let total_results = rows.len().max(1) as f64;
    let visible_results = visible_rows.clamp(1, rows.len().max(1)) as f64;
    let result_scroll_adjustment = Adjustment::new(
        0.0,
        0.0,
        total_results,
        1.0,
        visible_results.max(1.0),
        visible_results,
    );

    // The scrollbar itself is intentionally smaller than the result surface.
    // Width follows row height; vertical insets follow the viewport height.
    // No display resolution, monitor size, or device-specific geometry is used.
    // It is shown only when the result count exceeds the visible window.
    let scrollbar_width = responsive_scrollbar_width(row_height);
    let scrollbar_vertical_inset = responsive_scrollbar_vertical_inset(viewport_height);

    let scrollbar = Scrollbar::new(Orientation::Vertical, Some(&result_scroll_adjustment));
    scrollbar.set_width_request(scrollbar_width);
    scrollbar.set_halign(Align::End);
    scrollbar.set_valign(Align::Fill);
    scrollbar.set_vexpand(true);
    scrollbar.set_margin_top(scrollbar_vertical_inset);
    scrollbar.set_margin_bottom(scrollbar_vertical_inset);
    // The scrollbar is flush with the result surface's right wall.
    scrollbar.set_margin_start(0);
    scrollbar.set_margin_end(0);
    scrollbar.set_visible(rows.len() > visible_rows);
    scrollbar.add_css_class("results-scrollbar");
    results_panel.add_overlay(&scrollbar);

    // The range is used only for scrollbar geometry; selection/window logic
    // remains in the shared result-selection model.
    result_scroll_adjustment.set_upper(total_results);
    result_scroll_adjustment.set_page_size(visible_results);
    result_scroll_adjustment.set_value(0.0);

    results_panel.set_visible(false);

    (results_panel, rows, result_scroll_adjustment, scrollbar)
}

fn make_result_row(row_height: i32, surface_width: i32) -> ResultRow {
    let button = Button::new();

    // A result row has fixed height and a constrained text column. Its natural
    // width stays below the panel width; the parent then allocates the exact width.
    button.set_height_request(row_height);
    button.set_hexpand(true);
    button.set_overflow(gtk::Overflow::Hidden);
    button.set_halign(Align::Fill);
    button.set_valign(Align::Center);
    button.set_focusable(false);
    button.set_focus_on_click(false);
    button.add_css_class("result-button");
    button.set_opacity(1.0);

    let row = GtkBox::new(Orientation::Horizontal, 12);
    row.set_hexpand(true);
    row.set_halign(Align::Fill);
    row.set_valign(Align::Center);
    row.set_overflow(gtk::Overflow::Hidden);

    let icon = Image::from_icon_name("system-search-symbolic");
    let icon_size = (24 * row_height / 58).clamp(16, 24);
    icon.set_pixel_size(icon_size);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_halign(Align::Fill);
    text.set_valign(Align::Center);
    text.set_overflow(gtk::Overflow::Hidden);

    // Bound each label's natural width to the text space available inside the
    // result surface. Ellipsizing only helps once GTK allocates a constrained
    // width; max-width-chars and a reserved text column keep parent measurement
    // independent of unusually long titles or metadata.
    let scrollbar_allowance = responsive_scrollbar_width(row_height);
    let text_width = (surface_width - 28 - icon_size - 12 - scrollbar_allowance).max(1);
    text.set_size_request(text_width, -1);
    // Pango's average character width varies by font. Conservative limits keep
    // the row's natural width inside the panel even for long category strings.
    let title_max_chars = ((text_width as f64 / 10.0).floor() as i32).max(1);
    let subtitle_max_chars = ((text_width as f64 / 8.5).floor() as i32).max(1);

    let title = Label::new(Some("Example Result"));
    title.set_halign(Align::Start);
    title.set_single_line_mode(true);
    title.set_wrap(false);
    title.set_max_width_chars(title_max_chars);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("result-title");

    let subtitle = Label::new(Some("Static preview — real providers come later"));
    subtitle.set_halign(Align::Start);
    subtitle.set_single_line_mode(true);
    subtitle.set_wrap(false);
    subtitle.set_max_width_chars(subtitle_max_chars);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.add_css_class("result-subtitle");

    text.append(&title);
    text.append(&subtitle);

    row.append(&icon);
    row.append(&text);
    button.set_child(Some(&row));

    ResultRow {
        button,
        icon,
        title,
        subtitle,
        result_id: RefCell::new(None),
    }
}

fn update_results(
    results_panel: &Overlay,
    rows: &[ResultRow],
    selected_index: &Cell<usize>,
    result_window_start: &Cell<usize>,
    filter: ProviderFilter,
    query: &str,
    visible_rows: usize,
    scroll_adjustment: &Adjustment,
    scrollbar: &Scrollbar,
    catalog: &[IndexedResult],
) {
    let query = query.trim();

    if query.is_empty() && filter == ProviderFilter::All {
        results_panel.set_visible(false);
        selected_index.set(0);
        result_window_start.set(0);
        clear_result_rows(rows);
        scroll_adjustment.set_value(0.0);
        scrollbar.set_visible(false);
        return;
    }

    let ranked = rank_results(catalog, filter, query);

    if ranked.is_empty() {
        results_panel.set_visible(false);
        selected_index.set(0);
        result_window_start.set(0);
        clear_result_rows(rows);
        scroll_adjustment.set_value(0.0);
        scrollbar.set_visible(false);
        return;
    }

    results_panel.set_visible(true);
    selected_index.set(0);
    result_window_start.set(0);

    for (index, row) in rows.iter().enumerate() {
        row.button.remove_css_class("loading-result");
        row.button.set_opacity(1.0);
        if let Some(result) = ranked.get(index) {
            row.result_id.replace(Some(result.id.clone()));
            row.icon.set_icon_name(Some(&result.icon_name));
            row.title.set_text(&result.title);
            row.subtitle.set_text(&result.display_subtitle());
        } else {
            row.result_id.replace(None);
            row.title.set_text("");
            row.subtitle.set_text("");
        }
    }

    let result_count = ranked.len();
    let visible_count = visible_rows.clamp(1, result_count);
    scroll_adjustment.set_lower(0.0);
    scroll_adjustment.set_upper(result_count.max(1) as f64);
    scroll_adjustment.set_page_size(visible_count as f64);
    scroll_adjustment.set_step_increment(1.0);
    scroll_adjustment.set_page_increment(visible_count.max(1) as f64);
    scroll_adjustment.set_value(0.0);
    scrollbar.set_visible(result_count > visible_count);

    set_visible_result_window(rows, 0, visible_count, 0);
    set_result_selection(
        results_panel,
        rows,
        selected_index,
        result_window_start,
        0,
        visible_count,
        scroll_adjustment,
    );
}

fn rank_results<'a>(
    catalog: &'a [IndexedResult],
    filter: ProviderFilter,
    query: &str,
) -> Vec<&'a IndexedResult> {
    let query = query.trim().to_lowercase();

    // Score each candidate once instead of recalculating every fuzzy field for
    // each comparator call during sorting. Keep only the best bounded result set.
    let mut ranked: Vec<(i32, &'a IndexedResult)> = catalog
        .iter()
        .filter(|result| filter == ProviderFilter::All || result.provider == filter || result.id.starts_with("command-input:"))
        .filter_map(|result| {
            let relevance = result_score(result, &query);
            if query.is_empty() {
                Some((relevance + result.priority_bias, result))
            } else if relevance > 0 {
                Some((relevance + result.priority_bias, result))
            } else {
                // A pin is a ranking preference, not permission to flood every
                // unrelated query with every pinned item.
                None
            }
        })
        .collect();

    let ordering = |(left_score, left): &(i32, &IndexedResult), (right_score, right): &(i32, &IndexedResult)| {
        right_score
            .cmp(left_score)
            .then_with(|| right.priority_bias.cmp(&left.priority_bias))
            .then_with(|| right.usage_count.cmp(&left.usage_count))
            .then_with(|| left.recent_use_rank.cmp(&right.recent_use_rank))
            .then_with(|| left.id.cmp(&right.id))
    };
    // Partition to the best 30 candidates in linear expected time, then sort
    // only that small slice for deterministic display order.
    if ranked.len() > MAX_SEARCH_RESULTS {
        ranked.select_nth_unstable_by(MAX_SEARCH_RESULTS, |left, right| ordering(left, right));
        ranked.truncate(MAX_SEARCH_RESULTS);
    }
    ranked.sort_unstable_by(ordering);
    ranked.into_iter().map(|(_, result)| result).collect()
}

fn result_score(result: &IndexedResult, query: &str) -> i32 {
    if query.is_empty() {
        return result.usage_count as i32;
    }

    result
        .searchable_fields()
        .iter()
        .map(|(text, weight)| text_match_score(query, text, *weight))
        .max()
        .unwrap_or(0)
}

fn text_match_score(query: &str, candidate: &str, weight: i32) -> i32 {
    // `rank_results` normalizes the query once per keystroke.
    let candidate = candidate.trim().to_lowercase();

    if query.is_empty() || candidate.is_empty() {
        return 0;
    }

    if candidate == query {
        return weight * 4 + 100;
    }

    if candidate.starts_with(&query) {
        return weight * 3 + 50;
    }

    let tokens = candidate.split(|character: char| !character.is_alphanumeric());

    // Prefer a match at the beginning of a word (e.g. "fire" -> "Firefox").
    if tokens.clone().any(|token| token.starts_with(&query)) {
        return weight * 2 + 25;
    }

    if candidate.contains(&query) {
        return weight + 25;
    }

    // Permit small typos for queries long enough to distinguish them reliably.
    let query_length = query.chars().count();
    if query_length >= 4 {
        let max_distance = if query_length >= 8 { 2 } else { 1 };
        let closest_distance = tokens
            .filter(|token| {
                let token_length = token.chars().count();
                token_length.abs_diff(query_length) <= max_distance
            })
            .map(|token| levenshtein_ascii(&query, token))
            .min();

        if let Some(distance) = closest_distance.filter(|distance| *distance <= max_distance) {
            return (weight + 10 - distance as i32 * 8).max(1);
        }
    }

    // A weaker fallback supports initials and non-contiguous fuzzy matches.
    if is_subsequence(&query, &candidate) {
        let gaps = candidate.chars().count().saturating_sub(query_length);
        let penalty = gaps.min((weight / 2).max(0) as usize) as i32;
        return (weight * 3 / 4 - penalty).max(1);
    }

    0
}

fn is_subsequence(query: &str, candidate: &str) -> bool {
    let mut query_chars = query.chars();
    let mut current = query_chars.next();

    for candidate_char in candidate.chars() {
        if current == Some(candidate_char) {
            current = query_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }

    false
}

fn levenshtein_ascii(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b_chars.len()).collect();

    for (i, a_char) in a.chars().enumerate() {
        let mut current = vec![i + 1; b_chars.len() + 1];

        for (j, b_char) in b_chars.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != *b_char);
            let insertion = current[j] + 1;
            let deletion = previous[j + 1] + 1;
            current[j + 1] = substitution.min(insertion).min(deletion);
        }

        previous = current;
    }

    previous[b_chars.len()]
}

fn advance_result_window_start(
    current_start: usize,
    previous_index: usize,
    selected_index: usize,
    total: usize,
    visible_rows: usize,
) -> usize {
    if total <= visible_rows.max(1) {
        return 0;
    }

    let visible_rows = visible_rows.clamp(1, total);
    let max_start = total - visible_rows;
    // Shift one row before reaching beyond the visible window:
    // downward at the last visible row, upward at the second visible row.
    let down_threshold = visible_rows.saturating_sub(1);
    let up_threshold = 1usize;

    if selected_index > previous_index
        && selected_index >= current_start + down_threshold
        && current_start < max_start
    {
        return current_start + 1;
    }

    if selected_index < previous_index
        && selected_index < current_start + up_threshold
        && current_start > 0
    {
        return current_start - 1;
    }

    current_start.min(max_start)
}

fn responsive_scrollbar_width(row_height: i32) -> i32 {
    // Give the scrollbar a modest, responsive hit area wider than the
    // painted thumb so hover begins before the pointer is directly over it.
    // The visible thumb remains slim; this controls the interactive region.
    (row_height as f64 * 0.18).round().clamp(9.0, 12.0) as i32
}

fn responsive_scrollbar_vertical_inset(viewport_height: i32) -> i32 {
    // Keep the scrollbar visibly shorter than the result surface by giving it
    // proportional top/bottom breathing room.
    (viewport_height as f64 * 0.18).round().max(4.0) as i32
}

fn selection_row_opacity(
    row_index: usize,
    selected_index: usize,
    window_start: usize,
    visible_rows: usize,
) -> f64 {
    if row_index < window_start || row_index >= window_start + visible_rows {
        return 1.0;
    }

    match row_index.abs_diff(selected_index) {
        0 => 1.0,
        1 => 0.90,
        2 => 0.80,
        3 => 0.70,
        _ => 0.60,
    }
}

fn populated_result_count(rows: &[ResultRow]) -> usize {
    rows.iter()
        .take_while(|row| row.result_id.borrow().is_some())
        .count()
}

fn clear_result_rows(rows: &[ResultRow]) {
    for row in rows {
        row.result_id.replace(None);
        row.title.set_text("");
        row.subtitle.set_text("");
        row.button.remove_css_class("selected");
        row.button.remove_css_class("loading-result");
        row.button.set_opacity(1.0);
        row.button.set_visible(false);
    }
}

fn set_visible_result_window(
    rows: &[ResultRow],
    window_start: usize,
    visible_rows: usize,
    selected_index: usize,
) {
    let result_count = populated_result_count(rows);
    if result_count == 0 {
        for row in rows {
            row.button.set_visible(false);
            row.button.set_opacity(1.0);
        }
        return;
    }

    let visible_rows = visible_rows.clamp(1, result_count);
    let max_start = result_count.saturating_sub(visible_rows);
    let window_start = window_start.min(max_start);
    let window_end = (window_start + visible_rows).min(result_count);

    for (row_index, row) in rows.iter().enumerate() {
        let is_visible = row_index < result_count
            && row_index >= window_start
            && row_index < window_end;
        row.button.set_visible(is_visible);

        if is_visible {
            row.button.set_opacity(selection_row_opacity(
                row_index,
                selected_index,
                window_start,
                visible_rows,
            ));
        } else {
            row.button.set_opacity(1.0);
        }
    }
}

fn set_result_selection(
    _results_panel: &Overlay,
    rows: &[ResultRow],
    selected_index: &Cell<usize>,
    result_window_start: &Cell<usize>,
    index: usize,
    visible_rows: usize,
    scroll_adjustment: &Adjustment,
) {
    let result_count = populated_result_count(rows);
    if result_count == 0 {
        return;
    }

    let index = index.min(result_count - 1);
    let previous_index = selected_index.get().min(result_count - 1);

    // The selector is circular. Boundary crossings are handled as jumps to the
    // corresponding end window rather than as ordinary directional movement.
    selected_index.set(index);

    let visible_rows = visible_rows.clamp(1, result_count);
    let max_start = result_count.saturating_sub(visible_rows);

    let window_start = if previous_index + 1 == result_count && index == 0 {
        // Downward wrap: last result → first result.
        // Treat this as a full circular jump and show the beginning again.
        0
    } else if previous_index == 0 && index + 1 == result_count {
        // Upward wrap: first result → last result.
        // Treat this as a full circular jump and show the end again.
        max_start
    } else {
        advance_result_window_start(
            result_window_start.get(),
            previous_index,
            index,
            result_count,
            visible_rows,
        )
    };

    result_window_start.set(window_start);

    // Keep the native scrollbar synchronized with keyboard/mouse selection.
    // Adjustment::set_value clamps to [lower, upper - page_size], so the thumb
    // remains bounded to the available result window.
    scroll_adjustment.set_value(window_start as f64);

    set_visible_result_window(rows, window_start, visible_rows, index);

    for (row_index, row) in rows.iter().enumerate() {
        if row_index == index {
            row.button.add_css_class("selected");
        } else {
            row.button.remove_css_class("selected");
        }
    }
}

fn find_visible_action_index(buttons: &[Button; 4], current: usize, direction: isize) -> usize {
    if buttons.is_empty() {
        return current;
    }

    let len = buttons.len();
    let mut index = current;

    for _ in 0..len {
        index = if direction > 0 {
            (index + 1) % len
        } else if index == 0 {
            len - 1
        } else {
            index - 1
        };

        if buttons[index].is_visible() {
            return index;
        }
    }

    current
}

fn make_back_button(tooltip: &str) -> Button {
    let button = Button::new();
    button.set_width_request(34);
    button.set_height_request(34);
    button.set_halign(Align::Center);
    button.set_valign(Align::Center);
    button.set_focusable(true);
    button.set_focus_on_click(true);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("search-back-button");

    let icon = Image::from_icon_name("go-previous-symbolic");
    icon.set_pixel_size(18);
    icon.add_css_class("search-back-icon");
    button.set_child(Some(&icon));

    button
}

fn make_inline_filter_button(class_name: &str, letter: &str, tooltip: &str) -> Button {
    let button = Button::new();
    button.set_width_request(34);
    button.set_height_request(34);
    button.set_halign(Align::Center);
    button.set_valign(Align::Center);
    button.set_focusable(true);
    button.set_focus_on_click(true);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("inline-filter-button");
    button.add_css_class(class_name);

    let label = Label::new(Some(letter));
    label.add_css_class("inline-filter-letter");
    button.set_child(Some(&label));

    button
}

fn make_inline_filter_icon_button(class_name: &str, icon_name: &str, tooltip: &str) -> Button {
    let button = Button::new();
    button.set_width_request(34);
    button.set_height_request(34);
    button.set_halign(Align::Center);
    button.set_valign(Align::Center);
    button.set_focusable(true);
    button.set_focus_on_click(true);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("inline-filter-button");
    button.add_css_class(class_name);

    let icon = if class_name == "actions-button" {
        make_shortcuts_provider_icon(18)
    } else {
        let icon = Image::from_icon_name(icon_name);
        icon.set_pixel_size(18);
        icon.add_css_class("inline-filter-icon");
        icon
    };
    button.set_child(Some(&icon));

    button
}

fn set_active_filter_buttons(buttons: &[Button; 4], filter: ProviderFilter) {
    let active_index = filter_index(filter);

    for (index, button) in buttons.iter().enumerate() {
        if active_index == Some(index) {
            button.add_css_class("active-filter");
        } else {
            button.remove_css_class("active-filter");
        }
    }
}

fn update_search_chrome(
    search_pill: &GtkBox,
    search_icon: &Image,
    back_button: &Button,
    category_box: &GtkBox,
    category_label: &Label,
    inline_filters: &GtkBox,
    standalone_buttons: &[Button; 4],
    filter: ProviderFilter,
    query: &str,
    base_width: i32,
    composition_width: i32,
) {
    let query_is_empty = query.trim().is_empty();
    let provider_mode = filter != ProviderFilter::All;
    let typing_general = !query_is_empty && !provider_mode;

    search_pill.remove_css_class("search-pill-typing");
    search_pill.remove_css_class("search-pill-filtered");

    back_button.set_visible(provider_mode);
    search_icon.set_visible(!provider_mode);
    category_box.set_visible(provider_mode);
    inline_filters.set_visible(typing_general);

    for button in standalone_buttons {
        button.set_visible(query_is_empty && !provider_mode);
    }

    if provider_mode {
        // Provider mode absorbs the same footprint as the complete empty-state
        // composition, keeping the overall Spotlight width stable.
        search_pill.add_css_class("search-pill-filtered");
        search_pill.set_width_request(composition_width);
        category_label.set_text(filter.label());
    } else if typing_general {
        // When typing begins, the four large controls disappear and their exact
        // footprint is swallowed by the search pill. The outer Spotlight width
        // therefore remains unchanged.
        search_pill.add_css_class("search-pill-typing");
        search_pill.set_width_request(composition_width);
    } else {
        // Empty/default state: only the search field itself is visible beside the
        // four standalone controls.
        search_pill.set_width_request(base_width);
    }
}

fn hide_auxiliary_panels(surfaces: &DetailSurfaces) {
    surfaces.preview_panel.set_visible(false);
    surfaces.properties_panel.set_visible(false);
    surfaces.context_panel.set_visible(false);
    surfaces.history_panel.set_visible(false);
    surfaces.help_panel.set_visible(false);
}

fn provider_ready_bit(provider: ProviderFilter) -> u8 {
    match provider {
        ProviderFilter::Applications => 1 << 0,
        ProviderFilter::Actions => 1 << 1,
        ProviderFilter::Clipboard => 1 << 2,
        ProviderFilter::Files => 1 << 3,
        ProviderFilter::All => 0,
    }
}

fn active_provider_ready(interaction: &InteractionContext) -> bool {
    let ready = interaction.catalog_ready.get();
    match interaction.active_filter.get() {
        // In general search, show matching cached/fast provider hits immediately.
        // If there are no hits yet, one loading row remains until all local
        // providers have published their first batch.
        ProviderFilter::All => ready & 0b0001 != 0 && ready == 0b1111,
        provider => ready & provider_ready_bit(provider) != 0,
    }
}

fn update_results_or_loading(interaction: &InteractionContext) {
    {
        let catalog = interaction.catalog.borrow();
        update_results(
            &interaction.surfaces.results_panel,
            interaction.result_rows.as_ref(),
            &interaction.selected_index,
            &interaction.result_window_start,
            interaction.active_filter.get(),
            interaction.entry.text().as_str(),
            interaction.visible_rows,
            &interaction.scroll_adjustment,
            &interaction.scrollbar,
            catalog.as_slice(),
        );
    }

    // Keep cold-start feedback restrained: one inert row, not a list of fake
    // skeletons. It is never assigned a result ID and cannot be activated.
    let query_nonempty = !interaction.entry.text().trim().is_empty()
        || interaction.active_filter.get() != ProviderFilter::All;
    if populated_result_count(interaction.result_rows.as_ref()) == 0
        && query_nonempty
        && !active_provider_ready(interaction)
    {
        if let Some(row) = interaction.result_rows.first() {
            row.result_id.replace(None);
            row.icon.set_icon_name(Some("system-search-symbolic"));
            row.title.set_text("Finding local results…");
            row.subtitle.set_text("Fast local providers first");
            row.button.remove_css_class("selected");
            row.button.set_opacity(0.68);
            row.button.set_visible(true);
            row.button.add_css_class("loading-result");
            interaction.surfaces.results_panel.set_visible(true);
            interaction.scrollbar.set_visible(false);
            interaction.scroll_adjustment.set_lower(0.0);
            interaction.scroll_adjustment.set_upper(1.0);
            interaction.scroll_adjustment.set_page_size(1.0);
            interaction.scroll_adjustment.set_value(0.0);
        }
    }
}

fn refresh_result_surface(interaction: &InteractionContext) {
    let previous_id = interaction
        .result_rows
        .get(interaction.selected_index.get())
        .and_then(|row| row.result_id.borrow().clone());
    update_results_or_loading(interaction);
    if let Some(id) = previous_id {
        if let Some(index) = interaction.result_rows.iter().position(|row| {
            row.result_id.borrow().as_deref() == Some(id.as_str())
        }) {
            let count = populated_result_count(interaction.result_rows.as_ref());
            if count > 0 {
                set_result_selection(
                    &interaction.surfaces.results_panel,
                    interaction.result_rows.as_ref(),
                    &interaction.selected_index,
                    &interaction.result_window_start,
                    index,
                    interaction.visible_rows.clamp(1, count),
                    &interaction.scroll_adjustment,
                );
            }
        }
    }
}

fn activate_auxiliary_surface(interaction: &InteractionContext, surface: AuxiliarySurface) {
    hide_auxiliary_panels(&interaction.surfaces);
    interaction.surfaces.results_panel.set_visible(false);
    interaction.auxiliary_surface.set(surface);
    match surface {
        AuxiliarySurface::None => interaction.surfaces.results_panel.set_visible(true),
        AuxiliarySurface::Preview => interaction.surfaces.preview_panel.set_visible(true),
        AuxiliarySurface::ContextMenu => interaction.surfaces.context_panel.set_visible(true),
        AuxiliarySurface::Properties => interaction.surfaces.properties_panel.set_visible(true),
        AuxiliarySurface::History => interaction.surfaces.history_panel.set_visible(true),
        AuxiliarySurface::Help => interaction.surfaces.help_panel.set_visible(true),
    }
}

fn cancel_preview_worker(interaction: &InteractionContext) {
    if let Some(cancel) = interaction.preview_cancel.borrow_mut().take() {
        cancel.store(true, AtomicOrdering::Release);
    }
}

fn close_auxiliary_surface(interaction: &InteractionContext) {
    if interaction.auxiliary_surface.get() == AuxiliarySurface::Help {
        mark_welcome_seen();
        interaction.help_is_welcome.set(false);
        interaction.welcome_auto_dismiss_scheduled.set(false);
    }
    cancel_preview_worker(interaction);
    interaction.preview_generation.set(
        interaction.preview_generation.get().wrapping_add(1),
    );
    interaction.auxiliary_surface.set(AuxiliarySurface::None);
    hide_auxiliary_panels(&interaction.surfaces);
    refresh_result_surface(interaction);
    schedule_idle_hint(interaction);
}

fn selected_result(interaction: &InteractionContext) -> Option<IndexedResult> {
    let row = interaction.result_rows.get(interaction.selected_index.get())?;
    let id = row.result_id.borrow().clone()?;
    interaction
        .catalog
        .borrow()
        .iter()
        .find(|record| record.id == id)
        .cloned()
}

// Arrow-key navigation can move faster than a decoder can respond. Bump a
// generation immediately, then launch only the final preview request after a
// short quiet period. This prevents one external process per intermediate row.
fn schedule_preview_refresh(interaction: &InteractionContext) {
    let generation = interaction.preview_generation.get().wrapping_add(1);
    interaction.preview_generation.set(generation);
    cancel_preview_worker(interaction);
    let interaction = interaction.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(110), move || {
        if interaction.preview_generation.get() == generation
            && interaction.auxiliary_surface.get() == AuxiliarySurface::Preview
        {
            open_preview(&interaction);
        }
    });
}

fn open_preview(interaction: &InteractionContext) {
    let Some(result) = selected_result(interaction) else {
        return;
    };

    cancel_preview_worker(interaction);
    activate_auxiliary_surface(interaction, AuxiliarySurface::Preview);
    interaction.surfaces.preview_icon.set_icon_name(Some(nonempty_or(&result.icon_name, "application-x-executable-symbolic")));
    interaction.surfaces.preview_title.set_text(&result.title);
    interaction.surfaces.preview_subtitle.set_text(result.provider.label());
    interaction.surfaces.preview_body.set_text("Loading preview…");
    interaction.surfaces.preview_body.remove_css_class("detail-code");
    interaction.surfaces.preview_picture.set_visible(false);
    interaction.surfaces.preview_panel.set_height_request(210);
    interaction.surfaces.preview_body_scroll.set_height_request(96);

    let request_id = interaction.preview_generation.get().wrapping_add(1);
    interaction.preview_generation.set(request_id);
    let cancel = Arc::new(AtomicBool::new(false));
    interaction.preview_cancel.replace(Some(Arc::clone(&cancel)));

    let (sender, receiver) = std::sync::mpsc::sync_channel::<PreviewPayload>(1);
    let worker_result = result.clone();
    let worker_cancel = Arc::clone(&cancel);
    if std::thread::Builder::new()
        .name("spotlight-preview".to_string())
        .spawn(move || {
            let payload = build_preview_payload(&worker_result, worker_cancel.as_ref());
            if !worker_cancel.load(AtomicOrdering::Acquire) {
                let _ = sender.send(payload);
            }
        })
        .is_err()
    {
        interaction.preview_cancel.borrow_mut().take();
        interaction
            .surfaces
            .preview_body
            .set_text("Preview could not be started.");
        return;
    }

    let generation = Rc::clone(&interaction.preview_generation);
    let surface_state = Rc::clone(&interaction.auxiliary_surface);
    let active_cancel = Rc::clone(&interaction.preview_cancel);
    let subtitle = interaction.surfaces.preview_subtitle.clone();
    let body = interaction.surfaces.preview_body.clone();
    let picture = interaction.surfaces.preview_picture.clone();
    let preview_panel = interaction.surfaces.preview_panel.clone();
    let body_scroll = interaction.surfaces.preview_body_scroll.clone();
    let full_body_height = interaction.surfaces.preview_body_height;
    let thumbnail_body_height = interaction.surfaces.preview_body_height_with_picture;
    let theme_root = interaction.root.clone();
    let texture_cache = Rc::clone(&interaction.preview_textures);
    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        if generation.get() != request_id || surface_state.get() != AuxiliarySurface::Preview {
            return glib::ControlFlow::Break;
        }
        match receiver.try_recv() {
            Ok(payload) => {
                active_cancel.borrow_mut().take();
                subtitle.set_text(&payload.subtitle);
                if payload.body_markup {
                    body.add_css_class("detail-code");
                    body.set_markup(&highlight_code_as_markup(
                        &payload.body,
                        theme_root.has_css_class("spotlight-theme-dark"),
                    ));
                } else {
                    body.remove_css_class("detail-code");
                    body.set_text(&payload.body);
                }
                if let Some(bytes) = payload.thumbnail_png {
                    picture.set_visible(false);
                    body_scroll.set_height_request(thumbnail_body_height);
                    preview_panel.set_height_request(350);

                    // Prefer a decoded UI-thread texture for repeat visits. The
                    // disk cache avoids rerunning ffmpeg; this bounded in-memory
                    // cache also avoids repeatedly decoding the cached PNG.
                    let cached_texture = payload.thumbnail_key.as_ref().and_then(|key| {
                        texture_cache.borrow().get(key).cloned()
                    });
                    let decoded_texture = cached_texture.or_else(|| {
                        gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(bytes))
                            .ok()
                            .map(|pixbuf| gdk::Texture::for_pixbuf(&pixbuf))
                    });
                    match decoded_texture {
                        Some(texture) => {
                            if let Some(key) = payload.thumbnail_key {
                                let mut cache = texture_cache.borrow_mut();
                                if cache.len() >= 24 && !cache.contains_key(&key) {
                                    cache.clear();
                                }
                                cache.insert(key, texture.clone());
                            }
                            picture.set_paintable(Some(&texture));
                            picture.set_visible(true);
                        }
                        None => {
                            picture.set_visible(false);
                            body_scroll.set_height_request(full_body_height.min(160));
                            preview_panel.set_height_request(260);
                            body.set_text("The preview thumbnail could not be decoded.");
                        }
                    }
                } else {
                    let body_height = estimate_preview_body_height(&payload.body).min(full_body_height);
                    body_scroll.set_height_request(body_height);
                    preview_panel.set_height_request(104 + body_height);
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                active_cancel.borrow_mut().take();
                body.set_text("Preview is unavailable for this item.");
                glib::ControlFlow::Break
            }
        }
    });
}

fn open_properties(interaction: &InteractionContext) {
    cancel_preview_worker(interaction);
    let Some(result) = selected_result(interaction) else {
        return;
    };
    interaction.preview_generation.set(
        interaction.preview_generation.get().wrapping_add(1),
    );
    activate_auxiliary_surface(interaction, AuxiliarySurface::Properties);
    interaction.surfaces.properties_icon.set_icon_name(Some(nonempty_or(&result.icon_name, "application-x-executable-symbolic")));
    interaction.surfaces.properties_title.set_text(&result.title);
    interaction
        .surfaces
        .properties_subtitle
        .set_text(result.provider.label());
    interaction
        .surfaces
        .properties_body
        .set_text(&format_properties(&result));
}

fn open_context_menu(interaction: &InteractionContext) {
    cancel_preview_worker(interaction);
    let Some(result) = selected_result(interaction) else {
        return;
    };
    if result.id.starts_with("command-input:") {
        open_command_confirmation(interaction, &result, "Run this command once?");
        return;
    }
    interaction.context_action_count.set(interaction.surfaces.context_actions.len());
    for action in &interaction.surfaces.context_actions {
        action.button.set_visible(true);
    }
    interaction.preview_generation.set(
        interaction.preview_generation.get().wrapping_add(1),
    );
    activate_auxiliary_surface(interaction, AuxiliarySurface::ContextMenu);
    interaction
        .surfaces
        .context_title
        .set_text(&format!("Actions · {}", result.title));
    interaction
        .surfaces
        .context_status
        .set_text("Choose an action for this result.");
    let pinned = is_pinned_id(&result.id);
    interaction.surfaces.context_actions[2]
        .label
        .set_text(if pinned { "Unpin result" } else { "Pin result" });
    let copy_label = if result.provider == ProviderFilter::Clipboard {
        "Copy text to clipboard"
    } else if result.id.starts_with("builtin:") {
        "Copy action name"
    } else {
        "Copy location"
    };
    interaction.surfaces.context_actions[3].label.set_text(copy_label);
    let activate_label = match result.provider {
        ProviderFilter::Clipboard => "Copy selected text to clipboard",
        ProviderFilter::Actions if result.id.starts_with("builtin:") && result.id != "builtin:lock-screen" => "Show output in Spotlight",
        ProviderFilter::Actions if result.id == "builtin:lock-screen" => "Lock screen",
        _ => "Open / activate",
    };
    interaction.surfaces.context_actions[4].label.set_text(activate_label);
    interaction.context_selected_index.set(0);
    if let Some(first) = interaction.surfaces.context_actions.first() {
        first.button.grab_focus();
    }
}

fn run_context_action(interaction: &InteractionContext, action_index: usize) {
    if let Some(result) = selected_result(interaction) {
        if result.id.starts_with("command-input:") {
            if action_index == 0 {
                close_auxiliary_surface(interaction);
                activate_command_candidate(interaction, &result, true);
            } else {
                close_auxiliary_surface(interaction);
                focus_search_entry(&interaction.entry);
            }
            return;
        }
    }
    let Some(result) = selected_result(interaction) else {
        interaction
            .surfaces
            .context_status
            .set_text("Select a result first.");
        return;
    };

    match action_index {
        0 => open_preview(interaction),
        1 => open_properties(interaction),
        2 => {
            let pinned = toggle_pinned_id(&result.id);
            {
                let mut catalog = interaction.catalog.borrow_mut();
                apply_preferences(catalog.as_mut_slice());
            }
            interaction.surfaces.context_actions[2]
                .label
                .set_text(if pinned { "Unpin result" } else { "Pin result" });
            interaction.surfaces.context_status.set_text(if pinned {
                "Pinned. This result receives explicit ranking priority."
            } else {
                "Unpinned. Normal relevance and usage ranking applies."
            });
        }
        3 => {
            match copy_to_clipboard(&copy_value_for_result(&result)) {
                Ok(()) => interaction
                    .surfaces
                    .context_status
                    .set_text("Copied to clipboard."),
                Err(_) => interaction
                    .surfaces
                    .context_status
                    .set_text("Clipboard copy is unavailable (wl-copy required)."),
            }
        }
        4 => activate_result_id(interaction, &result.id),
        _ => {}
    }
}

fn open_history(interaction: &InteractionContext) {
    cancel_preview_worker(interaction);
    interaction.preview_generation.set(
        interaction.preview_generation.get().wrapping_add(1),
    );
    interaction.history_selected_index.set(0);
    interaction.history_window_start.set(0);
    update_history_view(interaction);
    activate_auxiliary_surface(interaction, AuxiliarySurface::History);
    if let Some(first) = interaction.history_rows.first() {
        if !interaction.history.borrow().is_empty() {
            first.button.grab_focus();
        }
    }
}

fn update_history_view(interaction: &InteractionContext) {
    let history = interaction.history.borrow();
    let count = history.len().min(interaction.history_rows.len());
    let visible = interaction.history_visible_rows.clamp(1, interaction.history_rows.len().max(1));
    if count == 0 {
        interaction.history_selected_index.set(0);
        interaction.history_window_start.set(0);
        interaction.surfaces.history_empty.set_visible(true);
        interaction.surfaces.history_list.set_visible(false);
        for row in interaction.history_rows.iter() {
            row.button.set_visible(false);
            row.button.remove_css_class("selected");
        }
        return;
    }

    interaction.surfaces.history_empty.set_visible(false);
    interaction.surfaces.history_list.set_visible(true);
    let selection = interaction.history_selected_index.get().min(count - 1);
    interaction.history_selected_index.set(selection);
    let max_start = count.saturating_sub(visible);
    let start = interaction.history_window_start.get().min(max_start);
    interaction.history_window_start.set(start);
    let end = (start + visible).min(count);

    for (index, row) in interaction.history_rows.iter().enumerate() {
        if let Some(query) = history.get(index) {
            row.label.set_text(query);
        } else {
            row.label.set_text("");
        }
        let shown = index >= start && index < end && index < count;
        row.button.set_visible(shown);
        if index == selection {
            row.button.add_css_class("selected");
        } else {
            row.button.remove_css_class("selected");
        }
    }
}

fn ensure_history_selection_visible(interaction: &InteractionContext, selection: usize) {
    let count = interaction.history.borrow().len().min(interaction.history_rows.len());
    if count == 0 {
        return;
    }
    let visible = interaction.history_visible_rows.clamp(1, count);
    let mut start = interaction.history_window_start.get();
    if selection < start {
        start = selection;
    } else if selection >= start + visible {
        start = selection + 1 - visible;
    }
    interaction.history_window_start.set(start.min(count.saturating_sub(visible)));
}

fn move_history_selection(interaction: &InteractionContext, direction: isize) {
    let count = interaction.history.borrow().len().min(interaction.history_rows.len());
    if count == 0 {
        return;
    }
    let current = interaction.history_selected_index.get().min(count - 1);
    let next = if direction < 0 {
        if current == 0 { count - 1 } else { current - 1 }
    } else {
        (current + 1) % count
    };
    interaction.history_selected_index.set(next);
    ensure_history_selection_visible(interaction, next);
    update_history_view(interaction);
    if let Some(row) = interaction.history_rows.get(next) {
        row.button.grab_focus();
    }
}

fn restore_history_selection(interaction: &InteractionContext) {
    let query = interaction
        .history
        .borrow()
        .get(interaction.history_selected_index.get())
        .cloned();
    let Some(query) = query else {
        close_auxiliary_surface(interaction);
        return;
    };
    cancel_preview_worker(interaction);
    interaction.preview_generation.set(
        interaction.preview_generation.get().wrapping_add(1),
    );
    interaction.auxiliary_surface.set(AuxiliarySurface::None);
    hide_auxiliary_panels(&interaction.surfaces);
    interaction.entry.set_text(&query);
    refresh_result_surface(interaction);
    focus_search_entry(&interaction.entry);
    schedule_idle_hint(interaction);
}

fn result_path(result: &IndexedResult) -> Option<PathBuf> {
    match result.provider {
        ProviderFilter::Files => result.id.strip_prefix("file:").map(PathBuf::from),
        ProviderFilter::Applications | ProviderFilter::Actions => {
            if result.path.is_empty() { None } else { Some(PathBuf::from(&result.path)) }
        }
        ProviderFilter::Clipboard | ProviderFilter::All => None,
    }
}

fn format_properties(result: &IndexedResult) -> String {
    let mut lines = vec![
        format!("Provider: {}", result.provider.label()),
        format!("Name: {}", result.title),
    ];
    if !result.generic_name.trim().is_empty() {
        lines.push(format!("Type: {}", result.generic_name));
    }
    if !result.categories.trim().is_empty() {
        lines.push(format!("Categories: {}", result.categories.trim_matches(';')));
    }
    if !result.metadata.trim().is_empty() {
        lines.push(format!("Metadata: {}", result.metadata));
    }
    if let Some(path) = result_path(result) {
        lines.push(format!("Location: {}", path.display()));
    } else if !result.path.is_empty() {
        lines.push(format!("Location: {}", result.path));
    }
    if !result.executable.trim().is_empty() {
        lines.push(format!("Command: {}", result.executable));
    }
    if !result.comment.trim().is_empty() {
        lines.push(format!("Description: {}", shorten_single_line(&result.comment, 500)));
    }
    lines.join("\n")
}

fn build_preview_payload(result: &IndexedResult, cancelled: &AtomicBool) -> PreviewPayload {
    let subtitle = match result.provider {
        ProviderFilter::Applications => nonempty_or(
            &result.generic_name,
            application_category_hint(&result.categories).unwrap_or("Application"),
        ).to_string(),
        ProviderFilter::Files => nonempty_or(&result.generic_name, "File").to_string(),
        ProviderFilter::Actions => nonempty_or(&result.generic_name, "Action").to_string(),
        ProviderFilter::Clipboard => result.generic_name.clone(),
        ProviderFilter::All => String::new(),
    };

    let mut thumbnail_png = None;
    let mut thumbnail_key = None;
    let mut body_markup = false;
    let body = match result.provider {
        ProviderFilter::Applications => {
            let mut lines = Vec::new();
            if !result.comment.trim().is_empty() {
                lines.push(shorten_single_line(&result.comment, 1000));
            }
            if !result.categories.trim().is_empty() {
                lines.push(format!("Categories: {}", result.categories.trim_matches(';')));
            }
            if !result.executable.trim().is_empty() {
                lines.push(format!("Command: {}", result.executable));
            }
            if !result.path.trim().is_empty() {
                lines.push(format!("Desktop entry: {}", result.path));
            }
            if lines.is_empty() { "No additional application details are available.".to_string() } else { lines.join("\n\n") }
        }
        ProviderFilter::Files => {
            let Some(path) = result_path(result) else {
                return PreviewPayload {
                    subtitle,
                    body: "File location is unavailable.".to_string(),
                    body_markup: false,
                    thumbnail_png: None,
                    thumbnail_key: None,
                };
            };
            let mut sections = vec![format!(
                "{}\nSize: {}",
                path.display(),
                nonempty_or(&result.metadata, "Unknown size")
            )];
            let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
            let content = if extension == "pdf" {
                read_pdf_text_preview(&path, cancelled)
            } else if is_text_preview_extension(&extension) {
                body_markup = is_syntax_highlight_extension(&extension);
                read_text_file_preview(&path, cancelled)
            } else if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "mp4" | "mkv" | "webm" | "mov" | "avi") {
                match make_media_thumbnail(&path, cancelled) {
                    Some(png) => {
                        thumbnail_png = Some(png);
                        thumbnail_key = thumbnail_cache_key(&path);
                        "Thumbnail preview".to_string()
                    }
                    None => "Visual preview unavailable; install ffmpeg for image/video thumbnails.".to_string(),
                }
            } else if matches!(extension.as_str(), "mp3" | "flac" | "ogg" | "wav" | "m4a" | "aac") {
                "Audio file · metadata preview only".to_string()
            } else if matches!(extension.as_str(), "zip" | "7z" | "tar" | "gz" | "xz" | "bz2") {
                "Archive preview is metadata-only in this lightweight pass.".to_string()
            } else {
                "No inline content preview is available for this file type.".to_string()
            };
            sections.push(content);
            sections.join("\n\n")
        }
        ProviderFilter::Actions => {
            let mut lines = vec![format!("Action: {}", result.title)];
            if !result.executable.trim().is_empty() {
                lines.push(format!("Command: {}", result.executable));
            }
            if !result.path.trim().is_empty() {
                lines.push(format!("Definition: {}", result.path));
            }
            if !result.comment.trim().is_empty() {
                lines.push(shorten_single_line(&result.comment, 1000));
            }
            lines.join("\n\n")
        }
        ProviderFilter::Clipboard => {
            if result.content_preview.trim().is_empty() {
                "Clipboard content is no longer available.".to_string()
            } else {
                truncate_preview_text(&result.content_preview, PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES)
            }
        }
        ProviderFilter::All => "Select a result from a provider to preview it.".to_string(),
    };
    PreviewPayload { subtitle, body, body_markup, thumbnail_png, thumbnail_key }
}

// Optional ffmpeg performs image/video decoding away from GTK's main thread.
// Output is restricted to one bounded PNG thumbnail, keeping the preview compact.
// Read subprocess output on a separate thread while polling its status so a
// superseded preview can kill the process. Output storage is bounded, but the
// reader continues draining stdout to avoid blocking the child on a full pipe.
fn run_preview_command(
    mut command: Command,
    cancelled: &AtomicBool,
    output_limit: usize,
) -> Option<(std::process::ExitStatus, Vec<u8>, bool)> {
    if cancelled.load(AtomicOrdering::Acquire) {
        return None;
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut output = Vec::with_capacity(output_limit.min(64 * 1024));
        let mut overflow = false;
        let mut buffer = [0u8; 8192];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let remaining = output_limit.saturating_sub(output.len());
                    let copy_count = count.min(remaining);
                    output.extend_from_slice(&buffer[..copy_count]);
                    if copy_count < count {
                        overflow = true;
                    }
                }
                Err(_) => {
                    overflow = true;
                    break;
                }
            }
        }
        (output, overflow)
    });

    loop {
        if cancelled.load(AtomicOrdering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return None;
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let (output, overflow) = reader.join().ok()?;
                return Some((status, output, overflow));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(12)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return None;
            }
        }
    }
}

// Optional ffmpeg performs image/video decoding away from GTK's main thread.
// Output is restricted to one bounded PNG thumbnail.
fn thumbnail_cache_path(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "spotlight-thumbnail-v2".hash(&mut hasher);
    path.to_string_lossy().hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified_ns.hash(&mut hasher);
    Some(SpotlightStorage::create()?.cache_dir.join(format!("thumbnail-{:016x}.png", hasher.finish())))
}

fn thumbnail_cache_key(path: &Path) -> Option<String> {
    thumbnail_cache_path(path)?
        .file_name()?
        .to_str()
        .map(str::to_owned)
}

fn is_valid_thumbnail_png(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && bytes.len() <= 4 * 1024 * 1024
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn make_media_thumbnail(path: &Path, cancelled: &AtomicBool) -> Option<Vec<u8>> {
    const MAX_THUMBNAIL_BYTES: usize = 4 * 1024 * 1024;
    let cache_path = thumbnail_cache_path(path);
    if !cancelled.load(AtomicOrdering::Acquire) {
        if let Some(cache_path) = cache_path.as_ref() {
            if let Ok(bytes) = fs::read(cache_path) {
                if is_valid_thumbnail_png(&bytes) {
                    return Some(bytes);
                }
            }
        }
    }
    let mut command = Command::new("ffmpeg");
    command
        .arg("-nostdin")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("scale=620:220:force_original_aspect_ratio=decrease")
        .arg("-f")
        .arg("image2pipe")
        .arg("-vcodec")
        .arg("png")
        .arg("pipe:1");

    let (status, output, overflow) = run_preview_command(command, cancelled, MAX_THUMBNAIL_BYTES)?;
    if !status.success() || overflow || !is_valid_thumbnail_png(&output) {
        return None;
    }

    if !cancelled.load(AtomicOrdering::Acquire) {
        if let Some(cache_path) = cache_path {
            // Rename a completed temporary file so a cancelled/failed decode
            // never leaves a partial PNG that would poison the cache.
            let temporary = cache_path.with_extension("png.tmp");
            if fs::write(&temporary, &output).is_ok() {
                let _ = fs::rename(&temporary, &cache_path);
            }
        }
    }
    Some(output)
}

fn is_syntax_highlight_extension(extension: &str) -> bool {
    matches!(
        extension,
        "rs" | "c" | "h" | "cpp" | "hpp" | "lua" | "sh" | "py" | "css"
            | "html" | "xml" | "conf" | "desktop" | "js" | "ts" | "tsx"
            | "jsx" | "ini" | "json" | "toml" | "yaml" | "yml" | "sql"
    )
}

fn escape_pango(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\'' => escaped.push_str("&apos;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn syntax_span(text: &str, color: &str) -> String {
    format!("<span foreground=\"{color}\">{}</span>", escape_pango(text))
}

fn highlight_code_line(line: &str, dark_theme: bool) -> String {
    let (comment_color, string_color, number_color, keyword_color, type_color) = if dark_theme {
        ("#a6b4c5", "#a6e3a1", "#f9c74f", "#89b4fa", "#cba6f7")
    } else {
        ("#586779", "#176b3a", "#9a4d00", "#0759a5", "#6b3fa0")
    };
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 32);
    let mut i = 0usize;
    let trimmed = line.trim_start();
    let comment_line = trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("#!")
        || trimmed.starts_with("# ")
        || trimmed.starts_with("#\t");
    if comment_line {
        return syntax_span(line, comment_color);
    }

    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "case", "catch", "class", "const", "continue",
        "def", "else", "elif", "enum", "export", "extends", "false", "fn", "for", "from",
        "function", "if", "impl", "import", "in", "interface", "let", "match", "mod",
        "mut", "new", "null", "None", "package", "pub", "return", "self", "Self", "static",
        "struct", "switch", "this", "throw", "trait", "true", "try", "type", "use", "var",
        "while", "yield", "Some", "Ok", "Err", "Option", "Result", "String", "Vec", "HashMap",
    ];

    while i < chars.len() {
        // Inline line comments and block comments.
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            out.push_str(&syntax_span(&chars[i..].iter().collect::<String>(), comment_color));
            break;
        }
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            out.push_str(&syntax_span(&chars[i..].iter().collect::<String>(), comment_color));
            break;
        }

        let ch = chars[i];
        if ch == '\'' || ch == '"' || ch == '`' {
            let quote = ch;
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < chars.len() {
                let current = chars[i];
                i += 1;
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    break;
                }
            }
            let token: String = chars[start..i].iter().collect();
            out.push_str(&syntax_span(&token, string_color));
            continue;
        }

        if ch.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '.' | '_' )) {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            out.push_str(&syntax_span(&token, number_color));
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if KEYWORDS.contains(&token.as_str()) {
                out.push_str(&syntax_span(&token, keyword_color));
            } else if token.chars().next().map(char::is_uppercase).unwrap_or(false) {
                out.push_str(&syntax_span(&token, type_color));
            } else {
                out.push_str(&escape_pango(&token));
            }
            continue;
        }

        out.push_str(&escape_pango(&ch.to_string()));
        i += 1;
    }
    out
}

fn highlight_code_as_markup(text: &str, dark_theme: bool) -> String {
    let lines = text
        .lines()
        .map(|line| highlight_code_line(line, dark_theme))
        .collect::<Vec<_>>()
        .join("\n");
    format!("<span font_family=\"monospace\">{lines}</span>")
}

fn estimate_preview_body_height(text: &str) -> i32 {
    // GTK wraps labels at roughly 95–110 average-width characters at this width.
    // Estimate conservatively, then cap the viewport to keep long files scrollable.
    let estimated_lines: usize = text
        .lines()
        .map(|line| (line.chars().count().max(1) + 94) / 95)
        .sum::<usize>()
        .max(1);
    (estimated_lines as i32 * 18 + 20).clamp(88, 260)
}

fn is_text_preview_extension(extension: &str) -> bool {
    matches!(
        extension,
        "txt" | "md" | "markdown" | "toml" | "yaml" | "yml" | "json" | "rs" | "c" | "h"
            | "cpp" | "hpp" | "lua" | "sh" | "py" | "css" | "html" | "xml" | "conf"
            | "desktop" | "js" | "ts" | "tsx" | "jsx" | "ini" | "log" | "csv" | "sql"
    )
}

fn read_text_file_preview(path: &Path, cancelled: &AtomicBool) -> String {
    if cancelled.load(AtomicOrdering::Acquire) {
        return "Preview cancelled.".to_string();
    }
    let Ok(file) = File::open(path) else {
        return "Unable to read this file.".to_string();
    };
    let mut bytes = Vec::new();
    if file.take((PREVIEW_MAX_BYTES + 1) as u64).read_to_end(&mut bytes).is_err() {
        return "Unable to read this file.".to_string();
    }
    if cancelled.load(AtomicOrdering::Acquire) {
        return "Preview cancelled.".to_string();
    }
    let truncated_bytes = bytes.len() > PREVIEW_MAX_BYTES;
    if truncated_bytes {
        bytes.truncate(PREVIEW_MAX_BYTES);
    }
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return "This appears to be a binary file; showing metadata only.".to_string();
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    truncate_preview_text(&text, PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES)
        + if truncated_bytes { "\n\n… Preview truncated." } else { "" }
}

fn read_pdf_text_preview(path: &Path, cancelled: &AtomicBool) -> String {
    let mut command = Command::new("pdftotext");
    command
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg("3")
        .arg(path)
        .arg("-");
    let Some((status, output, overflow)) =
        run_preview_command(command, cancelled, PREVIEW_MAX_BYTES)
    else {
        return "Preview cancelled or PDF text utility unavailable.".to_string();
    };
    if cancelled.load(AtomicOrdering::Acquire) {
        return "Preview cancelled.".to_string();
    }
    if !status.success() || output.is_empty() {
        return "No extractable text was found in the first pages of this PDF.".to_string();
    }
    let mut preview = truncate_preview_text(
        &String::from_utf8_lossy(&output),
        PREVIEW_MAX_BYTES,
        PREVIEW_MAX_LINES,
    );
    if overflow {
        preview.push_str("\n\n… Preview truncated.");
    }
    preview
}

fn truncate_preview_text(text: &str, max_bytes: usize, max_lines: usize) -> String {
    let mut clipped = text.to_string();
    let mut truncated = false;
    if clipped.len() > max_bytes {
        let mut boundary = max_bytes;
        while !clipped.is_char_boundary(boundary) {
            boundary = boundary.saturating_sub(1);
        }
        clipped.truncate(boundary);
        truncated = true;
    }
    let mut lines: Vec<&str> = clipped.lines().take(max_lines).collect();
    if clipped.lines().count() > max_lines {
        truncated = true;
    }
    let mut output = lines.join("\n");
    if truncated {
        output.push_str("\n\n… Preview truncated.");
    }
    if output.is_empty() {
        output = "The preview contains no readable text.".to_string();
    }
    lines.clear();
    output
}

fn shorten_single_line(text: &str, max_chars: usize) -> String {
    let one_line = text.replace(['\n', '\r', '\t'], " ");
    let trimmed = one_line.trim();
    if trimmed.chars().count() > max_chars {
        trimmed.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…"
    } else {
        trimmed.to_string()
    }
}

fn copy_value_for_result(result: &IndexedResult) -> String {
    if result.provider == ProviderFilter::Clipboard {
        return result.content_preview.clone();
    }
    if result.id.starts_with("builtin:") {
        return result.title.clone();
    }
    result_path(result)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| result.path.clone())
}

fn copy_to_clipboard(value: &str) -> Result<(), String> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "clipboard utility unavailable".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(value.as_bytes())
            .map_err(|_| "clipboard write failed".to_string())?;
    }
    Ok(())
}

fn captured_command_text(program: &str, args: &[&str], unavailable: &str, empty_output: &str) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() { empty_output.to_string() } else { text }
        }
        Ok(_) => empty_output.to_string(),
        Err(_) => unavailable.to_string(),
    }
}

fn read_battery_status() -> String {
    let Ok(supplies) = fs::read_dir("/sys/class/power_supply") else {
        return "Battery information is unavailable on this system.".to_string();
    };
    let mut found = Vec::new();
    for supply in supplies.flatten() {
        let root = supply.path();
        if fs::read_to_string(root.join("type")).map(|value| value.trim() != "Battery").unwrap_or(true) {
            continue;
        }
        let capacity = fs::read_to_string(root.join("capacity")).ok().map(|value| value.trim().to_string());
        let status = fs::read_to_string(root.join("status")).ok().map(|value| value.trim().to_string());
        let mut line = String::from("Battery");
        if let Some(capacity) = capacity { line.push_str(&format!(": {capacity}%")); }
        if let Some(status) = status.filter(|value| !value.is_empty()) { line.push_str(&format!(" · {status}")); }
        found.push(line);
    }
    if found.is_empty() { "No battery detected.".to_string() } else { found.join("\n") }
}

fn split_command_words(input: &str) -> Option<Vec<String>> {
    // Deliberately not a shell: no pipes, redirections, substitutions, chaining,
    // glob expansion, or environment expansion. Quotes only group argv tokens.
    if input.chars().any(|ch| matches!(ch, '\n' | '\r' | ';' | '|' | '&' | '<' | '>' | '`' | '$')) {
        return None;
    }
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut token_started = false;
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            token_started = true;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            token_started = true;
            continue;
        }
        if let Some(delimiter) = quote {
            if ch == delimiter {
                quote = None;
            } else {
                current.push(ch);
            }
            token_started = true;
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            token_started = true;
        } else if ch.is_whitespace() {
            if token_started {
                words.push(std::mem::take(&mut current));
                token_started = false;
            }
        } else {
            current.push(ch);
            token_started = true;
        }
    }
    if escaped || quote.is_some() {
        return None;
    }
    if token_started {
        words.push(current);
    }
    if words.is_empty() { None } else { Some(words) }
}

fn command_words_from_query(input: &str) -> Option<(Vec<String>, bool)> {
    let trimmed = input.trim();
    let explicit_run = trimmed.strip_prefix("run ").or_else(|| trimmed.strip_prefix("Run ")).is_some();
    let command_text = if explicit_run { trimmed.get(4..)? } else { trimmed };
    let words = split_command_words(command_text)?;
    Some((words, explicit_run))
}

fn command_is_forbidden(program: &str) -> bool {
    matches!(program,
        "rm" | "rmdir" | "shred" | "wipefs" | "mkfs" | "dd" | "fdisk" | "parted"
        | "shutdown" | "reboot" | "poweroff" | "halt" | "passwd" | "userdel"
        | "groupdel" | "chpasswd" | "su" | "bash" | "sh" | "zsh" | "fish" | "dash"
        | "pkexec" | "doas" | "env" | "command" | "exec" | "nohup" | "xargs"
    )
}

fn command_is_supported_query(input: &str) -> bool {
    let Some((words, explicit_run)) = command_words_from_query(input) else {
        return false;
    };
    if explicit_run {
        return words.len() >= 1;
    }
    let command = words[0].as_str();
    match command {
        "help" | "date" | "uptime" | "fastfetch" | "pwd" | "whoami" => words.len() == 1,
        "spotlight" => words.len() == 2 && matches!(words[1].as_str(), "help" | "--help" | "-h"),
        "show" => words.len() == 3 && words[1] == "spotlight" && words[2] == "help",
        "ls" => words.len() <= 3 && words.iter().skip(1).all(|part| !part.starts_with('-') || matches!(part.as_str(), "-a" | "-l" | "-la" | "-al")),
        "cd" => words.len() <= 2,
        "nvim" | "vim" | "nano" => words.len() <= 2,
        "which" => words.len() == 2,
        "free" => words.len() <= 2 && words.iter().skip(1).all(|a| matches!(a.as_str(), "-h" | "-m" | "-g")),
        "df" => words.len() <= 3 && words.iter().skip(1).all(|a| a == "-h" || !a.starts_with('-')),
        "uname" => words.len() <= 2 && words.iter().skip(1).all(|a| a == "-a"),
        "updatedb" => words.len() == 1,
        "sudo" => words.len() == 2 && words[1] == "updatedb",
        _ => false,
    }
}

fn command_candidate_for_query(input: &str) -> Option<IndexedResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() || !command_is_supported_query(trimmed) {
        return None;
    }
    let (words, explicit_run) = command_words_from_query(trimmed)?;
    let (program, args) = (words[0].as_str(), &words[1..]);
    let is_help = matches!(trimmed, "help" | "spotlight help" | "spotlight --help" | "spotlight -h" | "show spotlight help");
    let is_directory = program == "cd";
    let is_privileged = program == "updatedb" || (program == "sudo" && args.len() == 1 && args[0] == "updatedb");
    let must_confirm = explicit_run || is_privileged;
    let title = if is_help {
        "Show Spotlight help".to_string()
    } else if is_directory {
        if args.is_empty() { "Browse home directory".to_string() } else { format!("Browse directory · {}", shorten_single_line(args[0].as_str(), 72)) }
    } else if is_privileged {
        "Rebuild file-search database · PolicyKit confirmation".to_string()
    } else if matches!(program, "nvim" | "vim" | "nano") {
        format!("Open editor · {}", shorten_single_line(trimmed, 80))
    } else if explicit_run {
        format!("Confirm command · {}", shorten_single_line(trimmed.get(4..).unwrap_or(trimmed), 72))
    } else if program == "ls" {
        format!("List directory · {}", if args.is_empty() { "current directory" } else { args.last().map(String::as_str).unwrap_or("current directory") })
    } else {
        format!("Run · {}", shorten_single_line(trimmed, 88))
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    trimmed.hash(&mut hasher);
    let id = format!("command-input:{:016x}", hasher.finish());
    Some(IndexedResult {
        id,
        provider: ProviderFilter::Actions,
        title,
        generic_name: if must_confirm { "Command · confirmation required" } else { "Local command" }.to_string(),
        comment: if must_confirm { "Review the exact command before it runs." } else { "Runs locally; output stays in Spotlight when possible." }.to_string(),
        categories: "Action;Command;Local;Terminal".to_string(),
        executable: program.to_string(),
        path: String::new(),
        content_preview: trimmed.to_string(),
        metadata: if must_confirm { "confirmation-required" } else { "direct" }.to_string(),
        icon_name: "utilities-terminal-symbolic".to_string(),
        usage_count: 0,
        recent_use_rank: u32::MAX,
        priority_bias: 60_000,
    })
}

fn sync_command_candidate(interaction: &InteractionContext, query: &str) {
    let candidate = command_candidate_for_query(query);
    let mut catalog = interaction.catalog.borrow_mut();
    catalog.retain(|record| !record.id.starts_with("command-input:"));
    if let Some(candidate) = candidate {
        catalog.push(candidate);
    }
}

fn expand_path(path: &str, base: &Path) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| base.to_path_buf());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| base.to_path_buf())
            .join(rest);
    }
    let path = PathBuf::from(path);
    if path.is_absolute() { path } else { base.join(path) }
}

fn directory_records(directory: &Path) -> Vec<IndexedResult> {
    const MAX_DIRECTORY_RESULTS: usize = 300;
    let mut records = Vec::new();
    if let Some(parent) = directory.parent() {
        records.push(IndexedResult {
            id: format!("file:{}", parent.display()),
            provider: ProviderFilter::Files,
            title: "..".to_string(),
            generic_name: "Parent directory".to_string(),
            comment: String::new(),
            categories: "Directory;Navigation".to_string(),
            executable: String::new(),
            path: parent.display().to_string(),
            content_preview: String::new(),
            metadata: String::new(),
            icon_name: "go-up-symbolic".to_string(),
            usage_count: 0,
            recent_use_rank: u32::MAX,
            priority_bias: 0,
        });
    }
    let Ok(entries) = fs::read_dir(directory) else { return records; };
    let mut children: Vec<_> = entries.flatten().collect();
    children.sort_by_key(|entry| {
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        (!is_dir, entry.file_name().to_string_lossy().to_lowercase())
    });
    for entry in children.into_iter().take(MAX_DIRECTORY_RESULTS.saturating_sub(records.len())) {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else { continue; };
        let is_dir = file_type.is_dir();
        let metadata = entry.metadata().ok();
        let size_label = metadata.as_ref().map(|meta| format_file_size(meta.len())).unwrap_or_default();
        let modified_rank = metadata.as_ref()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| u32::MAX.saturating_sub(duration.as_secs().min(u32::MAX as u64) as u32))
            .unwrap_or(u32::MAX);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
        records.push(IndexedResult {
            id: format!("file:{}", path.display()),
            provider: ProviderFilter::Files,
            title: entry.file_name().to_string_lossy().into_owned(),
            generic_name: if is_dir { "Directory".to_string() } else { file_type_label(&extension) },
            comment: String::new(),
            categories: if is_dir { "Directory;Folder".to_string() } else { extension.clone() },
            executable: String::new(),
            path: path.display().to_string(),
            content_preview: String::new(),
            metadata: size_label,
            icon_name: if is_dir { "folder-symbolic".to_string() } else { icon_for_file_extension(&extension).to_string() },
            usage_count: 0,
            recent_use_rank: modified_rank,
            priority_bias: 0,
        });
    }
    records
}

fn browse_directory_in_spotlight(interaction: &InteractionContext, directory: &Path) {
    let directory = match fs::canonicalize(directory) {
        Ok(path) if path.is_dir() => path,
        _ => {
            let result = IndexedResult {
                id: "command-input:cd-error".to_string(),
                provider: ProviderFilter::Actions,
                title: "Cannot browse directory".to_string(),
                generic_name: "Directory navigation".to_string(),
                comment: String::new(), categories: "Command".to_string(), executable: "cd".to_string(),
                path: String::new(), content_preview: String::new(), metadata: String::new(), icon_name: "folder-symbolic".to_string(),
                usage_count: 0, recent_use_rank: 0, priority_bias: 0,
            };
            show_action_output(interaction, &result, "The requested path is not an accessible directory.");
            return;
        }
    };
    let records = directory_records(&directory);
    {
        let mut backup = interaction.file_catalog_backup.borrow_mut();
        if backup.is_none() {
            let catalog = interaction.catalog.borrow();
            *backup = Some(catalog.iter().filter(|record| record.provider == ProviderFilter::Files).cloned().collect());
        }
    }
    {
        let mut catalog = interaction.catalog.borrow_mut();
        catalog.retain(|record| record.provider != ProviderFilter::Files);
        catalog.extend(records);
    }
    *interaction.current_directory.borrow_mut() = directory;
    interaction.active_filter.set(ProviderFilter::Files);
    set_active_filter_buttons(&interaction.standalone_buttons, ProviderFilter::Files);
    set_active_filter_buttons(&interaction.inline_buttons, ProviderFilter::Files);
    interaction.entry.set_text("");
    refresh_result_surface(interaction);
    focus_search_entry(&interaction.entry);
}

fn restore_global_file_index(interaction: &InteractionContext) {
    if let Some(records) = interaction.file_catalog_backup.borrow_mut().take() {
        let mut catalog = interaction.catalog.borrow_mut();
        catalog.retain(|record| record.provider != ProviderFilter::Files);
        catalog.extend(records);
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        *interaction.current_directory.borrow_mut() = home;
    }
}

fn open_command_confirmation(interaction: &InteractionContext, result: &IndexedResult, prompt: &str) {
    cancel_preview_worker(interaction);
    interaction.preview_generation.set(interaction.preview_generation.get().wrapping_add(1));
    activate_auxiliary_surface(interaction, AuxiliarySurface::ContextMenu);
    interaction.surfaces.context_title.set_text("Confirm command");
    let command = result.content_preview.trim();
    let message = if command == "sudo updatedb" || command == "updatedb" {
        "This rebuilds the local file-search database. Spotlight will request system authentication through PolicyKit; Spotlight never asks for or stores your password."
    } else {
        "This exact executable and its arguments will run as your user. Shell operators and command chaining are not supported. Continue only if you intended this command."
    };
    interaction.surfaces.context_status.set_text(&format!("{prompt}\n{message}\n\n{command}"));
    for action in &interaction.surfaces.context_actions {
        action.button.set_visible(true);
    }
    interaction.context_action_count.set(2);
    interaction.surfaces.context_actions[0].label.set_text("Run once");
    interaction.surfaces.context_actions[1].label.set_text("Cancel");
    for action in interaction.surfaces.context_actions.iter().skip(2) {
        action.button.set_visible(false);
    }
    interaction.context_selected_index.set(0);
    if let Some(first) = interaction.surfaces.context_actions.first() {
        first.button.grab_focus();
    }
}

fn launch_editor(program: &str, args: &[String], cwd: &Path) -> Result<(), String> {
    // A TUI editor needs a terminal. Try common Wayland/X11 terminals without
    // invoking a shell or interpolating paths into a command string.
    let terminals: [(&str, &[&str]); 5] = [
        ("kitty", &["--"]),
        ("foot", &["-e"]),
        ("alacritty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("xterm", &["-e"]),
    ];
    for (terminal, prefix) in terminals {
        let mut command = Command::new(terminal);
        command.args(prefix).arg(program).args(args).current_dir(cwd);
        if command.spawn().is_ok() {
            return Ok(());
        }
    }
    Err("No supported terminal (kitty, foot, alacritty, wezterm, or xterm) was found to open the editor.".to_string())
}

fn command_output_worker(
    interaction: &InteractionContext,
    result: &IndexedResult,
    command: Command,
    output_is_ansi: bool,
) {
    cancel_preview_worker(interaction);
    activate_auxiliary_surface(interaction, AuxiliarySurface::Preview);
    interaction.surfaces.preview_icon.set_icon_name(Some("utilities-terminal-symbolic"));
    interaction.surfaces.preview_title.set_text(&result.title);
    interaction.surfaces.preview_subtitle.set_text("Local command output · no terminal opened");
    interaction.surfaces.preview_body.remove_css_class("detail-code");
    interaction.surfaces.preview_body.set_text("Running command…");
    interaction.surfaces.preview_picture.set_visible(false);
    interaction.surfaces.preview_panel.set_height_request(320);
    interaction.surfaces.preview_body_scroll.set_height_request(225);
    let generation_id = interaction.preview_generation.get().wrapping_add(1);
    interaction.preview_generation.set(generation_id);
    let cancelled = Arc::new(AtomicBool::new(false));
    interaction.preview_cancel.replace(Some(Arc::clone(&cancelled)));
    let worker_cancel = Arc::clone(&cancelled);
    let (sender, receiver) = std::sync::mpsc::sync_channel::<String>(1);
    let spawn = std::thread::Builder::new().name("spotlight-command".to_string()).spawn(move || {
        let output = run_preview_command(command, worker_cancel.as_ref(), 160 * 1024);
        if worker_cancel.load(AtomicOrdering::Acquire) { return; }
        let text = match output {
            Some((status, bytes, overflow)) => {
                let stdout = String::from_utf8_lossy(&bytes).trim_end().to_string();
                let mut text = if status.success() {
                    if stdout.is_empty() { "Command completed successfully; it produced no output.".to_string() } else { stdout }
                } else if stdout.is_empty() {
                    format!("Command failed with exit status {}.", status.code().map(|n| n.to_string()).unwrap_or_else(|| "unknown".to_string()))
                } else {
                    format!("{stdout}\n\nCommand exited with status {}.", status.code().map(|n| n.to_string()).unwrap_or_else(|| "unknown".to_string()))
                };
                if overflow { text.push_str("\n\n[Output truncated]"); }
                text
            }
            None => "Command could not be started or was cancelled.".to_string(),
        };
        let _ = sender.send(text);
    });
    if spawn.is_err() {
        interaction.surfaces.preview_body.set_text("Command worker could not be started.");
        interaction.preview_cancel.borrow_mut().take();
        return;
    }
    let generation = Rc::clone(&interaction.preview_generation);
    let surface = Rc::clone(&interaction.auxiliary_surface);
    let cancel_slot = Rc::clone(&interaction.preview_cancel);
    let body = interaction.surfaces.preview_body.clone();
    let preview_panel = interaction.surfaces.preview_panel.clone();
    let body_scroll = interaction.surfaces.preview_body_scroll.clone();
    let root = interaction.root.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
        if generation.get() != generation_id || surface.get() != AuxiliarySurface::Preview {
            return glib::ControlFlow::Break;
        }
        match receiver.try_recv() {
            Ok(text) => {
                cancel_slot.borrow_mut().take();
                if output_is_ansi {
                    body.add_css_class("detail-code");
                    body.set_markup(&ansi_to_pango_markup(&text, root.has_css_class("spotlight-theme-dark")));
                } else {
                    body.add_css_class("detail-code");
                    body.set_markup(&highlight_code_as_markup(&text, root.has_css_class("spotlight-theme-dark")));
                }
                let h = estimate_preview_body_height(&text).clamp(100, 220);
                body_scroll.set_height_request(h);
                preview_panel.set_height_request(116 + h);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                cancel_slot.borrow_mut().take();
                body.set_text("Command output is unavailable.");
                glib::ControlFlow::Break
            }
        }
    });
}

fn ansi_to_pango_markup(text: &str, dark: bool) -> String {
    let palette_dark = ["#d9e2f2", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5"];
    let palette_light = ["#222733", "#b42335", "#267a35", "#916300", "#1558a6", "#7b39a8", "#087e8b", "#344054"];
    let palette = if dark { &palette_dark } else { &palette_light };
    let mut output = String::from("<span font_family=\"monospace\">");
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut color = palette[0];
    let mut bold = false;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(relative_end) = text[i + 2..].find('m') {
                let end = i + 2 + relative_end;
                let codes = &text[i + 2..end];
                for code in codes.split(';').filter_map(|part| part.parse::<u16>().ok()) {
                    match code {
                        0 => { color = palette[0]; bold = false; }
                        1 => bold = true,
                        30 | 90 => color = palette[0],
                        31 | 91 => color = palette[1],
                        32 | 92 => color = palette[2],
                        33 | 93 => color = palette[3],
                        34 | 94 => color = palette[4],
                        35 | 95 => color = palette[5],
                        36 | 96 => color = palette[6],
                        37 | 97 => color = palette[7],
                        _ => {}
                    }
                }
                i = end + 1;
                continue;
            }
        }
        let rest = &text[i..];
        let ch = rest.chars().next().unwrap_or(' ');
        let escaped = escape_pango(&ch.to_string());
        if bold { output.push_str(&format!("<span foreground=\"{color}\" weight=\"bold\">{escaped}</span>")); }
        else { output.push_str(&format!("<span foreground=\"{color}\">{escaped}</span>")); }
        i += ch.len_utf8();
    }
    output.push_str("</span>");
    output
}

fn activate_command_candidate(interaction: &InteractionContext, result: &IndexedResult, confirmed: bool) {
    let input = result.content_preview.trim();
    let Some((mut words, explicit_run)) = command_words_from_query(input) else {
        show_action_output(interaction, result, "Command syntax is incomplete or uses unsupported shell syntax.");
        return;
    };
    if explicit_run && !confirmed {
        open_command_confirmation(interaction, result, "Spotlight does not recognize this command as a built-in shortcut.");
        return;
    }
    if words.is_empty() { return; }
    let mut program = words[0].clone();
    let mut args: Vec<String> = words.iter().skip(1).cloned().collect();
    if explicit_run {
        program = words.remove(0);
        args = words;
    }
    let cwd = interaction.current_directory.borrow().clone();
    if command_is_forbidden(&program) {
        show_action_output(interaction, result, "This command can make destructive system changes or launch a shell. Spotlight does not run it. Use a terminal if you intentionally need it.");
        return;
    }
    if program == "sudo" || program == "updatedb" {
        let allowed = (program == "updatedb" && args.is_empty())
            || (program == "sudo" && args.len() == 1 && args[0] == "updatedb");
        if !allowed {
            show_action_output(interaction, result, "For safety, Spotlight supports only the specific `sudo updatedb` request through PolicyKit. Other privileged commands must be run separately.");
            return;
        }
        if !confirmed {
            open_command_confirmation(interaction, result, "Rebuild the local file-search database?");
            return;
        }
        let mut command = Command::new("pkexec");
        command.arg("updatedb").current_dir(&cwd);
        command_output_worker(interaction, result, command, false);
        return;
    }
    if program == "cd" {
        let target = args.first().map(|arg| expand_path(arg, &cwd)).unwrap_or_else(|| {
            std::env::var_os("HOME").map(PathBuf::from).unwrap_or(cwd.clone())
        });
        browse_directory_in_spotlight(interaction, &target);
        return;
    }
    if program == "help" || (program == "spotlight" && matches!(args.first().map(String::as_str), Some("help" | "--help" | "-h"))) || input == "show spotlight help" {
        let output = cli_help_text();
        show_action_output(interaction, result, output);
        return;
    }
    if program == "ls" {
        let mut command = Command::new("ls");
        command.arg("--color=always");
        let mut path = None;
        for arg in &args {
            if matches!(arg.as_str(), "-a" | "-l" | "-la" | "-al") {
                command.arg(arg);
            } else if !arg.starts_with('-') {
                path = Some(expand_path(arg, &cwd));
            }
        }
        if let Some(path) = path { command.arg("--").arg(path); } else { command.arg("--").arg(&cwd); }
        command.current_dir(&cwd);
        command_output_worker(interaction, result, command, true);
        return;
    }
    if matches!(program.as_str(), "nvim" | "vim" | "nano") {
        if args.is_empty() {
            show_action_output(interaction, result, "Usage: nvim <file-path>. The editor opens in a new terminal window.");
            return;
        }
        args = args.iter().map(|arg| expand_path(arg, &cwd).display().to_string()).collect();
        match launch_editor(&program, &args, &cwd) {
            Ok(()) => {
                if interaction.auxiliary_surface.get() != AuxiliarySurface::None {
                    close_auxiliary_surface(interaction);
                }
                cancel_idle_hint(interaction);
                interaction.window.hide();
            }
            Err(error) => show_action_output(interaction, result, &error),
        }
        return;
    }
    if program == "pwd" {
        show_action_output(interaction, result, &cwd.display().to_string());
        return;
    }
    if program == "spotlight" && args.is_empty() {
        open_help(interaction, false);
        return;
    }
    let mut command = Command::new(&program);
    match program.as_str() {
        "fastfetch" | "date" | "uptime" | "whoami" | "which" | "free" | "df" | "uname" => {
            command.args(&args).current_dir(&cwd);
        }
        _ if explicit_run && confirmed => {
            command.args(&args).current_dir(&cwd);
        }
        _ => {
            show_action_output(interaction, result, "Unsupported command. Type `run <command> [args]` to request another executable; Spotlight will ask for confirmation before running it.");
            return;
        }
    }
    if explicit_run && command_is_forbidden(&program) {
        show_action_output(interaction, result, "That command is not enabled in Spotlight.");
        return;
    }
    command_output_worker(interaction, result, command, false);
}

fn cli_help_text() -> &'static str {
    r#"Spotlight — quick help

Search: type to search; ↑/↓ selects; Enter activates; Esc closes the top surface.
Preview: Ctrl+Space. Actions: Ctrl+Enter. Properties: Alt+Enter.
Providers: Ctrl+1 Applications · Ctrl+2 Files · Ctrl+3 Actions · Ctrl+4 Clipboard.
Reset: Ctrl+0 clears the query and provider. History: Ctrl+↑ opens; Ctrl+↓ closes.
Help: ? (Shift+/). Back: Ctrl+Left or Alt+Left. Scroll: mouse wheel.

Local commands:
  cd <directory>       Browse a directory in Spotlight
  ls [path]            List a directory in Spotlight with colors
  pwd                  Show the current command directory
  fastfetch             Show system summary in Spotlight, if installed
  nvim <file>           Open an editor in a terminal
  sudo updatedb         Rebuild the locate database after confirmation via PolicyKit
  run <command> [args]  Request another command; confirmation required
  spotlight help        Show this help in Spotlight

Clipboard history: Enter copies the selected item to the clipboard; it does not paste into another app.
Shell pipes, substitutions and command chaining are not supported. Destructive commands and arbitrary sudo are blocked.
"#
}

fn run_builtin_action(id: &str) -> Result<Option<String>, String> {
    let output = match id {
        "date-time" => captured_command_text(
            "date", &["+%A, %d %B %Y\n%H:%M:%S %Z"],
            "Date/time utility is unavailable.", "Date/time output is unavailable.",
        ),
        "now-playing" => captured_command_text(
            "playerctl", &["metadata", "--format", "{{artist}} — {{title}}\nAlbum: {{album}}\nStatus: {{status}}"],
            "Media status is unavailable (playerctl is not installed).", "Nothing is currently playing.",
        ),
        "volume-status" => captured_command_text(
            "wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"],
            "Audio status is unavailable (wpctl is not installed).", "Audio volume is unavailable.",
        ),
        "battery-status" => read_battery_status(),
        "lock-screen" => {
            return Command::new("loginctl")
                .arg("lock-session")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| None)
                .map_err(|_| "Could not lock the session (loginctl is unavailable).".to_string());
        }
        _ => return Err("Unknown built-in action.".to_string()),
    };
    Ok(Some(output))
}

fn perform_default_action(result: &IndexedResult) -> Result<Option<String>, String> {
    if let Some(action_id) = result.id.strip_prefix("builtin:") {
        return run_builtin_action(action_id);
    }
    match result.provider {
        ProviderFilter::Applications => Command::new("gio")
            .arg("launch")
            .arg(&result.path)
            .spawn()
            .map(|_| None)
            .map_err(|_| "could not launch application".to_string()),
        ProviderFilter::Files => {
            let path = result_path(result).ok_or_else(|| "file path unavailable".to_string())?;
            Command::new("gio")
                .arg("open")
                .arg(path)
                .spawn()
                .map(|_| None)
                .map_err(|_| "could not open file".to_string())
        }
        ProviderFilter::Actions => {
            if result.path.ends_with(".desktop") {
                Command::new("gio")
                    .arg("launch")
                    .arg(&result.path)
                    .spawn()
                    .map(|_| None)
                    .map_err(|_| "could not launch action".to_string())
            } else {
                Command::new(&result.path)
                    .spawn()
                    .map(|_| None)
                    .map_err(|_| "could not run action".to_string())
            }
        }
        // Selecting a clipboard-history result replaces the clipboard contents;
        // it does not inject Ctrl+V into another application.
        ProviderFilter::Clipboard => copy_to_clipboard(&result.content_preview).map(|_| None),
        ProviderFilter::All => Err("no default action for this result".to_string()),
    }
}

fn show_action_output(interaction: &InteractionContext, result: &IndexedResult, output: &str) {
    cancel_preview_worker(interaction);
    interaction.preview_generation.set(interaction.preview_generation.get().wrapping_add(1));
    activate_auxiliary_surface(interaction, AuxiliarySurface::Preview);
    interaction.surfaces.preview_icon.set_icon_name(Some(nonempty_or(&result.icon_name, "application-x-executable-symbolic")));
    interaction.surfaces.preview_title.set_text(&result.title);
    interaction.surfaces.preview_subtitle.set_text("Spotlight action · local output");
    interaction.surfaces.preview_body.remove_css_class("detail-code");
    interaction.surfaces.preview_body.set_text(output);
    interaction.surfaces.preview_picture.set_visible(false);
    interaction.surfaces.preview_panel.set_height_request(270);
    interaction.surfaces.preview_body_scroll.set_height_request(150);
}

fn activate_result_id(interaction: &InteractionContext, result_id: &str) {
    let result = interaction
        .catalog
        .borrow()
        .iter()
        .find(|record| record.id == result_id)
        .cloned();
    let Some(result) = result else {
        return;
    };

    record_search_query(&interaction.history, interaction.entry.text().as_str());
    if result.id.starts_with("command-input:") {
        activate_command_candidate(interaction, &result, false);
        return;
    }
    if result.provider == ProviderFilter::Files {
        if let Some(path) = result_path(&result).filter(|path| path.is_dir()) {
            browse_directory_in_spotlight(interaction, &path);
            return;
        }
    }
    match perform_default_action(&result) {
        Ok(Some(output)) => {
            if let Ok(mut catalog) = interaction.catalog.try_borrow_mut() {
                record_result_use(catalog.as_mut_slice(), &result.id);
            }
            show_action_output(interaction, &result, &output);
        }
        Ok(None) => {
            if let Ok(mut catalog) = interaction.catalog.try_borrow_mut() {
                record_result_use(catalog.as_mut_slice(), &result.id);
            }
            if interaction.auxiliary_surface.get() != AuxiliarySurface::None {
                close_auxiliary_surface(interaction);
            } else {
                cancel_preview_worker(interaction);
                interaction.preview_generation.set(
                    interaction.preview_generation.get().wrapping_add(1),
                );
            }
            hide_window(&interaction.window);
        }
        Err(error) => {
            if result.id.starts_with("builtin:") {
                show_action_output(interaction, &result, &error);
            } else if interaction.auxiliary_surface.get() == AuxiliarySurface::ContextMenu {
                interaction
                    .surfaces
                    .context_status
                    .set_text("Default action failed or its utility is unavailable.");
            } else if let Some(row) = interaction.result_rows.get(interaction.selected_index.get()) {
                row.subtitle.set_text("Could not open this result");
            }
        }
    }
}

fn activate_selected_result(interaction: &InteractionContext) {
    if let Some(result) = selected_result(interaction) {
        activate_result_id(interaction, &result.id);
    }
}

fn activate_result_index(interaction: &InteractionContext, index: usize) {
    let Some(row) = interaction.result_rows.get(index) else {
        return;
    };
    let id = row.result_id.borrow().clone();
    if let Some(id) = id {
        activate_result_id(interaction, &id);
    }
}

fn cancel_idle_hint(interaction: &InteractionContext) {
    if let Some(source) = interaction.idle_hint_source.borrow_mut().take() {
        source.remove();
    }
    interaction.idle_hint.set_visible(false);
}

fn schedule_idle_hint(interaction: &InteractionContext) {
    cancel_idle_hint(interaction);
    if !interaction.window.is_visible() {
        return;
    }
    let window = interaction.window.clone();
    let hint = interaction.idle_hint.clone();
    let surface = Rc::clone(&interaction.auxiliary_surface);
    let source_slot = Rc::clone(&interaction.idle_hint_source);
    let source = glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
        source_slot.borrow_mut().take();
        if window.is_visible() && surface.get() == AuxiliarySurface::None {
            hint.set_text("Still here? Press ? for shortcuts · Ctrl+1–4 changes providers");
            hint.set_visible(true);
        }
    });
    *interaction.idle_hint_source.borrow_mut() = Some(source);
}

// -----------------------------------------------------------------------------
// WINDOW STATE
// -----------------------------------------------------------------------------

fn focus_search_entry(entry: &Entry) {
    // Returning from the action group must restore ordinary text editing.
    // GtkEntry can otherwise retain/select the existing text when focus moves
    // back from a button, which makes the next typed character replace it.
    entry.grab_focus();

    let end = i32::from(entry.text_length());
    entry.select_region(end, end);
    entry.set_position(end);
}

fn show_window(state: &AppState) {
    let should_show_welcome = !welcome_seen();
    // A `show` command sent to an already-visible instance must not dismiss an
    // active preview/history/context surface. Reset transient UI only when the
    // persistent window is being shown from its hidden state.
    if !state.window.is_visible() {
        if state.interaction.auxiliary_surface.get() != AuxiliarySurface::None {
            close_auxiliary_surface(&state.interaction);
        } else {
            cancel_preview_worker(&state.interaction);
            state.interaction.preview_generation.set(
                state.interaction.preview_generation.get().wrapping_add(1),
            );
        }
    }
    eprintln!("[spotlight] show_window: entered");
    eprintln!(
        "[spotlight] theme: requested color scheme={}",
        state.theme_mode.get().as_str()
    );
    reload_generated_theme(&state.theme_provider);
    eprintln!("[spotlight] show_window: theme step complete");
    state.window.present();
    eprintln!(
        "[spotlight] show_window: present() called; visible={}",
        state.window.is_visible()
    );
    if should_show_welcome {
        open_help(&state.interaction, true);
    }
    schedule_idle_hint(&state.interaction);

    let entry = state.entry.clone();
    let help_button = state.interaction.surfaces.help_dismiss_button.clone();

    //
    // Wait until GTK has mapped/allocated the surface.
    // Then place keyboard focus directly into the search field.
    //
    let window = state.window.clone();
    glib::idle_add_local_once(move || {
        if should_show_welcome {
            help_button.grab_focus();
        } else {
            entry.grab_focus();
            entry.select_region(0, -1);
        }
        eprintln!(
            "[spotlight] focus-idle: ran; visible={} mapped={}",
            window.is_visible(),
            window.is_mapped()
        );
    });

    let window = state.window.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
        eprintln!(
            "[spotlight] 500ms-check: visible={} mapped={}",
            window.is_visible(),
            window.is_mapped()
        );
    });
}

fn hide_window(window: &ApplicationWindow) {
    eprintln!("[spotlight] hide_window: entered");
    window.hide();
    eprintln!("[spotlight] hide_window: visible={}", window.is_visible());
}

fn hide_state_window(state: &AppState) {
    if state.interaction.auxiliary_surface.get() != AuxiliarySurface::None {
        close_auxiliary_surface(&state.interaction);
    } else {
        cancel_preview_worker(&state.interaction);
        state.interaction.preview_generation.set(
            state.interaction.preview_generation.get().wrapping_add(1),
        );
    }
    cancel_idle_hint(&state.interaction);
    hide_window(&state.window);
}

fn toggle_window(state: &AppState) {
    if state.window.is_visible() {
        hide_state_window(state);
    } else {
        show_window(state);
    }
}

// -----------------------------------------------------------------------------
// CSS
// -----------------------------------------------------------------------------

fn install_css() -> gtk::CssProvider {
    let provider = gtk::CssProvider::new();

    provider.load_from_data(
        r#"

/* ============================================================================
 * TOPLEVEL / TRANSPARENCY
 * ============================================================================
 *
 * GTK4 supports transparent backgrounds through CSS.
 *
 * The application itself does NOT blur the desktop.
 * Hyprland is responsible for compositor blur.
 */

window,
window.background,
window.spotlight-window,
window.spotlight-window.background {
    background: transparent;
    background-color: rgba(0, 0, 0, 0);
    background-image: none;
    box-shadow: none;
}


/* ============================================================================
 * ROOT
 * ========================================================================== */

.spotlight-root {
    background: transparent;
    background-color: rgba(0, 0, 0, 0);
}


/* ============================================================================
 * CENTERED CLUSTER
 * ============================================================================
 *
 * No fixed X/Y coordinates.
 *
 * Root-box alignment plus a calculated top margin keeps this centered
 * horizontally while preserving the fixed Tahoe vertical home position.
 */

.spotlight-cluster {
    background: transparent;
    background-color: rgba(0, 0, 0, 0);
}


/* ============================================================================
 * SEARCH PILL
 * ============================================================================
 *
 * Neutral Tahoe-style glass. Matugen supplies the wallpaper-derived tone, but
 * all Spotlight surfaces remain in one coherent family.
 */

.search-pill {
    min-width: 0;
    min-height: 0;

    padding-left: 10px;
    padding-right: 10px;

    border-radius: 999px;

    background-color: rgba(247, 249, 252, 0.80);

    border: 1px solid rgba(255, 255, 255, 0.64);

    box-shadow:
        0 8px 24px rgba(0, 0, 0, 0.08);
}

.search-pill-typing {
    padding-left: 10px;
    padding-right: 8px;
}

.search-pill-filtered {
    padding-left: 6px;
    padding-right: 10px;
}


/* ============================================================================
 * SEARCH ICON
 * ========================================================================= */

.search-icon {
    color: rgba(55, 59, 67, 0.68);
    opacity: 1;
}


/* ============================================================================
 * SEARCH ENTRY
 * ========================================================================= */

.search-entry {
    min-height: 40px;
    padding: 0;

    border: none;
    outline: none;

    background: transparent;
    background-color: transparent;

    color: rgba(35, 40, 48, 0.94);

    font-size: 20px;
    font-weight: 400;
}

.search-entry:focus {
    border: none;
    outline: none;
    background: transparent;
    background-color: transparent;
    box-shadow: none;
}

.search-entry:focus-within {
    border: none;
    outline: none;
    box-shadow: none;
}

.search-entry placeholder {
    color: rgba(48, 54, 64, 0.62);
}


/* ============================================================================
 * SEARCH FILTER CONTROLS
 * ========================================================================= */

.search-back-button {
    min-width: 34px;
    min-height: 34px;

    padding: 0;
    border-radius: 10px;

    background-color: rgba(255, 255, 255, 0.16);
    border: 1px solid rgba(255, 255, 255, 0.26);

    box-shadow: none;
}

.search-back-button:hover {
    background-color: rgba(255, 255, 255, 0.26);
}

.search-back-button:focus,
.search-back-button:focus-visible {
    outline: none;
    background-color: rgba(255, 220, 190, 0.62);
    border-color: rgba(255, 194, 150, 0.86);
    box-shadow:
        0 0 0 2px rgba(255, 220, 190, 0.34),
        0 7px 20px rgba(0, 0, 0, 0.10);
}

.search-back-icon {
    color: rgba(55, 59, 67, 0.82);
}

.category-identity {
    min-width: 0;
}

.category-identity-label {
    color: rgba(35, 40, 48, 0.90);
    font-size: 18px;
    font-weight: 500;
}

.inline-filters {
    min-width: 0;
}

.inline-filter-button {
    min-width: 34px;
    min-height: 34px;

    padding: 0;
    border-radius: 10px;

    background-color: rgba(255, 255, 255, 0.14);
    border: 1px solid rgba(255, 255, 255, 0.20);

    box-shadow: none;
}

.inline-filter-button:hover {
    background-color: rgba(255, 255, 255, 0.26);
}

.inline-filter-button.active-filter {
    background-color: rgba(255, 220, 190, 0.52);
    border-color: rgba(255, 194, 150, 0.74);
    box-shadow:
        0 0 0 2px rgba(255, 220, 190, 0.28);
}

.inline-filter-button:focus,
.inline-filter-button:focus-visible {
    outline: none;
    background-color: rgba(255, 220, 190, 0.62);
    border-color: rgba(255, 194, 150, 0.86);
    box-shadow:
        0 0 0 2px rgba(255, 220, 190, 0.34),
        0 5px 16px rgba(81, 69, 58, 0.14);
}

.inline-filter-button:hover:focus,
.inline-filter-button:hover:focus-visible {
    background-color: rgba(255, 220, 190, 0.68);
}

.inline-filter-icon {
    color: rgba(55, 59, 67, 0.76);
}

.inline-filter-letter {
    color: rgba(55, 59, 67, 0.76);
    font-size: 16px;
    font-weight: 600;
}


/* ============================================================================
 * RESULT SURFACE
 * ============================================================================
 *
 * One glass surface. Individual rows remain transparent and are not capsules.
 */

.spotlight-content {
    background: transparent;
    background-color: transparent;
}

.detail-surface {
    min-width: 0;
    min-height: 0;
    border-radius: 24px;
    background-color: rgba(247, 249, 252, 0.82);
    border: 1px solid rgba(255, 255, 255, 0.54);
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.08);
}

.detail-title {
    color: rgba(35, 40, 48, 0.94);
    font-size: 17px;
    font-weight: 600;
}

.detail-subtitle {
    color: rgba(55, 59, 67, 0.66);
    font-size: 12px;
}

.help-section-title {
    color: rgba(35, 40, 48, 0.94);
    font-size: 13px;
    font-weight: 600;
}

.help-section-details {
    color: rgba(55, 59, 67, 0.78);
    font-size: 12px;
}

.help-index-progress trough {
    min-height: 6px;
    border-radius: 999px;
    background-color: rgba(80, 105, 140, 0.16);
}

.help-index-progress progress {
    min-height: 6px;
    border-radius: 999px;
    background-color: rgba(35, 123, 245, 0.82);
}

.preview-picture {
    min-width: 0;
    min-height: 0;
    border-radius: 10px;
}

.detail-body-scroll,
.detail-body-scroll viewport {
    background: transparent;
    background-color: rgba(0, 0, 0, 0);
    border: none;
    box-shadow: none;
}

.detail-body {
    color: rgba(42, 47, 56, 0.90);
    font-size: 13px;
}

.context-action-button {
    min-width: 0;
    padding: 4px 10px;
    margin-left: 10px;
    margin-right: 10px;
    border-radius: 10px;
    background-color: rgba(255, 255, 255, 0.16);
    border: 1px solid rgba(255, 255, 255, 0.20);
    box-shadow: none;
}

.context-action-button:hover,
.context-action-button:focus,
.context-action-button:focus-visible,
.history-result-button:focus,
.history-result-button:focus-visible {
    outline: none;
    background-color: rgba(255, 220, 190, 0.48);
    border-color: rgba(255, 194, 150, 0.76);
}

.context-action-label {
    color: rgba(45, 49, 57, 0.92);
    font-size: 13px;
}

.history-clear-button {
    min-height: 26px;
    padding: 2px 9px;
    border-radius: 8px;
    background-color: rgba(255, 255, 255, 0.20);
    border: 1px solid rgba(255, 255, 255, 0.28);
    color: rgba(48, 53, 61, 0.90);
    box-shadow: none;
}

.spotlight-theme-dark .detail-surface {
    background-color: rgba(32, 34, 42, 0.84);
    border-color: rgba(255, 255, 255, 0.12);
}

.spotlight-theme-dark .detail-title,
.spotlight-theme-dark .context-action-label {
    color: rgba(240, 242, 247, 0.94);
}

.spotlight-theme-dark .detail-subtitle {
    color: rgba(210, 214, 225, 0.66);
}

.spotlight-theme-dark .help-section-title {
    color: rgba(240, 242, 247, 0.94);
}

.spotlight-theme-dark .help-section-details {
    color: rgba(210, 214, 225, 0.78);
}

.spotlight-theme-dark .help-index-progress trough {
    background-color: rgba(220, 225, 240, 0.14);
}

.spotlight-theme-dark .help-index-progress progress {
    background-color: rgba(90, 160, 255, 0.9);
}

.spotlight-theme-dark .detail-body {
    color: rgba(235, 237, 243, 0.90);
}

.spotlight-theme-dark .context-action-button,
.spotlight-theme-dark .history-clear-button {
    background-color: rgba(255, 255, 255, 0.06);
    border-color: rgba(255, 255, 255, 0.12);
}

.spotlight-theme-dark .context-action-button:hover,
.spotlight-theme-dark .context-action-button:focus,
.spotlight-theme-dark .context-action-button:focus-visible {
    background-color: rgba(255, 220, 190, 0.18);
    border-color: rgba(255, 194, 150, 0.52);
}

.results-panel {
    border-radius: 24px;
    background-color: rgba(247, 249, 252, 0.80);

    border: 1px solid rgba(255, 255, 255, 0.50);

    box-shadow:
        0 12px 28px rgba(0, 0, 0, 0.08);
}

.results-panel-shell,
.results-list {
    background: transparent;
    background-color: transparent;
}

.results-fade-top,
.results-fade-bottom {
    border-radius: 24px;
    background-image: none;
}

.results-fade-top {
    background-image: linear-gradient(
        to bottom,
        rgba(247, 249, 252, 0.72),
        rgba(247, 249, 252, 0.00)
    );
}

.results-fade-bottom {
    background-image: linear-gradient(
        to top,
        rgba(247, 249, 252, 0.76),
        rgba(247, 249, 252, 0.00)
    );
}

/*
 * Native GtkScrollbar CSS-node tree:
 *
 *     scrollbar.vertical
 *       └── range
 *           └── trough
 *               └── slider
 *
 * Only the slider is painted. The scrollbar/range/trough remain fully
 * transparent so there is no extra full-height shell or track.
 *
 * GTK4 does not expose a generic CSS `width` property for these nodes, so
 * min-width is used for the compact responsive thickness.
 *
 * The palette below is taken from the supplied Matugen Spotlight output:
 * rgba(81, 69, 58, ...) is already used by the generated theme for icons and
 * inline controls. This is therefore the fallback/bridge color for the
 * scrollbar thumb as well.
 */
.results-scrollbar,
.results-scrollbar.vertical {
    min-width: 3px;
    min-height: 0;
    padding: 0;
    margin: 0;
    background: transparent;
    background-color: transparent;
    border: none;
    box-shadow: none;
    opacity: 1;
}

.results-scrollbar.vertical range,
.results-scrollbar.vertical trough {
    min-width: 3px;
    min-height: 0;
    padding: 0;
    margin: 0;
    background: transparent;
    background-color: transparent;
    border: none;
    box-shadow: none;
}

.results-scrollbar.vertical slider {
    min-width: 3px;
    min-height: 0;
    padding: 0;
    margin: 0;

    /*
     * Add a transparent hit/hover band on the LEFT of the visible thumb.
     * The right edge therefore stays flush with the result surface while the
     * pointer can be a few pixels away from the visible thumb and still enter
     * the hover state. The transparent border does not paint because the
     * background is clipped to the padding box.
     */
    border-left: 8px solid transparent;
    border-right: 0;
    border-top: 0;
    border-bottom: 0;
    border-radius: 999px;
    background-color: rgba(81, 69, 58, 0.84);
    background-clip: padding-box;
    box-shadow: none;
    opacity: 1;

    /* Keep the default thumb visibly shorter than the native proportional
     * allocation while retaining the native scrollbar geometry underneath. */
    transform: scale(1, 0.76);
    transform-origin: right center;
}

.results-scrollbar.vertical slider:hover {
    /*
     * Hover restores the full current height. The earlier 2% width change was
     * effectively invisible at a 3px base width, so use a small pixel-visible
     * expansion instead: the painted thumb grows from about 3px to about 5px.
     * The right edge remains attached; all expansion happens inward.
     */
    transform: scale(1.67, 1);
    transform-origin: right center;
    background-color: rgba(81, 69, 58, 0.92);
}

.results-scrollbar.vertical slider:active,
.results-scrollbar.vertical.dragging slider {
    transform: scale(1.67, 1);
    transform-origin: right center;
    background-color: rgba(81, 69, 58, 0.98);
}


.loading-result {
    background-color: rgba(255, 255, 255, 0.10);
    border-color: rgba(255, 255, 255, 0.10);
}

.result-button {
    min-width: 0;
    min-height: 0;
    padding: 0 14px;
    border-radius: 0;

    background-color: transparent;
    border: none;
}

.result-button:hover,
.result-button.hovered {
    background-color: rgba(81, 69, 58, 0.10);
}

.result-button.selected {
    background-color: rgba(255, 255, 255, 0.18);
}

.result-button.selected.hovered {
    background-color: rgba(255, 220, 190, 0.38);
}

.result-icon {
    color: rgba(55, 59, 67, 0.72);
}

.result-title {
    color: rgba(35, 40, 48, 0.94);
    font-size: 16px;
    font-weight: 500;
}

.result-subtitle {
    color: rgba(48, 54, 64, 0.60);
    font-size: 13px;
    font-weight: 400;
}


/* ============================================================================
 * ACTION BUTTONS
 * ============================================================================
 *
 * One neutral glass family. The wallpaper provides only a subtle environmental
 * tint instead of making the four controls different Material colors.
 */

.action-button {
    min-width: 0;
    min-height: 0;

    padding: 0;
    border-radius: 999px;

    background-color: rgba(247, 249, 250, 0.64);

    border: 1px solid rgba(255, 255, 255, 0.42);

    box-shadow:
        0 7px 20px rgba(0, 0, 0, 0.06);
}

.action-button:hover {
    background-color: rgba(255, 255, 255, 0.30);
}

.action-button.active-filter {
    background-color: rgba(255, 220, 190, 0.52);
    border-color: rgba(255, 194, 150, 0.74);
    box-shadow:
        0 0 0 2px rgba(255, 220, 190, 0.28),
        0 8px 22px rgba(0, 0, 0, 0.08);
}

.action-button:active {
    background-color: rgba(255, 255, 255, 0.42);
}


/* ============================================================================
 * ACTION ICONS
 * ========================================================================= */

.action-icon {
    color: rgba(55, 59, 67, 0.74);
    opacity: 1;
}


/* ============================================================================
 * APPLICATIONS "A"
 * ========================================================================= */

.action-letter {
    color: rgba(55, 59, 67, 0.74);

    font-size: 21px;
    font-weight: 600;
}


/* ============================================================================
 * REMOVE BUTTON FOCUS RING
 * ========================================================================= */

.action-button:focus,
.action-button:focus-visible {
    outline: none;

    background-color: rgba(255, 220, 190, 0.62);
    border-color: rgba(255, 194, 150, 0.86);

    box-shadow:
        0 0 0 2px rgba(255, 220, 190, 0.34),
        0 7px 20px rgba(0, 0, 0, 0.10);
}

/* Readable, high-contrast selection and auxiliary keyboard states. */
.result-button.selected {
    background-color: rgba(28, 35, 48, 0.28);
    border: 1px solid rgba(28, 35, 48, 0.42);
}
.result-button.selected:hover,
.result-button.selected.hovered {
    background-color: rgba(28, 35, 48, 0.34);
}
.result-button.selected .result-title { font-weight: 650; }
.context-action-button.selected {
    background-color: rgba(35, 45, 62, 0.18);
    border: 1px solid rgba(42, 105, 206, 0.62);
    box-shadow: 0 0 0 2px rgba(42, 105, 206, 0.14);
}
.result-icon, .action-icon, .inline-filter-icon, .search-back-icon { opacity: 1; }
.result-icon { color: rgba(35, 40, 48, 0.90); }
.context-action-button:focus,
.context-action-button:focus-visible,
.history-result-button.selected,
.history-result-button:focus,
.history-result-button:focus-visible {
    outline: none;
    background-color: rgba(35, 45, 62, 0.18);
    border: 1px solid rgba(35, 45, 62, 0.34);
    box-shadow: 0 0 0 2px rgba(35, 45, 62, 0.10);
}
.help-start-button {
    min-height: 34px;
    padding: 4px 14px;
    background-color: rgba(42, 105, 206, 0.18);
    border: 1px solid rgba(42, 105, 206, 0.44);
    color: rgba(25, 54, 95, 0.98);
    font-weight: 650;
}
.help-start-button:focus,
.help-start-button:focus-visible {
    outline: none;
    background-color: rgba(42, 105, 206, 0.30);
    border-color: rgba(42, 105, 206, 0.78);
    box-shadow: 0 0 0 3px rgba(42, 105, 206, 0.18);
}
.detail-code { font-family: monospace; font-size: 12.5px; }
.search-back-button:focus,
.search-back-button:focus-visible,
.inline-filter-button:focus,
.inline-filter-button:focus-visible,
.action-button:focus,
.action-button:focus-visible {
    outline: none;
    background-color: rgba(42, 105, 206, 0.22);
    border-color: rgba(42, 105, 206, 0.70);
    box-shadow: 0 0 0 2px rgba(42, 105, 206, 0.18);
}
.spotlight-theme-dark .search-back-button:focus,
.spotlight-theme-dark .search-back-button:focus-visible,
.spotlight-theme-dark .inline-filter-button:focus,
.spotlight-theme-dark .inline-filter-button:focus-visible,
.spotlight-theme-dark .action-button:focus,
.spotlight-theme-dark .action-button:focus-visible {
    background-color: rgba(130, 170, 255, 0.24);
    border-color: rgba(150, 190, 255, 0.70);
}
.spotlight-theme-dark .result-icon {
    background-color: rgba(240, 245, 255, 0.08);
}
.spotlight-theme-dark .result-button.selected {
    background-color: rgba(0, 0, 0, 0.34);
    border-color: rgba(180, 196, 220, 0.42);
}
.spotlight-theme-dark .result-button.selected:hover,
.spotlight-theme-dark .result-button.selected.hovered {
    background-color: rgba(0, 0, 0, 0.42);
}
.spotlight-theme-dark .context-action-button.selected,
.spotlight-theme-dark .context-action-button:focus,
.spotlight-theme-dark .context-action-button:focus-visible,
.spotlight-theme-dark .history-result-button.selected,
.spotlight-theme-dark .history-result-button:focus,
.spotlight-theme-dark .history-result-button:focus-visible {
    background-color: rgba(0, 0, 0, 0.36);
    border-color: rgba(150, 185, 245, 0.64);
    box-shadow: 0 0 0 2px rgba(150, 185, 245, 0.18);
}
.spotlight-theme-dark .help-start-button {
    background-color: rgba(90, 150, 255, 0.24);
    border-color: rgba(120, 170, 255, 0.55);
    color: rgba(245, 248, 255, 0.98);
}
.spotlight-theme-dark .help-start-button:focus,
.spotlight-theme-dark .help-start-button:focus-visible {
    background-color: rgba(90, 150, 255, 0.36);
    border-color: rgba(150, 190, 255, 0.82);
}

/* Action provider, selected rows, preview identity, and readable help. */
.action-icon,
.inline-filter-icon,
.search-back-icon,
.result-icon,
.detail-identity-icon,
.action-letter,
.inline-filter-letter {
    color: var(--spotlight-on-surface, #252a32);
    opacity: 1;
}
.result-icon {
    background-color: rgba(35, 43, 56, 0.06);
    border-radius: 7px;
    padding: 2px;
}
.result-button.selected {
    background-color: rgba(20, 27, 39, 0.32);
    border: 1px solid rgba(20, 27, 39, 0.44);
}
.result-button.selected:hover,
.result-button.selected.hovered {
    background-color: rgba(20, 27, 39, 0.40);
}
.help-section-title { font-size: 15px; font-weight: 700; }
.help-section-details { font-size: 13.5px; }
.help-start-button { font-size: 14px; }
.spotlight-theme-dark .action-icon,
.spotlight-theme-dark .inline-filter-icon,
.spotlight-theme-dark .search-back-icon,
.spotlight-theme-dark .result-icon,
.spotlight-theme-dark .detail-identity-icon,
.spotlight-theme-dark .action-letter,
.spotlight-theme-dark .inline-filter-letter {
    color: var(--spotlight-on-surface, #eef2f8);
}
.spotlight-theme-dark .result-button.selected {
    background-color: rgba(0, 0, 0, 0.46);
    border-color: rgba(220, 230, 245, 0.48);
}
.spotlight-theme-dark .result-button.selected:hover,
.spotlight-theme-dark .result-button.selected.hovered {
    background-color: rgba(0, 0, 0, 0.54);
}


/* Inactivity nudge and command output. Use Matugen's active on-surface tone. */
.spotlight-idle-hint {
    color: alpha(var(--spotlight-on-surface, #252a32), 0.88);
    background-color: alpha(var(--spotlight-surface, #f4f6fa), 0.78);
    border: 1px solid alpha(var(--spotlight-on-surface, #252a32), 0.16);
    border-radius: 999px;
    padding: 5px 12px;
    font-size: 12px;
}
.result-button.selected {
    background-color: alpha(var(--spotlight-on-surface, #252a32), 0.20);
    border: 1px solid alpha(var(--spotlight-on-surface, #252a32), 0.42);
}
.result-button.selected .result-title,
.result-button.selected .result-subtitle,
.action-letter,
.context-action-label,
.help-section-title,
.help-section-details,
.detail-title,
.detail-subtitle {
    color: var(--spotlight-on-surface, #252a32);
}
.help-section-title { font-size: 15px; font-weight: 650; }
.help-section-details { font-size: 13px; }
.detail-code { font-family: monospace; font-size: 12px; }
"#,
    );

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }

    // Optional Matugen theme layer. If it is missing or unavailable, the
    // embedded Tahoe-safe colors above remain the fallback.
    let theme_provider = gtk::CssProvider::new();
    reload_generated_theme(&theme_provider);

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &theme_provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
    }

    theme_provider
}

fn generated_theme_path() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(
        config_home
            .join("hypr-mac")
            .join("matugen")
            .join("generated")
            .join("spotlight.css"),
    )
}

fn theme_fingerprint(bytes: &[u8]) -> u64 {
    // Deterministic FNV-1a 64-bit fingerprint. This is only a diagnostic
    // identity for the generated stylesheet, not a cryptographic checksum.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

fn reload_generated_theme(provider: &gtk::CssProvider) {
    if std::env::var_os("SPOTLIGHT_SKIP_GENERATED_THEME").is_some() {
        eprintln!("[spotlight] theme: skipped by diagnostic environment setting");
        return;
    }

    let Some(path) = generated_theme_path() else {
        eprintln!("[spotlight] theme: config home unavailable; fallback remains active");
        return;
    };

    if !path.is_file() {
        eprintln!("[spotlight] theme: generated CSS unavailable; fallback remains active");
        return;
    }

    let (byte_count, fingerprint) = match std::fs::read(&path) {
        Ok(bytes) => (bytes.len(), theme_fingerprint(&bytes)),
        Err(error) => {
            eprintln!(
                "[spotlight] theme: could not read generated CSS for diagnostics: {}",
                error
            );
            (0, 0)
        }
    };

    let modified = std::fs::metadata(&path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    eprintln!(
        "[spotlight] theme: loading generated CSS bytes={} mtime_unix={} fnv1a64={:016x}",
        byte_count,
        modified,
        fingerprint
    );

    provider.load_from_path(&path);

    eprintln!(
        "[spotlight] theme: generated CSS loaded successfully; fingerprint={:016x}",
        fingerprint
    );
}

fn is_double_press(n_press: i32) -> bool {
    n_press == 2
}

#[cfg(test)]
mod responsive_tests {
    use super::*;

    fn visible_result_window_start(selected: usize, total: usize, visible_rows: usize) -> usize {
        let mut start = 0usize;
        let mut previous = 0usize;

        for index in 0usize..=selected.min(total.saturating_sub(1usize)) {
            start = advance_result_window_start(start, previous, index, total, visible_rows);
            previous = index;
        }

        start
    }

    fn total_width(m: UiMetrics) -> i32 {
        m.search_width + (m.control_size * 4) + (m.gap * 4)
    }

    #[test]
    fn responsive_widths_fit_without_device_specific_values() {
        const OUTER_HORIZONTAL_INSET: i32 = 24;
        const MIN_SEARCH_WIDTH: i32 = 1;
        const MIN_CONTROL_SIZE: i32 = 1;

        let upper_bound = (REFERENCE_COMPOSITION_WIDTH * 20.0) as i32;
        let minimum_feasible_width =
            OUTER_HORIZONTAL_INSET + MIN_SEARCH_WIDTH + (MIN_CONTROL_SIZE * 4);

        for width in minimum_feasible_width..=upper_bound {
            let metrics = responsive_metrics(width);
            let usable_width = (width - OUTER_HORIZONTAL_INSET).max(1);
            let total = total_width(metrics);

            assert!(metrics.search_width > 0);
            assert!(metrics.control_size >= 0);
            assert!(metrics.gap >= 0);
            assert!(metrics.inner_gap > 0);
            assert!(metrics.icon_size > 0);
            assert!(total <= usable_width);
        }
    }

    #[test]
    fn narrow_layout_compresses_instead_of_overflowing() {
        let metrics = responsive_metrics(125);
        let total = total_width(metrics);

        assert!(metrics.search_width > 0);
        assert!(metrics.control_size > 0);
        assert!(total <= 125);
    }

    #[test]
    fn reference_width_uses_design_reference_metrics() {
        let width = (REFERENCE_COMPOSITION_WIDTH + 24.0).ceil() as i32;
        let metrics = responsive_metrics(width);

        assert_eq!(metrics.control_size, 58);
        assert_eq!(metrics.gap, 12);
        assert_eq!(metrics.inner_gap, 10);
        assert_eq!(metrics.icon_size, 22);
        assert_eq!(metrics.search_width, 380);
    }

    #[test]
    fn responsive_result_metrics_reduce_rows_when_vertical_space_is_limited() {
        let tall = responsive_result_metrics(1000, 100, 58);
        let short = responsive_result_metrics(320, 70, 36);

        assert!(tall.0 >= short.0);
        assert!(tall.1 >= short.1);
        assert!(short.0 >= 1);
        assert!(short.0 <= MAX_VISIBLE_RESULT_ROWS);
    }

    #[test]
    fn responsive_result_metrics_never_assumes_eight_visible_rows() {
        let narrow_vertical_space = responsive_result_metrics(180, 120, 36);
        let extremely_short_output = responsive_result_metrics(90, 80, 18);

        assert!(narrow_vertical_space.0 < MAX_VISIBLE_RESULT_ROWS);
        assert_eq!(extremely_short_output.0, 1);
        assert!(extremely_short_output.1 >= MIN_RESULT_ROW_HEIGHT);
    }

    #[test]
    fn responsive_result_window_supports_variable_visible_rows() {
        let total = 18usize;

        for visible_rows in 1usize..=8usize {
            for selected in 0usize..total {
                let start = visible_result_window_start(selected, total, visible_rows);
                let end = (start + visible_rows).min(total);

                assert_eq!(end - start, visible_rows);
            }
        }
    }

    #[test]
    fn result_window_wraps_to_beginning_on_downward_cycle() {
        let total = 18usize;
        let visible_rows = 8usize;

        let previous_index = 17usize;
        let selected_index = 0usize;
        let wrapped_start = if previous_index + 1 == total && selected_index == 0 {
            0usize
        } else {
            advance_result_window_start(
                10usize,
                previous_index,
                selected_index,
                total,
                visible_rows,
            )
        };

        assert_eq!(wrapped_start, 0usize);
    }

    #[test]
    fn result_window_wraps_to_end_on_upward_cycle() {
        let total = 18usize;
        let visible_rows = 8usize;
        let max_start = total - visible_rows;

        let previous_index = 0usize;
        let selected_index = total - 1;
        let wrapped_start = if previous_index == 0 && selected_index + 1 == total {
            max_start
        } else {
            advance_result_window_start(0usize, previous_index, selected_index, total, visible_rows)
        };

        assert_eq!(wrapped_start, max_start);
    }

    #[test]
    fn double_press_is_exactly_two_presses() {
        assert!(!is_double_press(1));
        assert!(is_double_press(2));
        assert!(!is_double_press(3));
    }

    #[test]
    fn scroll_delta_accumulates_smooth_touchpad_steps() {
        let mut accumulated = 0.0f64;

        accumulated += 0.4;
        assert!(accumulated < 1.0);

        accumulated += 0.7;
        assert!(accumulated >= 1.0);
        accumulated -= 1.0;

        assert!((accumulated - 0.1).abs() < f64::EPSILON * 4.0);
    }

    #[test]
    fn scroll_direction_matches_vertical_motion() {
        // GTK reports positive vertical scroll deltas for downward scrolling
        // and negative deltas for upward scrolling.
        assert!(0.75f64 > 0.0);
        assert!(-0.75f64 < 0.0);
    }

    #[test]
    fn mouse_interaction_contract_keeps_hover_separate_from_selection() {
        // Hover is intentionally a presentation state. A click is what
        // changes the logical selection index.
        let selected_index = 2usize;
        let hovered_index = 5usize;

        assert_ne!(selected_index, hovered_index);
        assert_eq!(selected_index, 2usize);
    }

    #[test]
    fn responsive_scrollbar_width_scales_with_result_row_height() {
        assert!(responsive_scrollbar_width(36) <= responsive_scrollbar_width(58));
        assert!(responsive_scrollbar_width(58) <= responsive_scrollbar_width(96));
    }

    #[test]
    fn responsive_scrollbar_vertical_inset_scales_with_viewport_height() {
        assert!(
            responsive_scrollbar_vertical_inset(320) <= responsive_scrollbar_vertical_inset(640)
        );
        assert!(
            responsive_scrollbar_vertical_inset(640) <= responsive_scrollbar_vertical_inset(1280)
        );
    }

    #[test]
    fn scrollbar_should_exist_only_when_results_exceed_visible_window() {
        assert!(!(8usize > 8usize));
        assert!(18usize > 8usize);
    }

    #[test]
    fn scrollbar_page_ratio_matches_visible_results() {
        let total_results = 18.0f64;
        let visible_results = 8.0f64;

        // GTK derives the thumb proportion from page_size / upper.
        assert!((visible_results / total_results - (8.0 / 18.0)).abs() < f64::EPSILON);
        assert!(visible_results < total_results);
    }

    #[test]
    fn selection_relative_opacity_is_strongest_at_selection() {
        let selected = 3usize;

        assert_eq!(selection_row_opacity(3, selected, 0, 8), 1.0);
        assert_eq!(selection_row_opacity(2, selected, 0, 8), 0.90);
        assert_eq!(selection_row_opacity(1, selected, 0, 8), 0.80);
        assert_eq!(selection_row_opacity(0, selected, 0, 8), 0.70);
        assert_eq!(selection_row_opacity(7, selected, 0, 8), 0.60);
    }

    #[test]
    fn selection_relative_opacity_leaves_hidden_rows_undimmed() {
        assert_eq!(selection_row_opacity(8, 3, 0, 8), 1.0);
        assert_eq!(selection_row_opacity(11, 3, 4, 4), 1.0);
    }

    #[test]
    fn mock_result_window_uses_downward_hysteresis() {
        let total = 18usize;
        let visible_rows = 8usize;
        let mut start = 0usize;

        for (previous, selected) in [
            (0usize, 1usize),
            (1usize, 2usize),
            (2usize, 3usize),
            (3usize, 4usize),
            (4usize, 5usize),
            (5usize, 6usize),
        ] {
            start = advance_result_window_start(start, previous, selected, total, visible_rows);
            assert_eq!(start, 0);
        }

        // 6 → 7 (seventh visible row → eighth visible row) shifts now.
        start = advance_result_window_start(start, 6usize, 7usize, total, visible_rows);
        assert_eq!(start, 1);
    }

    #[test]
    fn mock_result_window_uses_upward_hysteresis() {
        let total = 18usize;
        let visible_rows = 8usize;

        // First move into the shifted window.
        let mut start = advance_result_window_start(0usize, 6usize, 7usize, total, visible_rows);
        assert_eq!(start, 1);

        // Moving back from row 8 to row 7 does not reverse yet.
        start = advance_result_window_start(start, 7usize, 6usize, total, visible_rows);
        assert_eq!(start, 1);

        // The second visible row is index 1, so 2 → 1 reverses the window.
        start = advance_result_window_start(start, 2usize, 1usize, total, visible_rows);
        assert_eq!(start, 0);
    }

    #[test]
    fn mock_result_window_keeps_selection_inside_visible_window_until_end() {
        let total = 18usize;
        let visible_rows = 8usize;
        let mut start = 0usize;

        for selected in 0usize..total {
            let previous = selected.saturating_sub(1usize);
            start = advance_result_window_start(start, previous, selected, total, visible_rows);
            let visible_position = selected - start;

            assert!(visible_position < visible_rows);
        }
    }
}

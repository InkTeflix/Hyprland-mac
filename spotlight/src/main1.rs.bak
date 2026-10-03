use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Entry, Image, Label, Overlay,
    Orientation, PolicyType, ScrolledWindow,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

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
// APPLICATION STATE
// -----------------------------------------------------------------------------

struct AppState {
    window: ApplicationWindow,
    entry: Entry,
    theme_provider: gtk::CssProvider,
}

#[derive(Clone)]
struct MockResultRow {
    button: Button,
    title: Label,
    subtitle: Label,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MockFilter {
    All,
    Applications,
    Files,
    Actions,
    Clipboard,
}

impl MockFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "Spotlight",
            Self::Applications => "Applications",
            Self::Files => "Files",
            Self::Actions => "Actions",
            Self::Clipboard => "Clipboard",
        }
    }

    fn row_prefix(self) -> &'static str {
        match self {
            Self::All => "Top result",
            Self::Applications => "Application result",
            Self::Files => "File result",
            Self::Actions => "Action result",
            Self::Clipboard => "Clipboard result",
        }
    }
}

fn filter_from_index(index: usize) -> MockFilter {
    match index {
        0 => MockFilter::Applications,
        1 => MockFilter::Files,
        2 => MockFilter::Actions,
        3 => MockFilter::Clipboard,
        _ => MockFilter::All,
    }
}

fn filter_index(filter: MockFilter) -> Option<usize> {
    match filter {
        MockFilter::All => None,
        MockFilter::Applications => Some(0),
        MockFilter::Files => Some(1),
        MockFilter::Actions => Some(2),
        MockFilter::Clipboard => Some(3),
    }
}

// -----------------------------------------------------------------------------
// MAIN
// -----------------------------------------------------------------------------

fn main() -> glib::ExitCode {
    eprintln!("[spotlight] main: entered");

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    eprintln!("[spotlight] main: application constructed");

    let state: Rc<RefCell<Option<AppState>>> =
        Rc::new(RefCell::new(None));

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
            let (window, entry, theme_provider) = build_window(app);
            eprintln!("[spotlight] startup: window built");

            window.hide();
            eprintln!("[spotlight] startup: window hidden");

            *state.borrow_mut() = Some(AppState {
                window,
                entry,
                theme_provider,
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
    //
    // Because GtkApplication uses a unique application ID, subsequent
    // invocations are forwarded to the already-running process.
    //
    {
        let state = Rc::clone(&state);

        app.connect_command_line(move |_app, command_line| {
            eprintln!("[spotlight] command-line: callback entered");
            let args = command_line.arguments();

            let command = args
                .get(1)
                .and_then(|arg| arg.to_str())
                .unwrap_or("");

            eprintln!("[spotlight] command-line: command={command:?}");

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
                    hide_window(&state.window);
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
    const REFERENCE_COMPOSITION_WIDTH: f64 = 660.0;
    const REFERENCE_SEARCH_WIDTH: f64 = 380.0;
    const REFERENCE_CONTROL_SIZE: f64 = 58.0;
    const REFERENCE_GAP: f64 = 12.0;
    const REFERENCE_INNER_GAP: f64 = 10.0;
    const REFERENCE_ICON_SIZE: f64 = 22.0;

    let available_width = (output_width - 24).max(1) as f64;

    // Never enlarge the four controls beyond their Tahoe reference size.
    // When the display is narrower than the reference composition, scale
    // the composition down as a whole.
    let scale = (available_width / REFERENCE_COMPOSITION_WIDTH).min(1.0);

    let control_size = (REFERENCE_CONTROL_SIZE * scale)
        .round()
        .clamp(18.0, REFERENCE_CONTROL_SIZE) as i32;

    let gap = (REFERENCE_GAP * scale)
        .round()
        .clamp(4.0, REFERENCE_GAP) as i32;

    let inner_gap = (REFERENCE_INNER_GAP * scale)
        .round()
        .clamp(4.0, REFERENCE_INNER_GAP) as i32;

    let icon_size = (REFERENCE_ICON_SIZE * scale)
        .round()
        .clamp(12.0, REFERENCE_ICON_SIZE) as i32;

    let search_width = if scale < 1.0 {
        (REFERENCE_SEARCH_WIDTH * scale).round().max(1.0) as i32
    } else {
        // Preserve the current 380px reference at normal widths.
        // On very wide outputs, let the search field grow, but keep a
        // deliberate ceiling so the interface never becomes enormous.
        let extra_width = ((output_width - 1366).max(0) as f64) * 0.25;
        (REFERENCE_SEARCH_WIDTH + extra_width)
            .min(900.0)
            .round() as i32
    };

    UiMetrics {
        search_width,
        control_size,
        gap,
        inner_gap,
        icon_size,
    }
}

// -----------------------------------------------------------------------------
// WINDOW
// -----------------------------------------------------------------------------

fn build_window(app: &Application) -> (ApplicationWindow, Entry, gtk::CssProvider) {
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
    //     - use 1366x768
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

    // Layer-shell will otherwise size the surface to the natural size of the
    // child hierarchy. Because we want the compositor to center the Spotlight
    // cluster inside the full output, derive the surface size from runtime
    // monitor geometry rather than hard-coding a resolution.
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
        .unwrap_or((640, 480));

    let metrics = responsive_metrics(output_width);

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
    let search_pill = GtkBox::new(Orientation::Horizontal, 8);
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

    let inline_applications =
        make_inline_filter_button("applications-button", "A", "Applications");
    let inline_files =
        make_inline_filter_icon_button("files-button", "folder-symbolic", "Files");
    let inline_actions =
        make_inline_filter_icon_button("actions-button", "view-grid-symbolic", "Actions");
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

    let applications =
        make_letter_button("applications-button", "A", "Applications", metrics.control_size);

    let files = make_icon_button(
        "files-button",
        "folder-symbolic",
        "Files",
        metrics.control_size,
        metrics.icon_size,
    );

    let actions = make_icon_button(
        "actions-button",
        "view-grid-symbolic",
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
    // STATIC RESULT PREVIEW
    // -----------------------------------------------------------------------------

    // Keep exactly eight rows visible. On the current 1366×768 reference layout,
    // row 8 ends in the lower dock-safe fade.
    const VISIBLE_MOCK_ROWS: i32 = 8;
    const MOCK_ROW_HEIGHT: i32 = 58;
    let result_view_height = VISIBLE_MOCK_ROWS * MOCK_ROW_HEIGHT;

    let (results_panel, results_scroll, result_rows) =
        make_mock_results(metrics.search_width, result_view_height);
    let result_rows = Rc::new(result_rows);
    let selected_index = Rc::new(Cell::new(0usize));
    let active_filter = Rc::new(Cell::new(MockFilter::All));

    content.append(&cluster);
    content.append(&results_panel);
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

    for (button_index, button) in standalone_filter_buttons.iter().cloned().enumerate() {
        let active_filter = Rc::clone(&active_filter);
        let results_panel = results_panel.clone();
        let results_scroll = results_scroll.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();
        let inline_filter_buttons = inline_filter_buttons.clone();
        let entry = entry.clone();

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
                380,
            );

            update_mock_results(
                &results_panel,
                &results_scroll,
                result_rows.as_ref(),
                &selected_index,
                filter,
                entry.text().as_str(),
            );
        });
    }

    for (button_index, button) in inline_filter_buttons.iter().cloned().enumerate() {
        let active_filter = Rc::clone(&active_filter);
        let results_panel = results_panel.clone();
        let results_scroll = results_scroll.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();
        let inline_filter_buttons = inline_filter_buttons.clone();
        let entry = entry.clone();

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
                380,
            );

            update_mock_results(
                &results_panel,
                &results_scroll,
                result_rows.as_ref(),
                &selected_index,
                filter,
                entry.text().as_str(),
            );
        });
    }

    // Back from a focused provider restores normal Spotlight. If text remains,
    // the compact four options return inside the pill; if empty, the four large
    // circular controls return outside the pill.
    {
        let active_filter = Rc::clone(&active_filter);
        let results_panel = results_panel.clone();
        let results_scroll = results_scroll.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();
        let inline_filter_buttons = inline_filter_buttons.clone();
        let entry = entry.clone();

        back_button.clone().connect_clicked(move |_| {
            active_filter.set(MockFilter::All);
            set_active_filter_buttons(&standalone_filter_buttons, MockFilter::All);
            set_active_filter_buttons(&inline_filter_buttons, MockFilter::All);

            update_search_chrome(
                &search_pill,
                &search_icon,
                &back_button,
                &category_box,
                &category_label,
                &inline_filters,
                &standalone_filter_buttons,
                MockFilter::All,
                entry.text().as_str(),
                380,
            );

            update_mock_results(
                &results_panel,
                &results_scroll,
                result_rows.as_ref(),
                &selected_index,
                MockFilter::All,
                entry.text().as_str(),
            );
        });
    }

    // KEYBOARD HANDLING
    // -------------------------------------------------------------------------

    let key_controller = gtk::EventControllerKey::new();

    {
        let window = window.clone();
        let results_panel = results_panel.clone();
        let results_scroll = results_scroll.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);

        key_controller.connect_key_pressed(
            move |_controller, key, _keycode, _state| {
                if key == gdk::Key::Escape {
                    hide_window(&window);

                    return glib::Propagation::Stop;
                }

                if !results_panel.is_visible() || result_rows.is_empty() {
                    return glib::Propagation::Proceed;
                }

                let current = selected_index.get();

                match key {
                    // Up/down control result selection. Left/right remain the
                    // normal text-cursor controls of the focused search entry.
                    gdk::Key::Down => {
                        let next = (current + 1) % result_rows.len();
                        set_mock_selection(
                            &results_scroll,
                            &result_rows,
                            &selected_index,
                            next,
                        );
                        glib::Propagation::Stop
                    }
                    gdk::Key::Up => {
                        let next = if current == 0 {
                            result_rows.len() - 1
                        } else {
                            current - 1
                        };
                        set_mock_selection(
                            &results_scroll,
                            &result_rows,
                            &selected_index,
                            next,
                        );
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            },
        );
    }

    window.add_controller(key_controller);

    // GtkEntry exposes Enter as its activation action. This is preferable to
    // intercepting Return at the key-event level because it preserves normal
    // text editing while treating both the main Enter key and the numeric
    // keypad Enter key as the same confirmation action.
    {
        let results_panel = results_panel.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);

        entry.connect_activate(move |_entry| {
            if results_panel.is_visible() && !result_rows.is_empty() {
                confirm_mock_selection(&result_rows, selected_index.get());
            }
        });
    }

    // -------------------------------------------------------------------------
    // CSS
    // -------------------------------------------------------------------------

    eprintln!("[spotlight] build_window: install CSS");
    let theme_provider = install_css();

    // V0.1.2: typing still uses mock data, while the search chrome transitions
    // between empty/general-search and focused-provider states.
    {
        let results_panel = results_panel.clone();
        let results_scroll = results_scroll.clone();
        let result_rows = Rc::clone(&result_rows);
        let selected_index = Rc::clone(&selected_index);
        let active_filter = Rc::clone(&active_filter);
        let search_pill = search_pill.clone();
        let search_icon = search_icon.clone();
        let back_button = back_button.clone();
        let category_box = category_box.clone();
        let category_label = category_label.clone();
        let inline_filters = inline_filters.clone();
        let standalone_filter_buttons = standalone_filter_buttons.clone();

        entry.connect_changed(move |entry| {
            update_search_chrome(
                &search_pill,
                &search_icon,
                &back_button,
                &category_box,
                &category_label,
                &inline_filters,
                &standalone_filter_buttons,
                active_filter.get(),
                entry.text().as_str(),
                380,
            );

            update_mock_results(
                &results_panel,
                &results_scroll,
                result_rows.as_ref(),
                &selected_index,
                active_filter.get(),
                entry.text().as_str(),
            );
        });
    }

    eprintln!("[spotlight] build_window: complete");
    (window, entry, theme_provider)
}

// -----------------------------------------------------------------------------
// BUTTONS
// -----------------------------------------------------------------------------

fn make_letter_button(
    class_name: &str,
    letter: &str,
    tooltip: &str,
    size: i32,
) -> Button {
    let button = Button::new();

    button.set_width_request(size);
    button.set_height_request(size);

    button.set_halign(Align::Center);
    button.set_valign(Align::Center);

    button.set_focusable(false);
    button.set_focus_on_click(false);

    button.set_tooltip_text(Some(tooltip));

    button.add_css_class("action-button");
    button.add_css_class(class_name);

    let label = Label::new(Some(letter));

    label.add_css_class("action-letter");

    button.set_child(Some(&label));

    button
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

    button.set_focusable(false);
    button.set_focus_on_click(false);

    button.set_tooltip_text(Some(tooltip));

    button.add_css_class("action-button");
    button.add_css_class(class_name);

    let icon = Image::from_icon_name(icon_name);

    icon.set_pixel_size(icon_size);

    icon.add_css_class("action-icon");

    button.set_child(Some(&icon));

    button
}

// -----------------------------------------------------------------------------
// STATIC RESULT PREVIEW
// -----------------------------------------------------------------------------

fn make_mock_results(
    width: i32,
    viewport_height: i32,
) -> (Overlay, ScrolledWindow, Vec<MockResultRow>) {
    let results_panel = Overlay::new();

    results_panel.set_width_request(width);
    results_panel.set_height_request(viewport_height);
    results_panel.set_halign(Align::Center);
    results_panel.set_valign(Align::Start);
    results_panel.add_css_class("results-panel-shell");

    let results = GtkBox::new(Orientation::Vertical, 0);
    results.set_width_request(width);
    results.set_halign(Align::Fill);
    results.set_valign(Align::Start);
    results.add_css_class("results-list");

    // Enough rows to make the future selection-following viewport visible.
    // Real providers will replace these with actual search results later.
    const MOCK_RESULT_COUNT: usize = 18;
    let mut rows = Vec::with_capacity(MOCK_RESULT_COUNT);

    for _ in 0..MOCK_RESULT_COUNT {
        let row = make_mock_result_row();
        results.append(&row.button);
        rows.push(row);
    }

    let scroll = ScrolledWindow::new();
    scroll.set_width_request(width);
    scroll.set_height_request(viewport_height);
    scroll.set_halign(Align::Fill);
    scroll.set_valign(Align::Start);
    scroll.set_vexpand(false);
    scroll.set_policy(PolicyType::Never, PolicyType::Never);
    scroll.set_has_frame(false);
    scroll.add_css_class("results-panel");
    scroll.set_visible(false);
    scroll.set_child(Some(&results));

    results_panel.set_child(Some(&scroll));

    // The fixed edge fades make the result viewport visually blend back into
    // the compositor-blurred desktop rather than ending like a hard rectangle.
    let top_fade = GtkBox::new(Orientation::Vertical, 0);
    top_fade.set_height_request(34);
    top_fade.set_halign(Align::Fill);
    top_fade.set_valign(Align::Start);
    top_fade.set_can_target(false);
    top_fade.add_css_class("results-fade-top");
    results_panel.add_overlay(&top_fade);

    let bottom_fade = GtkBox::new(Orientation::Vertical, 0);
    bottom_fade.set_height_request(50);
    bottom_fade.set_halign(Align::Fill);
    bottom_fade.set_valign(Align::End);
    bottom_fade.set_can_target(false);
    bottom_fade.add_css_class("results-fade-bottom");
    results_panel.add_overlay(&bottom_fade);

    results_panel.set_visible(false);

    (results_panel, scroll, rows)
}

fn make_mock_result_row() -> MockResultRow {
    let button = Button::new();

    button.set_hexpand(true);
    button.set_halign(Align::Fill);
    button.set_valign(Align::Center);
    button.set_focusable(false);
    button.set_focus_on_click(false);
    button.add_css_class("result-button");
    button.set_opacity(0.16);

    let row = GtkBox::new(Orientation::Horizontal, 12);
    row.set_hexpand(true);
    row.set_halign(Align::Fill);
    row.set_valign(Align::Center);

    let icon = Image::from_icon_name("system-search-symbolic");
    icon.set_pixel_size(24);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_halign(Align::Fill);
    text.set_valign(Align::Center);

    let title = Label::new(Some("Example Result"));
    title.set_halign(Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("result-title");

    let subtitle = Label::new(Some("Static preview — real providers come later"));
    subtitle.set_halign(Align::Start);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.add_css_class("result-subtitle");

    text.append(&title);
    text.append(&subtitle);

    row.append(&icon);
    row.append(&text);
    button.set_child(Some(&row));

    MockResultRow {
        button,
        title,
        subtitle,
    }
}

fn update_mock_results(
    results_panel: &Overlay,
    results_scroll: &ScrolledWindow,
    rows: &[MockResultRow],
    selected_index: &Cell<usize>,
    filter: MockFilter,
    query: &str,
) {
    let query = query.trim();

    // General empty state stays clean. Choosing one of the four provider
    // buttons is itself a request to browse that provider, so filtered
    // browsing is visible even with an empty text query.
    if query.is_empty() && filter == MockFilter::All {
        results_panel.set_visible(false);
        results_scroll.set_visible(false);
        selected_index.set(0);
        clear_mock_selection(rows);
        results_scroll.vadjustment().set_value(0.0);
        return;
    }

    results_panel.set_visible(true);
    results_scroll.set_visible(true);
    selected_index.set(0);
    results_scroll.vadjustment().set_value(0.0);

    const MOCK_KINDS_ALL: [&str; 18] = [
        "Top result",
        "Open",
        "Search",
        "Recent item",
        "Application match",
        "File match",
        "Document match",
        "Folder match",
        "Clipboard match",
        "Action match",
        "Related result",
        "More like this",
        "Suggested result",
        "History match",
        "Indexed result",
        "Secondary match",
        "Additional result",
        "More results",
    ];

    let display_query = if query.is_empty() {
        filter.label()
    } else {
        query
    };

    for (index, row) in rows.iter().enumerate() {
        let title = match filter {
            MockFilter::All => format!("{display_query} — {}", MOCK_KINDS_ALL[index]),
            _ if index == 0 => format!("{display_query} — {}", filter.row_prefix()),
            _ => format!("{display_query} — {} {}", filter.row_prefix(), index + 1),
        };

        let subtitle = match filter {
            MockFilter::All => "Static preview — real providers come later",
            MockFilter::Applications => "Applications filter — provider placeholder",
            MockFilter::Files => "Files filter — provider placeholder",
            MockFilter::Actions => "Actions filter — provider placeholder",
            MockFilter::Clipboard => "Clipboard filter — provider placeholder",
        };

        row.title.set_text(&title);
        row.subtitle.set_text(subtitle);
    }

    set_mock_selection(results_scroll, rows, selected_index, 0);
}

fn make_back_button(tooltip: &str) -> Button {
    let button = Button::new();
    button.set_width_request(34);
    button.set_height_request(34);
    button.set_halign(Align::Center);
    button.set_valign(Align::Center);
    button.set_focusable(false);
    button.set_focus_on_click(false);
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
    button.set_width_request(28);
    button.set_height_request(28);
    button.set_halign(Align::Center);
    button.set_valign(Align::Center);
    button.set_focusable(false);
    button.set_focus_on_click(false);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("inline-filter-button");
    button.add_css_class(class_name);

    let label = Label::new(Some(letter));
    label.add_css_class("inline-filter-letter");
    button.set_child(Some(&label));

    button
}

fn make_inline_filter_icon_button(
    class_name: &str,
    icon_name: &str,
    tooltip: &str,
) -> Button {
    let button = Button::new();
    button.set_width_request(28);
    button.set_height_request(28);
    button.set_halign(Align::Center);
    button.set_valign(Align::Center);
    button.set_focusable(false);
    button.set_focus_on_click(false);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("inline-filter-button");
    button.add_css_class(class_name);

    let icon = Image::from_icon_name(icon_name);
    icon.set_pixel_size(15);
    icon.add_css_class("inline-filter-icon");
    button.set_child(Some(&icon));

    button
}

fn set_active_filter_buttons(buttons: &[Button; 4], filter: MockFilter) {
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
    filter: MockFilter,
    query: &str,
    base_width: i32,
) {
    let query_is_empty = query.trim().is_empty();
    let provider_mode = filter != MockFilter::All;
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
        search_pill.add_css_class("search-pill-filtered");
        search_pill.set_width_request(base_width + 90);
        category_label.set_text(filter.label());
    } else if typing_general {
        search_pill.add_css_class("search-pill-typing");
        search_pill.set_width_request((base_width + 160).min(620));
    } else {
        search_pill.set_width_request(base_width);
    }
}

fn clear_mock_selection(rows: &[MockResultRow]) {
    for row in rows {
        row.button.remove_css_class("selected");
        row.button.set_opacity(0.16);
    }
}

fn set_mock_selection(
    results_scroll: &ScrolledWindow,
    rows: &[MockResultRow],
    selected_index: &Cell<usize>,
    index: usize,
) {
    if rows.is_empty() {
        return;
    }

    let index = index.min(rows.len() - 1);
    selected_index.set(index);

    for (row_index, row) in rows.iter().enumerate() {
        let distance = row_index.abs_diff(index);
        let opacity = match distance {
            0 => 1.0,
            1 => 0.88,
            2 => 0.72,
            3 => 0.54,
            4 => 0.38,
            _ => 0.18,
        };

        row.button.set_opacity(opacity);

        if row_index == index {
            row.button.add_css_class("selected");
        } else {
            row.button.remove_css_class("selected");
        }
    }

    scroll_mock_selection(results_scroll, index);
}

fn scroll_mock_selection(results_scroll: &ScrolledWindow, index: usize) {
    const MOCK_ROW_HEIGHT: f64 = 58.0;
    // The selected result may travel freely through the first six visible rows.
    // Once it reaches row 6, the viewport follows it upward, pushing the old
    // rows out through the top fade.
    const SELECTION_TOP_ROW: f64 = 0.0;
    const SELECTION_BOTTOM_ROW: f64 = 5.0;

    let adjustment = results_scroll.vadjustment();
    let current = adjustment.value();
    let page_size = adjustment.page_size().max(MOCK_ROW_HEIGHT);
    let max_value = (adjustment.upper() - page_size).max(0.0);
    let current_first_row = (current / MOCK_ROW_HEIGHT).floor();

    let index_f64 = index as f64;

    let target_first_row = if index_f64 > current_first_row + SELECTION_BOTTOM_ROW {
        (index_f64 - SELECTION_BOTTOM_ROW).floor()
    } else if index_f64 < current_first_row + SELECTION_TOP_ROW {
        (index_f64 - SELECTION_TOP_ROW).max(0.0).floor()
    } else {
        current_first_row
    };

    let target = (target_first_row * MOCK_ROW_HEIGHT).clamp(0.0, max_value);
    adjustment.set_value(target);
}

fn confirm_mock_selection(rows: &[MockResultRow], index: usize) {
    let Some(row) = rows.get(index) else {
        return;
    };

    row.subtitle
        .set_text("Selected — real execution comes in a later phase");
}

// -----------------------------------------------------------------------------
// WINDOW STATE
// -----------------------------------------------------------------------------

fn show_window(state: &AppState) {
    eprintln!("[spotlight] show_window: entered");
    reload_generated_theme(&state.theme_provider);
    eprintln!("[spotlight] show_window: theme step complete");
    state.window.present();
    eprintln!("[spotlight] show_window: present() called; visible={}", state.window.is_visible());

    let entry = state.entry.clone();

    //
    // Wait until GTK has mapped/allocated the surface.
    // Then place keyboard focus directly into the search field.
    //
    let window = state.window.clone();
    glib::idle_add_local_once(move || {
        entry.grab_focus();
        entry.select_region(0, -1);
        eprintln!("[spotlight] focus-idle: ran; visible={} mapped={}", window.is_visible(), window.is_mapped());
    });

    let window = state.window.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
        eprintln!("[spotlight] 500ms-check: visible={} mapped={}", window.is_visible(), window.is_mapped());
    });
}

fn hide_window(window: &ApplicationWindow) {
    eprintln!("[spotlight] hide_window: entered");
    window.hide();
    eprintln!("[spotlight] hide_window: visible={}", window.is_visible());
}

fn toggle_window(state: &AppState) {
    if state.window.is_visible() {
        hide_window(&state.window);
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
    min-width: 28px;
    min-height: 28px;

    padding: 0;
    border-radius: 9px;

    background-color: rgba(255, 255, 255, 0.14);
    border: 1px solid rgba(255, 255, 255, 0.20);

    box-shadow: none;
}

.inline-filter-button:hover {
    background-color: rgba(255, 255, 255, 0.26);
}

.inline-filter-button.active-filter {
    background-color: rgba(255, 255, 255, 0.40);
    border-color: rgba(255, 255, 255, 0.44);
}

.inline-filter-icon {
    color: rgba(55, 59, 67, 0.76);
}

.inline-filter-letter {
    color: rgba(55, 59, 67, 0.76);
    font-size: 13px;
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

.result-button {
    min-width: 0;
    min-height: 58px;
    padding: 0 14px;
    border-radius: 0;

    background-color: transparent;
    border: none;
}

.result-button:hover {
    background-color: rgba(255, 255, 255, 0.08);
}

.result-button.selected {
    background-color: rgba(255, 255, 255, 0.18);
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
    background-color: rgba(255, 255, 255, 0.52);
    border-color: rgba(255, 255, 255, 0.56);
    box-shadow:
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

    box-shadow:
        0 7px 20px rgba(0, 0, 0, 0.08);
}

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
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
        })?;

    Some(
        config_home
            .join("hypr-mac")
            .join("matugen")
            .join("generated")
            .join("spotlight.css"),
    )
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

    if path.is_file() {
        eprintln!("[spotlight] theme: generated CSS found; loading");
        provider.load_from_path(&path);
        eprintln!("[spotlight] theme: load_from_path returned");
    } else {
        eprintln!("[spotlight] theme: generated CSS missing; fallback remains active");
    }
}

#[cfg(test)]
mod responsive_tests {
    use super::*;

    fn total_width(m: UiMetrics) -> i32 {
        m.search_width
            + (m.control_size * 4)
            + (m.gap * 4)
    }

    #[test]
    fn realistic_resolutions_fit() {
        let resolutions = [
            640,
            800,
            1024,
            1280,
            1366,
            1440,
            1920,
            2560,
            3440,
            3840,
            5120,
            7680,
            13440,
        ];

        for width in resolutions {
            let metrics = responsive_metrics(width);
            let total = total_width(metrics);

            println!(
                "width={width}: search={} control={} gap={} inner_gap={} icon={} total={}",
                metrics.search_width,
                metrics.control_size,
                metrics.gap,
                metrics.inner_gap,
                metrics.icon_size,
                total
            );

            assert!(metrics.search_width > 0);
            assert!(metrics.control_size > 0);
            assert!(metrics.gap > 0);
            assert!(metrics.inner_gap > 0);
            assert!(metrics.icon_size > 0);

            assert!(
                total <= width,
                "composition overflows width {width}px: total={total}px"
            );
        }
    }

    #[test]
    fn very_wide_displays_do_not_grow_forever() {
        let metrics = responsive_metrics(13440);

        assert!(metrics.search_width <= 900);
        assert!(metrics.control_size <= 58);
        assert!(metrics.gap <= 12);
        assert!(metrics.icon_size <= 22);
    }

    #[test]
    fn reference_width_is_stable() {
        let metrics = responsive_metrics(1366);

        assert_eq!(metrics.control_size, 58);
        assert_eq!(metrics.gap, 12);
        assert_eq!(metrics.inner_gap, 10);
        assert_eq!(metrics.icon_size, 22);
        assert_eq!(metrics.search_width, 380);
    }
}

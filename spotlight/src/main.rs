use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CenterBox, Entry, Image,
    Label, Orientation,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use std::cell::RefCell;
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
}

// -----------------------------------------------------------------------------
// MAIN
// -----------------------------------------------------------------------------

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

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
            let (window, entry) = build_window(app);

            window.hide();

            *state.borrow_mut() = Some(AppState {
                window,
                entry,
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
            let args = command_line.arguments();

            let command = args
                .get(1)
                .and_then(|arg| arg.to_str())
                .unwrap_or("");

            let binding = state.borrow();
            let Some(state) = binding.as_ref() else {
                return 0.into();
            };

            match command {
                "toggle" => toggle_window(state),

                "show" => show_window(state),

                "hide" => hide_window(&state.window),

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

    app.run()
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

fn build_window(app: &Application) -> (ApplicationWindow, Entry) {
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

    let root = CenterBox::new();

    root.set_hexpand(true);
    root.set_vexpand(true);

    root.add_css_class("spotlight-root");

    // -------------------------------------------------------------------------
    // RESPONSIVE UI METRICS
    // -------------------------------------------------------------------------

    let output_width = gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_then(|item| item.downcast::<gdk::Monitor>().ok())
        .map(|monitor| monitor.geometry().width())
        .unwrap_or(640);

    let metrics = responsive_metrics(output_width);

    // -------------------------------------------------------------------------
    // CENTERED CLUSTER
    // -------------------------------------------------------------------------

    let cluster = GtkBox::new(Orientation::Horizontal, metrics.gap);

    cluster.set_halign(Align::Center);
    cluster.set_valign(Align::Center);

    cluster.set_hexpand(false);
    cluster.set_vexpand(false);

    cluster.add_css_class("spotlight-cluster");

    // -------------------------------------------------------------------------
    // SEARCH PILL
    // -------------------------------------------------------------------------

    let search_pill = GtkBox::new(Orientation::Horizontal, metrics.inner_gap);

    search_pill.set_halign(Align::Center);
    search_pill.set_valign(Align::Center);

    search_pill.set_width_request(metrics.search_width);
    search_pill.set_height_request(metrics.control_size);

    search_pill.add_css_class("search-pill");

    // -------------------------------------------------------------------------
    // SEARCH ICON
    // -------------------------------------------------------------------------

    let search_icon =
        Image::from_icon_name("system-search-symbolic");

    search_icon.set_pixel_size(metrics.icon_size);

    search_icon.add_css_class("search-icon");

    search_pill.append(&search_icon);

    // -------------------------------------------------------------------------
    // SEARCH ENTRY
    // -------------------------------------------------------------------------

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

    cluster.append(&search_pill);

    // -------------------------------------------------------------------------
    // FOUR TAHOE CONTROLS
    // -------------------------------------------------------------------------

    let applications =
        make_letter_button("applications-button", "A", "Applications", metrics.control_size);

    let files =
        make_icon_button("files-button", "folder-symbolic", "Files", metrics.control_size, metrics.icon_size);

    let actions =
        make_icon_button("actions-button", "view-grid-symbolic", "Actions", metrics.control_size, metrics.icon_size);

    let clipboard =
        make_icon_button("clipboard-button", "edit-copy-symbolic", "Clipboard", metrics.control_size, metrics.icon_size);

    cluster.append(&applications);
    cluster.append(&files);
    cluster.append(&actions);
    cluster.append(&clipboard);

    // -------------------------------------------------------------------------
    // CENTER EVERYTHING
    // -------------------------------------------------------------------------

    root.set_center_widget(Some(&cluster));

    window.set_child(Some(&root));

    // -------------------------------------------------------------------------
    // KEYBOARD HANDLING
    // -------------------------------------------------------------------------

    let key_controller = gtk::EventControllerKey::new();

    {
        let window = window.clone();

        key_controller.connect_key_pressed(
            move |_controller, key, _keycode, _state| {
                if key == gdk::Key::Escape {
                    hide_window(&window);

                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            },
        );
    }

    window.add_controller(key_controller);

    // -------------------------------------------------------------------------
    // CSS
    // -------------------------------------------------------------------------

    install_css();

    (window, entry)
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
// WINDOW STATE
// -----------------------------------------------------------------------------

fn show_window(state: &AppState) {
    state.window.present();

    let entry = state.entry.clone();

    //
    // Wait until GTK has mapped/allocated the surface.
    // Then place keyboard focus directly into the search field.
    //
    glib::idle_add_local_once(move || {
        entry.grab_focus();
        entry.select_region(0, -1);
    });
}

fn hide_window(window: &ApplicationWindow) {
    window.hide();
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

fn install_css() {
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
 * GTK CenterBox + Center alignment keeps this centered regardless of
 * output resolution.
 */

.spotlight-cluster {
    background: transparent;
    background-color: rgba(0, 0, 0, 0);
}


/* ============================================================================
 * SEARCH PILL
 * ============================================================================
 *
 * Tahoe-inspired:
 *
 *     light glass
 *     very soft border
 *     large radius
 *     restrained shadow
 */

.search-pill {
    min-width: 0;
    min-height: 0;

    padding-left: 17px;
    padding-right: 18px;

    border-radius: 999px;

    background-color: rgba(247, 249, 252, 0.84);

    border: 1px solid rgba(255, 255, 255, 0.78);

    box-shadow:
        0 8px 24px rgba(0, 0, 0, 0.10);
}


/* ============================================================================
 * SEARCH ICON
 * ========================================================================= */

.search-icon {
    color: rgba(55, 59, 67, 0.78);
    opacity: 1;
}


/* ============================================================================
 * SEARCH ENTRY
 * ============================================================================
 *
 * Remove GTK's normal frame/focus treatment.
 * The pill itself is the visual container.
 */

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
 * ACTION BUTTONS
 * ============================================================================
 *
 * The four controls are intentionally secondary to the search field.
 */

.action-button {
    min-width: 0;
    min-height: 0;

    padding: 0;

    border-radius: 999px;

    background-color: rgba(247, 249, 250, 0.46);

    border: 1px solid rgba(255, 255, 255, 0.52);

    box-shadow:
        0 7px 20px rgba(0, 0, 0, 0.08);
}

.action-button:hover {
    background-color: rgba(252, 253, 255, 0.82);
}

.action-button:active {
    background-color: rgba(232, 235, 240, 0.88);
}


/* ============================================================================
 * ACTION ICONS
 * ========================================================================= */

.action-icon {
	color: rgba(55, 59, 67, 0.78);
    opacity: 1;
}


/* ============================================================================
 * APPLICATIONS "A"
 * ========================================================================= */

.action-letter {
    color: rgba(55, 59, 67, 0.78);

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

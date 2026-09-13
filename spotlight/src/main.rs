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
    window.set_exclusive_zone(0);

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
    // CENTERED CLUSTER
    // -------------------------------------------------------------------------

    let cluster = GtkBox::new(Orientation::Horizontal, 12);

    cluster.set_halign(Align::Center);
    cluster.set_valign(Align::Center);

    cluster.set_hexpand(false);
    cluster.set_vexpand(false);

    cluster.add_css_class("spotlight-cluster");

    // -------------------------------------------------------------------------
    // SEARCH PILL
    // -------------------------------------------------------------------------

    let search_pill = GtkBox::new(Orientation::Horizontal, 10);

    search_pill.set_halign(Align::Center);
    search_pill.set_valign(Align::Center);

    search_pill.set_width_request(380);
    search_pill.set_height_request(58);

    search_pill.add_css_class("search-pill");

    // -------------------------------------------------------------------------
    // SEARCH ICON
    // -------------------------------------------------------------------------

    let search_icon =
        Image::from_icon_name("system-search-symbolic");

    search_icon.set_pixel_size(22);

    search_icon.add_css_class("search-icon");

    search_pill.append(&search_icon);

    // -------------------------------------------------------------------------
    // SEARCH ENTRY
    // -------------------------------------------------------------------------

    let entry = Entry::new();

    entry.set_placeholder_text(Some("Spotlight Search"));

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
        make_letter_button("applications-button", "A", "Applications");

    let files =
        make_icon_button("files-button", "folder-symbolic", "Files");

    let actions =
        make_icon_button("actions-button", "view-grid-symbolic", "Actions");

    let clipboard =
        make_icon_button("clipboard-button", "edit-copy-symbolic", "Clipboard");

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
) -> Button {
    let button = Button::new();

    button.set_width_request(58);
    button.set_height_request(58);

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
) -> Button {
    let button = Button::new();

    button.set_width_request(58);
    button.set_height_request(58);

    button.set_halign(Align::Center);
    button.set_valign(Align::Center);

    button.set_focusable(false);
    button.set_focus_on_click(false);

    button.set_tooltip_text(Some(tooltip));

    button.add_css_class("action-button");
    button.add_css_class(class_name);

    let icon = Image::from_icon_name(icon_name);

    icon.set_pixel_size(22);

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
    min-width: 380px;
    min-height: 58px;

    padding-left: 17px;
    padding-right: 18px;

    border-radius: 29px;

    background-color: rgba(247, 249, 252, 0.84);

    border: 1px solid rgba(255, 255, 255, 0.78);

    box-shadow:
        0 8px 24px rgba(0, 0, 0, 0.10);
}


/* ============================================================================
 * SEARCH ICON
 * ========================================================================= */

.search-icon {
    opacity: 0.60;
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
    min-width: 58px;
    min-height: 58px;

    padding: 0;

    border-radius: 29px;

    background-color: rgba(247, 249, 252, 0.72);

    border: 1px solid rgba(255, 255, 255, 0.70);

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
    opacity: 0.68;
}


/* ============================================================================
 * APPLICATIONS "A"
 * ========================================================================= */

.action-letter {
    color: rgba(45, 51, 60, 0.72);

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

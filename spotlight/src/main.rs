use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Entry, Image,
    Orientation,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "com.ufoton.Spotlight";
const LAYER_NAMESPACE: &str = "spotlight";

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // The window is created once and reused for the lifetime of the process.
    // This is important for instant invocation from Hyprland.
    let window: Rc<RefCell<Option<ApplicationWindow>>> =
        Rc::new(RefCell::new(None));

    {
        let window = Rc::clone(&window);

        app.connect_activate(move |app| {
            let win = build_window(app);

            // Keep the layer surface alive, but do not show it yet.
            win.hide();

            *window.borrow_mut() = Some(win);
        });
    }

    {
        let window = Rc::clone(&window);

        app.connect_command_line(move |app, command_line| {
            // Make sure the primary instance has created its window.
            if window.borrow().is_none() {
                app.activate();
            }

            let args = command_line.arguments();

            // argv[0] is the executable name.
            let command = args
                .get(1)
                .and_then(|arg| arg.to_str())
                .unwrap_or("");

            match command {
                "toggle" => {
                    if let Some(win) = window.borrow().as_ref() {
                        toggle_window(win);
                    }
                }

                "show" => {
                    if let Some(win) = window.borrow().as_ref() {
                        show_window(win);
                    }
                }

                "hide" => {
                    if let Some(win) = window.borrow().as_ref() {
                        hide_window(win);
                    }
                }

                _ => {
                    // No command:
                    //
                    // This starts the persistent process and leaves
                    // Spotlight hidden.
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

fn build_window(app: &Application) -> ApplicationWindow {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spotlight")
        .decorated(false)
        .resizable(false)
        .build();

    // -------------------------------------------------------------------------
    // WAYLAND LAYER-SHELL
    // -------------------------------------------------------------------------

    // Must happen before the window is realized.
    window.init_layer_shell();

    // Determine the runtime output and size.
    configure_layer_surface(&window);

    // Put Spotlight above normal application windows.
    window.set_layer(Layer::Overlay);

    // Stable namespace for Hyprland layer rules.
    window.set_namespace(Some(LAYER_NAMESPACE));

    // Spotlight needs keyboard input while visible.
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    // Stretch the layer across the selected output.
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    // This is an overlay, not a panel.
    // Never reserve screen space.
    window.set_exclusive_zone(0);

    // -------------------------------------------------------------------------
    // ROOT
    // -------------------------------------------------------------------------

    // The root fills the layer surface.
    // CenterBox places the Spotlight composition in the middle.
    let root = gtk::CenterBox::new();

    root.set_hexpand(true);
    root.set_vexpand(true);

    root.add_css_class("spotlight-root");

    // -------------------------------------------------------------------------
    // CENTERED CLUSTER
    // -------------------------------------------------------------------------

    let cluster = GtkBox::new(Orientation::Horizontal, 14);

    cluster.set_halign(Align::Center);
    cluster.set_valign(Align::Center);

    cluster.set_hexpand(false);
    cluster.set_vexpand(false);

    cluster.add_css_class("spotlight-cluster");

    // -------------------------------------------------------------------------
    // SEARCH PILL
    // -------------------------------------------------------------------------

    let search_pill = GtkBox::new(Orientation::Horizontal, 12);

    search_pill.set_halign(Align::Center);
    search_pill.set_valign(Align::Center);

    search_pill.set_height_request(58);
    search_pill.set_width_request(400);

    search_pill.add_css_class("search-pill");

    // Search icon.
    let search_icon = Image::from_icon_name("system-search-symbolic");

    search_icon.set_pixel_size(23);
    search_icon.add_css_class("search-icon");

    search_pill.append(&search_icon);

    // Actual editable search field.
    let entry = Entry::new();

    entry.set_placeholder_text(Some("Spotlight Search"));

    entry.set_hexpand(true);
    entry.set_halign(Align::Fill);
    entry.set_valign(Align::Center);

    entry.set_has_frame(false);

    entry.add_css_class("search-entry");

    search_pill.append(&entry);

    cluster.append(&search_pill);

    // -------------------------------------------------------------------------
    // FOUR TAHOE CONTROLS
    // -------------------------------------------------------------------------

    let applications = make_action_button(
        "applications-button",
        "system-software-install-symbolic",
        "Applications",
    );

    let files = make_action_button(
        "files-button",
        "folder-symbolic",
        "Files",
    );

    let actions = make_action_button(
        "actions-button",
        "system-run-symbolic",
        "Actions",
    );

    let clipboard = make_action_button(
        "clipboard-button",
        "edit-paste-symbolic",
        "Clipboard",
    );

    cluster.append(&applications);
    cluster.append(&files);
    cluster.append(&actions);
    cluster.append(&clipboard);

    // -------------------------------------------------------------------------
    // CENTER THE CLUSTER
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

    window
}

// -----------------------------------------------------------------------------
// LAYER-SHELL GEOMETRY
// -----------------------------------------------------------------------------

fn configure_layer_surface(window: &ApplicationWindow) {
    let Some(display) = gdk::Display::default() else {
        eprintln!("Spotlight: could not get default GDK display");
        return;
    };

    let monitors = display.monitors();

    let Some(object) = monitors.item(0) else {
        eprintln!("Spotlight: no monitor was found");
        return;
    };

    let Ok(monitor) = object.downcast::<gdk::Monitor>() else {
        eprintln!("Spotlight: GDK object was not a monitor");
        return;
    };

    let geometry = monitor.geometry();

    // Associate the layer-shell surface with this runtime-detected output.
    window.set_monitor(Some(&monitor));

    // IMPORTANT:
    //
    // This is runtime geometry. Nothing about the user's actual resolution
    // is stored in the project.
    //
    // The values are supplied by the current graphical session.
    window.set_default_size(
        geometry.width(),
        geometry.height(),
    );

    eprintln!(
        "Spotlight target output geometry: {} x {}",
        geometry.width(),
        geometry.height()
    );
}

// -----------------------------------------------------------------------------
// BUTTONS
// -----------------------------------------------------------------------------

fn make_action_button(
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

    button.set_tooltip_text(Some(tooltip));

    button.add_css_class("action-button");
    button.add_css_class(class_name);

    let icon = Image::from_icon_name(icon_name);

    icon.set_pixel_size(22);

    button.set_child(Some(&icon));

    button
}

// -----------------------------------------------------------------------------
// WINDOW STATE
// -----------------------------------------------------------------------------

fn show_window(window: &ApplicationWindow) {
    window.present();

    // Wait until GTK has performed a layout/allocation pass.
    //
    // This diagnostic is intentionally temporary. It lets us verify whether
    // the layer surface is now receiving the full runtime output geometry.
    let window = window.clone();

    glib::idle_add_local_once(move || {
        eprintln!(
            "Spotlight allocated size: {} x {}",
            window.width(),
            window.height()
        );

        if let Some(root) = window.child() {
            eprintln!(
                "Root allocated size: {} x {}",
                root.width(),
                root.height()
            );

            if let Some(cluster) = root.first_child() {
                eprintln!(
                    "Cluster allocated size: {} x {}",
                    cluster.width(),
                    cluster.height()
                );

                if let Some(search_pill) = cluster.first_child() {
                    if let Some(entry) = search_pill
                        .last_child()
                        .and_downcast::<Entry>()
                    {
                        entry.grab_focus();
                        entry.select_region(0, -1);
                    }
                }
            }
        }
    });
}

fn hide_window(window: &ApplicationWindow) {
    window.hide();
}

fn toggle_window(window: &ApplicationWindow) {
    if window.is_visible() {
        hide_window(window);
    } else {
        show_window(window);
    }
}

// -----------------------------------------------------------------------------
// CSS
// -----------------------------------------------------------------------------

fn install_css() {
    let provider = gtk::CssProvider::new();

    provider.load_from_data(
        r#"
        /*
         * ROOT
         *
         * The entire layer surface is transparent.
         * Hyprland is responsible for environmental blur.
         */

         window {
         	background-color: transparent;
         }

        .spotlight-root {
            background-color: transparent;
        }


        /*
         * CENTERED COMPOSITION
         */

        .spotlight-cluster {
            background-color: transparent;
        }


        /*
         * SEARCH PILL
         */

        .search-pill {
            min-width: 400px;
            min-height: 58px;

            padding-left: 18px;
            padding-right: 20px;

            border-radius: 29px;

            background-color: rgba(248, 250, 252, 0.82);

            border: 1px solid rgba(255, 255, 255, 0.72);

            box-shadow:
                0 10px 30px rgba(0, 0, 0, 0.10);
        }


        /*
         * SEARCH ICON
         */

        .search-icon {
            opacity: 0.72;
        }


        /*
         * ENTRY
         *
         * GTK's Entry is used instead of a custom text widget.
         * This gives us IME support, cursor behavior, selection,
         * keyboard handling, accessibility, etc.
         */

        .search-entry {
            min-height: 42px;

            padding: 0;

            border: none;
            outline: none;

            background: transparent;

            color: rgba(25, 30, 38, 0.92);

            font-size: 20px;
            font-weight: 400;
        }

        .search-entry placeholder {
            color: rgba(45, 52, 62, 0.68);
        }


        /*
         * CIRCULAR CONTROLS
         */

        .action-button {
            min-width: 58px;
            min-height: 58px;

            padding: 0;

            border-radius: 29px;

            background-color: rgba(248, 250, 252, 0.74);

            border: 1px solid rgba(255, 255, 255, 0.68);

            box-shadow:
                0 8px 24px rgba(0, 0, 0, 0.08);
        }


        .action-button:hover {
            background-color: rgba(255, 255, 255, 0.88);
        }


        .action-button:active {
            background-color: rgba(235, 238, 242, 0.90);
        }


        .action-button image {
            opacity: 0.76;
        }


        /*
         * Remove GTK's default button focus decoration from the
         * four browse controls.
         */

        .action-button:focus {
            outline: none;

            box-shadow:
                0 8px 24px rgba(0, 0, 0, 0.08);
        }
        "#,
    );

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

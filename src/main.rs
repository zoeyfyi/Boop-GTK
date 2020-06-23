extern crate gdk;
extern crate gio;
extern crate glib;
extern crate gtk;
extern crate sourceview;

use gio::prelude::*;
use gtk::prelude::*;
use sourceview::prelude::*;

use gtk::{Application, ApplicationWindow, Dialog, Label};

fn create_window(app: &Application) -> ApplicationWindow {
    let window = ApplicationWindow::new(app);
    window.set_can_focus(true);
    window.set_title("Boop");
    window.set_default_size(600, 400);

    let scroll = gtk::Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 10.0);
    let scrolled_window = gtk::ScrolledWindow::new(gtk::NONE_ADJUSTMENT, Some(&scroll));
    window.add(&scrolled_window);

    let source_view: sourceview::View = sourceview::View::new();
    source_view.set_show_line_numbers(true);
    scrolled_window.add(&source_view);

    window.show_all();

    return window;
}

fn create_command_pallete_dialog(window: &ApplicationWindow) -> Dialog {
    let dialog = Dialog::new();
    dialog.set_default_size(300, 300);
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    dialog.set_property_window_position(gtk::WindowPosition::CenterOnParent);
    dialog.set_transient_for(Some(window));

    let searchbar = gtk::Entry::new();
    searchbar.set_hexpand(true);
    let header = gtk::HeaderBar::new();
    header.set_custom_title(Some(&searchbar));
    dialog.set_titlebar(Some(&header));

    let dialog_box = dialog.get_content_area();
    dialog_box.add(&Label::new(Some("Command Pallete")));

    return dialog;
}

fn main() {
    let application = Application::new(Some("uk.co.mrbenshef.boop-gtk"), Default::default())
        .expect("failed to initialize GTK application");

    application.connect_activate(|app| {
        let menu = gio::Menu::new();
        menu.append(Some("Command Pallete..."), Some("app.command_pallete"));
        app.set_app_menu(Some(&menu));

        let window = create_window(app);

        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
        command_pallete_action.connect_activate(move |_, _| {
            let dialog = create_command_pallete_dialog(&window);
            dialog.show_all();
            dialog.run();
            dialog.destroy();
        });
        app.add_action(&command_pallete_action);
        app.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
    });

    application.run(&[]);
}

extern crate gdk;
extern crate gio;
extern crate glib;
extern crate gtk;
extern crate sourceview;

extern crate libc;
extern crate rust_embed;
extern crate rusty_v8;

extern crate serde;

mod executor;
use executor::Executor;
mod script;
use script::Script;

use rusty_v8 as v8;

use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Dialog, Label};
use sourceview::prelude::*;

use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "scripts"]
struct Scripts;

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

fn create_command_pallete_dialog(window: &ApplicationWindow, scripts: &Vec<Script>) -> Dialog {
    let dialog = Dialog::new();
    dialog.set_default_size(300, 0);
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
    for script in scripts {
        dialog_box.add(&Label::new(Some(&script.metadata().name)));
    }

    return dialog;
}
use std::{process, ptr};

fn main() -> Result<(), ()> {
    // initalize V8
    let platform = v8::new_default_platform().unwrap();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    let mut scripts: Vec<Script> = Vec::with_capacity(Scripts::iter().count());

    for file in Scripts::iter() {
        let file: Cow<'_, str> = file;
        let source: Cow<'static, [u8]> = Scripts::get(&file).unwrap();
        let script_source = String::from_utf8(source.to_vec()).unwrap();

        match Script::from_source(script_source) {
            Ok(script) => scripts.push(script),
            Err(e) => println!("failed to parse script {}: {}", file, e),
        };
    }

    let application = Application::new(Some("uk.co.mrbenshef.boop-gtk"), Default::default())
        .expect("failed to initialize GTK application");

    application.connect_activate(move |app| {
        let menu = gio::Menu::new();
        menu.append(Some("Command Pallete..."), Some("app.command_pallete"));
        app.set_app_menu(Some(&menu));

        let window = create_window(app);

        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
        let scripts = scripts.clone();
        command_pallete_action.connect_activate(move |_, _| {
            let dialog = create_command_pallete_dialog(&window, &scripts);
            dialog.show_all();
            dialog.run();
            dialog.destroy();
        });
        app.add_action(&command_pallete_action);
        app.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
    });

    application.run(&[]);

    Ok(())
}

#[macro_use]
extern crate lazy_static;

extern crate gdk;
extern crate gio;
extern crate glib;
extern crate gtk;
extern crate pango;
extern crate sourceview;

extern crate libc;
extern crate rust_embed;
extern crate rusty_v8;

extern crate serde;

mod executor;
use executor::Executor;
mod script;
use script::Script;
mod command_pallete;
use command_pallete::CommandPalleteDialog;

use rusty_v8 as v8;

use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, Statusbar};

use rust_embed::RustEmbed;
use std::borrow::Cow;

use sublime_fuzzy::ScoreConfig;

const SEARCH_CONFIG: ScoreConfig = ScoreConfig {
    bonus_consecutive: 12,
    bonus_word_start: 0,
    bonus_coverage: 64,
    penalty_distance: 4,
};

const HEADER_BUTTON_GET_STARTED: &str = "Press Ctrl+Shift+P to get started";
const HEADER_BUTTON_CHOOSE_ACTION: &str = "Select an action";

#[derive(RustEmbed)]
#[folder = "scripts"]
struct Scripts;

#[derive(Clone)]
struct App {
    window: ApplicationWindow,
    header_button: Button,
    source_view: sourceview::View,
    status_bar: Statusbar,
}

fn open_command_pallete(app: &App, scripts: &Vec<Script>, context_id: u32) {
    let scripts = scripts
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, s)| (i as u64, s))
        .collect::<Vec<(u64, Script)>>();
    let dialog = CommandPalleteDialog::new(&app.window, scripts.clone());
    dialog.show_all();

    &app.header_button.set_label(HEADER_BUTTON_CHOOSE_ACTION);

    if let gtk::ResponseType::Other(script_id) = dialog.run() {
        let script = scripts[script_id as usize].1.clone();

        println!("executing {}", script.metadata().name);

        app.status_bar.remove_all(context_id);

        let buffer = &app.source_view.get_buffer().unwrap();

        let result = Executor::execute(
            script.source(),
            &buffer
                .get_text(&buffer.get_start_iter(), &buffer.get_end_iter(), false)
                .unwrap()
                .to_string(),
        );

        buffer.set_text(&result.text);

        // TODO: how to handle multiple messages?
        if let Some(error) =  result.error {
            app.status_bar.push(context_id, &error);
        } else if let Some(info) = result.info {
            app.status_bar.push(context_id, &info);
        }
    }

    &app.header_button.set_label(HEADER_BUTTON_GET_STARTED);

    dialog.destroy();
}

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

    application.connect_activate(move |application| {
        let app_glade = include_str!("../ui/boop-gtk.glade");
        let builder = gtk::Builder::new_from_string(app_glade);
        builder.set_application(application);

        let app = App {
            window: builder.get_object("window").unwrap(),
            header_button: builder.get_object("header_button").unwrap(),
            source_view: builder.get_object("source_view").unwrap(),
            status_bar: builder.get_object("status_bar").unwrap(),
        };

        app.window.set_application(Some(application));
        app.header_button.set_label(HEADER_BUTTON_GET_STARTED);
        app.window.show_all();

        let context_id = app.status_bar.get_context_id("script execution");

        {
            let app_ = app.clone();
            let scripts = scripts.clone();
            app.header_button
                .connect_clicked(move |_| open_command_pallete(&app_, &scripts, context_id));
        }

        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);

        {
            let app = app.clone();
            let scripts = scripts.clone();
            command_pallete_action
                .connect_activate(move |_, _| open_command_pallete(&app, &scripts, context_id));
        }

        application.add_action(&command_pallete_action);
        application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
    });

    application.run(&[]);

    Ok(())
}

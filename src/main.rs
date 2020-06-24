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
mod command_pallete;
use command_pallete::CommandPalleteDialog;

use rusty_v8 as v8;

use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button};
use sourceview::prelude::*;

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
}

fn create_window(app: &Application, scripts: &Vec<Script>) -> App {
    let window = ApplicationWindow::new(app);
    window.set_can_focus(true);
    window.set_title("Boop");
    window.set_default_size(600, 400);

    let header = gtk::HeaderBar::new();
    let header_button = Button::new_with_label(HEADER_BUTTON_GET_STARTED);

    header.set_custom_title(Some(&header_button));
    // header.add(&gtk::Button::new_from_icon_name(
    //     Some("window-close-symbolic"),
    //     gtk::IconSize::Menu,
    // ));
    header.set_show_close_button(true);
    window.set_titlebar(Some(&header));

    let scroll = gtk::Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 10.0);
    let scrolled_window = gtk::ScrolledWindow::new(gtk::NONE_ADJUSTMENT, Some(&scroll));
    window.add(&scrolled_window);

    let source_view: sourceview::View = sourceview::View::new();
    source_view.set_show_line_numbers(true);
    scrolled_window.add(&source_view);

    let app = App {
        window,
        header_button,
        source_view,
    };

    {
        let app_ = app.clone();
        let scripts = scripts.clone();
        app.header_button
            .connect_clicked(move |_| open_command_pallete(&app_, &scripts));
    }

    app.window.show_all();

    return app;
}

fn open_command_pallete(app: &App, scripts: &Vec<Script>) {
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
        println!(
            "executing {}",
            scripts[script_id as usize].1.metadata().name
        );

        let buffer = &app.source_view.get_buffer().unwrap();

        let result = Executor::execute(
            scripts[script_id as usize].1.source(),
            &buffer
                .get_text(&buffer.get_start_iter(), &buffer.get_end_iter(), false)
                .unwrap()
                .to_string(),
        );

        buffer.set_text(&result);
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
        // let menu = gio::Menu::new();
        // menu.append(Some("Command Pallete..."), Some("app.command_pallete"));

        let app = create_window(application, &scripts);

        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);

        {
            let scripts = scripts.clone();
            command_pallete_action
                .connect_activate(move |_, _| open_command_pallete(&app, &scripts));
        }

        application.add_action(&command_pallete_action);
        application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
    });

    application.run(&[]);

    Ok(())
}

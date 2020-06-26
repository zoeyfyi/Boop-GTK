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

extern crate directories;

#[macro_use]
extern crate log;

mod executor;
use executor::Executor;
mod script;
use script::{ParseScriptError, Script};
mod command_pallete;
use command_pallete::CommandPalleteDialog;

use rusty_v8 as v8;

use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, Statusbar};
use sourceview::prelude::*;

use rust_embed::RustEmbed;
use std::{
    borrow::Cow,
    fmt,
    path::{Path, PathBuf},
};

use sublime_fuzzy::ScoreConfig;

use directories::{ProjectDirs};
use std::io::prelude::*;

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

fn open_command_pallete(app: &App, scripts: &[Script], context_id: u32) {
    let scripts = scripts
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, s)| (i as u64, s))
        .collect::<Vec<(u64, Script)>>();
    let dialog = CommandPalleteDialog::new(&app.window, scripts.to_owned());
    dialog.show_all();

    app.header_button.set_label(HEADER_BUTTON_CHOOSE_ACTION);

    if let gtk::ResponseType::Other(script_id) = dialog.run() {
        let script = scripts[script_id as usize].1.clone();

        info!("executing {}", script.metadata().name);

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
        if let Some(error) = result.error {
            app.status_bar.push(context_id, &error);
        } else if let Some(info) = result.info {
            app.status_bar.push(context_id, &info);
        }
    }

    app.header_button.set_label(HEADER_BUTTON_GET_STARTED);

    dialog.destroy();
}

enum LoadScriptError {
    FailedToCreateScriptDirectory,
    FailedToReadScriptDirectory,
}

impl fmt::Display for LoadScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadScriptError::FailedToCreateScriptDirectory => {
                write!(f, "Can't create scripts directory, check your permissions")
            }
            LoadScriptError::FailedToReadScriptDirectory => {
                write!(f, "Can't read scripts directory, check your premissions")
            }
        }
    }
}

fn load_user_scripts(
    config_dir: &Path,
) -> Result<Vec<Result<Script, ParseScriptError>>, LoadScriptError> {
    let scripts_dir: PathBuf = config_dir.join("scripts");

    std::fs::create_dir_all(&scripts_dir)
        .map_err(|_| LoadScriptError::FailedToCreateScriptDirectory)?;

    let paths = std::fs::read_dir(&scripts_dir)
        .map_err(|_| LoadScriptError::FailedToReadScriptDirectory)?;

    Ok(paths
        .filter_map(|f| f.ok())
        .map(|f| f.path())
        .filter(|path| path.is_file())
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(Script::from_source)
        .collect())
}

fn load_scripts() -> Vec<Script> {
    let mut scripts: Vec<Script> = Vec::with_capacity(Scripts::iter().count());

    for file in Scripts::iter() {
        let file: Cow<'_, str> = file;
        let source: Cow<'static, [u8]> = Scripts::get(&file).unwrap();
        let script_source = String::from_utf8(source.to_vec()).unwrap();

        match Script::from_source(script_source) {
            Ok(script) => scripts.push(script),
            Err(e) => error!("failed to parse script \"{}\", {}", file, e),
        };
    }

    scripts
}

fn main() -> Result<(), ()> {
    env_logger::init();

    let config_dir = ProjectDirs::from("uk.co", "mrbenshef", "boop-gtk")
        .expect("Unable to find a configuration location for your platform")
        .config_dir()
        .to_path_buf();

    info!("configuration directory at: {}", config_dir.display());

    let lang_file_path = {
        let mut path = config_dir.clone();
        path.push("boop.lang");
        path
    };

    if !lang_file_path.exists() {
        info!(
            "language file does not exist, creating a new one at: {}",
            lang_file_path.display()
        );
        let mut file =
            std::fs::File::create(&lang_file_path).expect("Could not create language file");
        file.write_all(include_bytes!("../boop.lang"))
            .expect("Failed to write language file");
        info!("language file created at: {}", lang_file_path.display());
    }

    // initalize V8
    let platform = v8::new_default_platform().unwrap();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    info!("V8 initialized");

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

        // set syntax highlighting
        {
            let language_manager = sourceview::LanguageManager::get_default().unwrap();
            
            // add config_dir to language manager's search path
            let dirs = language_manager.get_search_path();
            let mut dirs: Vec<&str> = dirs.iter().map(|s| s.as_ref()).collect();
            let config_dir_str = config_dir.to_string_lossy().to_string();
            dirs.push(&config_dir_str);
            info!("Language manager search directorys: {}", dirs.join(":"));
            language_manager.set_search_path(&dirs);

            let boop_language = language_manager.get_language("boop");
            if boop_language.is_none() {
                app.status_bar.push(context_id, "ERROR: failed to load language file");
            }

            println!("language: {:?}", boop_language.clone().unwrap().get_style_ids());

            // set language
            let buffer: sourceview::Buffer = app
                .source_view
                .get_buffer()
                .unwrap()
                .downcast::<sourceview::Buffer>()
                .unwrap();
            buffer.set_highlight_syntax(true);
            buffer.set_language(boop_language.as_ref());
        }
        
        let mut scripts = load_scripts();

        match load_user_scripts(&config_dir) {
            Ok(user_scripts) => {
                for script in user_scripts {
                    match script {
                        Ok(script) => scripts.push(script),
                        Err(e) => {
                            error!("failed to parse script: {}", e);
                            app.status_bar.push(context_id, &format!("ERROR: {}", e));
                        }
                    };
                }
            }
            Err(e) => {
                error!("failed to load scripts: {}", e);
                app.status_bar.push(context_id, &format!("ERROR: {}", e));
            }
        }

        // register button to open command pallete
        {
            let app_ = app.clone();
            let scripts = scripts.clone();
            app.header_button
                .connect_clicked(move |_| open_command_pallete(&app_, &scripts, context_id));
        }

        // add keyboard shortcut for opening command pallete
        {
            let command_pallete_action = gio::SimpleAction::new("command_pallete", None);

            command_pallete_action
                .connect_activate(move |_, _| open_command_pallete(&app, &scripts, context_id));

            application.add_action(&command_pallete_action);
            application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
        }
    });

    application.run(&[]);

    Ok(())
}

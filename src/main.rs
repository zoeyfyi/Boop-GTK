#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate shrinkwraprs;

extern crate gdk;
extern crate gio;
extern crate glib;
extern crate gtk;
extern crate gdk_pixbuf;
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
mod script;
use script::{ParseScriptError, Script};
mod app;
mod command_pallete;

use rusty_v8 as v8;

use gio::prelude::*;
use gtk::prelude::*;
use gtk::Application;

use rust_embed::RustEmbed;
use std::{
    borrow::Cow,
    fmt,
    path::{Path, PathBuf},
};

use sublime_fuzzy::ScoreConfig;

use app::App;
use directories::ProjectDirs;
use fmt::Display;
use std::{error::Error, io::prelude::*, rc::Rc};

const SEARCH_CONFIG: ScoreConfig = ScoreConfig {
    bonus_consecutive: 12,
    bonus_word_start: 0,
    bonus_coverage: 64,
    penalty_distance: 4,
};

#[derive(RustEmbed)]
#[folder = "submodules/Boop/Boop/Boop/scripts/"]
struct Scripts;

#[derive(Debug)]
enum LoadScriptError {
    FailedToCreateScriptDirectory,
    FailedToReadScriptDirectory,
}

impl Display for LoadScriptError {
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

#[derive(Debug)]
enum ScriptError {
    LoadError(LoadScriptError),
    ParseError(ParseScriptError),
}

impl Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::LoadError(e) => write!(f, "{}", e),
            ScriptError::ParseError(e) => write!(f, "{}", e),
        }
    }
}

impl Error for ScriptError {}

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

fn load_internal_scripts() -> Vec<Script> {
    let mut scripts: Vec<Script> = Vec::with_capacity(Scripts::iter().count());

    // scripts are internal, so we can unwrap "safely"
    for file in Scripts::iter() {
        let file: Cow<'_, str> = file;
        let source: Cow<'static, [u8]> = Scripts::get(&file).unwrap();
        let script_source = String::from_utf8(source.to_vec()).unwrap();
        scripts.push(Script::from_source(script_source).unwrap());
    }

    scripts
}

fn load_all_scripts(config_dir: &Path) -> (Vec<Script>, Option<ScriptError>) {
    let mut scripts = load_internal_scripts();

    match load_user_scripts(&config_dir) {
        Ok(user_scripts) => {
            for script in user_scripts {
                match script {
                    Ok(script) => scripts.push(script),
                    Err(e) => return (scripts, Some(ScriptError::ParseError(e))),
                };
            }
        }
        Err(e) => return (scripts, Some(ScriptError::LoadError(e))),
    }

    (scripts, None)
}

fn main() -> Result<(), ()> {
    env_logger::init();

    let config_dir = ProjectDirs::from("uk.co", "mrbenshef", "boop-gtk")
        .expect("Unable to find a configuration location for your platform")
        .config_dir()
        .to_path_buf();

    if !config_dir.exists() {
        info!("config directory does not exist, attempting to create it");
        match std::fs::create_dir_all(&config_dir) {
            Ok(()) => info!("created config directory"),
            Err(e) => panic!("could not create config directory: {}", e),
        }
    }

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

    // load scripts

    let application = Application::new(Some("uk.co.mrbenshef.Boop-GTK"), Default::default())
        .expect("failed to initialize GTK application");

    application.connect_activate(move |application| {
        let builder = gtk::Builder::new_from_string(include_str!("../ui/boop-gtk.glade"));
        builder.set_application(application);

        let (scripts, script_error) = load_all_scripts(&config_dir);
        let scripts = Rc::new(scripts);

        let app = App::from_builder(builder, &config_dir, scripts.clone());
        app.set_application(Some(application));
        app.show_all();

        if let Some(error) = script_error {
            app.push_error(error);
        }

        // add keyboard shortcut for opening command pallete
        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
        application.add_action(&command_pallete_action);
        application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
        
        // regisiter handler
        {
            let app_ = app.clone();
            command_pallete_action.connect_activate(move |_, _| app_.open_command_pallete());   
        }
    });

    application.run(&[]);

    Ok(())
}

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // don't spawn command line on windows

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate shrinkwraprs;

use gdk_pixbuf;
use gio;
use glib;
use gtk;

use sourceview;

use directories;
use rust_embed;
use rusty_v8;

#[macro_use]
extern crate log;

mod executor;
mod script;
use script::Script;
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
use std::{
    error::Error,
    fs,
    io::prelude::*,
    sync::{Arc, RwLock},
    thread,
};

lazy_static! {
    static ref PROJECT_DIRS: directories::ProjectDirs =
        ProjectDirs::from("uk.co", "mrbenshef", "boop-gtk")
            .expect("Unable to find a configuration location for your platform");
}

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
}

impl Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::LoadError(e) => write!(f, "{}", e),
        }
    }
}

impl Error for ScriptError {}

fn load_user_scripts(config_dir: &Path) -> Result<Vec<Script>, LoadScriptError> {
    let scripts_dir: PathBuf = config_dir.join("scripts");

    fs::create_dir_all(&scripts_dir).map_err(|_| LoadScriptError::FailedToCreateScriptDirectory)?;

    let paths =
        fs::read_dir(&scripts_dir).map_err(|_| LoadScriptError::FailedToReadScriptDirectory)?;

    Ok(paths
        .filter_map(Result::ok)
        .map(|f| f.path())
        .filter(|path| path.is_file())
        .map(Script::from_file)
        .filter_map(Result::ok)
        .collect())
}

fn load_internal_scripts() -> Vec<Script> {
    let mut scripts: Vec<Script> = Vec::with_capacity(Scripts::iter().count());

    // scripts are internal, so we can unwrap "safely"
    for file in Scripts::iter() {
        let file: Cow<'_, str> = file;
        let source: Cow<'static, [u8]> = Scripts::get(&file)
            .unwrap_or_else(|| panic!("failed to get file: {}", file.to_string()));
        let script_source = String::from_utf8(source.to_vec())
            .unwrap_or_else(|e| panic!("{} is not UTF8: {}", file, e));
        if let Ok(script) = Script::from_source(script_source, PathBuf::new()) {
            scripts.push(script);
        }
    }

    scripts
}

fn load_all_scripts(config_dir: &Path) -> (Vec<Script>, Option<ScriptError>) {
    let mut scripts = load_internal_scripts();

    let internal_script_count = scripts.len();

    match load_user_scripts(&config_dir) {
        Ok(mut user_scripts) => {
            scripts.append(&mut user_scripts);
        }
        Err(e) => return (scripts, Some(ScriptError::LoadError(e))),
    }

    info!(
        "found {} scripts ({} internal scripts)",
        scripts.len(),
        internal_script_count,
    );

    (scripts, None)
}

// extract language file, ideally we would use GResource for this but sourceview doesn't support that
fn extract_language_file() {
    let config_dir = PROJECT_DIRS.config_dir().to_path_buf();
    if !config_dir.exists() {
        info!("config directory does not exist, attempting to create it");
        match fs::create_dir_all(&config_dir) {
            Ok(()) => info!("created config directory"),
            Err(e) => panic!("could not create config directory: {}", e),
        }
    }

    info!("configuration directory at: {}", config_dir.display());

    let lang_file_path = {
        let mut path = config_dir;
        path.push("boop.lang");
        path
    };

    let mut file = fs::File::create(&lang_file_path).expect("Could not create language file");
    file.write_all(include_bytes!("../boop.lang"))
        .expect("Failed to write language file");
    info!("language file written at: {}", lang_file_path.display());
}

fn watch_scripts_folder(scripts: Arc<RwLock<Vec<Script>>>) {
    use notify::{RecommendedWatcher, RecursiveMode, Result, Watcher};

    trace!("watch_scripts_folder");

    // watch for changes to script folder
    let watcher: Result<RecommendedWatcher> = Watcher::new_immediate(move |res| {
        debug!("res: {:?}", res);
        match res {
            Ok(event) => {
                let event: notify::Event = event;

                for file in event.paths {
                    debug!("file: {}", file.display());

                    match file.extension() {
                        Some(s) => {
                            if s == "js" {
                            } else {
                                break;
                            }
                        }
                        None => break,
                    }

                    info!("{} changed, reloading", file.display());

                    // remove scripts, if they where modified we create a new instance bellow
                    let mut scripts = scripts.write().expect("script lock is poisoned");
                    for i in 0..scripts.len() {
                        if scripts[i].path == file {
                            scripts.remove(i);
                            break;
                        }
                    }

                    // .drain_filter(|script| script.path == file);

                    if !file.exists() {
                        break;
                    }

                    match Script::from_file(file.clone()) {
                        Ok(script) => {
                            scripts.push(script);
                            scripts.sort_by_key(|s| s.metadata.name.clone());
                        }
                        Err(e) => {
                            error!("error parsing {}: {}", file.display(), e);
                        }
                    }
                }
            }
            Err(e) => error!("watch error: {:?}", e),
        }
    });

    // configure and start watcher
    match watcher {
        Ok(mut watcher) => {
            let mut config_dir = PROJECT_DIRS.config_dir().to_path_buf();
            config_dir.push("scripts");

            info!("watching {}", config_dir.display());

            loop {
                if let Err(watch_error) = watcher.watch(&config_dir, RecursiveMode::Recursive) {
                    error!("watch start error: {}", watch_error);
                    break;
                }
            }
        }
        Err(watcher_error) => {
            error!("couldn't create watcher: {}", watcher_error);
        }
    }
}

fn main() {
    env_logger::init();

    debug!(
        "found {} pixbuf loaders",
        gdk_pixbuf::Pixbuf::get_formats().len()
    );

    extract_language_file();

    // initalize V8
    let platform = v8::new_default_platform().unwrap();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    info!("V8 initialized");

    let config_dir = PROJECT_DIRS.config_dir().to_path_buf();

    let (mut scripts, script_error) = load_all_scripts(&config_dir);

    // sort alphabetically
    scripts.sort_by_key(|s| s.metadata.name.clone());

    // watch scripts folder for changes
    let scripts = Arc::new(RwLock::new(scripts));
    {
        let scripts = scripts.clone();
        thread::spawn(move || {
            watch_scripts_folder(scripts);
        });
    }

    // needed on windows
    sourceview::View::static_type();

    let application = Application::new(Some("uk.co.mrbenshef.Boop-GTK"), Default::default())
        .expect("failed to initialize GTK application");

    application.connect_activate(move |application| {
        // resources.gresources is created by build.rs
        // it includes all the files in the resources directory
        let resource_bytes =
            include_bytes!(concat!(env!("OUT_DIR"), "/resources/resources.gresource"));
        let resource_data = glib::Bytes::from(&resource_bytes[..]);
        gio::resources_register(&gio::Resource::from_data(&resource_data).unwrap());

        // add embedeed icons to theme
        let icon_theme = gtk::IconTheme::get_default().expect("failed to get default icon theme");
        icon_theme.add_resource_path("/co/uk/mrbenshef/Boop-GTK/icons");

        let app = App::new(&config_dir, scripts.clone());
        app.set_application(Some(application));
        app.show_all();

        if let Some(error) = &script_error {
            app.push_error(error);
        }

        // add keyboard shortcut for opening command pallete
        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
        application.add_action(&command_pallete_action);
        application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
        command_pallete_action.connect_activate(move |_, _| app.open_command_pallete());
    });

    application.run(&[]);
}

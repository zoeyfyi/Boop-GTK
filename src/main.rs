#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // don't spawn command line on windows
#![feature(btree_drain_filter)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate shrinkwraprs;
#[macro_use]
extern crate log;

mod executor;
mod script;
mod app;
mod command_pallete;
mod scripts;

use gio::prelude::*;
use gtk::{prelude::*, Application};
use scripts::{LoadScriptError, ScriptMap};

use std::{fmt, path::PathBuf};

use app::{App, NOTIFICATION_LONG_DELAY};
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

// fn watch_scripts_folder(scripts: Arc<RwLock<Vec<Script>>>) {}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    debug!(
        "found {} pixbuf loaders",
        gdk_pixbuf::Pixbuf::get_formats().len()
    );

    extract_language_file();

    let config_dir = PROJECT_DIRS.config_dir().to_path_buf();

    // create user scripts directory
    let scripts_dir: PathBuf = config_dir.join("scripts");
    let script_error = fs::create_dir_all(&scripts_dir)
        .map_err(|_| LoadScriptError::FailedToCreateScriptDirectory)
        .map_err(ScriptError::LoadError)
        .err();

    let scripts = ScriptMap::new();

    // watch scripts folder for changes
    let scripts = Arc::new(RwLock::new(scripts));
    {
        let scripts = scripts.clone();
        thread::spawn(move || {
            ScriptMap::watch(scripts);
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
            app.post_notification_error(&error.to_string(), NOTIFICATION_LONG_DELAY);
        }

        // add keyboard shortcut for opening command pallete
        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
        application.add_action(&command_pallete_action);
        application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
        command_pallete_action.connect_activate(move |_, _| app.open_command_pallete());

        // Ctrl+Q keyboard shortcut for exiting
        let quit_action = gio::SimpleAction::new("quit", None);
        application.add_action(&quit_action);
        application.set_accels_for_action("app.quit", &["<Primary>Q"]);
        {
            let application = application.clone();
            quit_action.connect_activate(move |_, _| application.quit());
        }
    });

    application.run(&[]);
}

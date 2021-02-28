#![forbid(unsafe_code)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate shrinkwraprs;
#[macro_use]
extern crate log;
extern crate fs_extra;

mod app;
mod command_pallete;
mod executor;
mod script;
mod scripts;

use gio::prelude::*;
use glib;
use gtk::{prelude::*, Application, Window};
use scripts::{LoadScriptError, ScriptMap};

use std::{fmt, path::PathBuf};

use app::{App, NOTIFICATION_LONG_DELAY};
use fmt::Display;
use std::{
    error::Error,
    fs,
    io::prelude::*,
    sync::{Arc, RwLock},
    thread,
};

lazy_static! {
    static ref XDG_DIRS: xdg::BaseDirectories = match xdg::BaseDirectories::with_prefix("boop-gtk")
    {
        Ok(dirs) => dirs,
        Err(err) => panic!("Unable to find XDG directorys: {}", err),
    };
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
// returns true if the language file already existed, false otherwise
fn extract_language_file() -> bool {
    let lang_file_path = match XDG_DIRS.place_config_file("boop.lang") {
        Ok(path) => path,
        Err(err) => panic!("Could not construct language file path: {}", err),
    };

    let exists = lang_file_path.exists();

    if let Err(err) = fs::File::create(&lang_file_path)
        .and_then(|mut file| file.write_all(include_bytes!("../boop.lang")))
    {
        panic!("Could not create language file: {}", err)
    }

    exists
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    debug!(
        "found {} pixbuf loaders",
        gdk_pixbuf::Pixbuf::get_formats().len()
    );

    let lang_file_existed = extract_language_file();
    let is_first_launch = !lang_file_existed;

    glib::set_application_name("Boop-GTK");

    // create user scripts directory
    let scripts_dir: PathBuf = XDG_DIRS.get_config_home().join("scripts");
    let mut script_error = fs::create_dir_all(&scripts_dir)
        .map_err(|_| LoadScriptError::FailedToCreateScriptDirectory)
        .map_err(ScriptError::LoadError)
        .err();

    let (scripts, err) = ScriptMap::new();
    script_error = script_error.or(err.map(ScriptError::LoadError));

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

    let application = Application::new(Some("fyi.zoey.Boop-GTK"), Default::default())
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
        icon_theme.add_resource_path("/fyi/zoey/Boop-GTK/icons");

        Window::set_default_icon_name("fyi.zoey.Boop-GTK");

        let app = App::new(&XDG_DIRS.get_config_home(), scripts.clone());
        app.set_application(Some(application));
        app.show_all();
        if is_first_launch {
            app.open_shortcuts_window();
        }

        if let Some(error) = &script_error {
            app.post_notification_error(&error.to_string(), NOTIFICATION_LONG_DELAY);
        }

        // add keyboard shortcut for opening command pallete
        {
            let app = app.clone();
            let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
            application.add_action(&command_pallete_action);
            application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
            command_pallete_action.connect_activate(move |_, _| app.open_command_pallete());
        }

        {
            let app = app.clone();
            let reexecute_script_action = gio::SimpleAction::new("re_execute_script", None);
            application.add_action(&reexecute_script_action);
            application.set_accels_for_action("app.re_execute_script", &["<Primary><Shift>B"]);
            reexecute_script_action.connect_activate(move |_, _| app.re_execute());
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use directories::ProjectDirs;

    lazy_static! {
        static ref PROJECT_DIRS: directories::ProjectDirs =
            ProjectDirs::from("fyi", "zoey", "boop-gtk")
                .expect("Unable to find a configuration location for your platform");
    }

    #[test]
    fn test_project_dirs_dependency_change() {
        assert_eq!(
            PROJECT_DIRS.config_dir().to_path_buf(),
            XDG_DIRS.get_config_home()
        );
    }
}

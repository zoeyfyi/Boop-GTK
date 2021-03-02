#![forbid(unsafe_code)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate shrinkwraprs;
#[macro_use]
extern crate log;
#[macro_use]
extern crate eyre;
extern crate fs_extra;

mod config;
mod executor;
mod script;
mod scriptmap;
mod ui;
mod util;

use scriptmap::ScriptMap;
use ui::app::{App, NOTIFICATION_LONG_DELAY};

use crate::config::Config;
use eyre::{Context, Result};
use fs::File;
use gio::prelude::*;
use gtk::{prelude::*, Application, Window};

use std::{
    fs,
    io::prelude::*,
    path::PathBuf,
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

// extract language file, ideally we would use GResource for this but sourceview doesn't support that
// returns true if the language file already existed, false otherwise
fn extract_language_file() -> Result<()> {
    let lang_file_path = XDG_DIRS
        .place_config_file("boop.lang")
        .wrap_err("Failed to construct language file path")?;

    let mut lang_file = File::create(&lang_file_path).wrap_err("Failed to create language file")?;

    lang_file
        .write_all(include_bytes!("../boop.lang"))
        .wrap_err("Failed to write default language file")?;

    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let (config, config_file_created) = Config::load()?;
    let config = Arc::new(RwLock::new(config));

    extract_language_file()?;

    // create user scripts directory
    let scripts_dir: PathBuf = XDG_DIRS.get_config_home().join("scripts");
    fs::create_dir_all(&scripts_dir).wrap_err_with(|| {
        format!(
            "Failed to create scripts directory in config: {}",
            scripts_dir.display()
        )
    })?;

    let (scripts_map, load_script_error) = ScriptMap::new();
    let scripts = Arc::new(RwLock::new(scripts_map));

    // watch scripts folder for changes
    {
        let scripts = scripts.clone();
        thread::spawn(move || {
            ScriptMap::watch(scripts);
        });
    }

    // needed on windows
    sourceview::View::static_type();

    glib::set_application_name("Boop-GTK");

    let application = Application::new(Some("fyi.zoey.Boop-GTK"), Default::default())
        .wrap_err("Failed to initialize GTK application")?;

    application.connect_activate(move |application| {
        // resources.gresources is created by build.rs
        // it includes all the files in the resources directory
        let resource_bytes =
            include_bytes!(concat!(env!("OUT_DIR"), "/resources/resources.gresource"));
        let resource_data = glib::Bytes::from(&resource_bytes[..]);
        gio::resources_register(&gio::Resource::from_data(&resource_data).unwrap());

        // add embedeed icons to theme
        let icon_theme = gtk::IconTheme::get_default().expect("Failed to get default icon theme");
        icon_theme.add_resource_path("/fyi/zoey/Boop-GTK/icons");

        Window::set_default_icon_name("fyi.zoey.Boop-GTK");

        let app = App::new(&XDG_DIRS.get_config_home(), scripts.clone(), config.clone())
            .expect("Failed to construct App");
        app.set_application(Some(application));
        app.show_all();

        register_actions(&application, &app);

        if config_file_created
            || config
                .read()
                .expect("Config lock is poisoned")
                .show_shortcuts_on_open
        {
            app.open_shortcuts_window();
        }

        if let Some(error) = &load_script_error {
            app.post_notification_error(&error.to_string(), NOTIFICATION_LONG_DELAY);
        }
    });

    application.run(&[]);
    Ok(())
}

fn register_actions(application: &Application, app: &App) {
    // opening command pallete action
    {
        let app = app.clone();
        let command_pallete_action = gio::SimpleAction::new("command_pallete", None);
        application.add_action(&command_pallete_action);
        application.set_accels_for_action("app.command_pallete", &["<Primary><Shift>P"]);
        command_pallete_action.connect_activate(move |_, _| {
            app.run_command_pallete()
                .expect("Failed to run command pallete")
        });
    }

    // re-execute script action
    {
        let app = app.clone();
        let reexecute_script_action = gio::SimpleAction::new("re_execute_script", None);
        application.add_action(&reexecute_script_action);
        application.set_accels_for_action("app.re_execute_script", &["<Primary><Shift>B"]);
        reexecute_script_action
            .connect_activate(move |_, _| app.re_execute().expect("Failed to re-execute script"));
    }

    // quit action
    {
        let quit_action = gio::SimpleAction::new("quit", None);
        application.add_action(&quit_action);
        application.set_accels_for_action("app.quit", &["<Primary>Q"]);
        let application = application.clone();
        quit_action.connect_activate(move |_, _| application.quit());
    }
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

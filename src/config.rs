use std::{fs::File, io::Write};

use crate::XDG_DIRS;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub show_shortcuts_on_open: bool,
    pub editor: EditorConfig,
}

#[derive(Serialize, Deserialize)]
pub struct EditorConfig {
    pub colour_scheme: String,
}

impl Default for EditorConfig {
    fn default() -> Self {
        EditorConfig {
            colour_scheme: String::from("Classic"),
        }
    }
}

impl Config {
    pub fn load() -> (Config, bool) {
        let mut config_file_created = false;

        let config_path = XDG_DIRS
            .place_config_file("config.toml")
            .expect("Failed to create config folder");
        if !config_path.exists() {
            info!("creating config.toml");
            config_file_created = true;

            // no config file, write default
            let mut file = File::create(&config_path).expect("Failed to create config file");
            file.write_all(toml::to_string_pretty(&Config::default()).unwrap().as_bytes())
                .expect("Failed to write to config file");
        }

        let mut settings = config::Config::new();
        if let Err(err) = settings.merge(config::File::from(config_path)) {
            error!("Failed to read config file: {}", err);
        }

        (settings.try_into().unwrap(), config_file_created) // TODO: handle results
    }
}

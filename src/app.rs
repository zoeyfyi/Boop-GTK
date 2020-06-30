use crate::{
    command_pallete::CommandPalleteDialog,
    executor::{self, Executor},
    gtk::ButtonExt,
    script::Script,
};
use gio::{prelude::*};
use gtk::prelude::*;
use gtk::{ApplicationWindow, Builder, Button, ModelButton, Statusbar};
use sourceview::prelude::*;
use std::{path::Path, process::Command, rc::Rc};

const HEADER_BUTTON_GET_STARTED: &str = "Press Ctrl+Shift+P to get started";
const HEADER_BUTTON_CHOOSE_ACTION: &str = "Select an action";

#[derive(Clone, Shrinkwrap)]
pub struct App {
    #[shrinkwrap(main_field)]
    window: ApplicationWindow,
    
    header_button: Button,
    source_view: sourceview::View,
    status_bar: Statusbar,

    config_directory_button: ModelButton,
    more_scripts_button: ModelButton,
    about_button: ModelButton,

    context_id: u32,
    scripts: Rc<Vec<Script>>,
}

impl App {
    pub fn from_builder(builder: Builder, config_dir: &Path, scripts: Rc<Vec<Script>>) -> Self {
        let mut app = App {
            window: builder.get_object("window").unwrap(),
            
            header_button: builder.get_object("header_button").unwrap(),
            source_view: builder.get_object("source_view").unwrap(),
            status_bar: builder.get_object("status_bar").unwrap(),

            config_directory_button: builder.get_object("config_directory_button").unwrap(),
            more_scripts_button: builder.get_object("more_scripts_button").unwrap(),
            about_button: builder.get_object("about_button").unwrap(),

            context_id: 0,
            scripts,
        };

        app.context_id = app.status_bar.get_context_id("script execution");
        app.header_button.set_label(HEADER_BUTTON_GET_STARTED);
        app.setup_syntax_highlighting(config_dir);

        // launch config directory in default file manager
        {
            let app_ = app.clone();
            let config_dir_str = config_dir.display().to_string();
            app.config_directory_button.connect_clicked(move |_| {
                if let Err(launch_err) = {
                    #[cfg(target_os = "macos")]
                    return Command::new("open");
                    #[cfg(target_os = "windows")]
                    return Command::new("start");
                    Command::new("xdg-open")
                }
                .arg(config_dir_str.clone())
                .output()
                {
                    error!("could not launch config directory: {}", launch_err);
                    app_.push_error("failed to launch config directory");
                }
            });
        }

        // launch more scripts page in default web browser
        {
            let app_ = app.clone();
            app.more_scripts_button.connect_clicked(move |_| {
                if let Some(browser_app) = gio::AppInfo::get_default_for_uri_scheme("https") {
                    if let Err(launch_err) = browser_app.launch_uris(
                        &["https://github.com/IvanMathy/Boop/tree/main/Scripts"],
                        gio::NONE_APP_LAUNCH_CONTEXT,
                    ) {
                        error!("could not launch config directory: {}", launch_err);
                    }
                } else {
                    error!("could not find app for `https` type");
                    app_.push_error("could not find app for `https` type");
                }
            });
        }

        {
            let app_ = app.clone();
            app.header_button
                .connect_clicked(move |_| app_.open_command_pallete());
        }

        app
    }

    fn setup_syntax_highlighting(&self, config_dir: &Path) {
        let language_manager = sourceview::LanguageManager::get_default().unwrap();

        // add config_dir to language manager's search path
        let dirs = language_manager.get_search_path();
        let mut dirs: Vec<&str> = dirs.iter().map(|s| s.as_ref()).collect();
        let config_dir_path = config_dir.to_string_lossy().to_string();
        dirs.push(&config_dir_path);
        language_manager.set_search_path(&dirs);

        info!("language manager search directorys: {}", dirs.join(":"));

        let boop_language = language_manager.get_language("boop");
        if boop_language.is_none() {
            self.status_bar
                .push(self.context_id, "ERROR: failed to load language file");
        }

        // set language
        let buffer: sourceview::Buffer = self
            .source_view
            .get_buffer()
            .unwrap()
            .downcast::<sourceview::Buffer>()
            .unwrap();
        buffer.set_highlight_syntax(true);
        buffer.set_language(boop_language.as_ref());
    }

    pub fn push_error(&self, error: impl std::fmt::Display) {
        self.status_bar
            .push(self.context_id, &format!("ERROR: {}", error));
    }

    pub fn open_command_pallete(&self) {
        let scripts = self
            .scripts
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, s)| (i as u64, s))
            .collect::<Vec<(u64, Script)>>();
        let dialog = CommandPalleteDialog::new(&self.window, scripts.to_owned());
        dialog.show_all();

        self.header_button.set_label(HEADER_BUTTON_CHOOSE_ACTION);

        if let gtk::ResponseType::Other(script_id) = dialog.run() {
            let script = scripts[script_id as usize].1.clone();

            info!("executing {}", script.metadata().name);

            self.status_bar.remove_all(self.context_id);

            let buffer = &self.source_view.get_buffer().unwrap();

            let full_text = &buffer
                .get_text(&buffer.get_start_iter(), &buffer.get_end_iter(), false)
                .unwrap()
                .to_string();

            let selection_bounds = buffer.get_selection_bounds();
            let selection: Option<String> = if let Some((start, end)) = &selection_bounds {
                let selected_text = buffer.get_text(start, end, false).unwrap().to_string();
                Some(selected_text)
            } else {
                None
            };

            info!(
                "full_text length: {}, selection length: {}",
                full_text.len(),
                selection.as_ref().map(String::len).unwrap_or(0)
            );

            let result = Executor::new(&script).execute(full_text, selection.as_deref());

            match result.replacement {
                executor::TextReplacement::Full(text) => {
                    info!("replacing full text");
                    buffer.set_text(&text);
                }
                executor::TextReplacement::Selection(text) => {
                    info!("replacing selection");
                    let (start, end) = &mut selection_bounds.unwrap();
                    buffer.delete(start, end);
                    buffer.insert(start, &text);
                }
            }

            // TODO: how to handle multiple messages?
            if let Some(error) = result.error {
                self.status_bar.push(self.context_id, &error);
            } else if let Some(info) = result.info {
                self.status_bar.push(self.context_id, &info);
            }
        }

        self.header_button.set_label(HEADER_BUTTON_GET_STARTED);

        dialog.destroy();
    }
}

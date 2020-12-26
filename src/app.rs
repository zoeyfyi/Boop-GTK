use crate::{
    command_pallete::CommandPalleteDialog,
    executor::{self},
    script::Script,
    scripts::ScriptMap,
};
use gdk_pixbuf::prelude::*;
use gladis::Gladis;
use glib::SourceId;
use gtk::{prelude::*, Label, Revealer};
use sourceview::prelude::*;

use executor::TextReplacement;
use gtk::{AboutDialog, ApplicationWindow, Button, ModelButton};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};

const HEADER_BUTTON_GET_STARTED: &str = "Press Ctrl+Shift+P to get started";
const HEADER_BUTTON_CHOOSE_ACTION: &str = "Select an action";

pub const NOTIFICATION_LONG_DELAY: u32 = 5000;

#[derive(Gladis, Clone, Shrinkwrap)]
pub struct AppWidgets {
    #[shrinkwrap(main_field)]
    window: ApplicationWindow,

    header_button: Button,
    source_view: sourceview::View,
    // status_bar: Statusbar,
    notification_revealer: Revealer,
    notification_label: Label,
    notification_button: Button,

    reset_scripts_button: ModelButton,
    config_directory_button: ModelButton,
    more_scripts_button: ModelButton,
    about_button: ModelButton,

    about_dialog: AboutDialog,
}

#[derive(Clone, Shrinkwrap)]
pub struct App {
    #[shrinkwrap(main_field)]
    widgets: AppWidgets,
    scripts: Arc<RwLock<ScriptMap>>,
    notification_source_id: Arc<RwLock<Option<SourceId>>>,
}

impl App {
    pub(crate) fn new(config_dir: &Path, scripts: Arc<RwLock<ScriptMap>>) -> Self {
        let app = App {
            widgets: AppWidgets::from_resource("/fyi/zoey/Boop-GTK/boop-gtk.glade")
                .unwrap_or_else(|e| panic!("failed to load boop-gtk.glade: {}", e)),
            scripts,
            notification_source_id: Arc::new(RwLock::new(None)), // SourceId doesnt implement clone, so must be seperate from AppState
        };

        app.header_button.set_label(HEADER_BUTTON_GET_STARTED);
        app.about_dialog.set_logo(
            gdk_pixbuf::Pixbuf::from_resource("/fyi/zoey/Boop-GTK/boop-gtk.png")
                .ok()
                .as_ref(),
        );
        app.widgets
            .about_dialog
            .set_version(Some(env!("CARGO_PKG_VERSION")));

        for (_, script) in app
            .scripts
            .read()
            .expect("scripts lock is poisoned")
            .0
            .iter()
        {
            if let Some(author) = &script.metadata.author {
                app.about_dialog
                    .add_credit_section(&format!("{} script", &script.metadata.name), &[author]);
            }
        }

        app.setup_syntax_highlighting(config_dir);

        // close notification on dismiss
        {
            let notification_revealer = app.notification_revealer.clone();
            app.notification_button
                .connect_button_press_event(move |_button, _event| {
                    notification_revealer.set_reveal_child(false);
                    Inhibit(false)
                });
        }

        // reset the state of each script
        {
            let scripts = app.scripts.clone();
            app.reset_scripts_button.connect_clicked(move |_| {
                for (_, script) in scripts
                    .write()
                    .expect("scripts lock is poisoned")
                    .0
                    .iter_mut()
                {
                    script.kill_thread();
                }
            });
        }

        // launch config directory in default file manager
        {
            let config_dir_str = config_dir.display().to_string();
            let app_ = app.clone();
            app.config_directory_button.connect_clicked(move |_| {
                if let Err(open_err) = open::that(config_dir_str.clone()) {
                    error!("could not launch config directory: {}", open_err);
                    app_.post_notification_error(
                        "Failed to launch config directory",
                        NOTIFICATION_LONG_DELAY,
                    );
                }
            });
        }

        // launch more scripts page in default web browser
        {
            let app_ = app.clone();
            app.more_scripts_button.connect_clicked(move |_| {
                if let Err(open_err) = open::that("https://boop.okat.best/scripts/") {
                    error!("could not launch website: {}", open_err);
                    app_.post_notification_error(
                        "Failed to launch website",
                        NOTIFICATION_LONG_DELAY,
                    );
                }
            });
        }

        {
            let about_dialog: AboutDialog = app.about_dialog.clone();
            app.about_button.connect_clicked(move |_| {
                let responce = about_dialog.run();
                if responce == gtk::ResponseType::DeleteEvent
                    || responce == gtk::ResponseType::Cancel
                {
                    about_dialog.hide();
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

    fn post_notification(&self, text: &str, delay: u32) {
        let notification_source_id = self.notification_source_id.clone();
        let notification_revealer = self.notification_revealer.clone();
        let notification_label = self.notification_label.clone();

        {
            notification_label.set_markup(text);
            notification_revealer.set_reveal_child(true);

            let mut source_id = notification_source_id.write().unwrap();

            // cancel old notification timeout
            if source_id.is_some() {
                glib::source_remove(source_id.take().unwrap());
            }

            // dismiss after 3000ms
            let new_source_id = {
                let notification_source_id = notification_source_id.clone();
                glib::timeout_add_local(delay, move || {
                    notification_revealer.set_reveal_child(false);
                    *notification_source_id.write().unwrap() = None;
                    Continue(false)
                })
            };

            source_id.replace(new_source_id);
        }
    }

    pub fn post_notification_error(&self, text: &str, delay: u32) {
        self.post_notification(
            &format!(
                r#"<span foreground="red" weight="bold">ERROR:</span> {}"#,
                text
            ),
            delay,
        );
    }

    fn setup_syntax_highlighting(&self, config_dir: &Path) {
        let language_manager =
            sourceview::LanguageManager::get_default().expect("failed to get language manager");

        // add config_dir to language manager's search path
        let dirs = language_manager.get_search_path();
        let mut dirs: Vec<&str> = dirs.iter().map(|s| s.as_ref()).collect();
        let config_dir_path = config_dir.to_string_lossy().to_string();
        dirs.push(&config_dir_path);
        language_manager.set_search_path(&dirs);

        info!("language manager search directorys: {}", dirs.join(":"));

        let boop_language = language_manager.get_language("boop");
        if boop_language.is_none() {
            self.post_notification(
                r#"<span foreground="red">ERROR:</span> failed to load language file"#,
                NOTIFICATION_LONG_DELAY,
            );
        }

        // set language
        let buffer: sourceview::Buffer = self
            .source_view
            .get_buffer()
            .expect("failed to get buffer")
            .downcast::<sourceview::Buffer>()
            .expect("faild to downcast TextBuffer to sourceview Buffer");
        buffer.set_highlight_syntax(true);
        buffer.set_language(boop_language.as_ref());
    }

    // fn push_error_(status_bar: gtk::Statusbar, context_id: u32, error: impl std::fmt::Display) {
    //     status_bar.push(context_id, &format!("ERROR: {}", error));
    // }

    // pub fn push_error(&self, error: impl std::fmt::Display) {
    //     App::push_error_(self.status_bar.clone(), self.context_id, error);
    // }

    pub fn open_command_pallete(&self) {
        let dialog = CommandPalleteDialog::new(&self.window, self.scripts.clone());
        dialog.show_all();

        self.header_button.set_label(HEADER_BUTTON_CHOOSE_ACTION);

        if let gtk::ResponseType::Accept = dialog.run() {
            let selected: &str = dialog
                .get_selected()
                .expect("dialog didn't return a selection");

            let mut script_map = self.scripts.write().expect("scripts lock is poisoned");
            let script: &mut Script =
                &mut script_map.0.get_mut(selected).expect("script not in map");

            info!("executing {}", script.metadata.name);

            let buffer = &self.source_view.get_buffer().expect("failed to get buffer");

            let buffer_text = buffer
                .get_text(&buffer.get_start_iter(), &buffer.get_end_iter(), false)
                .expect("failed to get buffer text");

            let selection_text = buffer
                .get_selection_bounds()
                .map(|(start, end)| buffer.get_text(&start, &end, false))
                .flatten()
                .map(|s| s.to_string());

            let status_result = script.execute(buffer_text.as_str(), selection_text.as_deref());

            match status_result {
                Ok(status) => {
                    // TODO: how to handle multiple messages?
                    if let Some(error) = status.error() {
                        self.post_notification(
                            &format!(
                                r#"<span foreground="red" weight="bold">ERROR:</span> {}"#,
                                error
                            ),
                            NOTIFICATION_LONG_DELAY,
                        );
                    } else if let Some(info) = status.info() {
                        self.post_notification(&info, NOTIFICATION_LONG_DELAY);
                    }
                    self.do_replacement(status.into_replacement());
                }
                Err(err) => {
                    warn!("Exception: {:?}", err);
                    match err {
                        executor::ExecutorError::SourceExceedsMaxLength => {
                            self.post_notification_error(
                                "Script exceeds max length",
                                NOTIFICATION_LONG_DELAY,
                            );
                        }
                        executor::ExecutorError::Compile(exception) => {
                            let error_str = match (exception.line_number, exception.columns) {
                                (Some(line_number), Some((left_column, right_column))) => format!(
                                    r#"<span foreground="red" weight="bold">EXCEPTION:</span> {} ({}:{} - {}:{})"#,
                                    exception.exception_str,
                                    line_number,
                                    left_column,
                                    line_number,
                                    right_column
                                ),
                                _ => format!(
                                    r#"<span foreground="red" weight="bold">EXCEPTION:</span> {}"#,
                                    exception.exception_str,
                                ),
                            };

                            self.post_notification(&error_str, NOTIFICATION_LONG_DELAY);
                        }
                        executor::ExecutorError::Execute(exception) => {
                            let error_str = match (exception.line_number, exception.columns) {
                                (Some(line_number), Some((left_column, right_column))) => format!(
                                    r#"<span foreground="red" weight="bold">EXCEPTION:</span> {} ({}:{} - {}:{})"#,
                                    exception.exception_str,
                                    line_number,
                                    left_column,
                                    line_number,
                                    right_column
                                ),
                                _ => format!(
                                    r#"<span foreground="red" weight="bold">EXCEPTION:</span> {}"#,
                                    exception.exception_str,
                                ),
                            };

                            self.post_notification(&error_str, NOTIFICATION_LONG_DELAY);
                        }
                        executor::ExecutorError::NoMain => {
                            self.post_notification(
                                r#"<span foreground="red">ERROR:</span> No main function"#,
                                NOTIFICATION_LONG_DELAY,
                            );
                        }
                    }
                }
            }
        }

        self.header_button.set_label(HEADER_BUTTON_GET_STARTED);

        dialog.close();
    }

    fn do_replacement(&self, replacement: TextReplacement) {
        let buffer = &self.source_view.get_buffer().expect("failed to get buffer");

        match replacement {
            TextReplacement::Full(text) => {
                info!("replacing full text");

                let text = String::from_utf8(
                    text.into_bytes()
                        .into_iter()
                        .filter(|b| *b != 0)
                        .collect::<Vec<u8>>(),
                )
                .expect("failed to remove null bytes from text");

                buffer.set_text(&text);
            }
            TextReplacement::Selection(text) => {
                info!("replacing selection");

                let text = String::from_utf8(
                    text.into_bytes()
                        .into_iter()
                        .filter(|b| *b != 0)
                        .collect::<Vec<u8>>(),
                )
                .expect("failed to remove null bytes from text");

                match &mut buffer.get_selection_bounds() {
                    Some((start, end)) => {
                        buffer.delete(start, end);
                        buffer.insert(start, &text);
                    }
                    None => {
                        error!("tried to do a selection replacement, but no text is selected!");
                    }
                }
            }
            TextReplacement::Insert(insertions) => {
                let insert_text = insertions.join("");
                info!("inserting {} bytes", insert_text.len());

                let insert_text = String::from_utf8(
                    insert_text
                        .into_bytes()
                        .into_iter()
                        .filter(|b| *b != 0)
                        .collect::<Vec<u8>>(),
                )
                .expect("failed to remove null bytes from text");

                match &mut buffer.get_selection_bounds() {
                    Some((start, end)) => {
                        buffer.delete(start, end);
                        buffer.insert(start, &insert_text);
                    }
                    None => {
                        let mut insert_point =
                            buffer.get_iter_at_offset(buffer.get_property_cursor_position());
                        buffer.insert(&mut insert_point, &insert_text)
                    }
                }
            }
            TextReplacement::None => {
                info!("no text to replace");
            }
        }
    }
}

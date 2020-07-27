use crate::{
    command_pallete::CommandPalleteDialog,
    executor::{self, Executor},
    gtk::ButtonExt,
};
use gdk_pixbuf::{prelude::*, PixbufLoader};
use gladis::Gladis;
use gtk::prelude::*;
use sourceview::prelude::*;

use executor::TextReplacement;
use gtk::{AboutDialog, ApplicationWindow, Button, ModelButton, Statusbar};
use std::{cell::RefCell, path::Path, rc::Rc};

const HEADER_BUTTON_GET_STARTED: &str = "Press Ctrl+Shift+P to get started";
const HEADER_BUTTON_CHOOSE_ACTION: &str = "Select an action";

#[derive(Gladis, Clone, Shrinkwrap)]
pub struct AppWidgets {
    #[shrinkwrap(main_field)]
    window: ApplicationWindow,

    header_button: Button,
    source_view: sourceview::View,
    status_bar: Statusbar,

    config_directory_button: ModelButton,
    more_scripts_button: ModelButton,
    about_button: ModelButton,

    about_dialog: AboutDialog,
}

#[derive(Clone, Shrinkwrap)]
pub struct App {
    #[shrinkwrap(main_field)]
    widgets: AppWidgets,

    context_id: u32,
    scripts: Rc<RefCell<Vec<Executor<'static>>>>,
}

impl App {
    pub fn new(config_dir: &Path, scripts: Rc<RefCell<Vec<Executor<'static>>>>) -> Self {
        let mut app = App {
            widgets: AppWidgets::from_string(include_str!("../ui/boop-gtk.glade")),
            context_id: 0,
            scripts,
        };

        app.context_id = app.status_bar.get_context_id("script execution");
        app.header_button.set_label(HEADER_BUTTON_GET_STARTED);
        app.about_dialog.set_logo({
            let loader = PixbufLoader::with_type("png").unwrap();
            loader.write(include_bytes!("../ui/boop-gtk.png")).unwrap();
            loader.close().unwrap();
            loader.get_pixbuf().as_ref()
        });
        app.setup_syntax_highlighting(config_dir);

        let context_id = app.context_id;

        // launch config directory in default file manager
        {
            let status_bar = app.status_bar.clone();
            let config_dir_str = config_dir.display().to_string();
            app.config_directory_button.connect_clicked(move |_| {
                if let Err(open_err) = open::that(config_dir_str.clone()) {
                    error!("could not launch config directory: {}", open_err);
                    App::push_error_(
                        status_bar.clone(),
                        context_id,
                        "failed to launch config directory",
                    );
                }
            });
        }

        // launch more scripts page in default web browser
        {
            let status_bar = app.status_bar.clone();
            app.more_scripts_button.connect_clicked(move |_| {
                if let Err(open_err) = open::that("https://boop.okat.best/scripts/") {
                    error!("could not launch website: {}", open_err);
                    App::push_error_(status_bar.clone(), context_id, "failed to launch website");
                }
            });
        }

        {
            let about_dialog = app.about_dialog.clone();
            app.about_button.connect_clicked(move |_| {
                about_dialog.show();
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

    fn push_error_(status_bar: gtk::Statusbar, context_id: u32, error: impl std::fmt::Display) {
        status_bar.push(context_id, &format!("ERROR: {}", error));
    }

    pub fn push_error(&self, error: impl std::fmt::Display) {
        App::push_error_(self.status_bar.clone(), self.context_id, error);
    }

    pub fn open_command_pallete(&self) {
        let dialog = CommandPalleteDialog::new(&self.window, self.scripts.clone());
        dialog.show_all();

        self.header_button.set_label(HEADER_BUTTON_CHOOSE_ACTION);

        if let gtk::ResponseType::Other(script_id) = dialog.run() {
            info!(
                "executing {}",
                self.scripts.borrow()[script_id as usize]
                    .script()
                    .metadata()
                    .name
            );

            self.status_bar.remove_all(self.context_id);

            let buffer = &self.source_view.get_buffer().unwrap();

            let buffer_text = buffer
                .get_text(&buffer.get_start_iter(), &buffer.get_end_iter(), false)
                .unwrap();

            let selection_text = buffer
                .get_selection_bounds()
                .map(|(start, end)| buffer.get_text(&start, &end, false).unwrap().to_string());

            let status = self.scripts.borrow_mut()[script_id as usize]
                .execute(buffer_text.as_str(), selection_text.as_deref());

            // TODO: how to handle multiple messages?
            if let Some(error) = status.error() {
                self.status_bar.push(self.context_id, &error);
            } else if let Some(info) = status.info() {
                self.status_bar.push(self.context_id, &info);
            }

            match status.into_replacement() {
                TextReplacement::Full(text) => {
                    info!("replacing full text");
                    buffer.set_text(&text);
                }
                TextReplacement::Selection(text) => {
                    info!("replacing selection");
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

        self.header_button.set_label(HEADER_BUTTON_GET_STARTED);

        dialog.close();
    }
}

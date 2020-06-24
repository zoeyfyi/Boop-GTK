use gdk::EventKey;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Dialog, Entry, Label, ListBox, Window};
use shrinkwraprs::Shrinkwrap;
use sublime_fuzzy::FuzzySearch;

use crate::script::Script;
use crate::SEARCH_CONFIG;

#[derive(Shrinkwrap)]
pub struct CommandPalleteDialog {
    #[shrinkwrap(main_field)]
    dialog: Dialog,
    dialog_list_box: ListBox,
    searchbar: Entry,
    scripts: Vec<Script>,
}

impl CommandPalleteDialog {
    pub fn new<P: IsA<Window>>(window: &P, scripts: Vec<Script>) -> Self {
        let dialog = Dialog::new();
        dialog.set_default_size(300, 0);
        dialog.set_modal(true);
        dialog.set_destroy_with_parent(true);
        dialog.set_property_window_position(gtk::WindowPosition::CenterOnParent);
        dialog.set_transient_for(Some(window));

        let dialog_list_box = ListBox::new();
        dialog.get_content_area().add(&dialog_list_box);

        for script in &scripts {
            dialog_list_box.add(&Label::new(Some(&script.metadata().name)));
        }

        // select first row
        dialog_list_box.select_row((&dialog_list_box.get_row_at_index(0)).as_ref());

        let searchbar = gtk::Entry::new();
        searchbar.set_hexpand(true);
        let scripts = scripts.clone();

        let header = gtk::HeaderBar::new();
        header.set_custom_title(Some(&searchbar));
        dialog.set_titlebar(Some(&header));

        searchbar.grab_focus();

        let command_pallete_dialog = CommandPalleteDialog {
            dialog,
            dialog_list_box,
            searchbar,
            scripts,
        };
        command_pallete_dialog.register_handlers();

        command_pallete_dialog
    }

    fn register_handlers(&self) {
        let lb = self.dialog_list_box.clone();
        self.searchbar
            .connect_key_press_event(move |s, k| CommandPalleteDialog::on_key_press(s, k, &lb));

        let lb = self.dialog_list_box.clone();
        let scripts = self.scripts.clone();
        self.searchbar
            .connect_changed(move |s| CommandPalleteDialog::on_changed(s, &lb, &scripts));
    }

    fn on_key_press(_searchbar: &Entry, key: &EventKey, dialog_list_box: &ListBox) -> Inhibit {
        if let Some(selected_row) = dialog_list_box.get_selected_row() {
            let index = selected_row.get_index();
            let child_count = dialog_list_box.get_children().len() as i32;

            let mut new_index = match key.get_keyval() {
                gdk::enums::key::Up => index - 1,
                gdk::enums::key::Down => index + 1,
                _ => index,
            };

            // wrap
            if new_index < 0 {
                new_index = (child_count as i32) - 1;
            } else if new_index >= child_count {
                new_index = 0;
            }

            println!("key press {:?}, index: {}, new_index: {}", key, index, new_index);

            dialog_list_box.select_row(dialog_list_box.get_row_at_index(new_index).as_ref());
        }

        Inhibit(false)
    }

    fn on_changed(searchbar: &Entry, dialog_list_box: &ListBox, scripts: &Vec<Script>) {
        for child in dialog_list_box.get_children() {
            dialog_list_box.remove(&child);
        }

        let searchbar_text = searchbar
            .get_text()
            .map(|s| s.to_string())
            .unwrap_or_else(String::new);

        println!("searchbar text: {}", searchbar_text);

        let search_results = if searchbar_text.is_empty() {
            scripts.clone()
        } else {
            let mut scored_scripts = scripts
                .clone()
                .into_iter()
                .map(|script| {
                    let mut search =
                        FuzzySearch::new(&searchbar_text, &script.metadata().name, true);
                    search.set_score_config(SEARCH_CONFIG);

                    let score = search.best_match().map(|m| m.score()).unwrap_or(0);
                    println!("score: {}", score);
                    (script.clone(), score)
                })
                .filter(|(_, score)| *score > 0)
                .collect::<Vec<(Script, isize)>>();

            scored_scripts.sort_by_key(|(_, score)| *score);

            scored_scripts
                .into_iter()
                .map(|(script, _)| script)
                .collect()
        };

        for script in &search_results {
            dialog_list_box.add(&gtk::Label::new(Some(&script.metadata().name)));
        }

        // reset selection to first row
        dialog_list_box.select_row((&dialog_list_box.get_row_at_index(0)).as_ref());

        dialog_list_box.show_all();
    }
}

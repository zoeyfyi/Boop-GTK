use gdk::{enums::key, EventKey};
use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Dialog, Entry, TreeView, Window};
use shrinkwraprs::Shrinkwrap;
use sublime_fuzzy::FuzzySearch;

use crate::script::Script;
use crate::SEARCH_CONFIG;
use glib::Type;

const ICON_COLUMN: u32 = 0;
const TEXT_COLUMN: u32 = 1;
const ID_COLUMN: u32 = 2;

const COLUMNS: [u32; 3] = [ICON_COLUMN, TEXT_COLUMN, ID_COLUMN];
const COLUMN_TYPES: [Type; 3] = [Type::String, Type::String, Type::U64];

#[derive(Shrinkwrap)]
pub struct CommandPalleteDialog {
    #[shrinkwrap(main_field)]
    dialog: Dialog,
    dialog_tree_view: TreeView,
    search_bar: Entry,
    scripts: Vec<(u64, Script)>,
}

impl CommandPalleteDialog {
    pub fn new<P: IsA<Window>>(window: &P, scripts: Vec<(u64, Script)>) -> Self {
        let dialog_glade = include_str!("../ui/command-pallete.glade");
        let builder = gtk::Builder::new_from_string(dialog_glade);

        let command_pallete_dialog = CommandPalleteDialog {
            dialog: builder.get_object("dialog").unwrap(),
            dialog_tree_view: builder.get_object("dialog_tree_view").unwrap(),
            search_bar: builder.get_object("search_bar").unwrap(),
            scripts: scripts.clone(),
        };

        command_pallete_dialog
            .dialog
            .set_transient_for(Some(window));

        // create list store
        {
            let store = gtk::ListStore::new(&COLUMN_TYPES);

            // icon column
            {
                let renderer = gtk::CellRendererPixbuf::new();
                renderer.set_padding(8, 0);

                let column = gtk::TreeViewColumn::new();
                column.pack_start(&renderer, false);
                column.add_attribute(&renderer, "icon-name", ICON_COLUMN as i32);

                command_pallete_dialog
                    .dialog_tree_view
                    .append_column(&column);
            }

            // text column
            {
                let renderer = gtk::CellRendererText::new();
                renderer.set_property_wrap_mode(pango::WrapMode::Word);

                let column = gtk::TreeViewColumn::new();
                column.pack_start(&renderer, true);
                column.add_attribute(&renderer, "markup", TEXT_COLUMN as i32);

                command_pallete_dialog
                    .dialog_tree_view
                    .append_column(&column);
            }

            for (script_id, script) in &scripts {
                let icon_name = String::from(match script.metadata().icon.as_str() {
                    "broom" => "draw-eraser",
                    "counter" => "cm_markplus",
                    "fingerprint" => "auth-fingerprint-symbolic",
                    "flip" => "object-flip-verical",
                    "html" => "format-text-code",
                    "link" => "edit-link",
                    "metamorphose" => "shapes",
                    "quote" => "format-text-blockquote",
                    "table" => "table",
                    "watch" => "view-calendar-time-spent",
                    _ => "fcitx-remind-active",
                });

                let entry_text = format!(
                    "<b>{}</b>\n<span size=\"smaller\">{}</span>",
                    script.metadata().name.to_string(),
                    script.metadata().description.to_string()
                );

                let values: [&dyn ToValue; 3] = [&icon_name, &entry_text, script_id];
                store.set(&store.append(), &COLUMNS, &values);
            }

            command_pallete_dialog
                .dialog_tree_view
                .set_model(Some(&store));
        }

        // select first row
        command_pallete_dialog.dialog_tree_view.set_cursor(
            &gtk::TreePath::new_first(),
            gtk::NONE_TREE_VIEW_COLUMN,
            false,
        );

        command_pallete_dialog.register_handlers();
        command_pallete_dialog
    }

    fn register_handlers(&self) {
        let lb = self.dialog_tree_view.clone();
        let dialog = self.dialog.clone();
        self.dialog.connect_key_press_event(move |_, k| {
            CommandPalleteDialog::on_key_press(k, &lb, &dialog)
        });

        let lb = self.dialog_tree_view.clone();
        let scripts = self.scripts.clone();
        self.search_bar
            .connect_changed(move |s| CommandPalleteDialog::on_changed(s, &lb, &scripts));

        let dialog = self.dialog.clone();
        self.dialog_tree_view
            .connect_row_activated(move |tv, _, _| CommandPalleteDialog::on_click(tv, &dialog));
    }

    fn on_key_press(key: &EventKey, dialog_tree_view: &TreeView, dialog: &Dialog) -> Inhibit {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();
        let result_count: i32 = model.iter_n_children(None);

        let key = key.get_keyval();

        if key == key::Up || key == key::Down {
            if let (Some(mut path), _) = dialog_tree_view.get_cursor() {
                let index: i32 = path.get_indices()[0];

                match key {
                    key::Up => {
                        if index == 0 {
                            path = gtk::TreePath::new_from_indicesv(&[result_count - 1]);
                        } else {
                            path.prev();
                        }
                    }
                    key::Down => {
                        if index >= result_count - 1 {
                            path = gtk::TreePath::new_first();
                        } else {
                            path.next();
                        }
                    }
                    _ => (),
                };

                dialog_tree_view.set_cursor(&path, gtk::NONE_TREE_VIEW_COLUMN, false);
            }

            return Inhibit(true);
        } else if key == key::Return {
            CommandPalleteDialog::on_click(dialog_tree_view, dialog);
        } else if key == key::Escape {
            dialog.destroy();
        }

        Inhibit(false)
    }

    fn on_click(dialog_tree_view: &TreeView, dialog: &Dialog) {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();

        if let (Some(path), _) = dialog_tree_view.get_cursor() {
            let value = model.get_value(&model.get_iter(&path).unwrap(), ID_COLUMN as i32);

            let v = value.downcast::<u64>().unwrap().get().unwrap();

            println!("value is {:?}", v);

            dialog.response(gtk::ResponseType::Other(v as u16));
        }
    }

    fn on_changed(searchbar: &Entry, dialog_tree_view: &TreeView, scripts: &[(u64, Script)]) {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();
        model.clear();

        let searchbar_text = searchbar
            .get_text()
            .map(|s| s.to_string())
            .unwrap_or_else(String::new);

        println!("searchbar text: {}", searchbar_text);

        let search_results = if searchbar_text.is_empty() {
            scripts.to_owned()
        } else {
            let mut scored_scripts = scripts
                .iter()
                .map(|(script_id, script)| {
                    let mut search =
                        FuzzySearch::new(&searchbar_text, &script.metadata().name, true);
                    search.set_score_config(SEARCH_CONFIG);

                    let score = search.best_match().map(|m| m.score()).unwrap_or(0);
                    (*script_id, script.clone(), score)
                })
                .filter(|(_, _, score)| *score > 0)
                .collect::<Vec<(u64, Script, isize)>>();

            scored_scripts.sort_by_key(|(_, _, score)| *score);

            scored_scripts
                .into_iter()
                .map(|(script_id, script, _)| (script_id, script))
                .collect()
        };

        for (script_id, script) in &search_results {
            let icon_name = String::from(match script.metadata().icon.as_str() {
                "broom" => "draw-eraser",
                "counter" => "cm_markplus",
                "fingerprint" => "auth-fingerprint-symbolic",
                "flip" => "object-flip-verical",
                "html" => "format-text-code",
                "link" => "edit-link",
                "metamorphose" => "shapes",
                "quote" => "format-text-blockquote",
                "table" => "table",
                "watch" => "view-calendar-time-spent",
                _ => "fcitx-remind-active",
            });

            let entry_text = format!(
                "<b>{}</b>\n<span size=\"smaller\">{}</span>",
                script.metadata().name.to_string(),
                script.metadata().description.to_string()
            );

            let values: [&dyn ToValue; 3] = [&icon_name, &entry_text, script_id];
            model.set(&model.append(), &COLUMNS, &values);
        }

        // reset selection to first row
        dialog_tree_view.set_cursor(
            &gtk::TreePath::new_first(),
            gtk::NONE_TREE_VIEW_COLUMN,
            false,
        );

        dialog_tree_view.show_all();
    }
}

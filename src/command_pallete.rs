use gdk::{keys, EventKey};
use gio::prelude::*;
use gladis::Gladis;
use gtk::prelude::*;
use gtk::{Dialog, Entry, TreePath, TreeView, Window};
use shrinkwraprs::Shrinkwrap;
use sublime_fuzzy::FuzzySearch;

use crate::script::Script;
use crate::{executor::Executor, SEARCH_CONFIG};
use glib::Type;
use std::{cell::RefCell, rc::Rc};

const ICON_COLUMN: u32 = 0;
const TEXT_COLUMN: u32 = 1;
const ID_COLUMN: u32 = 2;

const COLUMNS: [u32; 3] = [ICON_COLUMN, TEXT_COLUMN, ID_COLUMN];
const COLUMN_TYPES: [Type; 3] = [Type::String, Type::String, Type::U64];

#[derive(Shrinkwrap, Gladis)]
pub struct CommandPalleteDialogWidgets {
    #[shrinkwrap(main_field)]
    dialog: Dialog,
    dialog_tree_view: TreeView,
    search_bar: Entry,
}

#[derive(Shrinkwrap)]
pub struct CommandPalleteDialog {
    #[shrinkwrap(main_field)]
    widgets: CommandPalleteDialogWidgets,

    scripts: Rc<RefCell<Vec<Executor>>>,
}

impl CommandPalleteDialog {
    pub fn new<P: IsA<Window>>(window: &P, scripts: Rc<RefCell<Vec<Executor>>>) -> Self {
        let widgets =
            CommandPalleteDialogWidgets::from_string(include_str!("../ui/command-pallete.glade"));

        let command_pallete_dialog = CommandPalleteDialog {
            widgets,
            scripts: scripts.clone(),
        };

        command_pallete_dialog.set_transient_for(Some(window));

        // create list store
        {
            let store = gtk::ListStore::new(&COLUMN_TYPES);

            // icon column
            {
                let renderer = gtk::CellRendererPixbuf::new();
                renderer.set_padding(8, 8);
                renderer.set_property_stock_size(gtk::IconSize::Dnd);

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

            for script in scripts.borrow().iter() {
                let mut icon_name = script.script().metadata().icon.to_lowercase();
                icon_name.insert_str(0, "boop-gtk-");
                icon_name.push_str("-symbolic");
                debug!("icon: {}", icon_name);

                let entry_text = format!(
                    "<b>{}</b>\n<span size=\"smaller\">{}</span>",
                    script.script().metadata().name.to_string(),
                    script.script().metadata().description.to_string()
                );

                let values: [&dyn ToValue; 3] = [&icon_name, &entry_text, &script.script().id];
                store.set(&store.append(), &COLUMNS, &values);
            }

            command_pallete_dialog
                .dialog_tree_view
                .set_model(Some(&store));
        }

        // select first row
        command_pallete_dialog.dialog_tree_view.set_cursor(
            &TreePath::new_first(),
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
            .connect_changed(move |s| CommandPalleteDialog::on_changed(s, &lb, scripts.clone()));

        let dialog = self.dialog.clone();
        self.dialog_tree_view
            .connect_row_activated(move |tv, _, _| CommandPalleteDialog::on_click(tv, &dialog));
    }

    fn on_key_press(key: &EventKey, dialog_tree_view: &TreeView, dialog: &Dialog) -> Inhibit {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();
        let result_count: i32 = model.iter_n_children(None);

        let key = key.get_keyval();
        if key == keys::constants::Up || key == keys::constants::Down {
            if let (Some(mut path), _) = dialog_tree_view.get_cursor() {
                let index: i32 = path.get_indices()[0];

                match key {
                    keys::constants::Up => {
                        if index == 0 {
                            path = TreePath::from_indicesv(&[result_count - 1]);
                        } else {
                            path.prev();
                        }
                    }
                    keys::constants::Down => {
                        if index >= result_count - 1 {
                            path = TreePath::new_first();
                        } else {
                            path.next();
                        }
                    }
                    _ => (),
                };

                dialog_tree_view.set_cursor(&path, gtk::NONE_TREE_VIEW_COLUMN, false);
            }

            return Inhibit(true);
        } else if key == keys::constants::Return {
            CommandPalleteDialog::on_click(dialog_tree_view, dialog);
        } else if key == keys::constants::Escape {
            dialog.close();
        }

        Inhibit(false)
    }

    fn on_click(dialog_tree_view: &TreeView, dialog: &Dialog) {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();

        if let (Some(path), _) = dialog_tree_view.get_cursor() {
            let value = model.get_value(&model.get_iter(&path).unwrap(), ID_COLUMN as i32);
            let v = value.downcast::<u64>().unwrap().get().unwrap();
            dialog.response(gtk::ResponseType::Other(v as u16));
        }
    }

    fn on_changed(
        searchbar: &Entry,
        dialog_tree_view: &TreeView,
        scripts: Rc<RefCell<Vec<Executor>>>,
    ) {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();
        model.clear();

        let searchbar_text = searchbar.get_text().to_owned();

        let search_results: Vec<Script> = if searchbar_text.is_empty() {
            scripts
                .borrow()
                .iter()
                .map(|s| s.script())
                .cloned()
                .collect()
        } else {
            let mut scored_scripts = scripts
                .borrow()
                .iter()
                .map(|script| {
                    let mut search =
                        FuzzySearch::new(&searchbar_text, &script.script().metadata().name, true);
                    search.set_score_config(SEARCH_CONFIG);

                    let score = search.best_match().map(|m| m.score()).unwrap_or(0);
                    (script.script().clone(), score)
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
            let mut icon_name = script.metadata().icon.to_lowercase();
            icon_name.insert_str(0, "boop-gtk-");
            icon_name.push_str("-symbolic");
            debug!("icon: {}", icon_name);

            let entry_text = format!(
                "<b>{}</b>\n<span size=\"smaller\">{}</span>",
                script.metadata().name.to_string(),
                script.metadata().description.to_string()
            );

            let values: [&dyn ToValue; 3] = [&icon_name, &entry_text, &script.id];
            model.set(&model.append(), &COLUMNS, &values);
        }

        // reset selection to first row
        dialog_tree_view.set_cursor(&TreePath::new_first(), gtk::NONE_TREE_VIEW_COLUMN, false);

        dialog_tree_view.show_all();
    }
}

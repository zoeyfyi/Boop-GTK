use gdk::EventKey;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Dialog, Entry, TreeView, Window};
use shrinkwraprs::Shrinkwrap;
use sublime_fuzzy::FuzzySearch;

use crate::script::Script;
use crate::SEARCH_CONFIG;

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

        command_pallete_dialog.dialog.set_transient_for(Some(window));

        // create list store
        let store = gtk::ListStore::new(&[glib::Type::String, glib::Type::U64]);
        
        let renderer = gtk::CellRendererText::new();
        let column = gtk::TreeViewColumn::new();
        column.pack_start(&renderer, true);
        column.add_attribute(&renderer, "text", 0);
        command_pallete_dialog.dialog_tree_view.append_column(&column);

        for (script_id, script) in &scripts {
            let values: [&dyn ToValue; 2] = [&script.metadata().name.to_string(), script_id];
            store.set(&store.append(), &[0, 1], &values);
        }

        command_pallete_dialog.dialog_tree_view.set_model(Some(&store));


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

        if key == gdk::enums::key::Up || key == gdk::enums::key::Down {
            if let (Some(mut path), _) = dialog_tree_view.get_cursor() {
                let index: i32 = path.get_indices()[0];

                match key {
                    gdk::enums::key::Up => {
                        if index == 0 {
                            path = gtk::TreePath::new_from_indicesv(&[result_count - 1]);
                        } else {
                            path.prev();
                        }
                    }
                    gdk::enums::key::Down => {
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
        } else if key == gdk::enums::key::Return {
            CommandPalleteDialog::on_click(dialog_tree_view, dialog);
        } else if key == gdk::enums::key::Escape {
            dialog.destroy();
        }

        Inhibit(false)
    }

    fn on_click(dialog_tree_view: &TreeView, dialog: &Dialog) {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();

        if let (Some(path), _) = dialog_tree_view.get_cursor() {
            let value = model.get_value(&model.get_iter(&path).unwrap(), 1);

            let v = value.downcast::<u64>().unwrap().get().unwrap();

            println!("value is {:?}", v);

            dialog.response(gtk::ResponseType::Other(v as u16));
        }
    }

    fn on_changed(searchbar: &Entry, dialog_tree_view: &TreeView, scripts: &Vec<(u64, Script)>) {
        let model: gtk::ListStore = dialog_tree_view.get_model().unwrap().downcast().unwrap();
        model.clear();

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
                .map(|(script_id, script)| {
                    let mut search =
                        FuzzySearch::new(&searchbar_text, &script.metadata().name, true);
                    search.set_score_config(SEARCH_CONFIG);

                    let score = search.best_match().map(|m| m.score()).unwrap_or(0);
                    (script_id, script.clone(), score)
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
            let values: [&dyn ToValue; 2] = [&script.metadata().name.to_string(), script_id];
            model.set(&model.append(), &[0, 1], &values);
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

use gdk::{keys, EventKey};
use gio::prelude::*;
use gladis::Gladis;
use gtk::prelude::*;
use gtk::{Dialog, Entry, TreePath, TreeView, Window};
use shrinkwraprs::Shrinkwrap;
use sublime_fuzzy::FuzzySearch;

use crate::{executor::Executor, script::Script, SEARCH_CONFIG};
use glib::Type;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, RwLock},
};

const ICON_COLUMN: u32 = 0;
const TEXT_COLUMN: u32 = 1;
const ID_COLUMN: u32 = 2;
const SCORE_COLUMN: u32 = 3;
const VISIBLE_COLUMN: u32 = 4;

const COLUMNS: [u32; 5] = [
    ICON_COLUMN,
    TEXT_COLUMN,
    ID_COLUMN,
    SCORE_COLUMN,
    VISIBLE_COLUMN,
];
const COLUMN_TYPES: [Type; 5] = [Type::String, Type::String, Type::U64, Type::I64, Type::Bool];

const DIALOG_WIDTH: i32 = 300;
const ICON_COLUMN_PADDING: i32 = 8;
const ICON_COLUMN_WIDTH: i32 = ICON_COLUMN_PADDING + 32 + ICON_COLUMN_PADDING; // IconSize::Dnd = 32
const TEXT_COLUMN_WIDTH: i32 = DIALOG_WIDTH - ICON_COLUMN_WIDTH;

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

    scripts: Arc<RwLock<Vec<Script>>>,
}

impl CommandPalleteDialog {
    pub fn new<P: IsA<Window>>(window: &P, scripts: Arc<RwLock<Vec<Script>>>) -> Self {
        let widgets =
            CommandPalleteDialogWidgets::from_string(include_str!("../ui/command-pallete.glade"))
                .unwrap();

        let command_pallete_dialog = CommandPalleteDialog {
            widgets,
            scripts: scripts.clone(),
        };

        command_pallete_dialog.set_transient_for(Some(window));

        // create list store
        {
            let store = gtk::ListStore::new(&COLUMN_TYPES);

            store.set_sort_column_id(
                gtk::SortColumn::Index(SCORE_COLUMN),
                gtk::SortType::Descending,
            );

            let filtered_store = gtk::TreeModelFilter::new(&store, None);
            filtered_store.set_visible_column(VISIBLE_COLUMN as i32);

            // icon column
            {
                let renderer = gtk::CellRendererPixbuf::new();
                renderer.set_padding(ICON_COLUMN_PADDING, ICON_COLUMN_PADDING);
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
                renderer.set_property_wrap_width(TEXT_COLUMN_WIDTH - 8); // -8 to account for padding

                let column = gtk::TreeViewColumn::new();
                column.pack_start(&renderer, true);
                column.set_sizing(gtk::TreeViewColumnSizing::Autosize);
                column.set_max_width(TEXT_COLUMN_WIDTH);
                column.add_attribute(&renderer, "markup", TEXT_COLUMN as i32);

                command_pallete_dialog
                    .dialog_tree_view
                    .append_column(&column);
            }

            #[cfg(debug_assertions)]
            {
                for c in &[ID_COLUMN, SCORE_COLUMN] {
                    let renderer = gtk::CellRendererText::new();

                    let column = gtk::TreeViewColumn::new();
                    column.pack_start(&renderer, false);
                    column.add_attribute(&renderer, "markup", *c as i32);

                    command_pallete_dialog
                        .dialog_tree_view
                        .append_column(&column);
                }
            }

            for (index, script) in scripts.read().unwrap().iter().enumerate() {
                let mut icon_name = script.metadata.icon.to_lowercase();
                icon_name.insert_str(0, "boop-gtk-");
                icon_name.push_str("-symbolic");

                let entry_text = format!(
                    "<b>{}</b>\n<span size=\"smaller\">{}</span>",
                    script.metadata.name.to_string(),
                    script.metadata.description.to_string()
                );

                let values: [&dyn ToValue; 5] = [
                    &icon_name,
                    &entry_text,
                    &(index as u64),
                    &(-(index as i64)),
                    &true,
                ];
                store.set(&store.append(), &COLUMNS, &values);
            }

            command_pallete_dialog
                .dialog_tree_view
                .set_model(Some(&filtered_store));
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
        let model: gtk::TreeModelFilter = dialog_tree_view.get_model().unwrap().downcast().unwrap();
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
        let model: gtk::TreeModelFilter = dialog_tree_view.get_model().unwrap().downcast().unwrap();

        if let (Some(path), _) = dialog_tree_view.get_cursor() {
            let value = model.get_value(&model.get_iter(&path).unwrap(), ID_COLUMN as i32);
            let v = value.downcast::<u64>().unwrap().get().unwrap();
            dialog.response(gtk::ResponseType::Other(v as u16));
        }
    }

    fn on_changed(
        searchbar: &Entry,
        dialog_tree_view: &TreeView,
        scripts: Arc<RwLock<Vec<Script>>>,
    ) {
        let filter_store: gtk::TreeModelFilter =
            dialog_tree_view.get_model().unwrap().downcast().unwrap();
        let store: gtk::ListStore = filter_store.get_model().unwrap().downcast().unwrap();

        // stop sorting
        // otherwise updating rows will trigger a sort making iterating over all rows difficult
        store.set_unsorted();

        let searchbar_text = searchbar.get_text().to_owned();

        // score each script using search text
        let script_to_score = scripts
            .read()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, script)| {
                let mut search = FuzzySearch::new(&searchbar_text, &script.metadata.name, true);
                search.set_score_config(SEARCH_CONFIG);

                let score = search.best_match().map(|m| m.score()).unwrap_or(-1000);
                (index as u64, score)
            })
            .collect::<HashMap<u64, isize>>();

        let script_count = store.iter_n_children(None);
        for i in 0..script_count {
            let mut path = gtk::TreePath::new();
            path.append_index(i);

            let iter = store.get_iter(&path).unwrap();

            let script_id: u64 = store
                .get_value(&iter, ID_COLUMN as i32)
                .get()
                .unwrap()
                .unwrap();

            let score = if searchbar_text.is_empty() {
                -(script_id as i64) // alphabetical sort
            } else {
                script_to_score[&script_id] as i64
            };

            let is_visible = if searchbar_text.is_empty() {
                true
            } else {
                score > 0
            };

            let values: [&dyn ToValue; 2] = [&score, &is_visible];
            store.set(&iter, &[SCORE_COLUMN, VISIBLE_COLUMN], &values);
        }

        // start sorting again
        store.set_sort_column_id(
            gtk::SortColumn::Index(SCORE_COLUMN),
            gtk::SortType::Descending,
        );

        // reset selection to first row
        dialog_tree_view.set_cursor(&TreePath::new_first(), gtk::NONE_TREE_VIEW_COLUMN, false);
    }
}

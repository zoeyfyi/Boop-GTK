use eyre::{Context, ContextCompat, Result};
use fuse_rust::Fuse;
use gdk::{keys, EventKey};
use gio::prelude::*;
use gladis::Gladis;
use glib::Type;
use gtk::prelude::*;
use gtk::{Dialog, Entry, TreePath, TreeView, Window};
use once_cell::unsync::OnceCell;
use shrinkwraprs::Shrinkwrap;

use crate::{script::Script, scriptmap::ScriptMap};

use std::{
    collections::HashMap,
    rc::Rc,
    sync::{Arc, RwLock},
};

const ICON_COLUMN: u32 = 0;
const TEXT_COLUMN: u32 = 1;
const NAME_COLUMN: u32 = 2;
const SCORE_COLUMN: u32 = 3;
const VISIBLE_COLUMN: u32 = 4;

const COLUMNS: [u32; 5] = [
    ICON_COLUMN,
    TEXT_COLUMN,
    NAME_COLUMN,
    SCORE_COLUMN,
    VISIBLE_COLUMN,
];
const COLUMN_TYPES: [Type; 5] = [
    Type::String,
    Type::String,
    Type::String,
    Type::F64,
    Type::Bool,
];

const DIALOG_WIDTH: i32 = 300;
const ICON_COLUMN_PADDING: i32 = 8;
const ICON_COLUMN_WIDTH: i32 = ICON_COLUMN_PADDING + 32 + ICON_COLUMN_PADDING; // IconSize::Dnd = 32
const TEXT_COLUMN_WIDTH: i32 = DIALOG_WIDTH - ICON_COLUMN_WIDTH;

#[derive(Shrinkwrap, Gladis)]
pub struct CommandPaletteDialogWidgets {
    #[shrinkwrap(main_field)]
    dialog: Dialog,
    dialog_tree_view: TreeView,
    search_bar: Entry,
}

#[derive(Shrinkwrap)]
pub struct CommandPaletteDialog {
    #[shrinkwrap(main_field)]
    widgets: CommandPaletteDialogWidgets,

    scripts: Arc<RwLock<ScriptMap>>,
    selected_script: Rc<OnceCell<String>>,
}

impl CommandPaletteDialog {
    pub(crate) fn new<P: IsA<Window>>(window: &P, scripts: Arc<RwLock<ScriptMap>>) -> Result<Self> {
        let widgets =
            CommandPaletteDialogWidgets::from_resource("/fyi/zoey/Boop-GTK/command-palette.glade")
                .wrap_err("Failed to load command-palette.glade")?;

        let command_palette_dialog = CommandPaletteDialog {
            widgets,
            scripts: scripts.clone(),
            selected_script: Rc::new(OnceCell::new()),
        };

        command_palette_dialog.set_transient_for(Some(window));

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

                command_palette_dialog
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

                command_palette_dialog
                    .dialog_tree_view
                    .append_column(&column);
            }

            #[cfg(debug_assertions)]
            {
                for c in &[NAME_COLUMN, SCORE_COLUMN] {
                    let renderer = gtk::CellRendererText::new();

                    let column = gtk::TreeViewColumn::new();
                    column.pack_start(&renderer, false);
                    column.add_attribute(&renderer, "markup", *c as i32);

                    command_palette_dialog
                        .dialog_tree_view
                        .append_column(&column);
                }
            }

            for (index, (name, script)) in scripts
                .read()
                .expect("scripts lock is poisoned")
                .0
                .iter()
                .enumerate()
            {
                let mut icon_name = script.metadata.icon.to_lowercase();
                icon_name.insert_str(0, "boop-gtk-");
                icon_name.push_str("-symbolic");

                let entry_text = format!(
                    "<b>{}</b>\n<span size=\"smaller\">{}</span>",
                    script.metadata.name.to_string(),
                    script.metadata.description.to_string()
                );

                let values: [&dyn ToValue; 5] =
                    [&icon_name, &entry_text, &name, &(-(index as i64)), &true];
                store.set(&store.append(), &COLUMNS, &values);
            }

            command_palette_dialog
                .dialog_tree_view
                .set_model(Some(&filtered_store));
        }

        // select first row
        command_palette_dialog.dialog_tree_view.set_cursor(
            &TreePath::new_first(),
            gtk::NONE_TREE_VIEW_COLUMN,
            false,
        );

        command_palette_dialog.register_handlers();
        Ok(command_palette_dialog)
    }

    pub(crate) fn get_selected(&self) -> Option<&String> {
        self.selected_script.get()
    }

    fn register_handlers(&self) {
        {
            let lb = self.dialog_tree_view.clone();
            let dialog = self.dialog.clone();
            let selected = self.selected_script.clone();

            self.dialog.connect_key_press_event(move |_, k| {
                CommandPaletteDialog::on_key_press(k, &lb, &dialog, &selected)
                    .expect("On key press handler failed")
            });
        }

        {
            let lb = self.dialog_tree_view.clone();
            let scripts = self.scripts.clone();
            self.search_bar.connect_changed(move |s| {
                CommandPaletteDialog::on_changed(s, &lb, scripts.clone())
                    .expect("On change handler failed")
            });
        }

        {
            let dialog = self.dialog.clone();
            let selected = self.selected_script.clone();
            self.dialog_tree_view
                .connect_row_activated(move |tv, _, _| {
                    CommandPaletteDialog::on_click(tv, &dialog, &selected)
                        .expect("On click handler failed")
                });
        }
    }

    fn on_key_press(
        key: &EventKey,
        dialog_tree_view: &TreeView,
        dialog: &Dialog,
        selected: &OnceCell<String>,
    ) -> Result<Inhibit> {
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

            return Ok(Inhibit(true));
        } else if key == keys::constants::Return || key == keys::constants::KP_Enter {
            CommandPaletteDialog::on_click(dialog_tree_view, dialog, selected)?;
        } else if key == keys::constants::Escape {
            dialog.close();
        }

        Ok(Inhibit(false))
    }

    fn on_click(
        dialog_tree_view: &TreeView,
        dialog: &Dialog,
        selected: &OnceCell<String>,
    ) -> Result<()> {
        let model: gtk::TreeModelFilter = dialog_tree_view.get_model().unwrap().downcast().unwrap();

        if let (Some(path), _) = dialog_tree_view.get_cursor() {
            let value = model.get_value(
                &model
                    .get_iter(&path)
                    .wrap_err_with(|| format!("failed to get iter for path: {:?}", path))?,
                NAME_COLUMN as i32,
            );

            let value_string = value
                .downcast::<String>()
                .map_err(|value| eyre!("Value: {:?}", value))
                .wrap_err("Cannot downcast value to String")?
                .get();

            if let Some(v) = value_string {
                debug!("v: {}", v);
                selected.set(v).unwrap();
                debug!("selected: {:?}", selected.get());
            }

            dialog.response(gtk::ResponseType::Accept);
        }

        Ok(())
    }

    fn on_changed(
        searchbar: &Entry,
        dialog_tree_view: &TreeView,
        scripts: Arc<RwLock<ScriptMap>>,
    ) -> Result<()> {
        let filter_store: gtk::TreeModelFilter =
            dialog_tree_view.get_model().unwrap().downcast().unwrap();
        let store: gtk::ListStore = filter_store.get_model().unwrap().downcast().unwrap();

        // stop sorting
        // otherwise updating rows will trigger a sort making iterating over all rows difficult
        // TODO: is it better to just rebuild the list?
        store.set_unsorted();

        let searchbar_text = searchbar.get_text().to_owned();
        let script_count = store.iter_n_children(None);
        let scripts_ref = scripts.read().expect("scripts lock is poisoned");
        let script_vec = scripts_ref.0.values().collect::<Vec<&Script>>();

        if searchbar_text.is_empty() {
            let script_order: HashMap<String, usize> = scripts_ref
                .0
                .iter()
                .enumerate()
                .map(|(idx, (name, _))| (name.clone(), idx))
                .collect();

            for i in 0..script_count {
                let mut path = gtk::TreePath::new();
                path.append_index(i);

                let iter = store
                    .get_iter(&path)
                    .ok_or_else(|| eyre!("failed to get iter for path: {:?}", path))?;

                // TODO: use gtk_liststore_item crate
                let script_name: String = store
                    .get_value(&iter, NAME_COLUMN as i32)
                    .get()
                    .unwrap()
                    .unwrap();

                let score = script_order[&script_name] as f64;
                let visible = true;

                let values: [&dyn ToValue; 2] = [&score, &visible];
                store.set(&iter, &[SCORE_COLUMN, VISIBLE_COLUMN], &values);
            }
        } else {
            let results: HashMap<String, f64> = Fuse::default()
                .search_text_in_fuse_list(&searchbar_text, &*script_vec)
                .into_iter()
                .map(|result| (script_vec[result.index].metadata.name.clone(), result.score))
                .collect();

            for i in 0..script_count {
                let mut path = gtk::TreePath::new();
                path.append_index(i);

                let iter = store
                    .get_iter(&path)
                    .wrap_err_with(|| format!("failed to get iter for path: {:?}", path))?;

                // TODO: use gtk_liststore_item crate
                let script_name: String = store
                    .get_value(&iter, NAME_COLUMN as i32)
                    .get()
                    .unwrap()
                    .unwrap();

                let score = *results.get(&script_name).unwrap_or(&0.0);
                let visible = results.contains_key(&script_name);

                let values: [&dyn ToValue; 2] = [&score, &visible];
                store.set(&iter, &[SCORE_COLUMN, VISIBLE_COLUMN], &values);
            }
        }

        // start sorting again
        store.set_sort_column_id(
            gtk::SortColumn::Index(SCORE_COLUMN),
            gtk::SortType::Ascending,
        );

        // reset selection to first row
        dialog_tree_view.set_cursor(&TreePath::new_first(), gtk::NONE_TREE_VIEW_COLUMN, false);

        Ok(())
    }
}

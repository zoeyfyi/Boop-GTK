use std::sync::{Arc, RwLock};

use eyre::{Context, Result};
use gladis::Gladis;
use gtk::prelude::*;

use crate::scriptmap::ScriptMap;

#[derive(Gladis, Clone, Shrinkwrap)]
pub struct AboutDialog {
    #[shrinkwrap(main_field)]
    about_dialog: gtk::AboutDialog,
}

impl AboutDialog {
    pub(crate) fn new(scripts: Arc<RwLock<ScriptMap>>) -> Result<Self> {
        let dialog = AboutDialog::from_resource("/fyi/zoey/Boop-GTK/boop-gtk.glade")
            .wrap_err("Failed to load boop-gtk.glade")?;

        dialog.set_version(Some(env!("CARGO_PKG_VERSION")));

        for (_, script) in scripts.read().expect("Scripts lock is poisoned").0.iter() {
            if let Some(author) = &script.metadata.author {
                dialog.add_credit_section(&format!("{} script", &script.metadata.name), &[author]);
            }
        }

        Ok(dialog)
    }
}

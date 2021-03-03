use std::sync::{Arc, RwLock};

use eyre::{Context, Result};
use gladis::Gladis;
use glib::SignalHandlerId;
use gtk::{prelude::*, Dialog, Switch};
use sourceview::{StyleScheme, StyleSchemeChooserExt, StyleSchemeExt, StyleSchemeManagerExt};

use crate::config::Config;

#[derive(Gladis, Clone, Shrinkwrap)]
pub struct PreferencesDialogWidgets {
    #[shrinkwrap(main_field)]
    preference_dialog: Dialog, // TODO: change to preferences_dialog

    color_scheme_button: sourceview::StyleSchemeChooserButton,
    shortcut_switch: Switch,
}

#[derive(Clone, Shrinkwrap)]
pub struct PreferencesDialog {
    #[shrinkwrap(main_field)]
    widgets: PreferencesDialogWidgets,
    config: Arc<RwLock<Config>>,
}

impl PreferencesDialog {
    pub(crate) fn new(config: Arc<RwLock<Config>>) -> Result<Self> {
        let mut dialog = PreferencesDialog {
            widgets: PreferencesDialogWidgets::from_resource("/fyi/zoey/Boop-GTK/boop-gtk.glade")
                .wrap_err("Failed to load boop-gtk.glade")?,
            config: config.clone(),
        };

        dialog.update_state_from_config()?;
        dialog.connect_config_style_scheme_notify(
            PreferencesDialog::on_config_style_scheme_notify(config.clone()),
        );
        dialog.connect_config_open_shortcuts_on_startup_notify(
            PreferencesDialog::on_config_open_shortcuts_on_startup_notify(config.clone()),
        );

        Ok(dialog)
    }

    // update the controls with values from config
    pub fn update_state_from_config(&mut self) -> Result<()> {
        let config = self
            .config
            .read()
            .map_err(|e| eyre!("Config lock poisoned: {}", e))?;

        // update color_scheme_button
        let scheme_id = &config.editor.colour_scheme_id;
        let scheme = sourceview::StyleSchemeManager::get_default()
            .ok_or_else(|| eyre!("Failed to get default style scheme manager"))?
            .get_scheme(scheme_id)
            .ok_or_else(|| eyre!("StyleSchemeManager could not find scheme '{}'", scheme_id))?;
        self.color_scheme_button.set_style_scheme(&scheme);

        // update shortcut_switch
        self.shortcut_switch
            .set_state(config.show_shortcuts_on_open);

        Ok(())
    }

    fn on_config_style_scheme_notify(config: Arc<RwLock<Config>>) -> impl Fn(Option<StyleScheme>) {
        move |scheme: Option<StyleScheme>| {
            if let Some(scheme_id) = scheme.and_then(|s| s.get_id()) {
                let mut config = config.write().expect("Config lock poisoned");
                config.editor.set_colour_scheme_id(scheme_id.as_str());
                config.save().expect("Failed to save config");
            } else {
                error!("Style scheme is None");
            }
        }
    }

    fn on_config_open_shortcuts_on_startup_notify(
        config: Arc<RwLock<Config>>,
    ) -> impl Fn(bool) -> Inhibit {
        move |enabled| {
            let mut config = config.write().expect("Config lock poisoned");
            config.set_show_shortcuts_on_open(enabled);
            config.save().expect("Failed to save config");

            Inhibit(false)
        }
    }

    pub fn connect_config_style_scheme_notify<F: Fn(Option<StyleScheme>) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.color_scheme_button
            .connect_property_style_scheme_notify(move |button| f(button.get_style_scheme()))
    }

    pub fn connect_config_open_shortcuts_on_startup_notify<F: Fn(bool) -> Inhibit + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.shortcut_switch
            .connect_state_set(move |_, state| f(state))
    }
}

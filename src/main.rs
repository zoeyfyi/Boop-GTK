#![forbid(unsafe_code)]

use eyre::Result;
use gtk::{
    gio::{prelude::*, Menu, MenuItem, SimpleAction},
    prelude::*,
    Align, Application, ApplicationWindow, Box, Button, Dialog, HeaderBar, Label, MenuButton,
    Overlay, PopoverMenu, Revealer, ScrolledWindow, SearchEntry, TextBuffer, TreeView,
};

const APP_ID: &str = "fyi.zoey.Boop-GTK";
const APP_NAME: &str = "Boop";

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(on_activate);
    app.run();

    Ok(())
}

fn on_activate(app: &Application) {
    let window = build_ui(app);
    register_actions(app, &window);
    window.show();
}

fn build_ui(app: &Application) -> ApplicationWindow {
    let command_palette_button = Button::builder().label("Open Command Palette...").build();
    let script_actions_menu = Menu::new();
    script_actions_menu.append_item(&MenuItem::new(
        Some("Re-execute Last Script"),
        Some("app.re_execute_last_script"),
    ));
    script_actions_menu.append_item(&MenuItem::new(
        Some("Reset Scripts"),
        Some("app.reset_scripts"),
    ));

    let navigation_actions_menu = Menu::new();
    navigation_actions_menu.append_item(&MenuItem::new(
        Some("Preferences..."),
        Some("app.open_preferences"),
    ));
    navigation_actions_menu.append_item(&MenuItem::new(
        Some("Open Config Directory"),
        Some("app.open_config_dir"),
    ));
    navigation_actions_menu.append_item(&MenuItem::new(
        Some("Get More Scripts"),
        Some("app.open_more_scripts"),
    ));
    navigation_actions_menu.append_item(&MenuItem::new(
        Some("Shortcuts"),
        Some("app.open_shortcuts"),
    ));
    navigation_actions_menu.append_item(&MenuItem::new(Some("About"), Some("app.open_about")));

    let aux_actions_menu = Menu::new();
    aux_actions_menu.append_item(&MenuItem::new(Some("Quit"), Some("app.quit")));

    let main_menu = Menu::new();
    main_menu.append_section(None, &script_actions_menu);
    main_menu.append_section(None, &navigation_actions_menu);
    main_menu.append_section(None, &aux_actions_menu);
    
    let main_popover_menu = PopoverMenu::from_model(Some(&main_menu));
    let main_menu_button = MenuButton::builder()
        .popover(&main_popover_menu)
        .icon_name("open-menu-symbolic")
        .build();
    let header_bar = HeaderBar::builder()
        .title_widget(&command_palette_button)
        .build();
    header_bar.pack_end(&main_menu_button);
    let notification_label = Label::builder()
        .label("Notification text")
        .wrap(true)
        .build();
    let notification_button = Button::builder()
        .icon_name("window-close-symbolic")
        .has_frame(false)
        .build();
    let notification_box = Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .hexpand(false)
        .receives_default(true)
        .css_classes(vec!["app-notification".to_string()])
        .build();
    notification_box.append(&notification_label);
    notification_box.append(&notification_button);
    let revealer = Revealer::builder()
        .child(&notification_box)
        .reveal_child(true)
        .valign(Align::Start)
        .halign(Align::Center)
        .build();
    let source = sourceview5::View::builder()
        .show_line_numbers(true)
        .show_line_marks(true)
        .tab_width(4)
        .indent_on_tab(true)
        .monospace(true)
        .build();
    let scrolled_window = ScrolledWindow::builder().child(&source).build();
    let overlay = Overlay::builder().child(&scrolled_window).build();
    overlay.add_overlay(&revealer);
    let window = ApplicationWindow::builder()
        .application(app)
        .title(APP_NAME)
        .default_width(600)
        .default_height(400)
        .child(&overlay)
        .build();
    window.set_titlebar(Some(&header_bar));
    window
}

fn build_command_palette_ui(window: &ApplicationWindow) -> Dialog {
    let search_bar = SearchEntry::builder().hexpand(true).build();

    let header_bar = HeaderBar::builder()
        .title_widget(&search_bar)
        .show_title_buttons(false)
        .build();

    let results_tree_view = TreeView::builder().build();

    let dialog = Dialog::builder()
        .child(&results_tree_view)
        .transient_for(window)
        .resizable(false)
        .modal(true)
        .default_width(300)
        .default_height(300)
        .title("test")
        .use_header_bar(1)
        .decorated(true)
        .destroy_with_parent(true)
        .build();
    dialog.set_titlebar(Some(&header_bar));
    // let header_bar = dialog.header_bar();
    // header_bar.set_title_widget(Some(&search_bar));

    // dialog.set_header_bar(Some(&header_bar));

    dialog
}

fn register_actions(app: &Application, window: &ApplicationWindow) {
    let re_execute_last_script_action = SimpleAction::new("re_execute_last_script", None);
    re_execute_last_script_action.connect_activate(move |_, _| {
        println!("Re-execute last script");
    });
    app.add_action(&re_execute_last_script_action);
    app.set_accels_for_action("app.re_execute_last_script", &["<Primary><Shift>B"]);

    let reset_scripts_action = SimpleAction::new("reset_scripts", None);
    reset_scripts_action.connect_activate(move |_, _| {
        println!("Reset scripts");
    });
    app.add_action(&reset_scripts_action);

    let open_preferences_action = SimpleAction::new("open_preferences", None);
    open_preferences_action.connect_activate(move |_, _| {
        println!("Open preferences");
    });
    app.add_action(&open_preferences_action);

    let open_config_dir_action = SimpleAction::new("open_config_dir", None);
    open_config_dir_action.connect_activate(move |_, _| {
        println!("Open config dir");
    });
    app.add_action(&open_config_dir_action);

    let open_more_scripts_action = SimpleAction::new("open_more_scripts", None);
    open_more_scripts_action.connect_activate(move |_, _| {
        println!("Open more scripts");
    });
    app.add_action(&open_more_scripts_action);

    let open_shortcuts_action = SimpleAction::new("open_shortcuts", None);
    open_shortcuts_action.connect_activate(move |_, _| {
        println!("Open shortcuts");
    });
    app.add_action(&open_shortcuts_action);

    let open_about_action = SimpleAction::new("open_about", None);
    open_about_action.connect_activate(move |_, _| {
        println!("Open about");
    });
    app.add_action(&open_about_action);

    let window_ = window.clone();
    let open_command_pallete_action = SimpleAction::new("open_command_palette", None);
    open_command_pallete_action.connect_activate(move |_, _| {
        println!("Open command palette");
        let dialog = build_command_palette_ui(&window_);
        dialog.show();
    });
    app.add_action(&open_command_pallete_action);
    app.set_accels_for_action("app.open_command_palette", &["<Primary><Shift>P"]);

    let app_ = app.clone();
    let quit_action = SimpleAction::new("quit", None);
    quit_action.connect_activate(move |_, _| {
        app_.quit();
    });
    app.add_action(&quit_action);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);
}

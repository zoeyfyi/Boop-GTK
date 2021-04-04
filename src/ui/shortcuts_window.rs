use gtk::{ContainerExt, WidgetExt};

#[derive(Shrinkwrap)]
pub struct ShortcutsWindow {
    #[shrinkwrap(main_field)]
    window: gtk::ShortcutsWindow,
}

const GENERAL_SHORTCUTS: [(&str, &str); 3] = [
    ("Open Command Pallette", "<Primary><Shift>P"),
    ("Quit", "<Primary>Q"),
    ("Re-execute Last Script", "<Primary><Shift>B"),
];

const EDITOR_SHORTCUTS: [(&str, &str); 12] = [
    ("Undo", "<Primary>Z"),
    ("Redo", "<Primary><Shift>Z"),
    ("Move line up", "<Alt>Up"),
    ("Move line down", "<Alt>Down"),
    ("Move cursor backwards one word", "<Primary>Left"),
    ("Move cursor forward one word", "<Primary>Right"),
    ("Move cursor to beginning of previous line", "<Primary>Up"),
    ("Move cursor to end of next line", "<Primary>Down"),
    ("Move cursor to beginning of line", "<Primary>Page_Up"),
    ("Move cursor to end of line", "<Primary>Page_Down"),
    ("Move cursor to beginning of document", "<Primary>Home"),
    ("Move cursor to end of document", "<Primary>End"),
];

impl ShortcutsWindow {
    pub fn new() -> ShortcutsWindow {
        let window = gtk::ShortcutsWindowBuilder::new().build();

        let general_group = gtk::ShortcutsGroupBuilder::new().title("General").build();
        for (title, accelerator) in GENERAL_SHORTCUTS.iter() {
            general_group.add(
                &gtk::ShortcutsShortcutBuilder::new()
                    .title(title)
                    .accelerator(accelerator)
                    .visible(true)
                    .build(),
            );
        }

        let editor_group = gtk::ShortcutsGroupBuilder::new().title("Editor").build();
        for (title, accelerator) in EDITOR_SHORTCUTS.iter() {
            editor_group.add(
                &gtk::ShortcutsShortcutBuilder::new()
                    .title(title)
                    .accelerator(accelerator)
                    .visible(true)
                    .build(),
            );
        }

        let section = gtk::ShortcutsSectionBuilder::new().build();
        section.add(&general_group);
        section.add(&editor_group);
        section.show_all();
        window.add(&section);

        ShortcutsWindow { window }
    }
}

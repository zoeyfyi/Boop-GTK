extern crate gio;
extern crate gtk;
extern crate sourceview;

use gio::prelude::*;
use gtk::prelude::*;
use sourceview::prelude::*;

use gtk::{Application, ApplicationWindow};

fn create_window(app: &Application) {
    let window = ApplicationWindow::new(app);
    window.set_title("Boop");
    window.set_default_size(600, 400);

    let scroll = gtk::Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 10.0);
    let scrolled_window = gtk::ScrolledWindow::new(gtk::NONE_ADJUSTMENT, Some(&scroll));
    window.add(&scrolled_window);

    let source_view: sourceview::View = sourceview::View::new();
    source_view.set_show_line_numbers(true);
    scrolled_window.add(&source_view);

    window.show_all();
}

fn main() {
    let application = Application::new(Some("uk.co.mrbenshef.boop-gtk"), Default::default())
        .expect("failed to initialize GTK application");

    application.connect_activate(create_window);
    application.run(&[]);
}

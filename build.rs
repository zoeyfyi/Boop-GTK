use io::Write;
use std::{fs, io, process::Command};

const XML_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gresources>
    <gresource prefix="/co/uk/mrbenshef/Boop-GTK">
"#;

const XML_FOOTER: &str = r#"    </gresource>
</gresources>
"#;

fn add_files(xml: &mut String, folder: &str) {
    for path in fs::read_dir(folder).unwrap() {
        let path = path.as_ref().unwrap();

        if path.path().display().to_string().ends_with("~") {
            continue;
        }

        if path.file_type().unwrap().is_file() {
            xml.push_str(&format!(
                "\t\t<file>{}</file>\n",
                path.path()
                    .display()
                    .to_string()
                    .replace("\\", "/")
                    .trim_start_matches("resources/")
            ));
        } else if path.file_type().unwrap().is_dir() {
            add_files(xml, &path.path().display().to_string());
        } else {
            panic!("expected file or folder");
        }
    }
}

fn main() {
    let mut xml = String::with_capacity(XML_HEADER.len() + XML_FOOTER.len() + 1024);

    xml.push_str(XML_HEADER);
    add_files(&mut xml, "resources");
    xml.push_str(XML_FOOTER);

    let mut file = fs::File::create("resources/resources.xml").unwrap();
    file.write_all(xml.as_bytes()).unwrap();

    let mut cmd = if cfg!(target_os = "windows") {
        Command::new("glib-compile-resources.exe")
    } else {
        Command::new("glib-compile-resources")
    };

    cmd.arg("resources.xml")
        .current_dir("resources")
        .output()
        .expect("failed to compile resources");
}

extern crate fs_extra;
#[cfg(windows)]
extern crate winres;

use io::Write;
use std::{env, fs, io, path::Path, process::Command};

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
    let out_dir = env::var("OUT_DIR").unwrap();
    let mut resources = Path::new(&out_dir).to_path_buf();
    resources.push("resources");

    fs::create_dir_all(resources.clone()).unwrap();
    fs_extra::dir::copy("resources", out_dir, &{
        let mut options = fs_extra::dir::CopyOptions::new();
        options.copy_inside = true;
        options.overwrite = true;
        options
    })
    .unwrap();

    let mut xml = String::with_capacity(XML_HEADER.len() + XML_FOOTER.len() + 1024);

    xml.push_str(XML_HEADER);
    add_files(&mut xml, "resources");
    xml.push_str(XML_FOOTER);

    let resource_xml = {
        let mut f = resources.clone();
        f.push("resources.xml");
        f
    };
    let mut file = fs::File::create(resource_xml).unwrap();
    file.write_all(xml.as_bytes()).unwrap();

    let mut cmd = if cfg!(target_os = "windows") {
        Command::new("glib-compile-resources.exe")
    } else {
        Command::new("glib-compile-resources")
    };

    cmd.arg("resources.xml")
        .current_dir(resources)
        .output()
        .expect("failed to compile resources");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("wix/boop-gtk.ico");
        res.compile().unwrap();
    }
}

use core::fmt;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rust_embed::RustEmbed;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use crate::script::Script;
use crate::PROJECT_DIRS;

pub(crate) struct ScriptMap(pub BTreeMap<String, Script>);

#[derive(RustEmbed)]
#[folder = "submodules/Boop/Boop/Boop/scripts/"]
pub(crate) struct Scripts;

impl ScriptMap {
    pub(crate) fn new() -> Self {
        let mut scripts = ScriptMap(BTreeMap::new());

        scripts.load_internal();

        // TODO: use directories or one of its forks once this functionality is implemented
        // TODO: add MacOS/Windows support
        if cfg!(target_os = "linux") {
            let env_var = std::env::var("XDG_CONFIG_DIRS")
                .ok()
                .filter(|value| value.is_empty());

            match env_var {
                Some(dirs) => {
                    // $XDG_CONFIG_DIRS is a ":" seperated list of directories
                    for dir in dirs.split(":") {
                        let path_str = format!("{}/boop-gtk/scripts", dir);
                        let path = Path::new(&path_str);
                        if std::fs::read_dir(path).is_ok() {
                            // load scripts (overrides any internal scripts)
                            scripts.load_path(path);
                        }
                    }
                }
                None => {
                    warn!("$XDG_CONFIG_DIRS is not set, defaulting to /etc/xdg");
                    // default for $XDG_CONFIG_DIRS is /etc/xdg
                    // https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
                    let path = Path::new("/etc/xdg/boop-gtk/scripts");
                    if std::fs::read_dir(path).is_ok() {
                        scripts.load_path(path);
                    }
                }
            }
        }

        // load user scripts overriding internal and global scripts
        scripts.load_path(&ScriptMap::user_scripts_dir());

        scripts
    }

    fn user_scripts_dir() -> PathBuf {
        let mut dir = PROJECT_DIRS.config_dir().to_path_buf();
        dir.push("scripts");
        dir
    }

    // load scripts included in the binary
    fn load_internal(&mut self) {
        for file in Scripts::iter() {
            let file: Cow<'_, str> = file;
            // scripts are internal, so we can unwrap "safely"
            let source: Cow<'static, [u8]> = Scripts::get(&file)
                .unwrap_or_else(|| panic!("failed to get file: {}", file.to_string()));
            let script_source = String::from_utf8(source.to_vec())
                .unwrap_or_else(|e| panic!("{} is not UTF8: {}", file, e));
            if let Ok(script) = Script::from_source(script_source, PathBuf::new()) {
                self.0.insert(script.metadata.name.clone(), script);
            }
        }

        info!("loaded {} internal scripts", Scripts::iter().count());
    }

    // load scripts from a path
    fn load_path(&mut self, dir: &Path) -> Result<(), LoadScriptError> {
        let paths = fs::read_dir(dir).map_err(|_| LoadScriptError::FailedToReadScriptDirectory)?;

        let scripts: HashMap<String, Script> = paths
            .filter_map(Result::ok)
            .map(|f| f.path())
            .filter(|path| path.is_file())
            .map(Script::from_file)
            .filter_map(Result::ok)
            .map(|script| (script.metadata.name.clone(), script))
            .collect();

        info!("loaded {} scripts from {}", scripts.len(), dir.display());

        self.0.extend(scripts);

        Ok(())
    }

    pub(crate) fn watch(scripts: Arc<RwLock<Self>>) {
        trace!("watch_scripts_folder");

        // watch for changes to script folder
        let watcher: notify::Result<RecommendedWatcher> = Watcher::new_immediate(move |res| {
            debug!("res: {:?}", res);
            match res {
                Ok(event) => {
                    let event: notify::Event = event;

                    for file in event.paths {
                        debug!("file: {}", file.display());

                        if file.extension().filter(|&s| s == "js").is_none() {
                            break;
                        }

                        info!("{} changed, reloading", file.display());

                        let mut scripts = scripts.write().expect("script lock is poisoned");

                        // remove script
                        scripts.0.drain_filter(|_, script| script.path == file);

                        if !file.exists() {
                            // file was deleted
                            break;
                        }

                        match Script::from_file(file.clone()) {
                            Ok(script) => {
                                // file added or changed
                                scripts.0.insert(script.metadata.name.clone(), script);
                            }
                            Err(e) => {
                                warn!("error parsing {}: {}", file.display(), e);
                            }
                        }
                    }
                }
                Err(e) => error!("watch error: {:?}", e),
            }
        });

        // configure and start watcher
        match watcher {
            Ok(mut watcher) => {
                let script_dir = &ScriptMap::user_scripts_dir();

                info!("watching {}", script_dir.display());

                if let Err(watch_error) = watcher.watch(&script_dir, RecursiveMode::Recursive) {
                    error!("watch start error: {}", watch_error);
                    return;
                }

                // keep the thread alive
                loop {
                    trace!("watching!");
                    thread::sleep(Duration::from_millis(1000));
                }
            }
            Err(watcher_error) => {
                error!("couldn't create watcher: {}", watcher_error);
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum LoadScriptError {
    FailedToCreateScriptDirectory,
    FailedToReadScriptDirectory,
}

impl Display for LoadScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadScriptError::FailedToCreateScriptDirectory => {
                write!(f, "Can't create scripts directory, check your permissions")
            }
            LoadScriptError::FailedToReadScriptDirectory => {
                write!(f, "Can't read scripts directory, check your premissions")
            }
        }
    }
}

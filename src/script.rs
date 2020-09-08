use crate::executor::{ExecutionStatus, Executor, ExecutorError};
use crossbeam::crossbeam_channel::bounded;
use crossbeam::{Receiver, Sender};
use serde::Deserialize;
use std::{fmt, fs, path::PathBuf, thread};

pub struct Script {
    pub metadata: Metadata,
    pub path: PathBuf,
    source: String,
    channel: Option<ExecutorChannel>,
}
#[derive(Debug)]
enum ExecutorJob {
    Request((String, Option<String>)),
    Responce(Result<ExecutionStatus, ExecutorError>),
    Kill,
}

struct ExecutorChannel {
    sender: Sender<ExecutorJob>,
    receiver: Receiver<ExecutorJob>,
}

#[derive(Debug)]
pub enum ParseScriptError {
    NoMetadata,
    InvalidMetadata(serde_jsonrc::error::Error),
    FailedToRead(std::io::Error),
}

impl fmt::Display for ParseScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseScriptError::NoMetadata => write!(f, "no metadata"),
            ParseScriptError::InvalidMetadata(e) => write!(f, "invalid metadata: {}", e),
            ParseScriptError::FailedToRead(e) => write!(f, "failed to read script: {}", e),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    pub api: u32,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub icon: String,
    pub tags: Option<String>,
}

impl Script {
    pub fn from_file(path: PathBuf) -> Result<Self, ParseScriptError> {
        match fs::read_to_string(path.clone()) {
            Ok(source) => Script::from_source(source, path),
            Err(e) => Err(ParseScriptError::FailedToRead(e)),
        }
    }

    pub fn from_source(source: String, path: PathBuf) -> Result<Self, ParseScriptError> {
        let start = source.find("/**").ok_or(ParseScriptError::NoMetadata)?;
        let end = source.find("**/").ok_or(ParseScriptError::NoMetadata)?;

        let mut metadata: Metadata = serde_jsonrc::from_str(&source[start + 3..end])
            .map_err(ParseScriptError::InvalidMetadata)?;

        metadata.icon = metadata.icon.to_lowercase();

        Ok(Script {
            metadata,
            source,
            channel: None,
            path,
        })
    }

    fn init_executor_thread(&mut self) {
        assert!(self.channel.is_none());

        let (sender, receiver) = bounded(0);

        {
            let t_name = self.metadata.name.clone();
            let t_source = self.source.clone();
            let (t_sender, t_receiver) = (sender.clone(), receiver.clone());

            thread::spawn(move || {
                info!("thread spawned for {}", t_name);

                let executor = Executor::new(&t_source);

                if let Err(err) = executor {
                    t_sender.send(ExecutorJob::Responce(Err(err))).unwrap();
                    return;
                }

                let mut executor = executor.unwrap();

                debug!("executor created");

                loop {
                    match t_receiver.recv().unwrap() // blocks until receive 
                    {
                        ExecutorJob::Request((full_text, selection)) => {
                            info!(
                                "request received, full_text: {} bytes, selection: {} bytes",
                                full_text.len(),
                                selection.as_ref().map(|s| s.len()).unwrap_or(0),
                            );
                            let result = executor.execute(&full_text, selection.as_deref());
                            t_sender.send(ExecutorJob::Responce(result)).unwrap(); // blocks until send
                        }
                        ExecutorJob::Responce(_) => {
                            warn!("executor thread received a responce on channel");
                        }
                        ExecutorJob::Kill => {
                            info!("killing thread for {}", t_name);
                            return;
                        }
                    }
                }
            });
        };

        self.channel = Some(ExecutorChannel { sender, receiver });
    }

    // kills the thread associated with this script, it will be recreated when `execute` is called
    pub fn kill_thread(&mut self) {
        if let Some(channel) = &self.channel {
            channel.sender.send(ExecutorJob::Kill).unwrap(); // blocks until send
        }

        self.channel = None;
    }

    pub fn execute(
        &mut self,
        full_text: &str,
        selection: Option<&str>,
    ) -> Result<ExecutionStatus, ExecutorError> {
        if self.channel.is_none() {
            self.init_executor_thread();
        }
        assert!(self.channel.is_some());

        let channel = self.channel.as_ref().expect("channel is none");

        // send request
        channel
            .sender
            .send(ExecutorJob::Request((
                full_text.to_owned(),
                selection.map(|s| s.to_owned()),
            )))
            .expect("channel is disconnected");

        // receive result
        let result = channel
            .receiver
            .recv()
            .expect("receive channel is empty and disconnected");

        if let ExecutorJob::Responce(status) = result {
            return status;
        }

        panic!(
            "expected a responce on channel, but got a request: {:?}",
            result
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{executor::TextReplacement, script::ParseScriptError};
    use std::borrow::Cow;

    #[test]
    fn test_retain_execution_context() {
        let mut script = Script::from_source(
            "
            /**
                {
                    \"api\":1,
                    \"name\":\"Counter\",
                    \"description\":\"Counts up\",
                    \"author\":\"Ben\",
                    \"icon\":\"html\",
                    \"tags\":\"count\"
                }
            **/
            
            let number = 0;
            
            function main(state) {
                number += 1;
                state.text = number;
            }"
            .to_string(),
            PathBuf::new(),
        )
        .unwrap();

        for i in 1..10 {
            let status = script.execute("", None);
            assert!(status.is_ok());
            assert_eq!(
                TextReplacement::Full(i.to_string()),
                status.unwrap().into_replacement()
            );
        }
    }

    #[test]
    fn test_is_selection() {
        let mut script = Script::from_source(
            r#"
            /**
                {
                    "api": 1,
                    "name": "Test",
                    "description": "Test script",
                    "author": "Ben",
                    "icon": "html",
                    "tags": "test"
                }
            **/
            
            let number = 0;
            
            function main(state) {
                state.fullText = state.isSelection;
            }"#
            .to_string(),
            PathBuf::new(),
        )
        .unwrap();

        let status = script.execute("", None);
        assert!(status.is_ok());
        assert_eq!(
            TextReplacement::Full("false".to_string()),
            status.unwrap().into_replacement()
        );

        let status = script.execute("foo", Some("fo"));
        assert!(status.is_ok());
        assert_eq!(
            TextReplacement::Full("true".to_string()),
            status.unwrap().into_replacement()
        );
    }

    #[test]
    fn test_builtin_scripts() {
        use rust_embed::RustEmbed;

        #[derive(RustEmbed)]
        #[folder = "submodules/Boop/Boop/Boop/scripts/"]
        struct Scripts;

        for file in Scripts::iter() {
            println!("testing {}", file);

            let source: Cow<'static, [u8]> = Scripts::get(&file).unwrap();
            let script_source = String::from_utf8(source.to_vec()).unwrap();

            let full_text = match file.as_ref() {
                "MinifyJSON.js" => "{\n\n\"foo\":\n\"bar\"}",
                "SumAll.js" => "100\n9.00\n230\n2.09",
                _ => "foobar ♈ ♉ ♊ ♋ ♌ ♍ ♎ ♏ ♐ ♑ ♒ ♓ 😁 😝 😋 😄",
            };

            match Script::from_source(script_source, PathBuf::new()) {
                Ok(mut script) => {
                    script.execute(full_text, None).unwrap();
                }
                Err(e) => match e {
                    ParseScriptError::NoMetadata => {
                        assert!(file.starts_with("lib/")); // only library files should fail
                    }
                    e => panic!(e),
                },
            }
        }
    }

    #[test]
    fn test_extra_scripts() {
        use rust_embed::RustEmbed;

        #[derive(RustEmbed)]
        #[folder = "submodules/Boop/Scripts/"]
        struct Scripts;

        for file in Scripts::iter() {
            if !file.ends_with(".js") {
                continue; // not a javascript file
            }

            println!("testing {}", file);

            let source: Cow<'static, [u8]> = Scripts::get(&file).unwrap();
            let script_source = String::from_utf8(source.to_vec()).unwrap();

            match Script::from_source(script_source, PathBuf::new()) {
                Ok(mut script) => {
                    script
                        .execute(
                            "foobar ♈ ♉ ♊ ♋ ♌ ♍ ♎ ♏ ♐ ♑ ♒ ♓ 😁 😝 😋 😄",
                            None,
                        )
                        .unwrap();
                }
                Err(e) => panic!(e),
            }
        }
    }
}

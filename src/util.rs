use std::string::FromUtf8Error;

use eyre::Result;
use glib::Cast;
use gtk::TextViewExt;

pub trait StringExt {
    fn remove_null_bytes(self) -> Result<String, FromUtf8Error>;
}

impl StringExt for String {
    fn remove_null_bytes(self) -> Result<String, FromUtf8Error> {
        String::from_utf8(
            self.into_bytes()
                .into_iter()
                .filter(|b| *b != 0)
                .collect::<Vec<u8>>(),
        )
    }
}

pub trait SourceViewExt {
    fn get_sourceview_buffer(&self) -> Result<sourceview::Buffer>;
}

impl SourceViewExt for sourceview::View {
    fn get_sourceview_buffer(&self) -> Result<sourceview::Buffer> {
        self.get_buffer()
            .ok_or_else(|| eyre!("Failed to get buffer"))?
            .downcast::<sourceview::Buffer>()
            .map_err(|_| eyre!("Failed to downcast TextBuffer to sourceview Buffer"))
    }
}

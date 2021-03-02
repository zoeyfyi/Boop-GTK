use std::string::FromUtf8Error;

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

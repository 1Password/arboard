use std::error::Error;

pub fn err(s: &str) -> Box<Error> {
    Box::<Error+Send+Sync>::from(s)
}

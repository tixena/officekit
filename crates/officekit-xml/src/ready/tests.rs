use core::future::{pending, ready};
use std::io;

use super::ready_now;

#[test]
fn a_ready_future_gives_its_output() {
    let output: Result<u8, io::Error> = ready_now(ready(Ok(7)));
    assert_eq!(output.unwrap(), 7);
}

#[test]
fn a_ready_future_gives_its_error() {
    let output: Result<u8, io::Error> =
        ready_now(ready(Err(io::Error::from(io::ErrorKind::NotFound))));
    assert_eq!(output.unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn a_future_that_waits_gives_an_error() {
    let error = ready_now(pending::<Result<u8, io::Error>>()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "the future is not ready on its first poll"
    );
}

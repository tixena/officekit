//! A file of `System::create`, whose stream is async, as the `Write` a package is streamed into.

use core::future::poll_fn;
use core::pin::Pin;
use std::io::{self, Write};

use futures_io::AsyncWrite;
use officekit_xml::ready::ready_now;

/// The stream `System::create` gave, on a system whose futures never wait.
pub struct FileSink<'system>(pub Box<dyn AsyncWrite + Send + Unpin + 'system>);

impl FileSink<'_> {
    /// Close the file, the point at which its bytes are sure to be stored.
    pub fn close(mut self) -> io::Result<()> {
        ready_now(poll_fn(|context| {
            Pin::new(&mut *self.0).poll_close(context)
        }))
    }
}

impl Write for FileSink<'_> {
    fn flush(&mut self) -> io::Result<()> {
        ready_now(poll_fn(|context| {
            Pin::new(&mut *self.0).poll_flush(context)
        }))
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        ready_now(poll_fn(|context| {
            Pin::new(&mut *self.0).poll_write(context, buf)
        }))
    }
}

//! `PartBuffer`: a part's bytes held in fixed chunks as they are written, then joined once.

use std::io::{self, Write};

#[cfg(test)]
mod tests;

/// The bytes of one chunk.
const CHUNK: usize = 64 * 1024;

/// A sink that keeps what it is given in chunks of 64 KiB, so a growing part never holds the
/// spare half a doubling vector would, and gives it back as one vector of its exact length.
#[derive(Debug, Default)]
pub struct PartBuffer {
    chunks: Vec<Vec<u8>>,
    len: usize,
}

impl PartBuffer {
    /// The bytes written, in one vector of their exact length; the last chunk gives back its
    /// spare room first, and each chunk is freed as soon as it is copied.
    #[must_use = "the bytes are returned and kept nowhere else"]
    pub fn into_bytes(mut self) -> Vec<u8> {
        if let Some(last) = self.chunks.last_mut() {
            last.shrink_to_fit();
        }
        let mut bytes = Vec::with_capacity(self.len);
        for chunk in self.chunks {
            bytes.extend_from_slice(&chunk);
        }
        bytes
    }

    /// Whether nothing was written.
    #[must_use = "this only answers the question"]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The number of bytes written.
    #[must_use = "this only returns the value"]
    pub const fn len(&self) -> usize {
        self.len
    }
}

impl Write for PartBuffer {
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut rest = buf;
        while !rest.is_empty() {
            let chunk = match self.chunks.last_mut() {
                Some(last) if last.len() < CHUNK => last,
                _ => {
                    self.chunks.push(Vec::with_capacity(CHUNK));
                    self.chunks.last_mut().ok_or(io::ErrorKind::OutOfMemory)?
                }
            };
            let (taken, left) = rest.split_at(rest.len().min(CHUNK - chunk.len()));
            chunk.extend_from_slice(taken);
            rest = left;
        }
        self.len += buf.len();
        Ok(buf.len())
    }
}

//! A future run to its end with no runtime, for a system whose futures never wait.

use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::io;

#[cfg(test)]
mod tests;

/// The output of `future`, polled once.
///
/// For synchronous code that calls an async function of officekit with a `RealSystem` or a
/// `MemorySystem` over its own bucket. Those buckets hold `Guarantee::FuturesNeverWait`: each of
/// their futures is ready on its first poll, so this needs no reactor and no timer, and it works
/// inside a runtime as well as outside one.
///
/// ```
/// use std::path::Path;
///
/// use officekit_xml::packaging::package::Package;
/// use officekit_xml::ready::ready_now;
/// use os_shim::mock::MemorySystem;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let system = MemorySystem::new().with_dir("/out")?;
/// ready_now(Package::new().save(&system, Path::new("/out/empty.zip")))?;
/// let reopened = ready_now(Package::open(&system, Path::new("/out/empty.zip")))?;
/// assert_eq!(reopened.part_names()?, []);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// The errors of `future`, and an [`io::Error`] when it is not ready on its first poll, as a
/// future of a system over a network bucket is not. Such a future needs its caller's runtime.
pub fn ready_now<F, T, E>(future: F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
    E: From<io::Error>,
{
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => Err(io::Error::other("the future is not ready on its first poll").into()),
    }
}

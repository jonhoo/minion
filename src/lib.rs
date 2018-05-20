//! This crate provides a wrapper type for making long-running service loops cancellable.
//!
//! Let's dive right in with an example. For further details see
//! [`Cancellable`](https://docs.rs/minion/*/minion/trait.Cancellable.html).
//!
//! ```
//! # use minion::*;
//! # use std::{time, thread};
//! # struct Service;
//! # impl Cancellable for Service {
//! #     type Error = ();
//! #     fn for_each(&mut self) -> Result<LoopState, ()> { Ok(LoopState::Break) }
//! # }
//! # impl Service { fn new() -> Self { Service } }
//! // impl Cancellable for Service { .. }
//! let s = Service::new();
//!
//! // start the service loop on a new thread
//! let h = s.spawn();
//!
//! // get a handle that allows cancelling the service loop
//! let exit = h.canceller();
//!
//! // spin up a new thread that will handle exit signals
//! thread::spawn(move || {
//!     // this might catch Ctrl-C from the user, wait for a particular packet,
//!     // or for any other condition that signals that the service should exit
//!     // cleanly. in this case, we just terminate after a fixed amount of time.
//!     thread::sleep(time::Duration::from_secs(1));
//!
//!     // tell the service loop to exit at the first opportunity
//!     exit.cancel();
//! });
//!
//! // block until the service loop exits or errors.
//! h.wait().unwrap();
//! ```
#![deny(missing_docs)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Indicate whether main service loop should continue accepting new work.
pub enum LoopState {
    /// Accept more work.
    Continue,
    /// Stop accepting work and return.
    Break,
}

/// A service that implements `Cancellable` can be told to stop accepting new work at any time, and
/// will return at the first following opportunity.
///
/// More concretely, it emulates a loop like the following:
///
/// ```rust,ignore
/// loop {
///     // fetch some work
///     // do some work that might error
/// }
/// ```
///
/// But where the `loop` can be "cancelled". That is, after the next piece of work is processed, no
/// more work is handled, and the loop breaks.
///
/// This trait provides two main methods, [`Cancellable::run`] and [`Cancellable::spawn`]. The
/// former runs the loop on the current thread (and thus blocks it). The latter spawns a new
/// thread, and executes the loop on that thread. Only loops started using `spawn` can be
/// cancelled.
///
/// For example, the implementation below shows how a classic server accept loop could be turned
/// into a cancellable accept loop. If [`Handle::cancel`] is called, then at most one more
/// connection will be accepted before the loop returns and [`Handle::wait`] would too.
///
/// ```no_run
/// # extern crate minion;
/// # use minion::*;
/// # use std::{
/// #     io::{self, prelude::*}, net, thread, time,
/// # };
/// struct Service(net::TcpListener);
/// impl Cancellable for Service {
///     type Error = io::Error;
///     fn for_each(&mut self) -> Result<minion::LoopState, Self::Error> {
///         let mut stream = self.0.accept()?.0;
///         write!(stream, "hello!\n")?;
///         Ok(minion::LoopState::Continue)
///     }
/// }
///
/// impl Service {
///     fn new() -> io::Result<Self> {
///         Ok(Service(net::TcpListener::bind("127.0.0.1:6556")?))
///     }
/// }
///
/// fn main() {
/// # fn foo() -> io::Result<()> {
///     Service::new()?.run()?;
/// # Ok(())
/// # }
/// # foo().unwrap();
/// }
/// ```
pub trait Cancellable {
    /// Error type for [`Cancellable::for_each`].
    type Error;

    /// This method is called once for every iteration of the loop.
    ///
    /// If it errors, the outer service loop will also return with that same error.
    /// If it returns a `LoopState`, the service loop will continue or break accordingly.
    /// If it panics, the panic will be propagated to the waiting thread.
    fn for_each(&mut self) -> Result<LoopState, Self::Error>;

    /// Continuously execute [`Cancellable::for_each`] until it returns an error or a
    /// [`LoopState::Break`].
    fn run(&mut self) -> Result<(), Self::Error> {
        loop {
            match self.for_each() {
                Ok(LoopState::Continue) => {}
                Ok(LoopState::Break) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Continuously execute [`Cancellable::for_each`] in a new thread, and return a [`Handle`] to
    /// that loop so that it can be cancelled or waited for.
    fn spawn(mut self) -> Handle<Self::Error>
    where
        Self: Sized + Send + 'static,
        Self::Error: Send + 'static,
    {
        let keep_running = Arc::new(AtomicBool::new(true));
        let jh = {
            let keep_running = keep_running.clone();
            thread::spawn(move || {
                while keep_running.load(Ordering::SeqCst) {
                    match self.for_each() {
                        Ok(LoopState::Continue) => {}
                        Ok(LoopState::Break) => break,
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            })
        };

        Handle {
            canceller: Canceller { keep_running },
            executor: jh,
        }
    }
}

/// A handle to a running service loop.
///
/// You can use it to cancel the running loop at the next opportunity (through [`Handle::cancel`]),
/// or to wait for the loop to terminate (through [`Handle::wait`]). You can also use
/// [`Handle::canceller`] to get a [`Canceller`] handle, which lets you terminate the service loop
/// elsewhere (e.g., while waiting).
pub struct Handle<E> {
    canceller: Canceller,
    executor: thread::JoinHandle<Result<(), E>>,
}

/// A handle that allows the cancellation of a running service loop.
#[derive(Clone)]
pub struct Canceller {
    keep_running: Arc<AtomicBool>,
}

impl<E> Handle<E> {
    /// Get another handle for cancelling the service loop.
    ///
    /// This can be handy if you want one thread to wait for the service loop to exit, while
    /// another watches for exit signals.
    pub fn canceller(&self) -> Canceller {
        Canceller {
            keep_running: self.keep_running.clone(),
        }
    }

    /// Wait for the service loop to exit, and return its result.
    ///
    /// If the service loop panics, this method will also panic with the same error.
    pub fn wait(self) -> Result<(), E> {
        match self.executor.join() {
            Ok(r) => r,
            Err(e) => {
                // propagate the panic
                panic!(e)
            }
        }
    }
}

use std::ops::Deref;
impl<E> Deref for Handle<E> {
    type Target = Canceller;
    fn deref(&self) -> &Self::Target {
        &self.canceller
    }
}

impl Canceller {
    /// Cancel the currently running service loop.
    ///
    /// Note that this will *not* interrupt a currently executing [`Cancellable::for_each`].
    /// Instead, the next time [`Cancellable::for_each`] *would* be called, the service loop will
    /// return.
    pub fn cancel(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{self, prelude::*}, net, thread,
    };

    struct Service(net::TcpListener);

    impl Cancellable for Service {
        type Error = io::Error;
        fn for_each(&mut self) -> Result<LoopState, Self::Error> {
            let mut stream = self.0.accept()?.0;
            write!(stream, "hello!")?;
            Ok(LoopState::Continue)
        }
    }

    impl Service {
        fn new() -> Self {
            Service(net::TcpListener::bind("127.0.0.1:0").unwrap())
        }

        fn port(&self) -> u16 {
            self.0.local_addr().unwrap().port()
        }
    }

    fn connect_assert(port: u16) {
        let mut c = net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut r = String::new();
        c.read_to_string(&mut r).unwrap();
        assert_eq!(r, "hello!");
    }

    #[test]
    fn it_runs() {
        let mut s = Service::new();
        let port = s.port();
        thread::spawn(move || {
            s.run().unwrap();
        });

        connect_assert(port);
        connect_assert(port);
    }

    #[test]
    fn it_cancels() {
        let s = Service::new();
        let port = s.port();
        let h = s.spawn();

        connect_assert(port);
        connect_assert(port);

        h.cancel();

        // cancel will ensure that for_each is not call *again*
        // it will *not* terminate the currently running for_each
        connect_assert(port);

        // instead of calling for_each again, the loop should now have exited
        h.wait().unwrap();
    }
}

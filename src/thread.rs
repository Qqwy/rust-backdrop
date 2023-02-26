// While the crate is no_std
// this module is not; it depends on std::thread
extern crate std;
use std::boxed::Box;

use crate::{Backdrop, BackdropStrategy};

/// Strategy which drops the contained value in a background thread.
///
/// A new thread is spawned (using [`std::thread::spawn`])
/// for every dropped value.
/// This is conceptually very simple, but relatively slow since a new thread is spawned every time.
pub struct ThreadStrategy();

impl BackdropStrategy for ThreadStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        std::thread::spawn(|| {
            core::mem::drop(droppable);
        });
    }
}

pub type ThreadBackdrop<T> = Backdrop<T, ThreadStrategy>;

/// Handle that can be used to send trash to the 'trash thread' that runs in the background for the [`TrashThreadStrategy`].
///
/// Only the global [`static@TRASH_THREAD_HANDLE`] thread is used.
pub struct TrashThreadHandle(std::sync::mpsc::SyncSender<Box<dyn Send>>);
use lazy_static::lazy_static;
lazy_static! {
    /// The global handle used by the [`TrashThreadStrategy`].
    ///
    /// This trash thread is a global thread that is started using [`mod@lazy_static`].
    ///
    /// If you use this strategy, you probably want to control when the thread
    /// is started using [`lazy_static::initialize(&TRASH_THREAD_HANDLE)`](lazy_static::initialize)
    /// (If you do not, it is started when it is used for the first time,
    /// meaning the very first drop will be slower.)
    pub static ref TRASH_THREAD_HANDLE: TrashThreadHandle = {
        let (send, recv) = std::sync::mpsc::sync_channel(10);
        std::thread::spawn(move || {
            for droppable in recv {
                core::mem::drop(droppable)
            }
        });
        TrashThreadHandle(send)
    };
}

/// Strategy which sends any to-be-dropped values to a dedicated 'trash thread'
///
/// This trash thread is a global thread that is started using [`mod@lazy_static`].
/// You probably want to control when it is started using [`lazy_static::initialize(&TRASH_THREAD_HANDLE)`]
/// (If you do not, it is started when it is used for the first time,
/// meaning the very first drop will be slower.)
///
/// Sending is done using a [`std::sync::mpsc::sync_channel`].
/// In the current implementation, there are 10 slots available in the channel
/// before a caller thread would block.
///
/// Also note that this global, detached, thread, will not be joined/cleaned up when your program exits.
/// This is probably fine, though it might mean that a few final destructors are not run.
/// But be aware that tools like e.g. Miri complain about this.
pub struct TrashThreadStrategy();

impl BackdropStrategy for TrashThreadStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        let handle = &TRASH_THREAD_HANDLE;
        let _ = handle.0.send(Box::new(droppable));
    }
}

pub type TrashThreadBackdrop<T> = Backdrop<T, TrashThreadStrategy>;

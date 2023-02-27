// While the crate is no_std
// this module is not; it depends on std::thread
extern crate std;
use std::boxed::Box;

use crate::{Backdrop, BackdropStrategy};

/// Strategy which drops the contained value in a newly spawned background thread.
///
/// A new thread is spawned (using [`std::thread::spawn`]) for every dropped value.
/// This is conceptually very simple, but relatively slow since spawning a thread has overhead.
pub struct ThreadStrategy();

impl<T: Send + 'static> BackdropStrategy<T> for ThreadStrategy {
    #[inline]
    fn execute(droppable: T) {
        std::thread::spawn(|| {
            core::mem::drop(droppable);
        });
    }
}

/// Convenient alias for a [`Backdrop`] that uses the [`ThreadStrategy`]
pub type ThreadBackdrop<T> = Backdrop<T, ThreadStrategy>;

/// Handle that can be used to send trash to the 'trash thread' that runs in the background for the [`TrashThreadStrategy`].
///
/// Only the global singleton [`static@GLOBAL_TRASH_THREAD_HANDLE`] instance of this struct is used.
pub struct GlobalTrashThreadHandle(std::sync::mpsc::SyncSender<Box<dyn Send>>);
use lazy_static::lazy_static;
lazy_static! {
    /// The global handle used by the [`TrashThreadStrategy`].
    ///
    /// This trash thread is a global thread that is started using [`mod@lazy_static`].
    ///
    /// If you use this strategy, you probably want to control when the thread
    /// is started using [`lazy_static::initialize(&GLOBAL_TRASH_THREAD_HANDLE)`](lazy_static::initialize)
    /// (If you do not, it is started when it is used for the first time,
    /// meaning the very first drop will be slower.)
    pub static ref GLOBAL_TRASH_THREAD_HANDLE: GlobalTrashThreadHandle = {
        let (send, recv) = std::sync::mpsc::sync_channel(10);
        std::thread::spawn(move || {
            for droppable in recv {
                core::mem::drop(droppable)
            }
        });
        GlobalTrashThreadHandle(send)
    };
}

/// Strategy which sends any to-be-dropped values to a dedicated global 'trash thread'
///
/// This trash thread is a global thread that is started using [`mod@lazy_static`].
/// You probably want to control when it is started using [`lazy_static::initialize(&GLOBAL_TRASH_THREAD_HANDLE)`]
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
pub struct GlobalTrashThreadStrategy();

impl<T: Send + 'static> BackdropStrategy<T> for GlobalTrashThreadStrategy {
    #[inline]
    fn execute(droppable: T) {
        let handle = &GLOBAL_TRASH_THREAD_HANDLE;
        let _ = handle.0.send(Box::new(droppable));
    }
}

/// Convenient alias for a [`Backdrop`] that uses the [`GlobalTrashThreadStrategy`]
pub type GlobalTrashThreadBackdrop<T> = Backdrop<T, GlobalTrashThreadStrategy>;


lazy_static! {
    static ref TRASH_THREAD_HANDLE: std::sync::RwLock<Option<std::sync::mpsc::SyncSender<Box<dyn Send>>>> = {
        std::sync::RwLock::new(None)
    };
}

/// Strategy which sends any to-be-dropped values to a dedicated 'trash thread'
///
/// This thread _must_ have been started first using [`TrashThreadStrategy::with_trash_thread`].
///
/// This strategy is similar to the [`GlobalTrashThreadStrategy`], but it makes sure that
/// the trash thread is cleaned up properly  (and any yet-to-be-dropped objects contained on it dropped) on exit.
///
/// # Panics
/// If Backdrop objects using this strategy are dropped without the thread having been started,
/// i.e. outside of the context of [`TrashThreadStrategy::with_trash_thread`],
/// a panic will happen.
pub struct TrashThreadStrategy();

impl TrashThreadStrategy {
    /// Starts a new Trash Thread,
    /// calls the given function or closure while it is alive,
    /// and afterwards cleans up the Trash Thread again.
    ///
    /// You probably want to e.g. surround the contents of your `main` or other 'big work' function
    /// with this.
    ///
    /// This function is reentrant: Nesting it will start a second (third, etc.) trash thread
    /// which will be used for the duration of the nested call.
    pub fn with_trash_thread<R>(fun: impl FnOnce() -> R) -> R {
        let (send, recv) = std::sync::mpsc::sync_channel(10);
        let thread_handle = std::thread::spawn(move || {
            for droppable in recv {
                core::mem::drop(droppable)
            }
        });
        let orig_handle = {
            let mut handle = TRASH_THREAD_HANDLE.write().unwrap();
            std::mem::replace(&mut *handle, Some(send))
        };

        let result = fun();

        {
            let mut handle = TRASH_THREAD_HANDLE.write().unwrap();
            // At this point the last 'send' handle is overwritten/dropped,
            // which should make the trash thread exit.
            *handle = orig_handle;
        };
        thread_handle.join().expect("Could not join on trash thread");
        result
    }
}

impl<T: Send + 'static> BackdropStrategy<T> for TrashThreadStrategy {
    #[inline]
    fn execute(droppable: T) {
        let handle = &TRASH_THREAD_HANDLE;
        let _ = handle.read().unwrap().as_ref().expect("No trash thread was started").send(Box::new(droppable));
    }
}

/// Convenient alias for a [`Backdrop`] that uses the [`TrashThreadStrategy`]
pub type TrashThreadBackdrop<T> = Backdrop<T, TrashThreadStrategy>;

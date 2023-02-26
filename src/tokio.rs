//! The contents of this module are only available when the `tokio` feature is enabled.
use crate::{Backdrop, BackdropStrategy};

/// Strategy which spawns a new tokio task which drops the contained value.
///
/// This only works within the context of a Tokio runtime.
/// (Dropping objects constructed with this strategy while no Tokio runtime is available
/// will result in a panic!)
///
/// Since the overhead of creating new Tokio tasks is very small, this is really fast
/// (at least from the perspective of the current task.)
///
/// Note that if dropping your value takes a very long time, you might be better off
/// using [`TokioBlockingTaskStrategy`] instead. Benchmark!
pub struct TokioTaskStrategy();
impl BackdropStrategy for TokioTaskStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        tokio::task::spawn(async move {
            core::mem::drop(droppable);
        });
    }
}

pub type TokioTaskBackdrop<T> = Backdrop<T, TokioTaskStrategy>;

/// Strategy which spawns a new 'blocking' tokio task which drops the contained value.
///
/// This only works within the context of a Tokio runtime.
/// (Dropping objects constructed with this strategy while no Tokio runtime is available
/// will result in a panic!)
///
/// This strategy is similar to [`TokioTaskStrategy`] but uses [`tokio::task::spawn_blocking`]
/// instead. This makes sure that the 'fast async tasks' thread pool can continue its normal work,
/// because the drop work is passed to the 'ok to block here' thread pool.
///
/// Benchmark to find out which approach suits your scenario better!
pub struct TokioBlockingTaskStrategy();
impl BackdropStrategy for TokioBlockingTaskStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        tokio::task::spawn_blocking(move || core::mem::drop(droppable));
    }
}

pub type TokioBlockingTaskBackdrop<T> = Backdrop<T, TokioBlockingTaskStrategy>;


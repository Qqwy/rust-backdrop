#![no_std]
#![cfg_attr(feature = "doc", feature(doc_auto_cfg))]

//! The `backdrop` crate allows you to customize when and how your values are dropped.
//! The main entry point of this crate is the [`Backdrop<T, Strategy>`] wrapper type.
//! This will wrap any 'normal' type `T` with a zero-cost wrapper
//! that customizes how it is dropped based on the given `Strategy`,
//! which is a marker (zero-size compile-time only) type that implements the
//! [`BackdropStrategy<T>`] trait.
//!
//! # Which strategy is best?
//! This probably depends very much on your application! Be sure to benchmark!
//! For general applications, the following are good starting recommendations:
//! - [`TrashThreadStrategy`] if you are working on a normal application where multithreading is possible.
//! - [`TokioTaskStrategy`] or [`TokioBlockingTaskStrategy`] if you are building an `async` application on top of the [`::tokio`] crate.
//! - [`TrashQueueStrategy`] if you are writing a single-threaded application.
//!
//! Backdrop also ships with a bunch of 'simple testing' strategies ([`LeakStrategy`], [`TrivialStrategy`], [`DebugStrategy`], [`ThreadStrategy`]),
//! that can help to understand how `backdrop` works, as leaning tool to build your own strategies, and as benchmarking baseline.
//!
//! # Features
//! - You can disable the `std` feature (enabled by default) to use this crate in no-std contexts.
//!   Without `std`, none of the [`thread`]-based strategies are available.
//!   The [`DebugStrategy`] is also disabled as it depends on `println`.
//! - As long as the `alloc` feature is not disabled (enabled by default; part of the `std` feature)
//!   the single-threaded [`TrashQueueStrategy`] is still available to you.
//!   If you also do not have access to `alloc` , you'll probably want to create your own strategy for your particular no-std situation.
//! - You can enable the optional `tokio` feature to get access to strategies that drop on a background _tokio task_. (C.f. the [`tokio`] module)
//!
//! # Limitations
//! `Backdrop<T, S>` implements the [`Deref`] and [`DerefMut`] traits, enabling you to use most methods available on `T` also on a `Backdrop<T>`.
//! On top of this, a bunch of common traits have been implemented for `Backdrop<T, S>` whenever they are implemented for `T`.
//! If something is missing that you really need, please open a PR and we can add it as an optional feature.

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod thread;
#[cfg(feature = "std")]
#[doc(inline)]
pub use thread::{ThreadBackdrop, TrashThreadBackdrop, ThreadStrategy, TrashThreadStrategy};

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "tokio")]
#[doc(inline)]
pub use crate::tokio::{TokioTaskBackdrop, TokioBlockingTaskBackdrop, TokioTaskStrategy, TokioBlockingTaskStrategy};


use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::mem::ManuallyDrop;

/// The strategy to use to drop `T`.
///
/// Most implementations of this trait place additional requirements on `T`.
/// For instance, all strategies that move T to a separate thread to be dropped there
/// introduce a `T: Send + 'static` bound.
///
pub trait BackdropStrategy<T> {
    /// Called whenever `T` should be dropped.
    ///
    /// The trivial implementation (and indeed, [`TrivialStrategy`] is implemented this way)
    /// is to do nothing. Then `T` will just directly be dropped right here, right now, because it is passed by value:
    ///
    /// ```ignore
    /// pub struct TrivialStrategy();
    ///
    /// impl<T> BackdropStrategy<T> for TrivialStrategy {
    ///    fn execute(_droppable: T) {
    ///    }
    /// }
    /// ```
    /// Or, for clarity:
    /// ```ignore
    /// pub struct TrivialStrategy();
    ///
    /// impl<T> BackdropStrategy<T> for TrivialStrategy {
    ///    fn execute(droppable: T) {
    ///        core::mem::drop(droppable)
    ///    }
    /// }
    /// ```
    ///
    /// But obviously that is not very exciting/helpful.
    /// Most implementations move the `T` to somewhere else somehow, and then it will be dropped there.
    ///
    /// To give you another example, here is how [`ThreadStrategy`] works:
    /// ```ignore
    /// pub struct ThreadStrategy();
    ///
    /// impl<T: Send + 'static> BackdropStrategy<T> for ThreadStrategy {
    ///     fn execute(droppable: T) {
    ///         std::thread::spawn(|| {
    ///             core::mem::drop(droppable);
    ///         });
    ///     }
    /// }
    /// ````
    fn execute(droppable: T);
}

/// Wrapper to drop any value at a later time, such as in a background thread.
///
/// `Backdrop<T, Strategy>` is guaranteed to have the same in-memory representation as `T`.
/// As such, wrapping (and unwrapping) a `T` into a `Backdrop<T, S>` has zero memory overhead.
///
/// Besides altering how `T` is dropped, a `Backdrop<T, S>` behaves as much as possible as a `T`.
/// This is done by implementing [`Deref`] and [`DerefMut`]
/// so most methods available for `T` are also immediately available for `Backdrop<T>`.
/// `Backdrop<T, S>` also implements many common traits whenever `T` implements these.
///
/// # Customizing the strategy
///
/// You customize what strategy is used by picking your desired `S` parameter,
/// which can be any type that implements the [`BackdropStrategy`] trait.
/// This crate comes with many common strategies, but you can also implement your own.
///
/// # Restrictions
///
/// `Backdrop<T, Strategy>` does not restrict `T` (besides `T` needing to be [`Sized`]). However,
/// Many [`Strategy`](`BackdropStrategy<T>`) only implement [`BackdropStrategy<T>`] when `T` fits certain restrictions.
/// For instance, the [`TrashThreadStrategy`] requires `T` to be `Send` since `T` will be moved to another thread to be cleaned up there.
///
/// What about [unsized/dynamically-sized](https://doc.rust-lang.org/nomicon/exotic-sizes.html) types? The current implementation of `Backdrop` restricts `T` to be [`Sized`] mostly for ease of implementation.
/// It is our expectation that your unsized datastructures probably are already nested in a [`std::boxed::Box<T>`] or other smart pointer,
/// which you can wrap with `Backdrop` as a whole.
/// _(Side note: Zero-sized types can be wrapped by `Backdrop` without problems.)_
///
/// There is one final important restriction:
/// ### The problem with Arc
/// A `Backdrop<Arc<T>, S>` will not behave as you might expect:
/// It will cause the backdrop strategy to run whenever the reference count is decremented.
/// But what you probably want, is to run the backdrop strategy exactly when the last [`Arc<T>`][arc] is dropped
/// (AKA when the reference count drops to 0) and the _contents_ of the [`Arc`][arc] go out of scope.
///
/// A `Arc<Backdrop<Box<T>, S>>` _will_ work as you expect, but you incur an extra pointer indirection (arc -> box -> T)
/// every time you read its internal value.
///
/// Instead, use the [`backdrop_arc`](https://crates.io/crates/backdrop_arc) crate, which contains
/// a specialized `Arc` datatype that does exactly what you want without a needless indirection.
///
/// [arc]: std::sync::Arc
#[repr(transparent)]
pub struct Backdrop<T, S: BackdropStrategy<T>> {
    val: ManuallyDrop<T>,
    _marker: PhantomData<S>,
}

impl<T, Strategy: BackdropStrategy<T>> Backdrop<T, Strategy> {
    /// Construct a new [`Backdrop<T, S>`] from any T. This is a zero-cost operation.
    ///
    /// From now on, T will no longer be dropped normally,
    /// but instead it will be dropped using the implementation of the given [`BackdropStrategy`].
    ///
    /// ```
    /// use backdrop::*;
    ///
    /// // Either specify the return type:
    /// let mynum: Backdrop<usize, LeakStrategy> = Backdrop::new(42);
    ///
    /// // Or use the 'Turbofish' syntax on the function call:
    /// let mynum2 = Backdrop::<_, LeakStrategy>::new(42);
    ///
    /// // Or use one of the shorthand type aliases:
    /// let mynum3 = LeakBackdrop::new(42);
    ///
    /// assert_eq!(mynum, mynum2);
    /// assert_eq!(mynum2, mynum3);
    /// // <- Because we are using the LeakStrategy, we leak memory here. Fun! :-)
    /// ```
    /// This function is the inverse of [`Backdrop::into_inner`].
    ///
    #[inline]
    pub fn new(val: T) -> Self {
        Self {
            val: ManuallyDrop::new(val),
            _marker: PhantomData,
        }
    }

    /// Turns a [`Backdrop<T, S>`] back into a normal T.
    /// This undoes the effect of Backdrop.
    /// The resulting T will be dropped again using normal rules.
    /// This function is the inverse of [`Backdrop::new`].
    ///
    /// This is a zero-cost operation.
    ///
    /// This is an associated function, so call it using fully-qualified syntax.
    #[inline]
    pub fn into_inner(mut this: Self) -> T {
        // SAFETY: we forget the container after `this.val` is taken out.
        unsafe {
            let inner = ManuallyDrop::take(&mut this.val);
            core::mem::forget(this);
            inner
        }
    }

    /// Changes the strategy used for a Backdrop.
    ///
    /// This is a zero-cost operation
    ///
    /// This is an associated function, so call it using fully-qualified syntax.
    ///
    /// ```
    /// use backdrop::*;
    ///
    /// let foo = LeakBackdrop::new(42);
    /// let foo = Backdrop::change_strategy::<TrivialStrategy>(foo);
    /// // Now `foo` will be dropped according to TrivialStrategy (which does the normal drop rules)
    /// // rather than LeakStrategy (which does not cleanup by leaking memory)
    /// ```
    pub fn change_strategy<S2: BackdropStrategy<T>>(this: Self) -> Backdrop<T, S2> {
        Backdrop::<T, S2>::new(Backdrop::into_inner(this))
    }
}

/// This is where the magic happens: Instead of dropping `T` normally, we run [`Strategy::execute`](BackdropStrategy::execute) on it.
impl<T, Strategy: BackdropStrategy<T>> Drop for Backdrop<T, Strategy> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: self.val is not used again after this call
        // and since self is already being dropped, no further cleanup is necessary
        let inner = unsafe { ManuallyDrop::take(&mut self.val)};
        Strategy::execute(inner)
    }
}

impl<T, S: BackdropStrategy<T>> core::ops::Deref for Backdrop<T, S> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: self.1 is filled with an initialized value on construction
        self.val.deref()
    }
}

impl<T, S: BackdropStrategy<T>> DerefMut for Backdrop<T, S> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: self.1 is filled with an initialized value on construction
        self.val.deref_mut()
    }
}

impl<T: core::fmt::Debug, S> core::fmt::Debug for Backdrop<T, S>
    where
    S: BackdropStrategy<T>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: core::fmt::Display, S> core::fmt::Display for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&**self, f)
    }
}

impl<T: Clone, S> Clone for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{
    fn clone(&self) -> Self {
        Self::new(self.deref().clone())
    }
}

impl<T: core::cmp::PartialEq, S> core::cmp::PartialEq for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other.deref())
    }
}

impl<T: core::cmp::Eq, S> core::cmp::Eq for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{ }

impl<T: core::cmp::PartialOrd, S> core::cmp::PartialOrd for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}

impl<T: core::cmp::Ord, S> core::cmp::Ord for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.deref().cmp(other.deref())
    }
}

impl<T: core::hash::Hash, S> core::hash::Hash for Backdrop<T, S>
where
    S: BackdropStrategy<T>,
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.deref().hash(state)
    }
}

/// Converting between a T and a Backdrop<T, S> is a zero-cost operation
///
/// c.f. [`Backdrop::new`]
impl<T, S> From<T> for Backdrop<T, S>
    where
    S: BackdropStrategy<T>,
{
    fn from(val: T) -> Self {
        Backdrop::new(val)
    }
}

/// Strategy which drops the contained value normally.
///
/// It behaves exactly as if the backdrop was not there.
///
/// Its main purpose is to be able to easily test the advantage of another strategy
/// in a benchmark, without having to completely alter the structure of your code.
pub struct TrivialStrategy();

impl<T> BackdropStrategy<T> for TrivialStrategy {
    #[inline]
    fn execute(droppable: T) {
        core::mem::drop(droppable)
    }
}

pub type TrivialBackdrop<T> = Backdrop<T, TrivialStrategy>;



/// Strategy which will leak the contained value rather than dropping it.
///
/// This is not normally useful, except for testing what the overhead is
/// of whatever code is surrounding your drop glue.
pub struct LeakStrategy();

impl<T> BackdropStrategy<T> for LeakStrategy {
    #[inline]
    fn execute(droppable: T) {
        core::mem::forget(droppable)
    }
}

pub type LeakBackdrop<T> = Backdrop<T, LeakStrategy>;


/// 'Wrapper' strategy that prints out T when executed.
///
/// Takes another strategy as generic type argument.
///
/// The exact printed message is not considered a stable API;
/// it is intended for human programmer eyes only.
#[cfg(feature = "std")]
pub struct DebugStrategy<InnerStrategy>(PhantomData<InnerStrategy>);

#[cfg(feature = "std")]
impl<T, InnerStrategy> BackdropStrategy<T> for DebugStrategy<InnerStrategy>
    where
    T: std::fmt::Debug,
    InnerStrategy: BackdropStrategy<T>,
{
    #[inline]
    fn execute(droppable: T) {
        use std::println;
        println!("Using BackdropStrategy '{}' to drop value {:?}", std::any::type_name::<InnerStrategy>(), &droppable);
        InnerStrategy::execute(droppable)
    }
}

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, collections::VecDeque};

#[cfg(feature = "alloc")]
use core::cell::RefCell;
#[cfg(feature = "alloc")]
use core::any::Any;

#[cfg(feature = "std")]
std::thread_local!{
    static TRASH_QUEUE: RefCell<VecDeque<Box<dyn Any>>> = VecDeque::new().into();
}

// Fallback implementation for when std::thread_local! is not available
#[cfg(all(not(feature = "std"), feature = "alloc"))]
static mut TRASH_QUEUE: Option<core::cell::RefCell<VecDeque<Box<dyn core::any::Any>>>> = None;

// When std::thread_local! exists we can safely call the closure
#[cfg(feature = "std")]
fn with_single_threaded_trash_queue(closure: impl FnOnce(&RefCell<VecDeque<Box<dyn Any>>>)) {
    TRASH_QUEUE.with(|tq_cell| {
        closure(tq_cell);
    });
}

// In no_std (but alloc) contexts, we expect the program to run single-threaded
// And we call the closure using unsafe in a best-effort basis.
#[cfg(all(not(feature = "std"), feature = "alloc"))]
fn with_single_threaded_trash_queue(closure: impl FnOnce(&RefCell<VecDeque<Box<dyn Any>>>)) {
    let tq_ref = unsafe { &mut TRASH_QUEUE };
    if tq_ref.is_none() {
        *tq_ref = Some(VecDeque::new().into());
    }
    closure(unsafe { &TRASH_QUEUE.as_ref().unwrap() });
}

/// Strategy which adds garbage to a global 'trash [`VecDeque`]'.
///
/// In `std` contexts, this trash queue is protected using [`std::thread_local!`].
/// In `no_std` contexts, it is instead implemented as a [mutable static](https://doc.rust-lang.org/reference/items/static-items.html#mutable-statics) variable.
///
/// Perfectly fine for truly single-threaded applications.
///
/// This does mean that that if you do use some sort of 'alternative threading' in a `no_std` context, this strategy will be unsound!
#[cfg(feature = "alloc")]
pub struct TrashQueueStrategy();

#[cfg(feature = "alloc")]
impl TrashQueueStrategy {
    /// Makes sure the global (thread local) queue is initialized
    /// If you do not call this, it will be initialized the first time an object is dropped,
    /// which will add some overhead at that moment.
    ///
    /// Called automatically by [`TrashQueueStrategy::cleanup_on_exit()`]
    pub fn ensure_initialized() {
        with_single_threaded_trash_queue(|_tq_cell| {});
    }

    /// Cleans up a single item in the trash queue.
    ///
    /// Returns `true` if there is more garbage in the queue at this moment.
    /// That could be used to e.g. clean up 'some' garbage but not all.
    pub fn cleanup_one() -> bool {
        let mut queue_nonempty = false;
        with_single_threaded_trash_queue(|tq_cell| {
            let mut tq = tq_cell.borrow_mut();
            let item = tq.pop_front();
            core::mem::drop(item);
            queue_nonempty = tq.is_empty();
        });
        queue_nonempty
    }

    /// Cleans up everything that is in the trash queue.
    pub fn cleanup_all() {
        with_single_threaded_trash_queue(|tq_cell| {
            let mut tq = tq_cell.borrow_mut();
            while let Some(item) = tq.pop_front() {
                core::mem::drop(item);
            }
        });
    }

    /// Wrapper which will:
    /// - Call [`TrashQueueStrategy::ensure_initialized()`] before your closure
    /// - Call your closure
    /// - Call [`TrashQueueStrategy::cleanup_all()`] after your closure.
    ///
    /// As such, you can use this to delay dropping until after your critical code section very easily:
    pub fn cleanup_on_exit<R>(closure: impl FnOnce() -> R) -> R {
        TrashQueueStrategy::ensure_initialized();
        let outcome = closure();
        TrashQueueStrategy::cleanup_all();
        outcome
    }
}

#[cfg(feature = "alloc")]
impl<T: 'static> BackdropStrategy<T> for TrashQueueStrategy {
    fn execute(droppable: T) {
        let boxed: Box<dyn core::any::Any> = Box::new(droppable);
        with_single_threaded_trash_queue(|tq_cell| {
            let mut tq = tq_cell.borrow_mut();
            tq.push_back(boxed);
        });
    }
}

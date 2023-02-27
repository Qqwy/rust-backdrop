#![no_std]
#![feature(doc_auto_cfg)]

//! The `backdrop` crate allows you to customize when and how your values are dropped.
//! The main entry point of this crate is the [`Backdrop<T, Strategy>`] wrapper type.
//! This will wrap any 'normal' type `T` with a zero-cost wrapper
//! that customizes how it is dropped based on the given `Strategy`,
//! which is a marker (zero-size compile-time only) type that implements the
//! [`BackdropStrategy<T>`] trait.
//!
//! # Features
//! - You can disable the `std` feature (enabled by default) to use this trait in no-std contexts.
//!   Without `std`, none of the [`thread`]-based strategies are available. The [`DebugStrategy`] is also disabled as it depends on `println`.
//!   You'll probably want to create your own strategy for your particular no-std situation.
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
    /// is to do nothing. Then `T` will just directly be dropped right here, right now.
    ///
    /// But obviously that is not very exciting/helpful.
    /// Most implementations move the `T` to somewhere else somehow, and then it will be dropped there.
    fn execute(droppable: T);
}

/// Wrapper to drop any value at a later time, such as in a background thread.
///
/// `Backdrop<T>` is guaranteed to have the same in-memory representation as `T`.
/// As such, it has zero memory overhead.
///
/// Besides altering how `T` is dropped, a `Backdrop<T>` behaves as much as possible as a `T`.
/// This is done by implementing [`Deref`] and [`DerefMut`]
/// so most methods available for `T` are also immediately available for `Backdrop<T>`.
/// `Backdrop<T>` also implements many common traits whenever `T` implements these.
///
/// # Restrictions
///
/// There are only two, highly logical, restrictions on the kinds of `T` that a `Backdrop<T>` can wrap:
/// 1. `T` needs to be Send. Many strategies rely on moving the `T` to a different thread, to be dropped there.
/// 2. `T` is not allowed to internally contain non-static references. Many strategies delay destruction to a later point in the future, when those references might have become invalid. (Moving to another thread also 'delays to the future' because code on that thread will not run in lockstep with our current thread.)
#[repr(transparent)]
pub struct Backdrop<T, S: BackdropStrategy<T>> {
    val: ManuallyDrop<T>,
    _marker: PhantomData<S>,
}

impl<T, Strategy: BackdropStrategy<T>> Backdrop<T, Strategy> {
    /// Construct a new [`Backdrop<T>`] from any T. This is a zero-cost operation.
    ///
    /// From now on, T will no longer be dropped normally,
    /// but instead it will be dropped using the implementation of the given [`BackdropStrategy`].
    #[inline]
    pub fn new(val: T) -> Self {
        Self {
            val: ManuallyDrop::new(val),
            _marker: PhantomData,
        }
    }

    /// Turns a [`Backdrop<T>`] back into a normal T.
    /// This undoes the effect of Backdrop.
    /// The resulting T will be dropped again using normal rules.
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

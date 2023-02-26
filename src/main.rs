use core::{marker::PhantomData, mem::MaybeUninit};
use core::ops::{Deref, DerefMut};

pub trait BackdropStrategy {
    fn execute<T: Send + 'static>(droppable: T);
}

/// Wrapper to drop any value at a later time, such as in a background thread.
///
/// `Backdrop<T>` is guaranteed to have the same in-memory representation as `T`.
/// As such, it has zero memory overhead.
///
/// Besides altering how `T` is dropped, a `Backdrop<T>` behaves as much as possible as a `T`.
/// This is done by implementing `Deref` and `DerefMut`
/// so most methods available for `T` are also immediately available for `Backdrop<T>`.
/// `Backdrop<T>` also implements many common traits whenever `T` implements these.
#[repr(transparent)]
pub struct Backdrop<T: Send + 'static, S: BackdropStrategy> {
    val: MaybeUninit<T>,
    _marker: PhantomData<S>,
}

impl<T: Send + 'static, Strategy: BackdropStrategy> Backdrop<T, Strategy> {
    /// Construct a new Backdrop<T> from any T.
    ///
    /// From now on, T will no longer be dropped normally,
    /// but instead it will be dropped using the implementation of the given [`BackdropStrategy`].
    #[inline]
    pub fn new(val: T) -> Self {
        Self {
            val: MaybeUninit::new(val),
            _marker: PhantomData,
        }
    }

    /// Turns a Backdrop<T> back into a normal T.
    /// This undoes the effect of Backdrop.
    /// The resulting T will be dropped again using normal rules.
    ///
    /// This is an associated function, so call it using fully-qualified syntax.
    #[inline]
    pub fn into_inner(mut this: Self) -> T {
        // SAFETY: self.1 is filled with an initialized value on construction
        let inner = core::mem::replace(&mut this.val, MaybeUninit::uninit());
        let inner = unsafe { inner.assume_init() };
        // Make sure we do not try to clean up uninitialized memory:
        core::mem::forget(this);
        inner
    }
}

/// This is where the magic happens: Instead of dropping `T` normally, we run [`Strategy::execute`](BackdropStrategy::execute) on it.
impl<T: Send + 'static, Strategy: BackdropStrategy> Drop for Backdrop<T, Strategy> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: self.1 is filled with an initialized value on construction
        let inner = core::mem::replace(&mut self.val, MaybeUninit::uninit());
        let inner = unsafe { inner.assume_init() };
        Strategy::execute(inner)
    }
}

impl<T: Send + 'static, S: BackdropStrategy> core::ops::Deref for Backdrop<T, S> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: self.1 is filled with an initialized value on construction
        unsafe { self.val.assume_init_ref() }
    }
}

impl<T: Send + 'static, S: BackdropStrategy> DerefMut for Backdrop<T, S> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: self.1 is filled with an initialized value on construction
        unsafe { self.val.assume_init_mut() }
    }
}

impl<T: core::fmt::Debug, S> core::fmt::Debug for Backdrop<T, S>
    where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: core::fmt::Display, S> core::fmt::Display for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&**self, f)
    }
}

impl<T: Clone, S> Clone for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn clone(&self) -> Self {
        Self::new(self.deref().clone())
    }
}

impl<T: core::cmp::PartialEq, S> core::cmp::PartialEq for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other.deref())
    }
}

impl<T: core::cmp::Eq, S> core::cmp::Eq for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{ }

impl<T: core::cmp::PartialOrd, S> core::cmp::PartialOrd for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}

impl<T: core::cmp::Ord, S> core::cmp::Ord for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.deref().cmp(other.deref())
    }
}

impl<T: core::hash::Hash, S> core::hash::Hash for Backdrop<T, S>
where
    T: Send + 'static,
    S: BackdropStrategy,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.deref().hash(state)
    }
}

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
/// Only the global [`static@TRASH_THREAD_HANDLE`] strategy is used.
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
    static ref TRASH_THREAD_HANDLE: TrashThreadHandle = {
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
pub struct TrashThreadStrategy();

impl BackdropStrategy for TrashThreadStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        let handle = &TRASH_THREAD_HANDLE;
        let _ = handle.0.send(Box::new(droppable));
    }
}

pub type TrashThreadBackdrop<T> = Backdrop<T, TrashThreadStrategy>;

/// Strategy which drops the contained value normally.
///
/// It behaves exactly as if the backdrop was not there.
///
/// Its main purpose is to be able to easily test the advantage of another strategy
/// in a benchmark, without having to completely alter the structure of your code.
pub struct FakeStrategy();

impl BackdropStrategy for FakeStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        core::mem::drop(droppable)
    }
}

pub type FakeBackdrop<T> = Backdrop<T, FakeStrategy>;

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

fn time(name: &'static str, f: impl FnOnce()) {
    let start = std::time::Instant::now();
    f();
    let end = std::time::Instant::now();
    println!("{name}, took {:?}", end.duration_since(start));
}

const LEN: usize = 5_000_000_0;

fn setup() -> Box<[Box<str>]> {
    (0..LEN)
        .map(|x| x.to_string().into_boxed_str())
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn main() {
    let boxed = setup();
    time("none", move || {
        let boxed = boxed;
        assert_eq!(boxed.len(), LEN);
        // Destructor runs here
    });

    lazy_static::initialize(&TRASH_THREAD_HANDLE);
    let backdropped: FakeBackdrop<_> = Backdrop::new(setup());
    time("fake backdrop", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    let backdropped: ThreadBackdrop<_> = Backdrop::new(setup());
    time("thread backdrop", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    let backdropped: TrashThreadBackdrop<_> = Backdrop::new(setup());
    time("trash thread backdrop", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let backdropped: TokioTaskBackdrop<_> = Backdrop::new(setup());
            time("tokio task (multithread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });

            let backdropped: TokioBlockingTaskBackdrop<_> = Backdrop::new(setup());
            time("tokio blocking task (multithread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });
        });

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let backdropped: TokioTaskBackdrop<_> = Backdrop::new(setup());
            time("tokio task (current thread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });

            let backdropped: TokioBlockingTaskBackdrop<_> = Backdrop::new(setup());
            time("tokio blocking task (current thread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });
        });
}

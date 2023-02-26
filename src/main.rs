use std::{marker::PhantomData, mem::MaybeUninit};

pub trait BackdropStrategy {
    fn execute<T: Send + 'static>(droppable: T);
}

#[repr(transparent)]
pub struct Backdrop<T: Send + 'static, S: BackdropStrategy>(PhantomData<S>, MaybeUninit<T>);


impl<T: Send + 'static, Strategy: BackdropStrategy> Backdrop<T, Strategy> {
    #[inline]
    pub fn new(val: T) -> Self {
        Self(PhantomData, MaybeUninit::new(val))
    }

    #[inline]
    pub fn into_inner(mut self) -> T {
        // SAFETY: self.1 is filled with an initialized value on construction
        let inner = core::mem::replace(&mut self.1, MaybeUninit::uninit());
        let inner = unsafe { inner.assume_init() };
        // Make sure we do not try to clean up uninitialized memory:
        core::mem::forget(self);
        inner
    }
}

impl<T: Send + 'static, Strategy: BackdropStrategy> Drop for Backdrop<T, Strategy> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: self.1 is filled with an initialized value on construction
        let inner = core::mem::replace(&mut self.1, MaybeUninit::uninit());
        let inner = unsafe { inner.assume_init() };
        Strategy::execute(inner)
    }
}

impl<T: Send + 'static, S: BackdropStrategy> core::ops::Deref for Backdrop<T, S> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: self.1 is filled with an initialized value on construction
        unsafe{ self.1.assume_init_ref() }
    }
}

impl<T: Send + 'static, S: BackdropStrategy> core::ops::DerefMut for Backdrop<T, S> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: self.1 is filled with an initialized value on construction
        unsafe{ self.1.assume_init_mut() }
    }
}

/// A strategy that drops the contained value in a background thread.
///
/// A new thread is spawned (using [`std::sync::thread`])
/// for every dropped value.
/// This is conceptually very simple, but relatively slow since a new thread is spawned every time.
pub struct ThreadStrategy();

impl BackdropStrategy for ThreadStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        std::thread::spawn(|| {
            println!("Dropping on a background thread");
            let _ = droppable;
        });
    }
}

pub type ThreadBackdrop<T> = Backdrop<T, ThreadStrategy>;

/// A strategy that drops the contained value normally.
///
/// It behaves exactly as if the backdrop was not there.
///
/// Its main purpose is to be able to easily test the advantage of another strategy
/// in a benchmark, without having to completely alter the structure of your code.
pub struct NormalDropStrategy();

impl BackdropStrategy for NormalDropStrategy {
    #[inline]
    fn execute<T: Send + 'static>(droppable: T) {
        core::mem::drop(droppable)
    }
}

pub type NormalDropBackdrop<T> = Backdrop<T, NormalDropStrategy>;

fn time(name: &'static str, f: impl FnOnce()) {
    let start = std::time::Instant::now();
    f();
    let end = std::time::Instant::now();
    println!("{name}, took {:?}", end.duration_since(start));
}

const LEN: usize = 50_000;

fn setup() -> Box<[Box<str>]> {
    (0..LEN)
        .map(|x| x.to_string().into_boxed_str())
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn main() {
    let backdropped: ThreadBackdrop<_> = Backdrop::new(setup());
    time("backdrop", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    let backdropped: NormalDropBackdrop<_> = Backdrop::new(setup());
    time("normal", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    let boxed = setup();
    time("none", move || {
        let boxed = boxed;
        assert_eq!(boxed.len(), LEN);
        // Destructor runs here
    });
}

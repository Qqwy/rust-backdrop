use backdrop::*;

fn time(name: &'static str, f: impl FnOnce()) {
    let start = std::time::Instant::now();
    f();
    let end = std::time::Instant::now();
    println!("{name}, took {:?}", end.duration_since(start));
}

const LEN: usize = 5_000;

fn setup() -> Box<[Box<str>]> {
    (0..LEN)
        .map(|x| x.to_string().into_boxed_str())
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn main() {
    let boxed = setup();
    let not_backdropped = boxed.clone();
    time("none", move || {
        assert_eq!(not_backdropped.len(), LEN);
        // Destructor runs here
    });

    let backdropped: TrivialBackdrop<_> = Backdrop::new(boxed.clone());
    time("fake backdrop", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    let backdropped: thread::ThreadBackdrop<_> = Backdrop::new(boxed.clone());
    time("thread backdrop", move || {
        assert_eq!(backdropped.len(), LEN);
        // Destructor runs here
    });

    TrashThreadStrategy::with_trash_thread(||{
        let backdropped: thread::TrashThreadBackdrop<_> = Backdrop::new(boxed.clone());
        time("trash thread backdrop", move || {
            assert_eq!(backdropped.len(), LEN);
            // Destructor runs here
        });
    });

    #[cfg(miri)]
    {
        println!("Skipping Tokio examples when running on Miri, since it does not support Tokio yet");
    }
    #[cfg(not(miri))]
    {
    ::tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let backdropped: crate::tokio::TokioTaskBackdrop<_> = Backdrop::new(boxed.clone());
            time("tokio task (multithread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });

            let backdropped: crate::tokio::TokioBlockingTaskBackdrop<_> = Backdrop::new(boxed.clone());
            time("tokio blocking task (multithread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });
        });

    ::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let backdropped: crate::tokio::TokioTaskBackdrop<_> = Backdrop::new(setup());
            time("tokio task (current thread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });

            let backdropped: crate::tokio::TokioBlockingTaskBackdrop<_> = Backdrop::new(setup());
            time("tokio blocking task (current thread runner)", move || {
                assert_eq!(backdropped.len(), LEN);
                // Destructor runs here
            });
        });
    }
}

// pub fn foo() -> usize {
//     let val: thread::ThreadBackdrop<_> = Backdrop::new(setup());
//     let orig = Backdrop::into_inner(val);
//     orig.len()
// }

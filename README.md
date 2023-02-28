# Backdrop &emsp; [![Latest Version]][crates.io] [![License]][license path] [![requires: rustc 1.56+]][Rust 1.56.1]


[Latest Version]: https://img.shields.io/crates/v/backdrop.svg
[crates.io]: https://crates.io/crates/backdrop
[License]: https://img.shields.io/badge/license-MIT-blue.svg
[license path]: https://github.com/qqwy/rust-backdrop/blob/main/LICENSE
[requires: rustc 1.56+]: https://img.shields.io/badge/rustc-1.56+-lightgray.svg
[Rust 1.56.1]: https://rust-lang.org/

The `backdrop` crate allows you to customize when and how your values are dropped.
The main entry point of this crate is the [`Backdrop<T, Strategy>`](https://docs.rs/backdrop/latest/backdrop/struct.Backdrop.html) wrapper type.
This will wrap any 'normal' type `T` with a zero-cost wrapper
that customizes how it is dropped based on the given `Strategy`.

`Strategy` is a marker (zero-size compile-time only) type that implements the
[BackdropStrategy<T>](https://docs.rs/backdrop/latest/backdrop/trait.BackdropStrategy.html) trait.

## Builtin strategies

Production-ready:
- `TrashQueueStrategy`: Single-threaded strategy to store to-be-cleaned objects in a queue which you can clean up all at once later.
- `TrashThreadStrategy`: Multi-threaded strategy where a dedicated thread will clean up objects in the background.
- `TokioTaskStrategy` and `TokioBlockingTaskStrategy`: Spawn a new Tokio task (on the 'normal' thread pool resp. the 'blocking OK here' threadpool) which will clean up your object while your hot code path can happily continue.

There are also some simple strategies included to learn from or to make benchmarking easier:
- `LeakStrategy`: Does nothing, forgets about your objects and leaks their memory. Useful to test the 'lower bound' of your code path or cleanup strategy.
- `TrivialStrategy`: Drops objects normally.
- `ThreadStrategy`: Spawns a new full-blown OS thread for each dropped object. This has a big overhead, but this strategy can be a useful learning tool/starting point to write your own strategies.

Of course, you can also very easily create your own!

## Quick Example


```rust
use backdrop::*;

{
  // Either specify the return type:
  let mynum: Backdrop<usize, LeakStrategy> = Backdrop::new(42);

  // Or use the 'Turbofish' syntax on the function call:
  let mynum2 = Backdrop::<_, LeakStrategy>::new(42);

  // Or use one of the shorthand type aliases:
  let mynum3 = LeakBackdrop::new(42);
} // <- Because we are using the LeakStrategy, we leak memory here, rather than dropping. Fun! :-)
```

To see how much certain strategies might matter, consider running the `comparison` example in the repo on your machine
(`cargo run --example comparison --features="tokio"`) which will try a bunch of included strategies to drop a large data structure (a `Box<[Box<str>; 5_000_000]>`):

```
none, took 99.785167ms
fake backdrop, took 91.171125ms

thread backdrop, took 184.708µs
trash thread backdrop, took 22.333µs

(single threaded) trash queue backdrop, took 21.542µs
(single threaded) trash queue backdrop (actually cleaning up later), took 87.6455ms

tokio task (multithread runner), took 33.875µs
tokio blocking task (multithread runner), took 55.875µs
tokio task (current thread runner), took 18.875µs
tokio blocking task (current thread runner), took 63µs
```

## Arc

A `Backdrop<Arc<T>, S>` will not work as you expect. ([more info](https://docs.rs/backdrop/latest/backdrop/struct.Backdrop.html#the-problem-with-arc))
Use the Arc from the [`backdrop_arc`](https://crates.io/crates/backdrop_arc) crate instead.

## MSRV

The Minimum Supported Rust Version of backdrop is Rust 1.56.1, because we use edition 2021 Rust syntax.
There are no (required) Rust features or (required) dependencies, making this a very lightweight and portable crate.

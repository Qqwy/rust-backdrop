# Backdrop

The `backdrop` crate allows you to customize when and how your values are dropped.
The main entry point of this crate is the [`Backdrop<T, Strategy>`](https://docs.rs/backdrop/0.1.0/backdrop/struct.Backdrop.html) wrapper type.
This will wrap any 'normal' type `T` with a zero-cost wrapper
that customizes how it is dropped based on the given `Strategy`.

`Strategy` is a marker (zero-size compile-time only) type that implements the
[BackdropStrategy<T>](https://docs.rs/backdrop/0.1.0/backdrop/trait.BackdropStrategy.html) trait.

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

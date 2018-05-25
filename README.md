# minion

[![Crates.io](https://img.shields.io/crates/v/minion.svg)](https://crates.io/crates/minion)
[![Documentation](https://docs.rs/minion/badge.svg)](https://docs.rs/minion/)
[![Build Status](https://travis-ci.org/jonhoo/minion.svg?branch=master)](https://travis-ci.org/jonhoo/minion)

This crate provides a wrapper type for making long-running service loops cancellable.

Let's dive right in with an example. For further details see
[`Cancellable`](https://docs.rs/minion/*/minion/trait.Cancellable.html).

```rust
// impl Cancellable for Service { .. }
let s = Service::new();

// start the service loop on a new thread
let h = s.spawn();

// get a handle that allows cancelling the service loop
let exit = h.canceller();

// spin up a new thread that will handle exit signals
thread::spawn(move || {
    // this might catch Ctrl-C from the user, wait for a particular packet,
    // or for any other condition that signals that the service should exit
    // cleanly. in this case, we just terminate after a fixed amount of time.
    thread::sleep(time::Duration::from_secs(1));

    // tell the service loop to exit at the first opportunity
    exit.cancel();
});

// block until the service loop exits or errors.
h.wait().unwrap();
```

# Live-coding

The crate is under development as part of a live-coding stream series intended
for users who are already somewhat familiar with Rust, and who want to see
something larger and more involved be built.

You can find the recordings of past sessions in [this YouTube
playlist](https://www.youtube.com/playlist?list=PLqbS7AVVErFgY2faCIYjJZv_RluGkTlKt).
This crate was made in [this
video](https://www.youtube.com/watch?v=KS14JIRZTBw). To get updates about
future streams, follow me on [Patreon](https://www.patreon.com/jonhoo) or
[Twitter](https://twitter.com/jonhoo).

# Thank you

For each of the projects I build, I like to thank the people who are willing
and able to take the extra step of supporting me in making these videos on
[Patreon](https://www.patreon.com/jonhoo) or
[Liberapay](https://liberapay.com/jonhoo/). You have my most sincere gratitude,
and I'm so excited that you find what I do interesting enough that you're
willing to give a stranger money to do something they love!

 - Rodrigo Valin
 - Roger Andersen (@hexjelly)
 - Pigeon F

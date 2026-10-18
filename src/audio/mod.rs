//! What the PC is playing: a loopback capture of the default output device,
//! downmixed into a ring, and a thread that listens to it.

pub mod analysis;
// Copied from Seoul unchanged, so `device_name` stays though nothing reads it yet.
#[allow(dead_code)]
pub mod capture;

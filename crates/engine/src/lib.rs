//! Yggdrasil engine: project model, music theory, DSP, instruments, real-time
//! sequencer and the control-side `Controller` used by the app.

#[cfg(feature = "audio-io")]
pub mod audio_io;
pub mod capture;
pub mod controller;
pub mod dsp;
pub mod engine;
pub mod export;
pub mod fx;
pub mod granular;
pub mod grooves;
pub mod instruments;
pub mod library;
pub mod midi;
pub mod model;
pub mod piano;
pub mod sampling;
pub mod session;
pub mod sound;
pub mod theory;

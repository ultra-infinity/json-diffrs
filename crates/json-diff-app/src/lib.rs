//! json-diff's app, without a window: the state behind every button and
//! assistant tool ([`state`]), the snapshot the frontend draws ([`view`]),
//! PDF reports ([`report`], [`layout`], [`pdf`]) and remembered settings.
//! The desktop app (Tauri) and the web app (WebAssembly) both drive it.
//!
//! Nothing here opens a window or a network connection. File and folder
//! commands use `std::fs`, which in the browser fails with an error instead.

pub mod agents_md;
pub mod changelog;
pub mod layout;
pub mod pdf;
pub mod report;
pub mod samples;
pub mod settings;
pub mod state;
pub mod tools;
pub mod view;

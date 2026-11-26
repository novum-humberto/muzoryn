//! MUZORYN: reproductor musical en terminal.
pub mod app;
pub mod audio;
pub mod config;
pub mod core;
pub mod cover;
pub mod downloads;
pub mod dsp;
pub mod feeds;
pub mod gfx;
#[cfg(unix)]
pub mod hooks;
#[cfg(unix)]
pub mod ipc;
pub mod keys;
pub mod library;
pub mod lyrics;
pub mod media;
#[cfg(all(unix, feature = "mpris"))]
pub mod mpris;
pub mod playlist_io;
pub mod podcast_directory;
pub mod radio_browser;
pub mod radio_playlist_io;
pub mod theme;
pub mod ui;
pub mod widgets;

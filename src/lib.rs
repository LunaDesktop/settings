//! # luna-settings
//!
//! Centrum ustawień Luny (odpowiednik GNOME Settings / KDE System Settings)
//! zbudowane wyłącznie na **libLunie** — bez GTK i Qt.
//!
//! Podział: [`app`] trzyma stan i akcje, [`pages`] opisuje grupy/wiersze oraz
//! czyta dane systemowe, [`ui`] rysuje i obsługuje wejście. Ten sam model można
//! renderować headless w testach.

pub mod accounts_admin;
pub mod app;
pub mod greeter_admin;
pub mod notify;
pub mod pages;
pub mod ui;

pub use app::run;

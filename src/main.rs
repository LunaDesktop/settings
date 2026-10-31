//! `luna-settings` — centrum ustawień środowiska i systemu Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_settings::run()
}

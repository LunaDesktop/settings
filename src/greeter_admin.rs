//! Zapis ustawień ekranu logowania (`/etc/luna/greeter.conf`).
//!
//! Ustawienia działają jako zwykły użytkownik, a plik systemowy może zapisać
//! tylko root. Dlatego najpierw próbujemy `pkexec luna-greeter-config write`
//! (polkit pokazuje okno autoryzacji — w Lunie robi to `luna-polkit`), a gdy
//! `pkexec` nie jest dostępny (np. uruchomienie `--preview` bez roota),
//! zapisujemy bezpośrednio pod `LUNA_GREETER_CONFIG` albo ścieżką systemową.
//!
//! Pomocnik jest podawany `pkexec` jako **ścieżka bezwzględna**: `pkexec`
//! resetuje `PATH` do katalogów systemowych, więc domyślny prefiks instalatora
//! (`/usr/local/bin`) nie byłby w nim widoczny.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use libluna_core::GreeterSettings;

/// Zapisuje konfigurację ekranu logowania.
pub fn save(settings: &GreeterSettings) -> Result<(), String> {
    // Tryb deweloperski/testowy: jawna ścieżka, bez pytania o hasło.
    if std::env::var_os("LUNA_GREETER_CONFIG").is_some() {
        return save_direct(settings);
    }
    match pkexec(settings) {
        Ok(()) => Ok(()),
        Err(Error::Unavailable) => save_direct(settings),
        Err(Error::Failed(message)) => Err(message),
    }
}

/// Zapisuje konfigurację, używając hasła administratora z formularza.
///
/// Hasło idzie do `sudo -S` przez stdin (nie przez argv), a ten sam strumień
/// dostaje potem konfigurację dla pomocnika — dzięki temu nie potrzebujemy
/// osobnego agenta polkit i formularz działa także bez działającego okna polkit.
pub fn save_with_password(settings: &GreeterSettings, password: &str) -> Result<(), String> {
    if std::env::var_os("LUNA_GREETER_CONFIG").is_some() {
        return save_direct(settings);
    }
    let helper = helper_path().ok_or_else(|| {
        "luna-greeter-config not found — zainstaluj pomocnik (scripts/install.sh)".to_owned()
    })?;
    let mut child = Command::new("sudo")
        .arg("-S")
        .arg("-p")
        .arg("")
        .arg("--")
        .arg(&helper)
        .arg("write")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                "sudo is not available — use the system authorization dialog".to_owned()
            }
            _ => format!("sudo: {err}"),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut input = String::with_capacity(password.len() + settings.render().len() + 2);
        input.push_str(password);
        input.push('\n');
        input.push_str(&settings.render());
        stdin
            .write_all(input.as_bytes())
            .map_err(|err| format!("sudo: {err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("sudo: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("sudo rejected the password");
    Err(detail.to_owned())
}

fn save_direct(settings: &GreeterSettings) -> Result<(), String> {
    let path = GreeterSettings::config_path();
    settings.save_local(&path).map_err(|err| {
        format!(
            "{}: {err} — zainstaluj pomocnik luna-greeter-config (scripts/install.sh)",
            path.display()
        )
    })
}

enum Error {
    /// Brak `pkexec` albo pomocnika — nie ma czego pytać o autoryzację.
    Unavailable,
    Failed(String),
}

fn pkexec(settings: &GreeterSettings) -> Result<(), Error> {
    let helper = helper_path().ok_or(Error::Unavailable)?;
    let mut child = Command::new("pkexec")
        .arg(helper)
        .arg("write")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::Unavailable,
            _ => Error::Failed(format!("pkexec: {err}")),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(settings.render().as_bytes())
            .map_err(|err| Error::Failed(format!("pkexec: {err}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| Error::Failed(format!("pkexec: {err}")))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    // Ostatnia linia zwykle niesie właściwy powód (anulowanie, brak agenta polkit).
    let message = detail
        .lines()
        .last()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "luna-greeter-config zakończył się kodem {}",
                output.status.code().unwrap_or(-1)
            )
        });
    Err(Error::Failed(message))
}

/// Szuka `luna-greeter-config`: obok `luna-settings`, w katalogach instalacji
/// i na końcu w `PATH` (uruchomienie z `target/debug`).
fn helper_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("luna-greeter-config"));
        }
    }
    candidates.extend([
        PathBuf::from("/usr/bin/luna-greeter-config"),
        PathBuf::from("/usr/local/bin/luna-greeter-config"),
        PathBuf::from("/usr/lib/luna/luna-greeter-config"),
    ]);
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join("luna-greeter-config")));
    }
    candidates.into_iter().find(|path| is_executable(path))
}

fn is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_lookup_ignores_missing_paths() {
        // Kontrakt: brak pliku nie może panikować ani zwracać nieistniejącej ścieżki.
        if let Some(path) = helper_path() {
            assert!(path.is_absolute(), "pomocnik musi mieć ścieżkę bezwzględną");
            assert!(is_executable(&path));
        }
    }
}

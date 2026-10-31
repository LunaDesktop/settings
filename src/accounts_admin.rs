//! Wywołanie pomocnika `luna-accounts` z uprawnieniami administratora.
//!
//! Najpierw `sudo -S -v` uwierzytelnia hasło administratora (jedna linia stdin),
//! potem `sudo -n` uruchamia pomocnika, który czyta już **tylko nowe hasło**.
//! Dzięki temu żadne hasło nie trafia do `argv` (widocznego w `ps`), a zachowanie
//! nie zależy od tego, czy `sudo` akurat ma ważny „timestamp”.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Operacja na kontach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountTask {
    /// Nowe konto: login, pełna nazwa (może być pusta), hasło podane osobno.
    Add { login: String, full_name: String },
    /// Zmiana hasła istniejącego konta.
    Password { login: String },
}

impl AccountTask {
    /// Argumenty pomocnika (bez hasła — to idzie przez stdin).
    fn helper_args(&self) -> Vec<String> {
        match self {
            AccountTask::Add { login, full_name } => {
                vec!["add".to_owned(), login.clone(), full_name.clone()]
            }
            AccountTask::Password { login } => {
                vec!["set-password".to_owned(), login.clone()]
            }
        }
    }

    /// Klucz komunikatu sukcesu (tłumaczony przez `tr`).
    pub fn success_key(&self) -> &'static str {
        match self {
            AccountTask::Add { .. } => "Account created",
            AccountTask::Password { .. } => "Password changed",
        }
    }
}

/// Ścieżka pomocnika: obok `luna-settings`, potem typowe katalogi instalacji.
pub fn helper_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LUNA_ACCOUNTS_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("luna-accounts");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    [
        "/usr/local/bin/luna-accounts",
        "/usr/bin/luna-accounts",
        "/usr/lib/luna/luna-accounts",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

/// Uwierzytelnia `sudo` **osobno**, żeby pomocnik zawsze dostał tylko swoje hasło.
///
/// `sudo -S` czyta linię z stdin wyłącznie wtedy, gdy wymaga hasła. Przy ważnym
/// „timestampie” nie czyta nic i wtedy pierwsza linia payloadu (hasło
/// administratora) trafiłaby do pomocnika jako hasło konta. Dlatego najpierw
/// robimy `sudo -v`, a potem uruchamiamy pomocnika przez `sudo -n`.
fn authenticate(admin_password: &str) -> Result<(), String> {
    let mut child = Command::new("sudo")
        .args(["-S", "-p", "", "-v"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                "Brak sudo — nie można zmienić konta bez uprawnień administratora".to_owned()
            }
            _ => format!("sudo: {err}"),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{admin_password}\n").as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("sudo: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(last_error(&output.stderr).unwrap_or_else(|| "odrzucono hasło administratora".to_owned()))
}

/// Ostatnia niepusta linia stderr (czytelniejsza niż cały log).
fn last_error(stderr: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Uruchamia pomocnika z `sudo`; hasła wyłącznie przez stdin.
pub fn run_with_password(
    task: &AccountTask,
    new_password: &str,
    admin_password: &str,
) -> Result<(), String> {
    let helper = helper_path().ok_or_else(|| {
        "Nie znaleziono luna-accounts — zainstaluj środowisko (scripts/install.sh)".to_owned()
    })?;
    authenticate(admin_password)?;

    let mut child = Command::new("sudo")
        .arg("-n")
        .arg("-p")
        .arg("")
        .arg("--")
        .arg(&helper)
        .args(task.helper_args())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                "Brak sudo — nie można zmienić konta bez uprawnień administratora".to_owned()
            }
            _ => format!("sudo: {err}"),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{new_password}\n").as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("sudo: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(last_error(&output.stderr).unwrap_or_else(|| "nieznany błąd".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_arguments_never_contain_the_password() {
        let add = AccountTask::Add {
            login: "ala".to_owned(),
            full_name: "Ala Kowalska".to_owned(),
        };
        assert_eq!(add.helper_args(), vec!["add", "ala", "Ala Kowalska"]);
        let password = AccountTask::Password {
            login: "bob".to_owned(),
        };
        assert_eq!(password.helper_args(), vec!["set-password", "bob"]);
        assert!(add.helper_args().iter().all(|arg| !arg.contains("tajne")));
        assert_eq!(add.success_key(), "Account created");
        assert_eq!(password.success_key(), "Password changed");
    }
}

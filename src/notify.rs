//! Testowe powiadomienie z Ustawień.
//!
//! Korzystamy bezpośrednio z D-Busa (`org.freedesktop.Notifications`), a nie
//! z `notify-send` — dzięki temu przycisk działa na każdej instalacji Luny,
//! o ile działa daemon powiadomień `luna-shell`.

use std::collections::HashMap;

use zbus::zvariant::Value;

/// Sprawdza, kto obecnie obsługuje `org.freedesktop.Notifications`.
///
/// Zwraca czytelny opis (np. `Luna 0.0.3 (libLuna)`) albo błąd, gdy nazwa
/// nie ma właściciela — to najczęstsza przyczyna „powiadomienia nie działają”.
pub fn server_status() -> Result<String, String> {
    let connection =
        zbus::blocking::Connection::session().map_err(|err| format!("D-Bus: {err}"))?;
    let has_owner: bool = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &"org.freedesktop.Notifications",
        )
        .map_err(|err| format!("D-Bus: {err}"))?
        .body()
        .deserialize()
        .map_err(|err| format!("D-Bus: {err}"))?;
    if !has_owner {
        return Err("org.freedesktop.Notifications has no owner".to_owned());
    }
    let reply = connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "GetServerInformation",
            &(),
        )
        .map_err(|err| format!("D-Bus: {err}"))?;
    let (name, vendor, version, _spec): (String, String, String, String) = reply
        .body()
        .deserialize()
        .map_err(|err| format!("D-Bus: {err}"))?;
    Ok(format!("{name} {version} ({vendor})"))
}

/// Wysyła powiadomienie do sesyjnego daemona powiadomień.
pub fn send_test_notification(title: &str, body: &str) -> Result<(), String> {
    let connection =
        zbus::blocking::Connection::session().map_err(|err| format!("D-Bus: {err}"))?;
    let actions: Vec<&str> = Vec::new();
    let hints: HashMap<&str, Value<'_>> = HashMap::new();
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Luna Settings",
                0u32,
                "dialog-information",
                title,
                body,
                actions,
                hints,
                5000i32,
            ),
        )
        .map_err(|err| format!("D-Bus: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostyka działa na żywej magistrali; w CI bez sesji D-Bus test jest pomijany.
    #[test]
    #[ignore = "wymaga sesyjnej magistrali D-Bus"]
    fn server_status_queries_the_bus() {
        match server_status() {
            Ok(info) => assert!(!info.is_empty(), "opis usługi nie może być pusty"),
            Err(err) => assert!(
                err.contains("has no owner"),
                "oczekiwano braku właściciela albo działającej usługi, jest: {err}"
            ),
        }
    }
}

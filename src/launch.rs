use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::Host;
use crate::{keychain, ui};

pub fn log_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Logs/rdOO")
}

/// Busca ironrdp-viewer: $RDOO_VIEWER, junto al ejecutable (bundle .app) o en vendor/ (desarrollo).
fn viewer_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RDOO_VIEWER") {
        return Some(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join("ironrdp-viewer");
        if p.exists() {
            return Some(p);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/IronRDP/target/release/ironrdp-viewer");
    dev.exists().then_some(dev)
}

fn safe_name(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn tail(path: &PathBuf, n: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Lanza la conexión en un hilo propio (no bloquea el menú).
pub fn connect(host: Host) {
    std::thread::spawn(move || run(host));
}

fn run(host: Host) {
    let Some(viewer) = viewer_path() else {
        ui::alert("rdOO", "No se encuentra ironrdp-viewer.", &["OK"]);
        return;
    };

    let account = host.keychain_account();
    let mut from_keychain = true;
    let password = match keychain::get(&account) {
        Some(p) => p,
        None => {
            from_keychain = false;
            match ui::ask_password(&host.name, &account) {
                Some(p) => p,
                None => return,
            }
        }
    };

    let _ = std::fs::create_dir_all(log_dir());
    let log = log_dir().join(format!("{}.log", safe_name(&host.name)));
    let _ = std::fs::remove_file(&log);

    let mut cmd = Command::new(&viewer);
    cmd.arg(&host.address)
        .arg("--username")
        .arg(&host.user)
        .arg("--log-file")
        .arg(&log)
        .env("RDP_PASSWORD", &password)
        .env("IRONRDP_LOG", "warn,ironrdp_client=info,ironrdp_viewer=info")
        .env("RDOO_TITLE", format!("{} — rdOO", host.name))
        .env("RDOO_LOGICAL_SIZE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.args(host.viewer_args());
    if !host.cmd_is_windows {
        cmd.env("RDOO_CMD_AS_CTRL", "1");
    }

    let started = Instant::now();
    let status = match cmd.spawn().and_then(|mut c| c.wait()) {
        Ok(s) => s,
        Err(e) => {
            ui::alert("rdOO", &format!("No se pudo lanzar el visor: {e}"), &["OK"]);
            return;
        }
    };

    // El visor puede salir con código 0 aunque falle: revisar también el log.
    let log_text = std::fs::read_to_string(&log).unwrap_or_default();
    let quick = started.elapsed() < Duration::from_secs(20);
    let failed = !status.success() || (quick && log_text.contains(" ERROR "));

    if !failed {
        if !from_keychain {
            keychain::set(&account, &password);
        }
        return;
    }

    if !from_keychain && !quick {
        // Sesión larga que terminó con error: la contraseña era válida.
        keychain::set(&account, &password);
    }
    let detail = explain(&log_text).unwrap_or_else(|| tail(&log, 6));
    let msg = format!("La conexión con «{}» terminó con error.\n\n{}", host.name, detail);
    let buttons: &[&str] = if from_keychain { &["Olvidar contraseña", "OK"] } else { &["OK"] };
    if ui::alert("rdOO", &msg, buttons).as_deref() == Some("Olvidar contraseña") {
        keychain::delete(&account);
    }
}

/// Traduce los fallos de negociación más comunes.
fn explain(log: &str) -> Option<String> {
    let msg = if log.contains("FailureCode(2)") {
        "El servidor solo acepta la seguridad RDP antigua (sin TLS), que este cliente no soporta.\n\
         En el Windows: Seguridad → «Negociar» o «SSL» (ver README, apartado Servidor)."
    } else if log.contains("FailureCode(5)") {
        "El servidor exige NLA (CredSSP) y no se pudo negociar."
    } else if log.contains("FailureCode(1)") {
        "El servidor exige TLS."
    } else if log.contains("FailureCode(3)") {
        "El servidor no tiene certificado TLS."
    } else if log.contains("0xc000006d") || log.to_lowercase().contains("logon failure") || log.contains("SEC_E_LOGON_DENIED") {
        "Usuario o contraseña incorrectos.\nRevisa el usuario en Ajustes (⌘,) o pulsa «Olvidar contraseña» para volver a escribirla."
    } else if log.contains("Connection refused") || log.contains("timed out") || log.contains("TimedOut") {
        "No se puede conectar con el equipo (apagado, IP o puerto incorrectos, o fuera de la red)."
    } else {
        return None;
    };
    Some(msg.to_string())
}

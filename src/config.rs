use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Host {
    pub name: String,
    /// host o host:puerto
    pub address: String,
    /// Usuario guardado (sección [[login]]); si está, sustituye a user/domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Tamaño inicial de la ventana en puntos
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
    /// Escalado del escritorio remoto en % (100–500)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
    /// Transporte UDP (más fluido con mala red)
    #[serde(default, skip_serializing_if = "is_false")]
    pub udp: bool,
    /// Redirigir el micrófono
    #[serde(default, skip_serializing_if = "is_false")]
    pub microphone: bool,
    /// Cursor local en vez del del servidor
    #[serde(default, skip_serializing_if = "is_false")]
    pub local_cursor: bool,
    /// Desactivar H.264 (modo gráfico antiguo)
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_h264: bool,
    /// Desactivar el portapapeles compartido
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_clipboard: bool,
    /// Cmd envía la tecla Windows (por defecto Cmd actúa como Ctrl)
    #[serde(default, skip_serializing_if = "is_false")]
    pub cmd_is_windows: bool,
    /// Argumentos extra para ironrdp-viewer
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

/// Usuario reutilizable entre conexiones (comparte contraseña en el Llavero).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Login {
    pub name: String,
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

impl Host {
    pub fn keychain_account(&self) -> String {
        match &self.login {
            Some(l) => format!("login:{l}"),
            None => format!("{}@{}", self.user, self.address),
        }
    }

    /// Copia con usuario y dominio tomados del login asociado.
    pub fn resolve(&self, logins: &[Login]) -> Result<Host, String> {
        let mut h = self.clone();
        if let Some(name) = &self.login {
            let l = logins
                .iter()
                .find(|l| &l.name == name)
                .ok_or_else(|| format!("«{}» usa el usuario «{name}», que no existe", self.name))?;
            h.user = l.user.clone();
            h.domain = l.domain.clone();
        }
        if h.user.is_empty() {
            return Err(format!("«{}» no tiene usuario", self.name));
        }
        Ok(h)
    }

    /// Opciones del visor derivadas de los campos.
    pub fn viewer_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        if let Some(d) = &self.domain {
            a.extend(["--domain".into(), d.clone()]);
        }
        if let Some(w) = self.width {
            a.extend(["--desktop-width".into(), w.to_string()]);
        }
        if let Some(h) = self.height {
            a.extend(["--desktop-height".into(), h.to_string()]);
        }
        if let Some(s) = self.scale {
            a.extend(["--scale-desktop".into(), s.to_string()]);
        }
        if self.udp {
            a.push("--udp".into());
        }
        if self.microphone {
            a.push("--microphone".into());
        }
        if self.local_cursor {
            a.push("--no-server-pointer".into());
        }
        if self.no_h264 {
            a.push("--egfx=false".into());
        }
        if self.no_clipboard {
            a.extend(["--clipboard-type".into(), "disable".into()]);
        }
        a.extend(self.args.iter().cloned());
        a
    }
}

#[derive(Default, Deserialize, Serialize)]
pub struct File {
    #[serde(default, rename = "login", skip_serializing_if = "Vec::is_empty")]
    pub logins: Vec<Login>,
    #[serde(default, rename = "host")]
    pub hosts: Vec<Host>,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

/// iCloud Drive si existe; si no, ~/.config/rdoo. `RDOO_CONFIG` lo sobrescribe.
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("RDOO_CONFIG") {
        return PathBuf::from(p);
    }
    let icloud = home().join("Library/Mobile Documents/com~apple~CloudDocs");
    let dir = if icloud.is_dir() {
        icloud.join("rdOO")
    } else {
        home().join(".config/rdoo")
    };
    dir.join("hosts.toml")
}

const HEADER: &str = "# rdOO — lista de equipos (se sincroniza por iCloud Drive)\n\
# Se puede editar desde la app: menú «Ajustes…» (⌘,).\n\
# Las contraseñas NO van aquí: se piden la primera vez y se guardan en el Llavero.\n\n";

const EXAMPLE: &str = r#"# Usuarios reutilizables (opcional): varias conexiones pueden compartirlos.
# [[login]]
# name = "Admin oficina"
# user = "administrador"
# domain = "EMPRESA"

[[host]]
name = "Ejemplo Windows"
address = "192.168.1.50"      # o "host:3389"
user = "usuario"               # o bien: login = "Admin oficina"
# domain = "EMPRESA"
# width = 1280                 # tamaño inicial de la ventana, en puntos
# height = 800
# scale = 100
# udp = true
# microphone = true
# local_cursor = true
# no_h264 = true
# no_clipboard = true          # sin portapapeles compartido
# cmd_is_windows = true        # Cmd = tecla Windows (por defecto Cmd = Ctrl)
# args = ["--audio-latency", "200"]
"#;

pub fn ensure_exists(path: &Path) {
    // Migración desde el nombre anterior (rdAPP): mover la carpeta si la nueva no existe.
    if !path.exists()
        && let Some(dir) = path.parent()
    {
        let old = dir.with_file_name("rdAPP");
        if old.join("hosts.toml").exists() && !dir.exists() {
            let _ = std::fs::rename(&old, dir);
        }
    }
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("{HEADER}{EXAMPLE}"));
    }
}

pub fn load(path: &Path) -> Result<File, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("No se puede leer hosts.toml: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("Error en hosts.toml: {}", e.message()))
}

/// Guarda de forma atómica (archivo temporal + rename).
pub fn save(path: &Path, file: &File) -> Result<(), String> {
    let body = toml::to_string(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, format!("{HEADER}{body}")).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

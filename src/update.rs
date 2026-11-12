//! Comprobación y descarga de actualizaciones desde el servidor de apps.
//!
//! Reutiliza el protocolo de fileOO: feed JSON pequeño, descarga con `curl`,
//! validación SHA-256 con `shasum`, extracción con `ditto` y un script que
//! sustituye la app cuando termina el proceso. La UI es una ventana wry/HTML
//! para mantener el mismo lenguaje visual que «Ajustes».

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tao::dpi::LogicalSize;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder, WindowId};
use wry::{WebView, WebViewBuilder};

use crate::UserEvent;

const DEFAULT_FEED: &str =
    "https://raw.githubusercontent.com/imr7997rmi/nivvoo-apps/main/rdOO-latest.json";
const APP_NAME: &str = "rdOO";
const HTML: &str = include_str!("update.html");

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub build: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug)]
pub enum UpdateEvent {
    Checked {
        generation: u64,
        result: Result<Manifest, String>,
    },
    Ready {
        generation: u64,
    },
    Error {
        generation: u64,
        message: String,
    },
}

#[derive(Clone)]
enum State {
    Idle,
    Checking,
    Available(Manifest),
    Downloading { version: String },
    Installing,
    UpToDate,
    Error(String),
}

impl State {
    fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Downloading { .. } | Self::Installing)
    }
}

struct Progress {
    done: u64,
    total: u64,
    phase: &'static str,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            done: 0,
            total: 0,
            phase: "downloading",
        }
    }
}

pub(crate) struct Update {
    feed: String,
    proxy: EventLoopProxy<UserEvent>,
    state: State,
    window: Option<UpdateWindow>,
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    generation: u64,
    auto: bool,
}

struct UpdateWindow {
    window: Window,
    webview: WebView,
}

impl UpdateWindow {
    fn open(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        view: serde_json::Value,
    ) -> Option<Self> {
        let window = WindowBuilder::new()
            .with_title("Actualización")
            .with_inner_size(LogicalSize::new(480.0, 300.0))
            .with_min_inner_size(LogicalSize::new(420.0, 240.0))
            .with_resizable(false)
            .build(target)
            .ok()?;
        let init = format!("window.RDOO_UPDATE_VIEW = {view};");
        let webview = WebViewBuilder::new()
            .with_html(HTML)
            .with_initialization_script(init)
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::UpdateIpc(req.body().clone()));
            })
            .build(&window)
            .ok()?;
        window.set_focus();
        Some(Self { window, webview })
    }

    fn id(&self) -> WindowId {
        self.window.id()
    }

    fn focus(&self) {
        self.window.set_visible(true);
        self.window.set_minimized(false);
        self.window.set_focus();
    }

    fn render(&self, view: &serde_json::Value) {
        let _ = self
            .webview
            .evaluate_script(&format!("rdooUpdate({view});"));
    }
}

impl Update {
    pub(crate) fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let feed = std::env::var("RDOO_UPDATE_URL").unwrap_or_else(|_| DEFAULT_FEED.to_string());
        let auto = std::env::var("RDOO_UPDATE_AUTO")
            .map(|v| v == "1")
            .unwrap_or(false);
        Self {
            feed,
            proxy,
            state: State::Idle,
            window: None,
            progress: Arc::new(Mutex::new(Progress::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            generation: 0,
            auto,
        }
    }

    pub(crate) fn auto_check(&self) -> bool {
        self.auto
    }

    pub(crate) fn is_downloading(&self) -> bool {
        matches!(self.state, State::Downloading { .. })
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(UpdateWindow::id)
    }

    pub(crate) fn check(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        if self.state.busy() {
            self.focus();
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.cancel.store(false, Ordering::SeqCst);
        self.state = State::Checking;
        self.show(target);

        let proxy = self.proxy.clone();
        let url = self.feed.clone();
        std::thread::spawn(move || {
            let result = fetch_manifest(&url);
            let _ = proxy.send_event(UserEvent::Update(UpdateEvent::Checked {
                generation,
                result,
            }));
        });
    }

    pub(crate) fn handle_event(
        &mut self,
        event: UpdateEvent,
        target: &EventLoopWindowTarget<UserEvent>,
    ) -> bool {
        match event {
            UpdateEvent::Checked { generation, result } if generation == self.generation => {
                match result {
                    Ok(manifest) => {
                        let version = current_version();
                        let build = current_build();
                        if is_newer(&manifest, &version, &build) {
                            let download = self.auto.then(|| manifest.clone());
                            self.state = State::Available(manifest);
                            if let Some(manifest) = download {
                                self.start_download(manifest);
                            } else {
                                self.show(target);
                                self.render();
                            }
                        } else {
                            self.state = State::UpToDate;
                            self.show(target);
                            self.render();
                        }
                    }
                    Err(error) => {
                        self.state = State::Error(error);
                        self.show(target);
                        self.render();
                    }
                }
                false
            }
            UpdateEvent::Ready { generation } if generation == self.generation => {
                self.state = State::Installing;
                self.render();
                true
            }
            UpdateEvent::Error { generation, message } if generation == self.generation => {
                self.state = State::Error(message);
                self.show(target);
                self.render();
                false
            }
            _ => false,
        }
    }

    pub(crate) fn handle_ipc(
        &mut self,
        message: &str,
        target: &EventLoopWindowTarget<UserEvent>,
    ) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(message) else {
            return;
        };
        match value["cmd"].as_str() {
            Some("download") => {
                if let State::Available(manifest) = self.state.clone() {
                    self.start_download(manifest);
                }
            }
            Some("cancel") => self.dismiss(),
            Some("close") => self.dismiss(),
            Some("check") => self.check(target),
            _ => {}
        }
    }

    pub(crate) fn poll(&mut self) {
        if self.is_downloading() {
            self.render();
        }
    }

    pub(crate) fn window_closed(&mut self) {
        self.dismiss();
    }

    fn start_download(&mut self, manifest: Manifest) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.cancel.store(false, Ordering::SeqCst);
        if let Ok(mut progress) = self.progress.lock() {
            *progress = Progress {
                done: 0,
                total: manifest.size,
                phase: "downloading",
            };
        }
        self.state = State::Downloading {
            version: manifest.version.clone(),
        };
        self.render();

        let proxy = self.proxy.clone();
        let feed = self.feed.clone();
        let progress = self.progress.clone();
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            match download_and_prepare(&manifest, &feed, &progress, &cancel) {
                Ok(()) => {
                    let _ = proxy.send_event(UserEvent::Update(UpdateEvent::Ready { generation }));
                }
                Err(message) => {
                    let _ = proxy.send_event(UserEvent::Update(UpdateEvent::Error {
                        generation,
                        message,
                    }));
                }
            }
        });
    }

    fn dismiss(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.cancel.store(true, Ordering::SeqCst);
        self.state = State::Idle;
        self.window = None;
    }

    fn focus(&self) {
        if let Some(window) = &self.window {
            window.focus();
        }
    }

    fn show(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        if self.window.is_none() {
            let view = self.view();
            self.window = UpdateWindow::open(target, self.proxy.clone(), view);
        }
        if let Some(window) = &self.window {
            window.focus();
            window.render(&self.view());
        }
    }

    fn render(&self) {
        if let Some(window) = &self.window {
            window.render(&self.view());
        }
    }

    fn view(&self) -> serde_json::Value {
        let version = current_version();
        let build = current_build();
        let current = if build.is_empty() {
            format!("{APP_NAME} {version}")
        } else {
            format!("{APP_NAME} {version} (compilación {build})")
        };
        let mut view = serde_json::json!({
            "mode": "checking",
            "title": "Actualización",
            "message": "Comprobando actualizaciones…",
            "notes": "",
            "detail": "",
            "progress": 0.0,
            "current": current,
        });

        match &self.state {
            State::Idle => {}
            State::Checking => {}
            State::Available(manifest) => {
                let mut message = format!("{APP_NAME} {} ({})", manifest.version, manifest.build);
                if manifest.size > 0 {
                    message.push_str(&format!(" · {}", fmt_bytes(manifest.size)));
                }
                view["mode"] = "available".into();
                view["title"] = "Nueva versión disponible".into();
                view["message"] = message.into();
                view["notes"] = manifest.notes.trim().into();
            }
            State::Downloading { version } => {
                let progress = self
                    .progress
                    .lock()
                    .map(|p| (p.done, p.total, p.phase))
                    .unwrap_or((0, 0, "downloading"));
                let (done, total, phase) = progress;
                let fraction = if total > 0 {
                    (done as f64 / total as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let detail = match phase {
                    "verifying" => "Verificando la descarga…".to_string(),
                    "extracting" => "Descomprimiendo la actualización…".to_string(),
                    "installing" => "Preparando la instalación…".to_string(),
                    _ if total > 0 => format!(
                        "{} de {} · {}%",
                        fmt_bytes(done),
                        fmt_bytes(total),
                        (fraction * 100.0).round() as u64
                    ),
                    _ => fmt_bytes(done),
                };
                view["mode"] = "downloading".into();
                view["title"] = "Actualización".into();
                view["message"] = format!("Descargando {APP_NAME} {version}…").into();
                view["detail"] = detail.into();
                view["progress"] = if phase == "downloading" { fraction } else { 1.0 }.into();
            }
            State::Installing => {
                view["mode"] = "installing".into();
                view["title"] = "Actualización".into();
                view["message"] = "Instalando la actualización…".into();
                view["detail"] = "La aplicación se reiniciará automáticamente.".into();
                view["progress"] = 1.0.into();
            }
            State::UpToDate => {
                let current = view["current"].clone();
                view["mode"] = "info".into();
                view["title"] = "Estás en la última versión".into();
                view["message"] = current;
            }
            State::Error(message) => {
                view["mode"] = "error".into();
                view["title"] = "No se pudo actualizar".into();
                view["message"] = message.clone().into();
            }
        }
        view
    }
}

fn fetch_manifest(url: &str) -> Result<Manifest, String> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "-fL",
            "--connect-timeout",
            "20",
            "--max-time",
            "20",
            "--silent",
            "--show-error",
            url,
        ])
        .output()
        .map_err(|e| format!("No se pudo ejecutar curl: {e}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if message.is_empty() {
            format!("Error HTTP al consultar {url}")
        } else {
            message
        });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("El feed no es válido: {e}"))
}

fn resolve_url(manifest: &Manifest, feed: &str) -> String {
    if manifest.url.starts_with("http://")
        || manifest.url.starts_with("https://")
        || manifest.url.starts_with("file://")
    {
        return manifest.url.clone();
    }
    let name = if !manifest.file.is_empty() {
        manifest.file.clone()
    } else if !manifest.url.is_empty() {
        manifest.url.clone()
    } else {
        format!("{APP_NAME}-latest.zip")
    };
    let base = feed.rsplit_once('/').map(|(base, _)| base).unwrap_or(feed);
    format!("{base}/{name}")
}

fn is_newer(manifest: &Manifest, version: &str, build: &str) -> bool {
    if !manifest.build.is_empty() && !build.is_empty() {
        manifest.build.as_str() > build
    } else {
        !manifest.version.is_empty() && manifest.version != version
    }
}

pub(crate) fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub(crate) fn current_build() -> String {
    current_app_bundle()
        .and_then(|app| plist_value(&app.join("Contents/Info.plist"), "CFBundleVersion"))
        .unwrap_or_default()
}

fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|path| path.extension().map(|ext| ext == "app").unwrap_or(false))
        .map(Path::to_path_buf)
}

fn plist_value(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let marker = format!("<key>{key}</key>");
    let rest = &text[text.find(&marker)? + marker.len()..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")?;
    Some(rest[start..start + end].trim().to_string())
}

fn fmt_bytes(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1} MB", n as f64 / 1e6).replace('.', ",")
    } else if n >= 1_000 {
        format!("{:.1} KB", n as f64 / 1e3).replace('.', ",")
    } else {
        format!("{n} B")
    }
}

fn download_and_prepare(
    manifest: &Manifest,
    feed: &str,
    progress: &Arc<Mutex<Progress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let workdir = std::env::temp_dir().join(format!("{APP_NAME}-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(&workdir)
        .map_err(|e| format!("No se pudo crear la carpeta temporal: {e}"))?;

    let zip = workdir.join(format!("{APP_NAME}-latest.zip"));
    let url = resolve_url(manifest, feed);
    let mut child = Command::new("/usr/bin/curl")
        .args(["-fL", "--connect-timeout", "20", "--max-time", "600", "--silent", "--show-error", "--output"])
        .arg(&zip)
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&workdir);
            format!("No se pudo iniciar la descarga: {e}")
        })?;

    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&workdir);
            return Err("Actualización cancelada.".into());
        }
        let done = std::fs::metadata(&zip).map(|m| m.len()).unwrap_or(0);
        if let Ok(mut p) = progress.lock() {
            p.done = done;
            p.total = manifest.size;
            p.phase = "downloading";
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let _ = std::fs::remove_dir_all(&workdir);
                    return Err("La descarga no terminó correctamente.".into());
                }
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&workdir);
                return Err(format!("Error durante la descarga: {e}"));
            }
        }
    }

    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("Actualización cancelada.".into());
    }
    let size = std::fs::metadata(&zip).map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("La descarga no creó ningún archivo.".into());
    }

    set_phase(progress, size, manifest.size, "verifying");
    let hash = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(&zip)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase()
        })
        .unwrap_or_default();
    if !manifest.sha256.is_empty() && hash != manifest.sha256.to_lowercase() {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("El archivo descargado no coincide con el SHA-256 publicado.".into());
    }

    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("Actualización cancelada.".into());
    }
    set_phase(progress, size, manifest.size, "extracting");
    let extract = workdir.join("extraido");
    let _ = std::fs::create_dir_all(&extract);
    let unzip = Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&zip)
        .arg(&extract)
        .status();
    if !unzip.map(|s| s.success()).unwrap_or(false) {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("No se pudo descomprimir la actualización.".into());
    }
    let new_app = std::fs::read_dir(&extract)
        .ok()
        .and_then(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.extension().map(|ext| ext == "app").unwrap_or(false))
        });
    let Some(new_app) = new_app else {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("El ZIP no contiene ninguna aplicación.".into());
    };

    let target = current_app_bundle().ok_or_else(|| {
        let _ = std::fs::remove_dir_all(&workdir);
        "No se pudo localizar la aplicación instalada.".to_string()
    })?;

    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&workdir);
        return Err("Actualización cancelada.".into());
    }
    set_phase(progress, size, manifest.size, "installing");
    let result = install_update(&new_app, &target, &workdir);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&workdir);
    }
    result
}

fn set_phase(progress: &Arc<Mutex<Progress>>, done: u64, total: u64, phase: &'static str) {
    if let Ok(mut p) = progress.lock() {
        p.done = done;
        p.total = total;
        p.phase = phase;
    }
}

fn install_update(new_app: &Path, target: &Path, workdir: &Path) -> Result<(), String> {
    let script = workdir.join("instalar.sh");
    let body = "#!/bin/sh\npid=$1\ntarget=$2\nnew=$3\ntmp=$4\nwhile kill -0 \"$pid\" 2>/dev/null; do sleep 0.2; done\nbackup=\"$target.old.$$\"\nrm -rf \"$backup\"\nif mv \"$target\" \"$backup\" 2>/dev/null; then\n  if ditto \"$new\" \"$target\"; then\n    xattr -dr com.apple.quarantine \"$target\" 2>/dev/null || true\n    rm -rf \"$backup\"\n    open \"$target\"\n  else\n    rm -rf \"$target\"\n    mv \"$backup\" \"$target\"\n    open \"$target\"\n  fi\nelse\n  open \"$target\"\nfi\nrm -rf \"$tmp\"\n";
    std::fs::write(&script, body)
        .map_err(|e| format!("No se pudo preparar el instalador: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }

    let mut command = Command::new("/bin/sh");
    command
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(target)
        .arg(new_app)
        .arg(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("No se pudo iniciar el instalador: {e}"))
}

//! Editor visual de conexiones: ventana tao + WebView del sistema (wry).

use tao::dpi::LogicalSize;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder, WindowId};
use wry::{WebView, WebViewBuilder};

use crate::config::File;
use crate::{UserEvent, VERSION};

const HTML: &str = include_str!("editor.html");

pub struct Editor {
    window: Window,
    webview: WebView,
}

impl Editor {
    pub fn open(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        file: &File,
    ) -> Option<Self> {
        let window = WindowBuilder::new()
            .with_title("Ajustes")
            .with_inner_size(LogicalSize::new(860.0, 800.0))
            .with_min_inner_size(LogicalSize::new(640.0, 440.0))
            .build(target)
            .ok()?;

        // Qué cuentas tienen contraseña en el Llavero (solo booleanos, nunca la contraseña).
        let mut pw = serde_json::Map::new();
        let accounts = file
            .logins
            .iter()
            .map(|l| format!("login:{}", l.name))
            .chain(file.hosts.iter().filter(|h| h.login.is_none()).map(|h| h.keychain_account()));
        for acc in accounts {
            if crate::keychain::has(&acc) {
                pw.insert(acc, serde_json::Value::Bool(true));
            }
        }
        let init = serde_json::json!({
            "version": VERSION,
            "build": crate::update::current_build(),
            "hosts": file.hosts,
            "logins": file.logins,
            "pw": pw
        });
        let webview = WebViewBuilder::new()
            .with_html(HTML)
            .with_initialization_script(format!("window.RDOO_INIT = {init};"))
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::Ipc(req.body().clone()));
            })
            .build(&window)
            .ok()?;

        window.set_focus();
        Some(Self { window, webview })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.set_visible(true);
        self.window.set_focus();
    }

    pub fn saved(&self, result: Result<(), String>) {
        let js = match result {
            Ok(()) => "rdooSaved(true)".to_string(),
            Err(e) => format!("rdooSaved(false, {})", serde_json::Value::String(e)),
        };
        let _ = self.webview.evaluate_script(&js);
    }

    pub fn password_saved(&self, account: &str) {
        let _ = self
            .webview
            .evaluate_script(&format!("rdooPwSaved({})", serde_json::Value::String(account.to_string())));
    }

    pub fn notice(&self, msg: &str) {
        let _ = self
            .webview
            .evaluate_script(&format!("rdooNotice({})", serde_json::Value::String(msg.to_string())));
    }
}

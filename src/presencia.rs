//! «Mostrar en el Dock» y «Mostrar en la barra de menús» (común a la suite nivvoo Apps).
//!
//! Las dos opciones se guardan en las preferencias de la app (NSUserDefaults).
//! Si se oculta del Dock, el icono de la barra de menús se muestra siempre:
//! si no, no habría forma de volver a abrir la app.
//!
//! Este archivo es idéntico en fileOO, langOO y rdOO. Usar solo en el hilo principal.

#![allow(dead_code)]

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSString, NSUserDefaults};

const CLAVE_DOCK: &str = "nivvooMostrarEnDock";
const CLAVE_BARRA: &str = "nivvooMostrarEnBarraMenus";

pub const TITULO_DOCK: &str = "Mostrar en el Dock";
pub const TITULO_BARRA: &str = "Mostrar en la barra de menús";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presencia {
    pub dock: bool,
    pub barra: bool,
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn leer_bool(defaults: &NSUserDefaults, clave: &str, por_defecto: bool) -> bool {
    let k = ns(clave);
    if defaults.objectForKey(&k).is_some() { defaults.boolForKey(&k) } else { por_defecto }
}

impl Presencia {
    /// Lee las preferencias. `barra_por_defecto`: si la app muestra el icono de la
    /// barra de menús cuando el usuario aún no ha elegido.
    pub fn leer(barra_por_defecto: bool) -> Self {
        let d = NSUserDefaults::standardUserDefaults();
        Self {
            dock: leer_bool(&d, CLAVE_DOCK, true),
            barra: leer_bool(&d, CLAVE_BARRA, barra_por_defecto),
        }
        .normalizada()
    }

    /// Sin Dock, la barra de menús es obligatoria.
    pub fn normalizada(mut self) -> Self {
        if !self.dock {
            self.barra = true;
        }
        self
    }

    pub fn guardar(self) {
        let d = NSUserDefaults::standardUserDefaults();
        d.setBool_forKey(self.dock, &ns(CLAVE_DOCK));
        d.setBool_forKey(self.barra, &ns(CLAVE_BARRA));
    }
}

/// Muestra u oculta el icono del Dock (política Regular / Accessory).
pub fn aplicar_dock(dock: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let politica = if dock {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    if app.activationPolicy() != politica {
        app.setActivationPolicy(politica);
        // Al volver al Dock, macOS no reactiva la app por sí solo.
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

/// Una opción del menú del icono. `titulo == "-"` es un separador.
/// `objetivo == None` envía la acción por la cadena de respuesta (p. ej. al delegado de la app).
pub struct Opcion<'a> {
    pub titulo: &'a str,
    pub accion: Option<Sel>,
    pub objetivo: Option<&'a AnyObject>,
}

/// Icono en la barra de menús. Se quita al soltarlo.
pub struct IconoBarra {
    item: Retained<NSStatusItem>,
}

impl IconoBarra {
    /// `simbolo`: nombre de SF Symbol (se dibuja como plantilla, como los iconos del sistema).
    pub fn crear(simbolo: &str, texto_alternativo: &str, opciones: &[Opcion]) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let barra = NSStatusBar::systemStatusBar();
        let item = barra.statusItemWithLength(NSVariableStatusItemLength);
        if let Some(boton) = item.button(mtm) {
            let imagen = NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns(simbolo), Some(&ns(texto_alternativo)));
            match imagen {
                Some(img) => {
                    img.setTemplate(true);
                    boton.setImage(Some(&img));
                }
                None => boton.setTitle(&ns(texto_alternativo)),
            }
            boton.setToolTip(Some(&ns(texto_alternativo)));
        }
        let menu = NSMenu::new(mtm);
        for o in opciones {
            if o.titulo == "-" {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
                continue;
            }
            // SAFETY: inicialización estándar de NSMenuItem en el hilo principal.
            let mi = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(o.titulo), o.accion, &ns(""))
            };
            // SAFETY: el objetivo vive mientras viva la app (lo retiene quien lo crea).
            unsafe { mi.setTarget(o.objetivo) };
            menu.addItem(&mi);
        }
        item.setMenu(Some(&menu));
        Some(Self { item })
    }
}

impl Drop for IconoBarra {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

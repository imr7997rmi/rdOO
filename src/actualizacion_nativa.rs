//! UI nativa de actualización común a la suite (mismo aspecto que fileOO):
//! avisos con NSAlert (llevan el icono de la app) y una ventanita de progreso
//! de 460×130 con barra, detalle y botón «Cancelar».
//!
//! Este archivo es idéntico en rdOO y langOO. Solo debe usarse desde el hilo
//! principal.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSBackingStoreType, NSButton, NSColor, NSFont,
    NSFloatingWindowLevel, NSProgressIndicator, NSProgressIndicatorStyle, NSTextField, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn activar(mtm: MainThreadMarker) {
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

/// Aviso con un solo botón.
pub fn alerta(titulo: &str, texto: &str, boton: &str, critica: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    activar(mtm);
    let a = NSAlert::new(mtm);
    a.setMessageText(&ns(titulo));
    a.setInformativeText(&ns(texto));
    if critica {
        a.setAlertStyle(NSAlertStyle::Critical);
    }
    a.addButtonWithTitle(&ns(boton));
    a.runModal();
}

/// Pregunta con dos botones. Devuelve true si se pulsa `si`.
pub fn confirmar(titulo: &str, texto: &str, si: &str, no: &str) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    activar(mtm);
    let a = NSAlert::new(mtm);
    a.setMessageText(&ns(titulo));
    a.setInformativeText(&ns(texto));
    a.addButtonWithTitle(&ns(si));
    a.addButtonWithTitle(&ns(no));
    a.runModal() == NSAlertFirstButtonReturn
}

/// Ventanita de progreso de la descarga.
pub struct Progreso {
    win: Retained<NSWindow>,
    titulo: Retained<NSTextField>,
    detalle: Retained<NSTextField>,
    barra: Retained<NSProgressIndicator>,
}

impl Progreso {
    pub fn abrir(ventana: &str, titulo: &str, cancelar: &str) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        activar(mtm);
        // SAFETY: construcción estándar de una ventana AppKit en el hilo principal.
        unsafe {
            let win = NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0., 0., 460., 130.),
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            );
            win.setTitle(&ns(ventana));
            win.setReleasedWhenClosed(false);
            win.setLevel(NSFloatingWindowLevel);
            win.center();
            let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 460., 130.));
            win.setContentView(Some(&view));

            let t = NSTextField::labelWithString(&ns(titulo), mtm);
            t.setFrame(rect(16., 90., 428., 20.));
            t.setFont(Some(&NSFont::boldSystemFontOfSize(13.)));
            view.addSubview(&t);

            let barra = NSProgressIndicator::initWithFrame(NSProgressIndicator::alloc(mtm), rect(16., 64., 428., 14.));
            barra.setStyle(NSProgressIndicatorStyle::Bar);
            barra.setIndeterminate(true);
            barra.setMinValue(0.);
            barra.setMaxValue(100.);
            barra.startAnimation(None);
            view.addSubview(&barra);

            let d = NSTextField::labelWithString(&ns(""), mtm);
            d.setFrame(rect(16., 40., 330., 17.));
            d.setFont(Some(&NSFont::systemFontOfSize(11.)));
            d.setTextColor(Some(&NSColor::secondaryLabelColor()));
            view.addSubview(&d);

            // «Cancelar» oculta la ventana; quien la usa lo detecta con `cancelado()`.
            let b = NSButton::buttonWithTitle_target_action(&ns(cancelar), Some(&win), Some(sel!(orderOut:)), mtm);
            b.setFrame(rect(356., 34., 88., 28.));
            view.addSubview(&b);

            win.makeKeyAndOrderFront(None);
            Some(Self { win, titulo: t, detalle: d, barra })
        }
    }

    /// `pct` en 0..=100; `None` = barra indeterminada.
    pub fn actualizar(&self, pct: Option<f64>, detalle: &str) {
        match pct {
            Some(p) => {
                self.barra.setIndeterminate(false);
                self.barra.setDoubleValue(p.clamp(0., 100.));
            }
            None => {
                self.barra.setIndeterminate(true);
                // SAFETY: llamada estándar en el hilo principal.
                unsafe { self.barra.startAnimation(None) };
            }
        }
        self.detalle.setStringValue(&ns(detalle));
    }

    pub fn titulo(&self, titulo: &str) {
        self.titulo.setStringValue(&ns(titulo));
    }

    /// true si el usuario ha pulsado «Cancelar».
    pub fn cancelado(&self) -> bool {
        !self.win.isVisible()
    }
}

impl Drop for Progreso {
    fn drop(&mut self) {
        self.win.orderOut(None);
    }
}

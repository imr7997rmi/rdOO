//! Exportar / importar el archivo de conexiones (hosts.toml) con paneles nativos.
//! Las contraseñas no van en el archivo (están en el Llavero). Solo en el hilo principal.

use std::path::{Path, PathBuf};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSSavePanel};
use objc2_foundation::NSString;

fn activar(mtm: MainThreadMarker) {
    #[allow(deprecated)]
    objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

/// Pregunta dónde guardar y copia el archivo. Ok(None) si se cancela.
pub fn exportar(origen: &Path) -> Result<Option<PathBuf>, String> {
    let mtm = MainThreadMarker::new().ok_or("hilo incorrecto")?;
    activar(mtm);
    let panel = NSSavePanel::savePanel(mtm);
    panel.setNameFieldStringValue(&NSString::from_str("rdOO-conexiones.toml"));
    panel.setMessage(Some(&NSString::from_str(
        "Exportar conexiones y usuarios de rdOO (las contraseñas no se incluyen)",
    )));
    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    let destino = panel
        .URL()
        .and_then(|u| u.path())
        .map(|p| PathBuf::from(p.to_string()))
        .ok_or("ruta no válida")?;
    std::fs::copy(origen, &destino).map_err(|e| e.to_string())?;
    Ok(Some(destino))
}

/// Pide un archivo .toml. None si se cancela.
pub fn elegir_para_importar() -> Option<PathBuf> {
    let mtm = MainThreadMarker::new()?;
    activar(mtm);
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    panel.setMessage(Some(&NSString::from_str("Elige un archivo de conexiones de rdOO (.toml)")));
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    panel.URL().and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()))
}

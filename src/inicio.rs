//! «Abrir al iniciar sesión» (común a la suite nivvoo Apps).
//!
//! Registra la propia app con `SMAppService.mainAppService` (macOS 13+). La
//! fuente de verdad es el sistema, no los ajustes: el usuario puede quitarla
//! desde Ajustes del Sistema › General › Elementos de inicio de sesión y la
//! casilla lo refleja al volver a abrir Ajustes. En macOS 11-12 la opción no
//! se muestra.

use objc2::runtime::AnyClass;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// Texto estándar de macOS para esta opción (menú del Dock › Opciones).
#[allow(dead_code)]
pub const TITULO: &str = "Abrir al iniciar sesión";

pub enum Cambio {
    Hecho,
    /// Registrada, pero macOS pide aprobarla en Ajustes del Sistema (ya abiertos).
    NecesitaAprobacion,
}

/// ¿Puede esta instalación registrarse? (macOS 13+ y ejecutándose como .app)
pub fn disponible() -> bool {
    AnyClass::get(c"SMAppService").is_some()
        && std::env::current_exe()
            .map(|e| e.ancestors().any(|p| p.extension().is_some_and(|x| x == "app")))
            .unwrap_or(false)
}

fn estado() -> Option<SMAppServiceStatus> {
    disponible().then(|| unsafe { SMAppService::mainAppService().status() })
}

/// Activada (o pendiente de aprobación por el usuario).
pub fn activo() -> bool {
    matches!(
        estado(),
        Some(SMAppServiceStatus::Enabled) | Some(SMAppServiceStatus::RequiresApproval)
    )
}

pub fn fijar(activar: bool) -> Result<Cambio, String> {
    if !disponible() {
        return Err("requiere macOS 13 o posterior".into());
    }
    let servicio = unsafe { SMAppService::mainAppService() };
    let error = |e: objc2::rc::Retained<objc2_foundation::NSError>| e.localizedDescription().to_string();
    if activar {
        if unsafe { servicio.status() } != SMAppServiceStatus::Enabled {
            unsafe { servicio.registerAndReturnError() }.map_err(error)?;
        }
        if unsafe { servicio.status() } == SMAppServiceStatus::RequiresApproval {
            unsafe { SMAppService::openSystemSettingsLoginItems() };
            return Ok(Cambio::NecesitaAprobacion);
        }
    } else {
        let st = unsafe { servicio.status() };
        if st != SMAppServiceStatus::NotRegistered && st != SMAppServiceStatus::NotFound {
            unsafe { servicio.unregisterAndReturnError() }.map_err(error)?;
        }
    }
    Ok(Cambio::Hecho)
}

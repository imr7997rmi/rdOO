# rdOO — notas para continuar el desarrollo

Cliente RDP ligero para macOS. La app (`src/`) es un lanzador/gestor; la conexión la hace
`ironrdp-viewer` de IronRDP, empaquetado dentro de `rdOO.app/Contents/MacOS/`.

## Compilar
- `scripts/bundle.sh --install` → compila todo, firma ad-hoc e instala en `~/Applications/rdOO.app`.
- IronRDP se descarga en `vendor/IronRDP` fijado a `IRONRDP_REV` (bundle.sh) y se le aplica `patches/ironrdp-rdoo.patch`.
- Tras tocar `vendor/IronRDP`, regenerar el parche: `git -C vendor/IronRDP diff > patches/ironrdp-rdoo.patch`.
- Requiere cmake (para libopus). El bundle se firma fuera de Documents (iCloud añade xattrs que rompen codesign).
- Versión: `Cargo.toml` → se muestra en menú, editor, «Acerca de» e Info.plist. Etiquetar `vX.Y.Z` al publicar.
- Compilación (`CFBundleVersion`): fecha y hora `YYYYMMDD.HHMM`, generada por `bundle.sh`. El feed de actualizaciones compara ese campo.

## Arquitectura
- `main.rs`: bucle tao, icono de barra de menú (tray-icon/muda), menú de app, menú del Dock, watcher de hosts.toml.
- `config.rs`: `hosts.toml` (`[[login]]` usuarios reutilizables + `[[host]]` conexiones). Ruta: iCloud Drive `rdOO/hosts.toml`.
- `launcher.rs` + `launcher.html`: ventana principal de conexiones (búsqueda, ↩ conecta). IPC: launch, settings.
- `editor.rs` + `editor.html`: editor «Ajustes…» (⌘,) (wry/WKWebView). IPC JSON: save, connect, forget, forget_login, set_password, close.
- `keychain.rs`: Llavero, servicio `rdOO`; cuenta `user@address` o `login:<nombre>`.
- `launch.rs`: lanza el visor (contraseña por `RDP_PASSWORD`), log en `~/Library/Logs/rdOO/`, traduce errores.
- `dock.rs`: añade `applicationDockMenu:` al delegado de NSApplication.
- `update.rs` + `update.html`: feed `rdOO-latest.json`, descarga con curl, SHA-256 con shasum,
  extracción con ditto y script de sustitución tras cerrar la app. Se abre desde el menú
  rdOO → «Buscar actualizaciones…» y desde Ajustes → General. `RDOO_UPDATE_URL` cambia el feed.
- `scripts/publish-update.sh` publica `dist/rdOO-latest.zip/json` por SFTP; el workflow
  `.github/workflows/release.yml` compila universal, publica en `/apps` y en `nivvoo-apps`.

## Parche sobre IronRDP
- Redimensionado con EGFX: se da por completado al llegar ResetGraphics con el nuevo tamaño (antes: timeout + reconexión).
- Portapapeles de texto macOS: `ironrdp-viewer/src/mac_clipboard.rs` (sondeo de changeCount, texto UTF-16). Sin imágenes ni archivos aún.
- `RDOO_CMD_AS_CTRL`: Cmd se envía como Ctrl (rdOO lo activa salvo `cmd_is_windows`).
- `RDOO_TITLE` (título de ventana), `RDOO_LOGICAL_SIZE` (tamaño inicial en puntos), log `rdOO: sending resize request`.

## Limitaciones conocidas
- IronRDP no soporta «Seguridad RDP» antigua (RC4): el servidor debe aceptar TLS/NLA.
- Redirección de carpetas/impresoras de ironrdp-viewer solo en Windows.

## Ideas pendientes
- Portapapeles de imágenes/archivos, multimonitor, ajuste fino del teclado español, icono propio para la ventana del visor.

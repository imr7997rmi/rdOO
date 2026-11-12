#!/bin/sh
# Compila rdOO + ironrdp-viewer y genera rdOO.app (y opcionalmente lo instala).
# Uso: scripts/bundle.sh [--install]
set -e
. "$HOME/.cargo/env" 2>/dev/null || true
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export MACOSX_DEPLOYMENT_TARGET=11.0
# cmake es necesario para compilar libopus (audio). pip lo instala en segundos.
for d in "$HOME"/Library/Python/*/bin; do [ -x "$d/cmake" ] && PATH="$d:$PATH"; done
export PATH
command -v cmake >/dev/null || { echo "Falta cmake: python3 -m pip install --user cmake"; exit 1; }

# Versión de IronRDP sobre la que están hechos los parches.
IRONRDP_REV=b69cda22ec692e212caffdbb9fe0c4e5ff1d2696
if [ ! -d vendor/IronRDP ]; then
  git init -q vendor/IronRDP
  git -C vendor/IronRDP remote add origin https://github.com/Devolutions/IronRDP.git
  git -C vendor/IronRDP fetch -q --depth 1 origin "$IRONRDP_REV"
  git -C vendor/IronRDP checkout -q FETCH_HEAD
fi
# Parches propios sobre IronRDP (se aplican una sola vez)
for p in patches/*.patch; do
  [ -f "$p" ] || continue
  git -C vendor/IronRDP apply --check "$ROOT/$p" 2>/dev/null && git -C vendor/IronRDP apply "$ROOT/$p" && echo "Parche aplicado: $p"
done

# Binario universal (Intel + Apple Silicon). UNIVERSAL=0 compila solo para este Mac.
if [ "${UNIVERSAL:-1}" = "1" ]; then
  rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null 2>&1 || true
  (cd vendor/IronRDP && rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null 2>&1 || true)
  for target in x86_64-apple-darwin aarch64-apple-darwin; do
    (cd vendor/IronRDP && cargo build --release --target "$target" -p ironrdp-viewer --features openh264)
    cargo build --release --target "$target"
  done
  lipo -create -output target/release/rdoo-universal \
    target/x86_64-apple-darwin/release/rdoo target/aarch64-apple-darwin/release/rdoo
  lipo -create -output target/release/ironrdp-viewer-universal \
    vendor/IronRDP/target/x86_64-apple-darwin/release/ironrdp-viewer \
    vendor/IronRDP/target/aarch64-apple-darwin/release/ironrdp-viewer
  RDOO_BIN=target/release/rdoo-universal
  VIEWER_BIN=target/release/ironrdp-viewer-universal
else
  (cd vendor/IronRDP && cargo build --release -p ironrdp-viewer --features openh264)
  cargo build --release
  RDOO_BIN=target/release/rdoo
  VIEWER_BIN=vendor/IronRDP/target/release/ironrdp-viewer
fi

# Se construye y firma fuera de Documents (iCloud añade atributos que rompen codesign).
APP="$(mktemp -d)/rdOO.app"
mkdir -p "$APP/Contents/MacOS"
cp "$RDOO_BIN" "$APP/Contents/MacOS/rdoo"
cp "$VIEWER_BIN" "$APP/Contents/MacOS/ironrdp-viewer"
strip -x "$APP/Contents/MacOS/ironrdp-viewer"

# Icono (.icns) desde assets/icon.png
mkdir -p "$APP/Contents/Resources"
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s*2)); sips -z $d $d assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
BUILD="$(date +%Y%m%d.%H%M)"
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>rdOO</string>
  <key>CFBundleDisplayName</key><string>rdOO</string>
  <key>CFBundleIdentifier</key><string>io.github.rdoo</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>rdoo</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

xattr -cr "$APP"
codesign --force --deep -s - "$APP"

mkdir -p "$ROOT/dist"
rm -rf "$ROOT/dist/rdOO.app"
ditto --norsrc "$APP" "$ROOT/dist/rdOO.app"
echo "OK: $ROOT/dist/rdOO.app (v$VERSION, compilación $BUILD)"

if [ "$1" = "--install" ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/rdOO.app"
  ditto --norsrc "$APP" "$HOME/Applications/rdOO.app"
  codesign --verify "$HOME/Applications/rdOO.app" && echo "Instalado en ~/Applications/rdOO.app"
fi
rm -rf "$(dirname "$APP")"

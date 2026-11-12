#!/bin/sh
# Publica <App>-latest.zip y <App>-latest.json en el servidor de apps.
# Uso: scripts/publish-update.sh <App> [ruta/al/App.app]
# La clave SSH NO va en el repo: por defecto ~/.ssh/nivvoo-suite.
set -e

APP="${1:?Uso: publish-update.sh <App> [ruta/al/App.app]}"
BUNDLE="${2:-dist/$APP.app}"
PUBLIC_BASE="${NIVVOO_APPS_URL:-https://www.nivvoo.com/apps}"
KEY="${NIVVOO_SSH_KEY:-$HOME/.ssh/nivvoo-suite}"
HOST="${NIVVOO_SSH_HOST:-135.148.33.78}"
USER="${NIVVOO_SSH_USER:-nivvoo-suite}"

[ -d "$BUNDLE" ] || { echo "No existe $BUNDLE" >&2; exit 1; }
[ -f "$KEY" ] || { echo "Falta la clave SSH: $KEY" >&2; exit 1; }

PLIST="$BUNDLE/Contents/Info.plist"
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")
BUILD=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST")
ZIP="dist/$APP-latest.zip"
JSON="dist/$APP-latest.json"
NOTES_FILE="dist/$APP-latest-notes.txt"

mkdir -p dist
rm -f "$ZIP"
ditto -c -k --sequesterRsrc --keepParent "$BUNDLE" "$ZIP"
SHA=$(shasum -a 256 "$ZIP" | awk '{print $1}')
SIZE=$(stat -f%z "$ZIP")
NOTES=""
[ -f "$NOTES_FILE" ] && NOTES=$(cat "$NOTES_FILE")

python3 - "$JSON" "$APP" "$VERSION" "$BUILD" "$APP-latest.zip" "$SHA" "$SIZE" "$NOTES" <<'PY'
import json, sys
out, app, version, build, url, sha, size, notes = sys.argv[1:]
with open(out, "w") as f:
    json.dump({
        "name": app,
        "version": version,
        "build": build,
        "url": url,
        "file": f"{app}-latest.zip",
        "sha256": sha,
        "size": int(size),
        "notes": notes,
    }, f, indent=2, ensure_ascii=False)
    f.write("\n")
PY

sftp -q -i "$KEY" -P 22 -o BatchMode=yes -o StrictHostKeyChecking=accept-new "$USER@$HOST" <<SFTP
put "$ZIP" /apps/$APP-latest.zip
put "$JSON" /apps/$APP-latest.json
SFTP

echo "Publicado $APP $VERSION ($BUILD):"
echo "  $PUBLIC_BASE/$APP-latest.json"
echo "  $PUBLIC_BASE/$APP-latest.zip  ($SIZE bytes, sha256 $SHA)"

#!/bin/sh
# One-shot build: extract the frontend from the official Windows installer,
# apply the macOS patch, and produce Wuwa Tappo.app (+ .dmg).
#
#   scripts/build.sh --dist ~/Downloads/WuwaTappo-global_1.0.4_x64-setup.exe
#
# Options:
#   --dist <installer.exe>  Official Windows installer to extract dist/ from.
#                           Optional when dist/ already exists.
#   --debug                 Debug bundle (verbose log, F5 devtools), .app only.
#   --adhoc                 Sign ad-hoc instead of "Wuwa Tappo Local Signing"
#                           (Input Monitoring must be re-granted per rebuild).
#   --open                  Launch the built app afterwards.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
INSTALLER=""; DEBUG=0; ADHOC=0; OPEN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dist) INSTALLER="$2"; shift 2 ;;
    --debug) DEBUG=1; shift ;;
    --adhoc) ADHOC=1; shift ;;
    --open) OPEN=1; shift ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
cd "$ROOT"

for tool in node npm cargo python3; do
  command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
done

if [ -n "$INSTALLER" ]; then
  python3 scripts/extract-dist.py "$INSTALLER"
elif [ ! -f dist/index.html ]; then
  echo "dist/ is missing; pass --dist <WuwaTappo-global_*_x64-setup.exe>" >&2
  exit 1
else
  python3 scripts/patch-dist.py
fi

[ -d node_modules ] || npm install

if [ "$ADHOC" = 1 ]; then
  export APPLE_SIGNING_IDENTITY="-"
else
  sh scripts/make-signing-identity.sh >/dev/null
fi

if [ "$DEBUG" = 1 ]; then
  npx tauri build --debug --bundles app
  APP="$ROOT/src-tauri/target/debug/bundle/macos/Wuwa Tappo.app"
else
  npx tauri build
  APP="$ROOT/src-tauri/target/release/bundle/macos/Wuwa Tappo.app"
  echo "dmg: $ROOT/src-tauri/target/release/bundle/dmg/"
fi
echo "app: $APP"
[ "$OPEN" = 1 ] && open "$APP"
exit 0

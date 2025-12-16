#!/usr/bin/env bash
set -euo pipefail

SCRIPT_ID="kwin-focus-helper"

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/kwin/scripts/${SCRIPT_ID}"
BIN_DIR="${HOME}/.local/bin"

ADD_DEFAULTS="0"
for arg in "${@:-}"; do
  case "$arg" in
    --add-defaults|--add-chrome)
      ADD_DEFAULTS="1"
      ;;
    -h|--help)
      cat <<EOF
Usage: ./install.sh [--add-defaults]

Installs:
  - KWin script into: ${XDG_DATA_HOME:-$HOME/.local/share}/kwin/scripts/${SCRIPT_ID}
  - focusctl binary into: ${HOME}/.local/bin (if cargo exists)
Enables the script in kwinrc and asks KWin to reconfigure.

Options:
  --add-defaults   Adds common browser classes to the forced list:
                   google-chrome, google-chrome-stable, chromium, chromium-browser
EOF
      exit 0
      ;;
  esac
done

echo "==> Installing KWin script to: ${DEST_DIR}"
mkdir -p "${DEST_DIR}/contents/code"
cp -v "${SRC_DIR}/metadata.json" "${DEST_DIR}/"
cp -v "${SRC_DIR}/contents/code/focus-helper.js" "${DEST_DIR}/contents/code/"

echo

# ---------------------------------------------------------
# Build + install focusctl CLI (Rust)
# ---------------------------------------------------------
FOCUSCTL_BIN=""

if command -v cargo >/dev/null 2>&1 && [ -d "${SRC_DIR}/focusctl" ]; then
  echo "==> Building focusctl (Rust CLI)…"
  (
    cd "${SRC_DIR}/focusctl"
    cargo build --release
  )

  mkdir -p "${BIN_DIR}"
  cp -v "${SRC_DIR}/focusctl/target/release/focusctl" "${BIN_DIR}/"
  FOCUSCTL_BIN="${BIN_DIR}/focusctl"

  echo "==> Installed focusctl to: ${FOCUSCTL_BIN}"
  echo "    (Make sure ${BIN_DIR} is in your PATH)"
else
  echo "==> Skipping focusctl build (cargo missing or focusctl/ not found)"
fi

echo

# ---------------------------------------------------------
# Enable the KWin script in kwinrc (Plasma 5/6)
# ---------------------------------------------------------
KWRC_TOOL=""

if command -v kwriteconfig6 >/dev/null 2>&1; then
  KWRC_TOOL="kwriteconfig6"
elif command -v kwriteconfig5 >/dev/null 2>&1; then
  KWRC_TOOL="kwriteconfig5"
fi

if [ -n "${KWRC_TOOL}" ]; then
  echo "==> Enabling KWin script plugin '${SCRIPT_ID}' in kwinrc via ${KWRC_TOOL}…"
  "${KWRC_TOOL}" --file kwinrc --group Plugins --key "${SCRIPT_ID}Enabled" "true"
  echo "    Wrote: [Plugins] ${SCRIPT_ID}Enabled=true"
else
  echo "!! kwriteconfig5/6 not found – cannot auto-enable the script."
  echo "   Enable manually in: System Settings → Window Management → KWin Scripts"
fi

echo

# ---------------------------------------------------------
# Ask KWin to reload configuration
# ---------------------------------------------------------
echo "==> Requesting KWin to reload configuration…"

if command -v qdbus6 >/dev/null 2>&1; then
  qdbus6 org.kde.KWin /KWin reconfigure || true
elif command -v qdbus-qt6 >/dev/null 2>&1; then
  qdbus-qt6 org.kde.KWin /KWin reconfigure || true
elif command -v qdbus-qt5 >/dev/null 2>&1; then
  qdbus-qt5 org.kde.KWin /KWin reconfigure || true
elif command -v qdbus >/dev/null 2>&1; then
  qdbus org.kde.KWin /KWin reconfigure || true
else
  echo "!! No qdbus found – cannot tell KWin to reconfigure automatically."
  echo "   Log out/in or restart KWin if needed."
fi

echo

# ---------------------------------------------------------
# Optional: add default classes
# ---------------------------------------------------------
if [ "${ADD_DEFAULTS}" = "1" ]; then
  if [ -n "${FOCUSCTL_BIN}" ] && [ -x "${FOCUSCTL_BIN}" ]; then
    echo "==> Adding default forced classes…"
    "${FOCUSCTL_BIN}" add-class google-chrome || true
    "${FOCUSCTL_BIN}" add-class google-chrome-stable || true
    "${FOCUSCTL_BIN}" add-class chromium || true
    "${FOCUSCTL_BIN}" add-class chromium-browser || true
    echo "==> Defaults added."
  else
    echo "!! Cannot add defaults because focusctl is not installed."
  fi
fi

echo "==> Done."
echo
echo "Next steps:"
echo "  1) Verify the script is enabled:"
echo "     System Settings → Window Management → KWin Scripts → kwin-focus-helper"
echo
echo "  2) Add a class (if you didn't use --add-defaults):"
echo "     focusctl add-class google-chrome-stable"
echo "     focusctl add-class ProcletChrome"
echo
echo "  3) Test by opening a new Chrome window and verifying it appears on top."

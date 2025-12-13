#!/usr/bin/env bash
set -euo pipefail

SCRIPT_ID="kwin-focus-helper"
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/kwin/scripts/${SCRIPT_ID}"

echo "Installing ${SCRIPT_ID} to ${DEST_DIR}"

mkdir -p "$DEST_DIR"
cp -v "${SRC_DIR}/metadata.json" "$DEST_DIR/"
mkdir -p "${DEST_DIR}/contents/code"
cp -v "${SRC_DIR}/contents/code/focus-helper.js" "${DEST_DIR}/contents/code/"

echo
echo "Now enable it in:"
echo "  System Settings → Window Management → KWin Scripts → \"KWin Focus Helper\""
echo
echo "Then you can tweak allowed classes via:"
echo "  focusctl add-class google-chrome-stable"
echo "  focusctl add-class ProcletChrome"

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_ID="kwin-focus-helper"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"

# -----------------------------
# args
# -----------------------------
FORCE=0
NO_ENABLE=0
NO_RECONF=0
NO_FOCUSCTL=0
USER_UID=""

usage() {
  cat <<EOF
Usage: ./install.sh [options]

Options:
  --force            Uninstall existing script without asking
  --no-enable        Do not auto-enable script in kwinrc
  --no-reconfigure   Do not call qdbus6 reconfigure
  --no-focusctl      Do not build/install focusctl
  --user <uid>       Run install actions as UID (recommended if your KDE session is not root)
  -h, --help         Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1; shift;;
    --no-enable) NO_ENABLE=1; shift;;
    --no-reconfigure) NO_RECONF=1; shift;;
    --no-focusctl) NO_FOCUSCTL=1; shift;;
    --user) USER_UID="${2:-}"; shift 2;;
    -h|--help) usage; exit 0;;
    *) echo "Unknown option: $1" >&2; usage; exit 2;;
  esac
done

run_as() {
  if [[ -n "$USER_UID" ]]; then
    sudo -u "#${USER_UID}" -H -- "$@"
  else
    "$@"
  fi
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "!! Missing required command: $1" >&2
    exit 1
  }
}

echo "==> kwin-focus-helper installer"
echo "    repo: $REPO_DIR"
[[ -n "$USER_UID" ]] && echo "    installing as uid: $USER_UID"

need_cmd kpackagetool6

# -----------------------------
# sanity checks for package structure
# -----------------------------
if [[ ! -f "$REPO_DIR/metadata.json" ]]; then
  echo "!! metadata.json not found in repo root" >&2
  exit 1
fi

if [[ ! -f "$REPO_DIR/contents/code/main.js" ]]; then
  echo "!! contents/code/main.js not found" >&2
  echo "   (KWin requires main script at contents/code/main.js)" >&2
  exit 1
fi

# -----------------------------
# detect existing install
# -----------------------------
already_installed=0
if run_as kpackagetool6 --type=KWin/Script -l | grep -qx "$SCRIPT_ID"; then
  already_installed=1
fi

if [[ $already_installed -eq 1 ]]; then
  echo "==> Detected existing install: $SCRIPT_ID"
  if [[ $FORCE -eq 1 ]]; then
    echo "==> --force set: uninstalling existing script..."
    run_as kpackagetool6 --type=KWin/Script -r "$SCRIPT_ID"
  else
    read -r -p "Reinstall (uninstall + install) it? [y/N] " ans
    if [[ "$ans" =~ ^[Yy]$ ]]; then
      run_as kpackagetool6 --type=KWin/Script -r "$SCRIPT_ID"
    else
      echo "==> Cancelled."
      exit 0
    fi
  fi
fi

# -----------------------------
# install package
# -----------------------------
echo "==> Installing KWin script via kpackagetool6..."
run_as kpackagetool6 --type=KWin/Script -i "$REPO_DIR"

echo "==> Installed. Verifying..."
run_as kpackagetool6 --type=KWin/Script -l | grep -qx "$SCRIPT_ID" || {
  echo "!! Install verification failed (script not listed)" >&2
  exit 1
}

# -----------------------------
# build + install focusctl
# -----------------------------
if [[ $NO_FOCUSCTL -eq 0 ]]; then
  if command -v cargo >/dev/null 2>&1 && [[ -d "$REPO_DIR/focusctl" ]]; then
    echo "==> Building focusctl..."
    run_as bash -lc "cd '$REPO_DIR/focusctl' && cargo build --release"
    mkdir -p "$BIN_DIR"
    cp -v "$REPO_DIR/focusctl/target/release/focusctl" "$BIN_DIR/"
    echo "==> focusctl installed to: $BIN_DIR/focusctl"
    echo "    (ensure $BIN_DIR is in PATH)"
  else
    echo "==> Skipping focusctl (cargo missing or focusctl/ directory not found)"
  fi
else
  echo "==> --no-focusctl set: skipping focusctl build/install"
fi

# -----------------------------
# enable + reconfigure
# -----------------------------
if [[ $NO_ENABLE -eq 0 ]]; then
  if command -v kwriteconfig6 >/dev/null 2>&1; then
    echo "==> Enabling script in kwinrc: [Plugins] ${SCRIPT_ID}Enabled=true"
    run_as kwriteconfig6 --file kwinrc --group Plugins --key "${SCRIPT_ID}Enabled" true
  else
    echo "!! kwriteconfig6 not found; cannot auto-enable" >&2
  fi
else
  echo "==> --no-enable set: not enabling in kwinrc"
fi

if [[ $NO_RECONF -eq 0 ]]; then
  if command -v qdbus6 >/dev/null 2>&1; then
    echo "==> Requesting KWin reconfigure..."
    run_as qdbus6 org.kde.KWin /KWin reconfigure || true
  else
    echo "!! qdbus6 not found; cannot auto-reconfigure KWin" >&2
  fi
else
  echo "==> --no-reconfigure set: skipping KWin reconfigure"
fi

echo
echo "==> Done."
echo "Next:"
echo "  focusctl add-class google-chrome-stable"
echo "  focusctl add-class ProcletChrome"
echo "Then test opening new windows."

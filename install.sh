#!/usr/bin/env bash
set -euo pipefail

SCRIPT_ID="kwin-focus-helper"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ACTION="install"   # install | uninstall | reinstall
YES=0
NO_ENABLE=0
NO_RECONF=0
NO_FOCUSCTL=0
USER_UID=""        # run kpackagetool/kwriteconfig as this uid (recommended)
BIN_DIR=""         # defaults to target user's ~/.local/bin

usage() {
  cat <<EOF
kwin-focus-helper installer

Usage:
  ./install.sh [install|uninstall|reinstall] [options]

Options:
  -y, --yes              Do not prompt
  --no-enable            Do not write [Plugins] ${SCRIPT_ID}Enabled=true
  --no-reconfigure       Do not call DBus reconfigure
  --no-focusctl          Do not build/install focusctl
  --user <uid>           Run actions as UID (recommended if your KDE session user != current user)
  --bin-dir <path>       Where to copy focusctl (default: target user's ~/.local/bin)
  -h, --help             Show this help

Notes:
  - KWin/Script packages are expected to have:
      metadata.json
      contents/code/main.js
    (X-Plasma-MainScript usually points to "code/main.js" which is relative to contents/)
  - kpackagetool installs per-user (into that user's ~/.local/share)
EOF
}

# ----- parse args -----
if [[ $# -gt 0 ]]; then
  case "$1" in
    install|uninstall|reinstall) ACTION="$1"; shift;;
  esac
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    -y|--yes) YES=1; shift;;
    --no-enable) NO_ENABLE=1; shift;;
    --no-reconfigure) NO_RECONF=1; shift;;
    --no-focusctl) NO_FOCUSCTL=1; shift;;
    --user) USER_UID="${2:-}"; shift 2;;
    --bin-dir) BIN_DIR="${2:-}"; shift 2;;
    -h|--help) usage; exit 0;;
    *) echo "Unknown option: $1" >&2; usage; exit 2;;
  esac
done

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "!! Missing required command: $1" >&2
    exit 1
  }
}

detect_kde_session_uid() {
  command -v loginctl >/dev/null 2>&1 || return 1

  local sid uid type active class state
  while read -r sid _rest; do
    [[ -z "$sid" ]] && continue

    type="$(loginctl show-session "$sid" -p Type --value 2>/dev/null || true)"
    active="$(loginctl show-session "$sid" -p Active --value 2>/dev/null || true)"
    class="$(loginctl show-session "$sid" -p Class --value 2>/dev/null || true)"
    state="$(loginctl show-session "$sid" -p State --value 2>/dev/null || true)"
    uid="$(loginctl show-session "$sid" -p User --value 2>/dev/null || true)"

    if [[ "$active" == "yes" ]] \
       && [[ "$class" == "user" ]] \
       && [[ "$type" == "wayland" || "$type" == "x11" ]] \
       && [[ "$state" == "active" || "$state" == "online" ]] \
       && [[ -n "$uid" ]]; then
      echo "$uid"
      return 0
    fi
  done < <(loginctl list-sessions --no-legend 2>/dev/null || true)

  return 1
}

get_session_env() {
  command -v loginctl >/dev/null 2>&1 || return 1

  local sid
  sid="$(loginctl list-sessions --no-legend 2>/dev/null \
        | awk '{print $1}' \
        | while read -r s; do
            [[ -z "$s" ]] && continue
            if [[ "$(loginctl show-session "$s" -p Active --value 2>/dev/null)" == "yes" ]] &&
               [[ "$(loginctl show-session "$s" -p Class --value 2>/dev/null)" == "user" ]] &&
               [[ "$(loginctl show-session "$s" -p Type --value 2>/dev/null)" =~ ^(wayland|x11)$ ]]; then
              echo "$s"
              break
            fi
          done)"

  [[ -z "$sid" ]] && return 1

  local xdg dbus
  xdg="$(loginctl show-session "$sid" -p XDG_RUNTIME_DIR --value 2>/dev/null || true)"
  dbus="$(loginctl show-session "$sid" -p DBUS_SESSION_BUS_ADDRESS --value 2>/dev/null || true)"

  [[ -n "$xdg" ]] && echo "XDG_RUNTIME_DIR=$xdg"
  [[ -n "$dbus" ]] && echo "DBUS_SESSION_BUS_ADDRESS=$dbus"

  [[ -n "$xdg" && -n "$dbus" ]]
}

run_as() {
  if [[ -n "$USER_UID" && "$(id -u)" == "0" ]]; then
    sudo -u "#${USER_UID}" -H -- "$@"
  else
    "$@"
  fi
}

target_home() {
  if [[ -n "$USER_UID" ]]; then
    getent passwd "$USER_UID" | cut -d: -f6
  else
    echo "$HOME"
  fi
}

# ----- sanity checks -----
need_cmd kpackagetool6

if [[ ! -f "$REPO_DIR/metadata.json" ]]; then
  echo "!! metadata.json not found in repo root: $REPO_DIR" >&2
  exit 1
fi

# ---------------------------------------------------------
# Auto-detect desktop session UID when running as root
# ---------------------------------------------------------
if [[ -z "$USER_UID" ]] && [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
  if uid_guess="$(detect_kde_session_uid 2>/dev/null)"; then
    USER_UID="$uid_guess"
    echo "==> Auto-detected active graphical session UID: $USER_UID"
  else
    if [[ -n "${SUDO_USER:-}" ]] && id -u "$SUDO_USER" >/dev/null 2>&1; then
      USER_UID="$(id -u "$SUDO_USER")"
      echo "==> Fallback: using SUDO_USER UID: $USER_UID ($SUDO_USER)"
    else
      echo "!! Could not auto-detect graphical session user via loginctl." >&2
      echo "   Please rerun with: ./install.sh --user <uid>" >&2
      exit 1
    fi
  fi
fi

THOME="$(target_home)"
if [[ -z "$BIN_DIR" ]]; then
  BIN_DIR="${THOME}/.local/bin"
fi

echo "==> kwin-focus-helper"
echo "    action: $ACTION"
echo "    repo:   $REPO_DIR"
echo "    as uid: ${USER_UID:-$(id -u)} (home: $THOME)"
echo "    bindir: $BIN_DIR"
echo

# ---------------------------------------------------------
# Auto-heal package layout:
# KWin/Script expects contents/code/main.js
# ---------------------------------------------------------
heal_layout() {
  local want="$REPO_DIR/contents/code/main.js"
  local flat="$REPO_DIR/code/main.js"

  if [[ -f "$want" ]]; then
    return 0
  fi

  if [[ -f "$flat" ]]; then
    echo "==> Repo has code/main.js but missing contents/code/main.js"
    echo "==> Creating contents/code/main.js (KWin package layout)…"
    mkdir -p "$REPO_DIR/contents/code"
    cp -f "$flat" "$want"
    return 0
  fi

  echo "!! Missing main script." >&2
  echo "   Expected either:" >&2
  echo "     - contents/code/main.js (preferred)" >&2
  echo "     - code/main.js (will be copied into contents/)" >&2
  exit 1
}

is_installed() {
  run_as kpackagetool6 --type=KWin/Script -l | grep -qx "$SCRIPT_ID"
}

do_uninstall() {
  if is_installed; then
    echo "==> Removing existing script: $SCRIPT_ID"
    run_as kpackagetool6 --type=KWin/Script -r "$SCRIPT_ID"
  else
    echo "==> Not installed: $SCRIPT_ID"
  fi
}

do_install() {
  heal_layout

  if is_installed; then
    echo "==> Already installed: $SCRIPT_ID"
    if [[ $YES -eq 1 ]]; then
      echo "==> --yes: reinstalling (remove + install)"
      run_as kpackagetool6 --type=KWin/Script -r "$SCRIPT_ID"
    else
      read -r -p "Reinstall it (remove + install)? [y/N] " ans
      if [[ ! "$ans" =~ ^[Yy]$ ]]; then
        echo "==> Keeping existing install."
        return
      fi
      run_as kpackagetool6 --type=KWin/Script -r "$SCRIPT_ID"
    fi
  fi

  echo "==> Installing script via kpackagetool6..."
  run_as kpackagetool6 --type=KWin/Script -i "$REPO_DIR"

  echo "==> Verifying install..."
  is_installed || { echo "!! Install verification failed (not listed)" >&2; exit 1; }
}

build_focusctl() {
  if [[ $NO_FOCUSCTL -ne 0 ]]; then
    echo "==> --no-focusctl: skipping focusctl"
    return
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    echo "==> cargo not found; skipping focusctl"
    return
  fi

  if [[ ! -d "$REPO_DIR/focusctl" ]]; then
    echo "==> focusctl/ not found; skipping focusctl"
    return
  fi

  echo "==> Building focusctl..."
  run_as bash -lc "cd '$REPO_DIR/focusctl' && cargo build --release"

  echo "==> Installing focusctl to $BIN_DIR"
  run_as mkdir -p "$BIN_DIR"
  run_as cp -v "$REPO_DIR/focusctl/target/release/focusctl" "$BIN_DIR/"
}

enable_script() {
  if [[ $NO_ENABLE -ne 0 ]]; then
    echo "==> --no-enable: skipping kwinrc enable"
    return
  fi

  local kwrite=""
  if command -v kwriteconfig6 >/dev/null 2>&1; then
    kwrite="kwriteconfig6"
  elif command -v kwriteconfig5 >/dev/null 2>&1; then
    kwrite="kwriteconfig5"
  fi

  if [[ -z "$kwrite" ]]; then
    echo "!! kwriteconfig6/5 not found; cannot auto-enable script" >&2
    return
  fi

  echo "==> Enabling in kwinrc: [Plugins] ${SCRIPT_ID}Enabled=true"
  run_as "$kwrite" --file kwinrc --group Plugins --key "${SCRIPT_ID}Enabled" true
}

kwin_reconfigure() {
  if [[ $NO_RECONF -ne 0 ]]; then
    echo "==> --no-reconfigure: skipping DBus reconfigure"
    return
  fi

  local qdbus=""
  for c in qdbus6 qdbus-qt6 qdbus-qt5 qdbus; do
    if command -v "$c" >/dev/null 2>&1; then
      qdbus="$c"
      break
    fi
  done

  if [[ -z "$qdbus" ]]; then
    echo "!! No qdbus found; cannot request KWin reconfigure" >&2
    return
  fi

  echo "==> Requesting KWin reconfigure (direct)…"
  if run_as "$qdbus" org.kde.KWin /KWin reconfigure >/dev/null 2>&1; then
    echo "==> KWin reconfigure succeeded (direct)"
    return
  fi

  echo "==> Direct DBus failed; attempting session-aware fallback…"

  if session_env="$(get_session_env 2>/dev/null)"; then
    local xdg="" dbus=""
    xdg="$(printf '%s\n' "$session_env" | awk -F= '$1=="XDG_RUNTIME_DIR"{print $2}')"
    dbus="$(printf '%s\n' "$session_env" | awk -F= '$1=="DBUS_SESSION_BUS_ADDRESS"{print $2}')"

    if [[ -n "$xdg" && -n "$dbus" ]]; then
      run_as bash -lc "
        export XDG_RUNTIME_DIR='${xdg}'
        export DBUS_SESSION_BUS_ADDRESS='${dbus}'
        $qdbus org.kde.KWin /KWin reconfigure
      " >/dev/null 2>&1 && {
        echo "==> KWin reconfigure succeeded (session-aware)"
        return
      }
    fi
  fi

  echo "!! Could not reconfigure KWin automatically."
  echo "   This is usually fine — changes will apply shortly or via focusctl."
}

case "$ACTION" in
  install)
    do_install
    build_focusctl
    enable_script
    kwin_reconfigure
    ;;
  uninstall)
    do_uninstall
    kwin_reconfigure
    ;;
  reinstall)
    do_uninstall
    do_install
    build_focusctl
    enable_script
    kwin_reconfigure
    ;;
  *)
    echo "Unknown action: $ACTION" >&2
    usage
    exit 2
    ;;
esac

echo
echo "==> Done."
echo "Next (optional):"
echo "  $BIN_DIR/focusctl add-class google-chrome-stable"
echo "  $BIN_DIR/focusctl add-class ProcletChrome"
echo
echo "Test DBus (best-effort):"
echo "  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.isScriptLoaded ${SCRIPT_ID}"

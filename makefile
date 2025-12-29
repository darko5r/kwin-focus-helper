SCRIPT_ID := kwin-focus-helper
INSTALLER := ./install.sh

# Packaging defaults
prefix ?= /usr
DESTDIR ?=
KWINSCRIPTDIR := $(prefix)/share/kwin/scripts/$(SCRIPT_ID)
BINDIR := $(prefix)/bin
MANDIR := $(prefix)/share/man

FOCUSCTL_DIR := focusctl
FOCUSCTL_BIN := $(FOCUSCTL_DIR)/target/release/focusctl
FOCUSCTL_MAN := $(FOCUSCTL_DIR)/man/focusctl.1

# Build artifacts should NOT pollute the repo during packaging
# AUR/PKGBUILD can override this too.
CARGO_TARGET_DIR ?= $(FOCUSCTL_DIR)/target

.PHONY: help build install install-user uninstall-user status test lint clean

help:
	@echo "Targets:"
	@echo "  make build                           - build focusctl (release)"
	@echo "  make install [prefix=/usr] [DESTDIR=] - packaging install (no kpackagetool)"
	@echo "  make install-user [ARGS='...']        - developer install via install.sh/kpackagetool"
	@echo "  make uninstall-user [ARGS='...']      - remove dev install (kpackagetool)"
	@echo "  make status                           - show installed/enabled status (both fs + kpackagetool)"
	@echo "  make test                             - DBus isScriptLoaded() check (best-effort)"
	@echo "  make lint                             - basic sanity checks"
	@echo "  make clean                            - remove build artifacts"
	@echo
	@echo "Examples:"
	@echo "  make build"
	@echo "  make install DESTDIR=$$PWD/pkgdir prefix=/usr"
	@echo "  make install-user ARGS='--user 1000 -y'"
	@echo "  make uninstall-user ARGS='--user 1000 -y'"

# --------------------
# Build (packaging-safe)
# --------------------
build:
	@echo "==> Building focusctl (release)"
	@cd $(FOCUSCTL_DIR) && CARGO_TARGET_DIR="$(abspath $(CARGO_TARGET_DIR))" cargo build --release

# --------------------
# Install (for AUR/pkg)
#   - copies files only
#   - NO kpackagetool
# --------------------
install: build
	@echo "==> Installing KWin script to $(DESTDIR)$(KWINSCRIPTDIR)"
	@install -d "$(DESTDIR)$(KWINSCRIPTDIR)"
	@install -m 0644 metadata.json "$(DESTDIR)$(KWINSCRIPTDIR)/metadata.json"
	@install -d "$(DESTDIR)$(KWINSCRIPTDIR)/contents/code"
	@install -m 0644 contents/code/main.js "$(DESTDIR)$(KWINSCRIPTDIR)/contents/code/main.js"

	@echo "==> Installing focusctl to $(DESTDIR)$(BINDIR)/focusctl"
	@install -d "$(DESTDIR)$(BINDIR)"
	@install -m 0755 "$(FOCUSCTL_BIN)" "$(DESTDIR)$(BINDIR)/focusctl"

	@echo "==> Installing man page to $(DESTDIR)$(MANDIR)/man1"
	@install -d "$(DESTDIR)$(MANDIR)/man1"
	@gzip -c "$(FOCUSCTL_MAN)" > "$(DESTDIR)$(MANDIR)/man1/focusctl.1.gz"

# --------------------
# Dev install (your current workflow)
# --------------------
install-user:
	@$(INSTALLER) install $(ARGS)

uninstall-user:
	@$(INSTALLER) uninstall $(ARGS)

# --------------------
# Status (robust awk + show both install modes)
# --------------------
status:
	@echo "==> Filesystem install locations:"
	@sys="$(prefix)/share/kwin/scripts/$(SCRIPT_ID)"; \
	user="$$HOME/.local/share/kwin/scripts/$(SCRIPT_ID)"; \
	if [ -d "$$sys" ]; then echo "  [system] $$sys"; else echo "  [system] (not found) $$sys"; fi; \
	if [ -d "$$user" ]; then echo "  [user]   $$user"; else echo "  [user]   (not found) $$user"; fi
	@echo
	@echo "==> kpackagetool6 registry (if present):"
	@{ command -v kpackagetool6 >/dev/null 2>&1 && kpackagetool6 --type=KWin/Script -l | sed -n '1,200p'; } || \
	 (echo "  (kpackagetool6 not found)")
	@echo
	@echo "==> Enabled flag in kwinrc (best-effort):"
	@kwinrc="$$HOME/.config/kwinrc"; \
	if [ -f "$$kwinrc" ]; then \
	  awk -v id="$(SCRIPT_ID)" ' \
	    $$0=="[Plugins]" { in_plugins=1; next } \
	    in_plugins && $$0 ~ /^\[/ { in_plugins=0 } \
	    in_plugins && $$0 ~ ("^" id "Enabled=") { print "  " $$0 } \
	  ' "$$kwinrc" || true; \
	else \
	  echo "  (no $$kwinrc)"; \
	fi

# --------------------
# DBus test (best-effort)
# --------------------
test:
	@echo "==> DBus: isScriptLoaded($(SCRIPT_ID)) (best-effort)"
	@ok=0; \
	for c in qdbus6 qdbus-qt6 qdbus-qt5 qdbus; do \
	  if command -v $$c >/dev/null 2>&1; then \
	    if $$c org.kde.KWin /Scripting org.kde.kwin.Scripting.isScriptLoaded $(SCRIPT_ID) 2>/dev/null; then \
	      ok=1; break; \
	    fi; \
	  fi; \
	done; \
	if [ $$ok -eq 0 ]; then \
	  echo "  No working qdbus found (or no session bus in this shell)."; \
	fi

lint:
	@echo "==> Lint: install.sh syntax"
	@bash -n install.sh
	@echo "==> Lint: package layout"
	@test -f metadata.json
	@test -f contents/code/main.js
	@test -f $(FOCUSCTL_DIR)/src/main.rs
	@test -f $(FOCUSCTL_MAN)
	@echo "OK"

clean:
	@echo "==> Cleaning build artifacts"
	@rm -rf "$(CARGO_TARGET_DIR)"

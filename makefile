SCRIPT_ID := kwin-focus-helper
INSTALLER := ./install.sh

.PHONY: help install uninstall reinstall enable disable status test lint

help:
	@echo "Targets:"
	@echo "  make install [ARGS='...']    - install script (and focusctl)"
	@echo "  make uninstall [ARGS='...']  - remove installed script"
	@echo "  make reinstall [ARGS='...']  - uninstall + install"
	@echo "  make enable                  - enable plugin flag in kwinrc (best-effort)"
	@echo "  make disable                 - disable plugin flag in kwinrc (best-effort)"
	@echo "  make status                  - show installed + enabled status"
	@echo "  make test                    - DBus isScriptLoaded() check (best-effort)"
	@echo "  make lint                    - basic sanity checks"
	@echo
	@echo "Examples:"
	@echo "  make install ARGS='--user 1000'"
	@echo "  make reinstall ARGS='-y --no-reconfigure'"

install:
	@$(INSTALLER) install $(ARGS)

uninstall:
	@$(INSTALLER) uninstall $(ARGS)

reinstall:
	@$(INSTALLER) reinstall $(ARGS)

enable:
	@echo "==> Enabling $(SCRIPT_ID) in kwinrc (best-effort)"
	@{ command -v kwriteconfig6 >/dev/null 2>&1 && kwriteconfig6 --file kwinrc --group Plugins --key "$(SCRIPT_ID)Enabled" true; } || \
	 { command -v kwriteconfig5 >/dev/null 2>&1 && kwriteconfig5 --file kwinrc --group Plugins --key "$(SCRIPT_ID)Enabled" true; } || \
	 (echo "!! kwriteconfig6/5 not found"; exit 0)

disable:
	@echo "==> Disabling $(SCRIPT_ID) in kwinrc (best-effort)"
	@{ command -v kwriteconfig6 >/dev/null 2>&1 && kwriteconfig6 --file kwinrc --group Plugins --key "$(SCRIPT_ID)Enabled" false; } || \
	 { command -v kwriteconfig5 >/dev/null 2>&1 && kwriteconfig5 --file kwinrc --group Plugins --key "$(SCRIPT_ID)Enabled" false; } || \
	 (echo "!! kwriteconfig6/5 not found"; exit 0)

status:
	@echo "==> Installed scripts (kpackagetool6):"
	@{ command -v kpackagetool6 >/dev/null 2>&1 && kpackagetool6 --type=KWin/Script -l | sed -n '1,200p'; } || \
	 (echo "!! kpackagetool6 not found"; exit 0)
	@echo
	@echo "==> Enabled flag in kwinrc (best-effort):"
	@kwinrc="$$HOME/.config/kwinrc"; \
	if [ -f "$$kwinrc" ]; then \
	  awk -v id="$(SCRIPT_ID)" '\
	    $$0=="[Plugins]"{in=1;next} \
	    in && $$0 ~ /^\[/{in=0} \
	    in && $$0 ~ ("^"id"Enabled="){print $$0} \
	  ' "$$kwinrc" || true; \
	else \
	  echo "(no $$kwinrc)"; \
	fi

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
	  echo "No working qdbus found (or no session bus in this shell)."; \
	fi

lint:
	@echo "==> Lint: install.sh syntax"
	@bash -n install.sh
	@echo "==> Lint: package layout"
	@test -f metadata.json
	@test -f contents/code/main.js
	@echo "OK"
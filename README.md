# kwin-focus-helper

Designed for sandboxed and wrapped applications that need predictable window focus behavior.

A lightweight KWin script + optional Rust CLI that allows **selected applications**
to bypass KWin’s *focus stealing prevention* — **without changing global window
behavior**.

This is especially useful for sandboxed or wrapped applications that legitimately
need to raise or focus new windows, such as those launched via:

- proclet
- firejail
- bubblewrap
- flatpak / custom containers
- custom launchers or security wrappers

---

## Requirements

- KDE Plasma (KWin window manager)
- Plasma 6 / KWin 6 (should also work on many Plasma 5 setups)
- `kpackagetool6` (install)
- Optional: Rust toolchain (`cargo`) — install via your distro packages (e.g. “rust”/“cargo”) or via rustup

---

## What problem does this solve?

KWin’s *Focus stealing prevention* (often set to **Medium**) is a good global default,
but it can break legitimate workflows:

- New browser windows opening behind existing ones
- Dialogs appearing unfocused
- Sandboxed apps being treated as “suspicious” focus stealers

Lowering the global setting affects **all applications**, which is not ideal.

**kwin-focus-helper fixes this per-application.**

---

## How it works

- Your global KWin focus policy remains unchanged (e.g. *Medium*)
- You define a list of **window classes** that are allowed to:
  - be raised (`workspace.raiseWindow`)
  - receive focus (`workspace.activeWindow`)
- Only windows matching those classes are affected

This effectively gives those apps *“Focus stealing = None”* behavior — **and nothing else**.

---

## Components

### 1) KWin Script (JavaScript)

- Runs inside KWin
- Watches for:
  - new windows
  - window activation
- Applies focus rules **only** to whitelisted window classes

### 2) `focusctl` (optional Rust CLI)

A small helper to manage configuration safely:

```
focusctl list-classes
focusctl add-class google-chrome
focusctl remove-class google-chrome
```

## Install

```
git clone https://github.com/darko5r/kwin-focus-helper.git
cd kwin-focus-helper
make install

Pass installer options through ARGS, e.g.:
make reinstall ARGS='-y'
make install ARGS='--no-focusctl'

Installation check:
make status
make test
```

## Usage

Add one or more window classes that should be allowed to receive focus:

```
focusctl add-class google-chrome
focusctl add-class firefox
```

New windows from these applications should now appear on top,
even when global focus stealing prevention is set to *Medium*.

To list or remove entries:

```
focusctl list-classes
focusctl remove-class google-chrome
focusctl remove-class firefox
```

## Integration

`kwin-focus-helper` is designed to be used by launchers and sandboxing tools.

Typical integrations include:

- Adding a window class before launching an application
- Reconfiguring KWin
- Launching the sandboxed process
- Removing the class afterward (optional)

This allows sandboxed applications to behave normally
without permanently changing user focus policy.

Programmatic integration examples will be added over time.

## Troubleshooting

### Script installs but does not appear / update in KWin

In rare cases, KDE’s service cache may be stale (especially after manual file
removals or repeated installs).

You can fully reset the script and rebuild the cache:

```
# Remove installed script
kpackagetool6 --type=KWin/Script -r kwin-focus-helper

# Hard-remove leftovers (per-user)
rm -rf ~/.local/share/kwin/scripts/kwin-focus-helper

# Rebuild KDE service cache (Plasma 6)
rm -f ~/.cache/ksycoca6_*
kbuildsycoca6
```

After this, install again:
```
make install
```
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
chmod +x install.sh
./install.sh
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
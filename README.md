# kwin-focus-helper

Small KWin script + Rust CLI to let specific apps (e.g. Chrome launched via `proclet`)
bypass focus stealing prevention, **without** changing your global KWin settings.

## What it does

- Global KWin setting stays at e.g. `Focus stealing prevention = Medium`.
- Windows whose class is in `forceFocusClasses` are:
  - Raised (`workspace.raiseWindow(window)`)
  - Focused (`workspace.activeWindow = window`)

This means your proclet-launched Chrome (or any other app you choose) behaves
as if its focus stealing prevention were set to `None`, but only for that app.

## Install

```bash
git clone <your-repo-url> kwin-focus-helper
cd kwin-focus-helper
chmod +x install.sh
./install.sh

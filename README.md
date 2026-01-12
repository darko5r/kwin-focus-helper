## <small>`kwin-focus-helper`</small>

> ***Per-application focus control for KWin — without touching global policy***
>
> `kwin-focus-helper` is a lightweight KWin script with an optional Rust CLI that
> allows explicitly selected applications to bypass KWin’s focus stealing
> prevention — while keeping global window behavior unchanged.  
> It is designed for non-standard launch contexts where legitimate applications
> are incorrectly treated as focus stealers.

## <sub>`Intended use (important)`</sub>

> This tool is not a general desktop tweak.
>
> It is intended for users who run applications via:
>
> sandbox wrappers (e.g. proclet, firejail, bubblewrap)
>
> Flatpak or custom containers
>
> privileged or wrapped launchers
>
> security-conscious workflows that alter window ownership or activation flow
>
> If you launch applications normally as a regular user and do not experience focus
> issues, you probably do not need this tool.
>
> By default, `kwin-focus-helper` does nothing until explicitly configured.

## <sub>`Requirements`</sub>
> ***Runtime***
> 
> - KDE Plasma (KWin window manager)
>
> ***Optional (recommended)***
>
> - `qdbus6` (or compatible `qdbus`) — for `focusctl reconfigure`
>
> ***Build dependencies (only if building from source)***
>
> - Rust toolchain (`cargo`) — for `focusctl`
>
> - `kpackagetool6` — only for manual / per-user installs

## <sub>`The problem it solves`</sub>

> KWin’s Focus stealing prevention (often set to _Medium_) is a good global default,
> but it can break legitimate workflows under certain conditions:
>
> - New browser windows opening behind existing ones
>
> - Dialogs appearing unfocused
>
> - Sandboxed or wrapped applications being misclassified as “suspicious”
>
> Lowering the global setting affects all applications, which is undesirable.
>
> `kwin-focus-helper` solves this per application.

## <sub>`How it works`</sub>

> - Your global KWin focus policy remains unchanged
>
> - You define a whitelist of window classes
>
> - Only windows matching those classes are allowed to:
>
>    - raise themselves (`workspace.raiseWindow`)
>
>    - receive focus (`workspace.activeWindow`)
>
> This gives those applications “```Focus stealing = None```” behavior —
> and nothing else.
>
> No global overrides. No heuristics. No surprises.

## <sub>`Components`</sub>

> 1\) 🔹 KWin Script (JavaScript)
>
> - Runs inside KWin
>
> - Observes:
>
>   - new windows
>
>   - activation requests
> 
> - Applies focus behavior only to whitelisted window classes
>
> 2\) 🔹 `focusctl` (optional Rust CLI)
>
> A small helper to manage configuration explicitly and safely:
>
>```
>focusctl list-classes
>focusctl add-class google-chrome-stable
>focusctl remove-class google-chrome-stable
>focusctl list-keys
>```

## <sub>Installation</sub>

> ***From source***
> ```
> git clone https://github.com/darko5r/kwin-focus-helper.git
> cd kwin-focus-helper
> make install
> ```
> ***Installer options can be passed via ARGS:***
> ```
> make reinstall ARGS='-y'
> make install ARGS='--no-focusctl'
> ```
> ***Verify installation:***
> ```
> make status
> make test
> ```
> ***From AUR***
> ```
> yay -S kwin-focus-helper
> ```

## <sub>Usage</sub>

> Add one or more window classes that should be allowed to receive focus:
> ```
> focusctl add-class google-chrome-stable
> focusctl add-class firefox
>```
>New windows from these applications will now raise and focus correctly,
>even when global focus stealing prevention is set to Medium.
>
>Manage entries:
>```
>focusctl list-classes
>focusctl remove-class google-chrome-stable
>```

## <sub>Integration & automation</sub>

> `kwin-focus-helper` is designed to integrate cleanly with
> launchers and sandboxing tools.
>
> A typical flow:
>
>    1. Temporarily allow a window class
>    2. Reconfigure KWin
>    3. Launch the wrapped / sandboxed application
>    4. (Optional) remove the class afterward
>
> This enables correct behavior without permanently changing user policy.
>
> `focusctl wrap` (recommended)
>
> For most integrations, `focusctl wrap` is the preferred interface.
>
> It provides an explicit, minimal boundary between launch logic and KWin behavior.
> 
> ***Explicit class***
> ```
> focusctl wrap ProcletChrome -- google-chrome-stable
>```
> ***Sandboxed example***
> ```
> focusctl wrap ProcletChrome -- proclet -- google-chrome-stable
> ```
> ***Automatic class (derived from argv[0])***
> ```
> focusctl wrap --auto -- google-chrome-stable
> ```

## <sub>Troubleshooting</sub>

> ***Script installs but does not appear or update in KWin***
>
> In rare cases, KDE’s service cache may become stale.
>
> You can fully reset the script and rebuild caches:
>```
> kpackagetool6 --type=KWin/Script -r kwin-focus-helper
> rm -rf ~/.local/share/kwin/scripts/kwin-focus-helper
> rm -f ~/.cache/ksycoca6_*
> kbuildsycoca6
>```
> Then reinstall:
>```
> make install
>```

## <sub>Finding your window class</sub>

> Click the target window and run:
>```
> xprop WM_CLASS
>```
> Usually, the second string is the correct class name.

## <sub>License</sub>

> [MIT](LICENSE.md)
/*
 * kwin-focus-helper
 *
 * Force-focus selected window classes when they appear, without changing your
 * global "Focus stealing prevention" setting.
 *
 * Config is read from KWin's script config:
 *   [Script-kwin-focus-helper]
 *   forceFocusClasses=google-chrome;google-chrome-stable;ProcletChrome
 *
 * You can manage this via the focusctl CLI (recommended).
 */

(function () {
    "use strict";

    // --- Utilities ----------------------------------------------------------

    function normClass(s) {
        if (!s) return "";
        s = String(s).trim();
        if (!s.length) return "";

        // Many Wayland apps expose desktop file names like "google-chrome.desktop".
        // Allow matching "google-chrome" against "google-chrome.desktop".
        if (s.endsWith(".desktop")) s = s.slice(0, -(".desktop".length));

        return s.toLowerCase();
    }

    function parseClasses(value) {
        if (!value) return [];
        // Split on ; , or whitespace
        var parts = String(value).split(/[\s;,]+/);
        var out = [];
        for (var i = 0; i < parts.length; i++) {
            var c = normClass(parts[i]);
            if (c.length) out.push(c);
        }
        return out;
    }

    // --- Config -------------------------------------------------------------

    var forced = []; // normalized list

    function reloadConfig() {
        // readConfig reads from [Script-<scriptId>] in kwinrc automatically.
        // Our key is "forceFocusClasses".
        var raw = "";
        try {
            raw = readConfig("forceFocusClasses", "");
        } catch (e) {
            // In some environments readConfig may not be available (rare).
            raw = "";
        }

        forced = parseClasses(raw);

        print("kwin-focus-helper: config reloaded, forced classes = [" + forced.join(", ") + "]");
    }

    function isClassForced(cls) {
        cls = normClass(cls);
        if (!cls) return false;

        // Also match if list contains desktop variant and we got non-desktop,
        // or vice versa, by normalizing both.
        for (var i = 0; i < forced.length; i++) {
            if (forced[i] === cls) return true;
        }
        return false;
    }

    // --- Window matching ----------------------------------------------------

    function windowClassKey(w) {
        if (!w) return "";

        // Prefer desktopFileName on Wayland when available.
        // Fallback to resourceClass for X11.
        // Also include resourceName as last resort.
        try {
            if (w.desktopFileName && w.desktopFileName.length) return w.desktopFileName;
        } catch (_) {}

        try {
            if (w.resourceClass && w.resourceClass.length) return w.resourceClass;
        } catch (_) {}

        try {
            if (w.resourceName && w.resourceName.length) return w.resourceName;
        } catch (_) {}

        return "";
    }

    function isEligibleWindow(w) {
        if (!w) return false;

        // Avoid acting on windows that are effectively dying.
        try {
            if (w.deleted) return false;
        } catch (_) {}

        // Only windows that should reasonably take focus.
        // KWin exposes helpers like normalWindow/dialog in many versions.
        try {
            if (typeof w.normalWindow !== "undefined" && w.normalWindow) return true;
        } catch (_) {}

        try {
            if (typeof w.dialog !== "undefined" && w.dialog) return true;
        } catch (_) {}

        // Fallback: if wantsInput exists and true, allow.
        try {
            if (typeof w.wantsInput !== "undefined" && w.wantsInput) return true;
        } catch (_) {}

        return false;
    }

    // --- Focus forcing ------------------------------------------------------

    function forceFocusNow(w, why) {
        if (!w) return;

        var cls = windowClassKey(w);
        if (!isClassForced(cls)) return;

        if (!isEligibleWindow(w)) return;

        var cap = "";
        try { cap = w.caption; } catch (_) { cap = ""; }

        print("kwin-focus-helper: forcing focus (" + why + ") for class=" + cls + " caption='" + cap + "'");

        // Raise first (stacking), then activate.
        try { workspace.raiseWindow(w); } catch (_) {}

        // workspace.activeWindow works widely, but activateWindow exists in some versions too.
        try {
            if (typeof workspace.activateWindow === "function") {
                workspace.activateWindow(w);
            } else {
                workspace.activeWindow = w;
            }
        } catch (_) {}
    }

    // Some windows are created, then shown/activated shortly after.
    // A short delay can help win races with focus-stealing prevention.
    function forceFocusSoon(w, why) {
        if (!w) return;

        // setTimeout is available in KWin scripts.
        // If for some reason it isn't, fallback to immediate.
        try {
            setTimeout(function () { forceFocusNow(w, why); }, 0);
            setTimeout(function () { forceFocusNow(w, why + "+retry"); }, 50);
        } catch (e) {
            forceFocusNow(w, why);
        }
    }

    // --- Hooks --------------------------------------------------------------

    function onWindowAdded(w) {
        var cls = windowClassKey(w);
        if (!isClassForced(cls)) return;
        forceFocusSoon(w, "windowAdded");
    }

    function onWindowActivated(w) {
        // Don’t fight the user: only raise if THIS activated window is forced.
        var cls = windowClassKey(w);
        if (!isClassForced(cls)) return;

        // Raise it to ensure it’s not hidden behind.
        try { workspace.raiseWindow(w); } catch (_) {}
    }

    // --- Init ---------------------------------------------------------------

    reloadConfig();

    // React to config changes (Plasma usually emits this for scripts).
    try {
        if (typeof options !== "undefined" && options.configChanged) {
            options.configChanged.connect(function () {
                reloadConfig();
            });
        }
    } catch (_) {}

    // Core hooks
    try { workspace.windowAdded.connect(onWindowAdded); } catch (_) {}
    try { workspace.windowActivated.connect(onWindowActivated); } catch (_) {}

    print("kwin-focus-helper: active (windowAdded/windowActivated hooked)");
})();

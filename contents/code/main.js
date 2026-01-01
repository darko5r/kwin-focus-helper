/*
 * kwin-focus-helper (compat-first v2)
 *
 * Goal:
 *   Allow selected window classes to be raised / activated when created,
 *   without changing global "Focus stealing prevention".
 *
 * Config (kwinrc):
 *   [Script-kwin-focus-helper]
 *   forceFocusClasses=google-chrome;google-chrome-stable;ProcletChrome
 *   mode=activate        # or: raise
 *   debug=false
 *
 * Notes:
 * - Matching is case-insensitive and strips a trailing ".desktop".
 * - For best coverage, we match against desktopFileName, resourceClass, resourceName.
 */

(function () {
  "use strict";

  // -----------------------
  // Small compat helpers
  // -----------------------

  function safeStr(v) {
    try { return (v === undefined || v === null) ? "" : String(v); } catch (_) { return ""; }
  }

  function normClass(s) {
    s = safeStr(s).trim();
    if (!s) return "";
    if (s.slice(-8) === ".desktop") s = s.slice(0, -8);
    return s.toLowerCase();
  }

  function splitClasses(raw) {
    // Split on whitespace, ';' or ','.
    // Avoid fancy JS features for max compatibility.
    var parts = safeStr(raw).split(/[\s;,]+/);
    var set = Object.create(null);
    var list = [];
    for (var i = 0; i < parts.length; i++) {
      var c = normClass(parts[i]);
      if (c && !set[c]) {
        set[c] = true;
        list.push(c);
      }
    }
    return { set: set, list: list };
  }

  // -----------------------
  // Config
  // -----------------------

  var forcedSet = Object.create(null);
  var forcedList = [];
  var debug = false;
  var mode = "activate"; // "activate" or "raise"

  function log(msg) {
    if (!debug) return;
    try { print("kwin-focus-helper: " + msg); } catch (_) {}
  }

  function reloadConfig() {
    var raw = "";
    var rawDebug = "false";
    var rawMode = "activate";

    try { raw = readConfig("forceFocusClasses", ""); } catch (_) { raw = ""; }
    try { rawDebug = readConfig("debug", "false"); } catch (_) { rawDebug = "false"; }
    try { rawMode = readConfig("mode", "activate"); } catch (_) { rawMode = "activate"; }

    debug = (safeStr(rawDebug).toLowerCase() === "true");
    mode = normClass(rawMode) || "activate";
    if (mode !== "raise" && mode !== "activate") mode = "activate";

    var parsed = splitClasses(raw);
    forcedSet = parsed.set;
    forcedList = parsed.list;

    log("config reloaded: forced=[" + forcedList.join(", ") + "], mode=" + mode);
  }

  // -----------------------
  // Window identification
  // -----------------------

  function windowCandidates(w) {
    // Return normalized candidates, best-first.
    var out = [];
    function push(v) {
      var n = normClass(v);
      if (n) out.push(n);
    }

    if (!w) return out;

    // Prefer desktopFileName (Wayland-ish), then resourceClass (X11-ish), then resourceName.
    try { if (w.desktopFileName) push(w.desktopFileName); } catch (_) {}
    try { if (w.resourceClass) push(w.resourceClass); } catch (_) {}
    try { if (w.resourceName) push(w.resourceName); } catch (_) {}

    return out;
  }

  function matchForced(w) {
    var c = windowCandidates(w);
    for (var i = 0; i < c.length; i++) {
      if (forcedSet[c[i]]) return c[i]; // matched key
    }
    return "";
  }

  function isDeleted(w) {
    try { return !!w.deleted; } catch (_) { return false; }
  }

  function isMinimized(w) {
    try { return !!w.minimized; } catch (_) { return false; }
  }

  function isEligibleWindow(w) {
    if (!w) return false;
    if (isDeleted(w)) return false;

    // Keep it conservative: act on normal windows and dialogs.
    try { if (w.normalWindow) return true; } catch (_) {}
    try { if (w.dialog) return true; } catch (_) {}

    // Fallback: if wantsInput exists and true, allow.
    try { if (w.wantsInput) return true; } catch (_) {}

    return false;
  }

  function isAlreadyActive(w) {
    try { return workspace.activeWindow === w; } catch (_) { return false; }
  }

  // -----------------------
  // Debounce scheduling
  // -----------------------

  var scheduledWeak = (typeof WeakMap === "function") ? new WeakMap() : null;
  var scheduledById = Object.create(null);

  function markScheduled(w) {
    // true = already scheduled
    if (!w) return false;

    if (scheduledWeak) {
      if (scheduledWeak.get(w)) return true;
      scheduledWeak.set(w, true);
      return false;
    }

    // Fallback: internalId is commonly present
    try {
      var id = w.internalId;
      if (id !== undefined && id !== null) {
        id = safeStr(id);
        if (scheduledById[id]) return true;
        scheduledById[id] = true;
        return false;
      }
    } catch (_) {}

    return false;
  }

  // -----------------------
  // Focus forcing
  // -----------------------

  function doRaise(w) {
    try { workspace.raiseWindow(w); } catch (_) {}
  }

  function doActivate(w) {
    try {
      if (typeof workspace.activateWindow === "function") {
        workspace.activateWindow(w);
      } else {
        workspace.activeWindow = w;
      }
    } catch (_) {}
  }

  function forceNow(w, why) {
    if (!w) return;
    if (isDeleted(w)) return;

    var matched = matchForced(w);
    if (!matched) return;

    if (!isEligibleWindow(w)) return;
    if (isMinimized(w)) return;

    // Don’t fight the user if it’s already active.
    if (isAlreadyActive(w)) {
      log("skip (already active): " + matched + " (" + why + ")");
      return;
    }

    // Perform action
    log("apply " + mode + ": class=" + matched + " (" + why + ")");
    doRaise(w);
    if (mode === "activate") doActivate(w);
  }

  function forceSoon(w, why) {
    if (!w) return;
    if (markScheduled(w)) return;

    // Timing ladder: cheap but helps races (Wayland / focus prevention).
    var delays = [0, 60, 180];

    for (var i = 0; i < delays.length; i++) {
      (function (delay, tag) {
        try {
          setTimeout(function () {
            forceNow(w, tag);
          }, delay);
        } catch (_) {
          // If setTimeout is somehow unavailable, just try once.
          if (delay === 0) forceNow(w, tag);
        }
      })(delays[i], why + (i ? "+retry" + i : ""));
    }
  }

  // -----------------------
  // Hooks
  // -----------------------

  function onWindowAdded(w) {
    // Quick gate before scheduling timers
    if (!w) return;
    if (!matchForced(w)) return;
    forceSoon(w, "windowAdded");
  }

  function onWindowActivated(w) {
    // Raise forced window if it’s being activated (no focus fight).
    if (!w) return;
    if (!matchForced(w)) return;

    // Only stacking correction, activation already happened.
    doRaise(w);
  }

  // -----------------------
  // Init
  // -----------------------

  reloadConfig();

  // React to config changes (available on most Plasma versions).
  try {
    if (typeof options !== "undefined" && options.configChanged) {
      options.configChanged.connect(function () {
        reloadConfig();
      });
    }
  } catch (_) {}

  try { workspace.windowAdded.connect(onWindowAdded); } catch (_) {}
  try { workspace.windowActivated.connect(onWindowActivated); } catch (_) {}

  // One non-debug startup line is usually OK; keep it minimal.
  try { print("kwin-focus-helper: loaded"); } catch (_) {}
})();

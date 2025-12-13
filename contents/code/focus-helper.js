// contents/code/focus-helper.js
//
// KWin Focus Helper
// - watches for new windows
// - if window.resourceClass is in forceFocusClasses, it is raised and focused
//
// This lets you keep global "Focus stealing prevention" = Medium,
// but still allow specific apps (e.g. Chrome from proclet) to grab focus.

(function () {
    print("kwin-focus-helper: script loaded");

    // ----- Config handling ---------------------------------------------------

    function parseClassList(str) {
        if (!str)
            return [];
        return String(str)
            .split(/[;,]/)
            .map(function (s) { return s.trim(); })
            .filter(function (s) { return s.length > 0; });
    }

    // Default: handle Chrome & ProcletChrome
    var forceClassesDefault = "google-chrome-stable;google-chrome;ProcletChrome";

    // Read from the script's own config (group [Script-kwin-focus-helper])
    // key: forceFocusClasses
    var forceClasses = parseClassList(readConfig("forceFocusClasses", forceClassesDefault));

    function isClassForced(cls) {
        if (!cls)
            return false;
        cls = String(cls);
        for (var i = 0; i < forceClasses.length; ++i) {
            if (forceClasses[i] === cls) {
                return true;
            }
        }
        return false;
    }

    function windowClassKey(w) {
        // For X11 apps, resourceClass is usually the right match (e.g. "google-chrome").
        // For Wayland apps, desktopFileName is often more reliable.
        if (w.resourceClass && w.resourceClass.length)
            return w.resourceClass;
        if (w.desktopFileName && w.desktopFileName.length)
            return w.desktopFileName;
        return "";
    }

    function maybeForceFocus(w) {
        // Only normal windows / dialogs that actually want input
        if (!w)
            return;
        if (!w.wantsInput)
            return;

        var cls = windowClassKey(w);
        if (!isClassForced(cls))
            return;

        print("kwin-focus-helper: forcing focus for class " + cls +
              " (caption: " + w.caption + ")");

        // First raise above others…
        workspace.raiseWindow(w);
        // …then make it the active window.
        workspace.activeWindow = w;
    }

    // ----- Hooks -------------------------------------------------------------

    // New window appears
    workspace.windowAdded.connect(function (w) {
        maybeForceFocus(w);
    });

    // Some windows are created hidden and then shown; if Focus prevention
    // wins first round, we can try again on activation.
    workspace.windowActivated.connect(function (w) {
        // We don't want to fight the user if they switched away,
        // so we only act when the activated window itself is in our list.
        if (!w)
            return;
        var cls = windowClassKey(w);
        if (isClassForced(cls)) {
            // Just make sure it's raised properly.
            workspace.raiseWindow(w);
        }
    });

    print("kwin-focus-helper: watching for windowAdded/windowActivated");
})();

function windowClassKey(w) {
    // For X11 apps, resourceClass is usually the right match (e.g. "google-chrome").
    // For Wayland apps, desktopFileName is often more reliable.
    if (!w)
        return "";

    // Prefer desktopFileName for Wayland if present, then fall back to resourceClass.
    if (w.desktopFileName && w.desktopFileName.length)
        return w.desktopFileName;
    if (w.resourceClass && w.resourceClass.length)
        return w.resourceClass;

    return "";
}

function maybeForceFocus(w) {
    if (!w)
        return;

    // Some windows may be technically "there" but marked for deletion.
    if (w.deleted)
        return;

    // Only normal windows / dialogs that actually want input
    if (typeof w.wantsInput !== "undefined" && !w.wantsInput)
        return;

    var cls = windowClassKey(w);
    if (!cls || !isClassForced(cls))
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
    if (!w || w.deleted)
        return;

    var cls = windowClassKey(w);
    if (cls && isClassForced(cls)) {
        // Just make sure it's raised properly.
        workspace.raiseWindow(w);
    }
});

print("kwin-focus-helper: watching for windowAdded/windowActivated");

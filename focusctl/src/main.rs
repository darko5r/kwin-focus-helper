use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GROUP_NAME: &str = "Script-kwin-focus-helper";
const KEY_NAME: &str = "forceFocusClasses";

const SCRIPT_ID: &str = "kwin-focus-helper";
const PLUGINS_GROUP: &str = "Plugins";

// -------------------------------
// Pretty output (aligned + subtle)
// -------------------------------

fn colors_enabled() -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match env::var("TERM") {
        Ok(t) => t != "dumb",
        Err(_) => false,
    }
}

fn paint(s: &str, code: &str) -> String {
    if !colors_enabled() {
        return s.to_string();
    }
    format!("\x1b[{}m{}\x1b[0m", code, s)
}

fn bold(s: &str) -> String {
    paint(s, "1")
}
fn dim(s: &str) -> String {
    paint(s, "2")
}
fn cyan(s: &str) -> String {
    paint(s, "36")
}
fn soft_red(s: &str) -> String {
    paint(s, "31")
}

// -------------------------------------
// Display width (no deps, pragmatic)
// -------------------------------------
// This is a small "good enough" width estimator for CLI alignment.
// - Combining marks -> width 0
// - CJK fullwidth/wide ranges -> width 2
// - Common emoji ranges -> width 2
// Everything else -> width 1
//
// This avoids .len() and keeps columns aligned even with non-ASCII text.
fn is_combining_mark(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x0300..=0x036F // Combining Diacritical Marks
            | 0x1AB0..=0x1AFF // Combining Diacritical Marks Extended
            | 0x1DC0..=0x1DFF // Combining Diacritical Marks Supplement
            | 0x20D0..=0x20FF // Combining Diacritical Marks for Symbols
            | 0xFE20..=0xFE2F // Combining Half Marks
    )
}

fn is_wide(c: char) -> bool {
    let u = c as u32;

    // CJK, fullwidth forms, hangul, etc.
    if matches!(
        u,
        0x1100..=0x115F // Hangul Jamo init
            | 0x2329..=0x232A
            | 0x2E80..=0xA4CF // CJK + Yi + etc (broad)
            | 0xAC00..=0xD7A3 // Hangul syllables
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60 // Fullwidth Forms
            | 0xFFE0..=0xFFE6
    ) {
        return true;
    }

    // Common emoji blocks (not perfect, but works well for CLI)
    matches!(
        u,
        0x1F300..=0x1F5FF // Misc Symbols and Pictographs
            | 0x1F600..=0x1F64F // Emoticons
            | 0x1F680..=0x1F6FF // Transport and Map
            | 0x1F700..=0x1F77F // Alchemical Symbols
            | 0x1F780..=0x1F7FF // Geometric Extended
            | 0x1F800..=0x1F8FF // Supplemental Arrows-C
            | 0x1F900..=0x1F9FF // Supplemental Symbols and Pictographs
            | 0x1FA00..=0x1FAFF // Symbols and Pictographs Extended-A
            | 0x2600..=0x26FF // Misc symbols
            | 0x2700..=0x27BF // Dingbats
    )
}

fn display_width(s: &str) -> usize {
    let mut w = 0usize;
    for c in s.chars() {
        if c == '\n' || c == '\r' || c == '\t' {
            // treat controls as 1 cell (safe for help formatting)
            w += 1;
            continue;
        }
        if is_combining_mark(c) {
            continue;
        }
        if is_wide(c) {
            w += 2;
        } else {
            w += 1;
        }
    }
    w
}

// Pad the *plain* left column to `w` display cells, then optionally color it.
// Alignment remains correct regardless of ANSI codes (padding happens before coloring).
fn col_left(plain: &str, w: usize, color_code: Option<&str>) -> String {
    let mut s = plain.to_string();
    let cur = display_width(&s);
    if cur < w {
        s.push_str(&" ".repeat(w - cur));
    }
    match color_code {
        Some(code) => paint(&s, code),
        None => s,
    }
}

fn line2(w: usize, left_plain: &str, left_color: Option<&str>, right: &str, right_dim: bool) {
    let left = col_left(left_plain, w, left_color);
    let right = if right_dim { dim(right) } else { right.to_string() };
    eprintln!("  {}  {}", left, right);
}

fn section(title: &str) {
    // Use cyan for the section title for a subtle 2nd tone.
    eprintln!("{}", cyan(&bold(title)));
}

fn info(msg: &str) {
    eprintln!("{} {}", dim("focusctl:"), msg);
}
fn err(msg: &str) {
    eprintln!("{} {}", soft_red("focusctl:"), msg);
}

fn usage() {
    const W: usize = 34;

    eprintln!("{}", bold("kwin-focus-helper / focusctl"));
    eprintln!();

    section("Global options:");
    line2(W, "--uid <uid>", Some("36"), "Target this uid's KWin config/session", true);
    line2(W, "--user <name>", Some("36"), "Target this user's KWin config/session", true);
    line2(
        W,
        "--session-auto",
        Some("36"),
        "Auto-detect active graphical session user (root-friendly)",
        true,
    );
    eprintln!();

    section("Commands:");
    line2(
        W,
        "list-classes [--keys|-k]",
        Some("36"),
        "List stored classes (optional: show match keys)",
        true,
    );
    line2(W, "list-keys", Some("36"), "Show stored value -> normalized match key", true);
    line2(
        W,
        "add-class <window-class>",
        Some("36"),
        "Add class (spelling preserved, matching normalized)",
        true,
    );
    line2(
        W,
        "remove-class <window-class>",
        Some("36"),
        "Remove by match key (case-insensitive, strips .desktop)",
        true,
    );
    line2(
        W,
        "set-classes <c1;c2;c3>",
        Some("36"),
        "Replace entire list (separators: ';' ',' whitespace)",
        true,
    );
    line2(W, "clear", Some("36"), "Clear all configured classes", true);
    line2(W, "enable", Some("36"), "Set [Plugins] kwin-focus-helperEnabled=true", true);
    line2(W, "disable", Some("36"), "Set [Plugins] kwin-focus-helperEnabled=false", true);
    line2(W, "enabled", Some("36"), "Print enabled state: true/false/(unset)", true);
    line2(
        W,
        "reconfigure",
        Some("36"),
        "Request org.kde.KWin /KWin reconfigure (best-effort)",
        true,
    );
    eprintln!();

    section("Integration wrappers:");
    line2(
        W,
        "wrap <ClassName> -- <cmd...>",
        Some("36"),
        "Ensure class exists + (optional) enable/reconfigure, then exec",
        true,
    );
    line2(
        W,
        "wrap --auto -- <cmd...>",
        Some("36"),
        "Auto class name from argv[0] (example: echo -> EchoApp)",
        true,
    );
    line2(W, "wrap ... [--dry-run]", Some("36"), "Print actions only (no changes, no exec)", true);
    line2(W, "wrap ... [--no-enable]", Some("36"), "Do not set plugin enabled flag", true);
    line2(
        W,
        "wrap ... [--no-reconfigure]",
        Some("36"),
        "Do not request KWin reconfigure",
        true,
    );
    eprintln!();

    section("Notes:");
    eprintln!("  {}", dim("• Matching is case-insensitive and ignores trailing '.desktop'."));
    eprintln!("  {}", dim("• Stored/display names preserve your spelling (e.g. ProcletChrome)."));
    eprintln!("  {}", dim("• Set NO_COLOR=1 to disable colors."));
}

// -------------------------------
// Target selection (uid/user/auto)
// -------------------------------

#[derive(Clone, Debug)]
struct Target {
    uid: u32,
    user: String,
    home: PathBuf,
}

fn parse_passwd() -> io::Result<Vec<(String, u32, PathBuf)>> {
    let s = fs::read_to_string("/etc/passwd")?;
    let mut out = Vec::new();
    for line in s.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        // name:pw:uid:gid:gecos:home:shell
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 7 {
            continue;
        }
        let name = parts[0].to_string();
        let uid: u32 = match parts[2].parse() {
            Ok(x) => x,
            Err(_) => continue,
        };
        let home = PathBuf::from(parts[5]);
        out.push((name, uid, home));
    }
    Ok(out)
}

fn find_user_by_name(name: &str) -> io::Result<Option<Target>> {
    for (n, uid, home) in parse_passwd()? {
        if n == name {
            return Ok(Some(Target { uid, user: n, home }));
        }
    }
    Ok(None)
}

fn find_user_by_uid(uid: u32) -> io::Result<Option<Target>> {
    for (n, u, home) in parse_passwd()? {
        if u == uid {
            return Ok(Some(Target { uid: u, user: n, home }));
        }
    }
    Ok(None)
}

// Best-effort "who am I" without libc.
fn current_uid() -> u32 {
    if let Ok(u) = env::var("UID") {
        if let Ok(x) = u.parse::<u32>() {
            return x;
        }
    }
    if let Ok(out) = Command::new("id").arg("-u").output() {
        if out.status.success() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                if let Ok(x) = s.trim().parse::<u32>() {
                    return x;
                }
            }
        }
    }
    0
}

fn current_user() -> String {
    env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}

fn current_home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

// -------------------------------
// KWin config path + IO
// -------------------------------

fn config_path_for(target: &Target) -> PathBuf {
    // For a different user we can't reliably know XDG_CONFIG_HOME; assume ~/.config.
    target.home.join(".config").join("kwinrc")
}

fn read_kwinrc(target: &Target) -> io::Result<String> {
    fs::read_to_string(config_path_for(target))
}

fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp.kwin-focus-helper");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

// -------------------------------
// Parsing + normalization
// -------------------------------

fn class_key(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    let lower = s.to_lowercase();
    let lower = lower.strip_suffix(".desktop").unwrap_or(&lower);
    lower.to_string()
}

fn parse_classes(value: &str) -> Vec<String> {
    value
        .split(|c: char| c == ';' || c == ',' || c.is_whitespace())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn join_classes(classes: &[String]) -> String {
    classes.join(";")
}

#[derive(Debug)]
struct ScriptConfig {
    group_header_index: Option<usize>,
    value_line_index: Option<usize>,
    value: String,
}

/// Finds `[Script-kwin-focus-helper]` group and `forceFocusClasses=...` within it.
fn extract_script_config(lines: &[String]) -> ScriptConfig {
    let target_header = format!("[{}]", GROUP_NAME);
    let mut in_group = false;

    let mut group_header_index = None;
    let mut value_line_index = None;
    let mut value = String::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == target_header {
                in_group = true;
                group_header_index = Some(i);
            } else {
                in_group = false;
            }
            continue;
        }

        if in_group {
            let prefix = format!("{}=", KEY_NAME);
            if trimmed.starts_with(&prefix) {
                value_line_index = Some(i);
                value = trimmed[prefix.len()..].to_string();
            }
        }
    }

    ScriptConfig {
        group_header_index,
        value_line_index,
        value,
    }
}

/// Finds `[Plugins]` and `kwin-focus-helperEnabled=...` within it.
fn extract_plugins_enabled(lines: &[String]) -> (Option<usize>, Option<usize>, Option<bool>) {
    let header = format!("[{}]", PLUGINS_GROUP);
    let key = format!("{}Enabled", SCRIPT_ID);

    let mut in_group = false;
    let mut group_header_index = None;
    let mut value_line_index = None;
    let mut enabled: Option<bool> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == header {
                in_group = true;
                group_header_index = Some(i);
            } else {
                in_group = false;
            }
            continue;
        }

        if in_group {
            let prefix = format!("{}=", key);
            if trimmed.starts_with(&prefix) {
                value_line_index = Some(i);
                let v = trimmed[prefix.len()..].trim().to_lowercase();
                enabled = Some(v == "true" || v == "1" || v == "yes");
            }
        }
    }

    (group_header_index, value_line_index, enabled)
}

// -------------------------------
// KWin DBus reconfigure (root-friendly target)
// -------------------------------

fn have_cmd(name: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Find active graphical session env for a target uid:
/// returns (XDG_RUNTIME_DIR, DBUS_SESSION_BUS_ADDRESS)
fn detect_session_env_for_uid(uid: u32) -> io::Result<Option<(String, String)>> {
    if !have_cmd("loginctl") {
        return Ok(None);
    }

    let out = Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .output()?;

    if !out.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let sid = match line.split_whitespace().next() {
            Some(x) => x,
            None => continue,
        };

        // Active?
        let active = Command::new("loginctl")
            .args(["show-session", sid, "-p", "Active", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if active != "yes" {
            continue;
        }

        // Class user?
        let class = Command::new("loginctl")
            .args(["show-session", sid, "-p", "Class", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if class != "user" {
            continue;
        }

        // Type wayland/x11?
        let ty = Command::new("loginctl")
            .args(["show-session", sid, "-p", "Type", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if ty != "wayland" && ty != "x11" {
            continue;
        }

        // State active/online?
        let state = Command::new("loginctl")
            .args(["show-session", sid, "-p", "State", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if state != "active" && state != "online" {
            continue;
        }

        // User uid?
        let user_uid = Command::new("loginctl")
            .args(["show-session", sid, "-p", "User", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let session_uid: u32 = match user_uid.parse() {
            Ok(x) => x,
            Err(_) => continue,
        };

        if session_uid != uid {
            continue;
        }

        let xdg = Command::new("loginctl")
            .args(["show-session", sid, "-p", "XDG_RUNTIME_DIR", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let dbus = Command::new("loginctl")
            .args(["show-session", sid, "-p", "DBUS_SESSION_BUS_ADDRESS", "--value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if !xdg.is_empty() && !dbus.is_empty() {
            return Ok(Some((xdg, dbus)));
        }
    }

    Ok(None)
}

fn run_as_target(target: &Target, mut cmd: Command) -> io::Result<std::process::ExitStatus> {
    let self_uid = current_uid();
    if self_uid == 0 && target.uid != 0 && have_cmd("sudo") {
        let mut sudo = Command::new("sudo");
        sudo.arg("-u").arg(format!("#{}", target.uid)).arg("-H");
        let prog = cmd.get_program().to_os_string();
        let args: Vec<_> = cmd.get_args().map(|a| a.to_os_string()).collect();
        sudo.arg(prog);
        for a in args {
            sudo.arg(a);
        }
        sudo.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        return sudo.status();
    }

    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    cmd.status()
}

fn reload_kwin_config(target: &Target) {
    let cmds: &[(&str, [&str; 3])] = &[
        ("qdbus6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus-qt6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus-qt5", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus", ["org.kde.KWin", "/KWin", "reconfigure"]),
    ];

    let mut session_env: Option<(String, String)> = None;
    if let Ok(Some((xdg, dbus))) = detect_session_env_for_uid(target.uid) {
        session_env = Some((xdg, dbus));
    }

    for (prog, args) in cmds {
        if !have_cmd(prog) {
            continue;
        }

        let mut c = Command::new(prog);
        c.args(args);

        if let Some((ref xdg, ref dbus)) = session_env {
            c.env("XDG_RUNTIME_DIR", xdg);
            c.env("DBUS_SESSION_BUS_ADDRESS", dbus);
        }

        match run_as_target(target, c) {
            Ok(st) if st.success() => {
                info(&format!("requested KWin reconfigure via {}", prog));
                return;
            }
            _ => {}
        }
    }

    err("could not call qdbus/qdbus6; you may need to run manually:");
    eprintln!("\tqdbus org.kde.KWin /KWin reconfigure");
}

// -------------------------------
// Config operations
// -------------------------------

fn get_classes(target: &Target) -> io::Result<Vec<String>> {
    let contents = read_kwinrc(target).unwrap_or_default();
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let cfg = extract_script_config(&lines);

    if cfg.value.is_empty() {
        return Ok(Vec::new());
    }

    Ok(parse_classes(&cfg.value))
}

fn set_classes(target: &Target, new_classes: &[String], do_reconfigure: bool) -> io::Result<()> {
    let path = config_path_for(target);
    let contents = read_kwinrc(target).unwrap_or_default();

    let mut lines: Vec<String> = if contents.is_empty() {
        Vec::new()
    } else {
        contents.lines().map(|s| s.to_string()).collect()
    };

    let cfg = extract_script_config(&lines);

    let joined = join_classes(new_classes);
    let new_line = format!("{}={}", KEY_NAME, joined);

    match (cfg.group_header_index, cfg.value_line_index) {
        (Some(_hdr), Some(val_idx)) => lines[val_idx] = new_line,
        (Some(hdr_idx), None) => lines.insert(hdr_idx + 1, new_line),
        (None, _) => {
            if !lines.is_empty() && !lines.last().unwrap().is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("[{}]", GROUP_NAME));
            lines.push(new_line);
        }
    }

    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }

    atomic_write(&path, &out)?;

    if do_reconfigure {
        reload_kwin_config(target);
    }

    Ok(())
}

fn get_enabled(target: &Target) -> io::Result<Option<bool>> {
    let contents = read_kwinrc(target).unwrap_or_default();
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let (_hdr, _val, enabled) = extract_plugins_enabled(&lines);
    Ok(enabled)
}

fn set_enabled(target: &Target, enabled: bool, do_reconfigure: bool) -> io::Result<()> {
    let path = config_path_for(target);
    let contents = read_kwinrc(target).unwrap_or_default();

    let mut lines: Vec<String> = if contents.is_empty() {
        Vec::new()
    } else {
        contents.lines().map(|s| s.to_string()).collect()
    };

    let (hdr_idx, val_idx, _cur) = extract_plugins_enabled(&lines);

    let key = format!("{}Enabled", SCRIPT_ID);
    let new_line = format!("{}={}", key, if enabled { "true" } else { "false" });

    match (hdr_idx, val_idx) {
        (Some(_h), Some(v)) => lines[v] = new_line,
        (Some(h), None) => lines.insert(h + 1, new_line),
        (None, _) => {
            if !lines.is_empty() && !lines.last().unwrap().is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("[{}]", PLUGINS_GROUP));
            lines.push(new_line);
        }
    }

    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }

    atomic_write(&path, &out)?;

    if do_reconfigure {
        reload_kwin_config(target);
    }

    Ok(())
}

// -------------------------------
// Wrapper: auto class naming
// -------------------------------

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn auto_class_from_argv0(argv0: &str) -> String {
    let base = basename(argv0);
    let base = base.strip_suffix(".desktop").unwrap_or(base);
    let base = base.strip_suffix(".sh").unwrap_or(base);

    let mut words = Vec::new();
    let mut cur = String::new();
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch);
        } else if !cur.is_empty() {
            words.push(cur.clone());
            cur.clear();
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }

    if words.is_empty() {
        return "App".to_string();
    }

    // echo -> EchoApp
    let mut out = String::new();
    let first = &words[0];
    let mut chars = first.chars();
    if let Some(c0) = chars.next() {
        out.push(c0.to_ascii_uppercase());
        for c in chars {
            out.push(c.to_ascii_lowercase());
        }
    }
    out.push_str("App");
    out
}

// -------------------------------
// Exec helper
// -------------------------------

#[cfg(unix)]
fn exec_replace(mut cmd: Command) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    let e = cmd.exec(); // only returns on error
    Err(e)
}

#[cfg(not(unix))]
fn exec_replace(mut cmd: Command) -> io::Result<()> {
    let st = cmd.status()?;
    if st.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, format!("exit: {}", st)))
    }
}

// -------------------------------
// Main
// -------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();
    let _prog = args.get(0).cloned().unwrap_or_else(|| "focusctl".to_string());

    // Parse global options
    let mut i = 1usize;
    let mut target_uid: Option<u32> = None;
    let mut target_user: Option<String> = None;
    let mut session_auto = false;

    while i < args.len() {
        match args[i].as_str() {
            "--uid" => {
                i += 1;
                if i >= args.len() {
                    err("--uid requires a value");
                    usage();
                    return;
                }
                match args[i].parse::<u32>() {
                    Ok(x) => target_uid = Some(x),
                    Err(_) => {
                        err("invalid uid");
                        return;
                    }
                }
                i += 1;
            }
            "--user" => {
                i += 1;
                if i >= args.len() {
                    err("--user requires a value");
                    usage();
                    return;
                }
                target_user = Some(args[i].clone());
                i += 1;
            }
            "--session-auto" => {
                session_auto = true;
                i += 1;
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            _ => break,
        }
    }

    // Determine target user
    let target: Target = if let Some(name) = target_user.clone() {
        match find_user_by_name(&name) {
            Ok(Some(t)) => t,
            Ok(None) => {
                err(&format!("unknown user: {}", name));
                return;
            }
            Err(e) => {
                err(&format!("failed to read /etc/passwd: {}", e));
                return;
            }
        }
    } else if let Some(uid) = target_uid {
        match find_user_by_uid(uid) {
            Ok(Some(t)) => t,
            Ok(None) => {
                err(&format!("unknown uid: {}", uid));
                return;
            }
            Err(e) => {
                err(&format!("failed to read /etc/passwd: {}", e));
                return;
            }
        }
    } else if session_auto {
        if !have_cmd("loginctl") {
            err("loginctl not available; cannot --session-auto");
            return;
        }

        // Pick the first Active user session (wayland/x11, class user).
        let out = Command::new("loginctl")
            .args(["list-sessions", "--no-legend"])
            .output()
            .ok();

        let mut picked: Option<u32> = None;
        if let Some(out) = out {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                'outer: for line in text.lines() {
                    let sid = match line.split_whitespace().next() {
                        Some(x) => x,
                        None => continue,
                    };
                    let active = Command::new("loginctl")
                        .args(["show-session", sid, "-p", "Active", "--value"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if active != "yes" {
                        continue;
                    }
                    let class = Command::new("loginctl")
                        .args(["show-session", sid, "-p", "Class", "--value"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if class != "user" {
                        continue;
                    }
                    let ty = Command::new("loginctl")
                        .args(["show-session", sid, "-p", "Type", "--value"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if ty != "wayland" && ty != "x11" {
                        continue;
                    }
                    let user_uid = Command::new("loginctl")
                        .args(["show-session", sid, "-p", "User", "--value"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if let Ok(u) = user_uid.parse::<u32>() {
                        picked = Some(u);
                        break 'outer;
                    }
                }
            }
        }

        let uid = match picked {
            Some(u) => u,
            None => {
                err("could not auto-detect active graphical session user");
                return;
            }
        };

        match find_user_by_uid(uid) {
            Ok(Some(t)) => t,
            _ => {
                err("could not resolve session uid to a user");
                return;
            }
        }
    } else {
        // Default: current user context
        let uid = current_uid();
        let user = current_user();
        let home = current_home();
        Target { uid, user, home }
    };

    // Use Target.user so it isn't dead-code, and it’s genuinely useful for UX.
    // Keep it subtle (dim).
    info(&format!(
        "target: {} (uid {})",
        target.user,
        target.uid
    ));

    // Remaining args: command...
    if i >= args.len() {
        usage();
        return;
    }

    let cmd = args[i].clone();
    i += 1;

    match cmd.as_str() {
        "list-classes" => {
            let mut show_keys = false;
            while i < args.len() {
                match args[i].as_str() {
                    "--keys" | "-k" => show_keys = true,
                    _ => break,
                }
                i += 1;
            }

            match get_classes(&target) {
                Ok(classes) => {
                    if classes.is_empty() {
                        println!("(no forced classes configured)");
                    } else if show_keys {
                        for c in classes {
                            println!("{:<24} -> {}", c, class_key(&c));
                        }
                    } else {
                        for c in classes {
                            println!("{}", c);
                        }
                    }
                }
                Err(e) => err(&format!("failed to read config: {}", e)),
            }
        }

        "list-keys" => match get_classes(&target) {
            Ok(classes) => {
                if classes.is_empty() {
                    println!("(no forced classes configured)");
                } else {
                    for c in classes {
                        println!("{:<24} -> {}", c, class_key(&c));
                    }
                }
            }
            Err(e) => err(&format!("failed to read config: {}", e)),
        },

        "add-class" => {
            let class = match args.get(i) {
                Some(c) => c.clone(),
                None => {
                    err("add-class requires <window-class>");
                    return;
                }
            };

            let input = class.trim().to_string();
            let ikey = class_key(&input);
            if ikey.is_empty() {
                err("empty class");
                return;
            }

            let mut classes = get_classes(&target).unwrap_or_default();
            let exists = classes.iter().any(|c| class_key(c) == ikey);

            if exists {
                info("class already present");
                return;
            }

            classes.push(input);
            if let Err(e) = set_classes(&target, &classes, true) {
                err(&format!("failed to write config: {}", e));
            } else {
                info("added class");
            }
        }

        "remove-class" => {
            let class = match args.get(i) {
                Some(c) => c.clone(),
                None => {
                    err("remove-class requires <window-class>");
                    return;
                }
            };

            let tkey = class_key(&class);
            if tkey.is_empty() {
                err("empty class");
                return;
            }

            let mut classes = get_classes(&target).unwrap_or_default();
            let before = classes.len();
            classes.retain(|c| class_key(c) != tkey);

            if classes.len() == before {
                info("class not found");
                return;
            }

            if let Err(e) = set_classes(&target, &classes, true) {
                err(&format!("failed to write config: {}", e));
            } else {
                info("removed class");
            }
        }

        "set-classes" => {
            let spec = match args.get(i) {
                Some(s) => s.clone(),
                None => {
                    err("set-classes requires a list like 'a;b;c'");
                    return;
                }
            };

            let classes = parse_classes(&spec);
            if let Err(e) = set_classes(&target, &classes, true) {
                err(&format!("failed to write config: {}", e));
            } else {
                info("set classes");
            }
        }

        "clear" => {
            let classes: Vec<String> = Vec::new();
            if let Err(e) = set_classes(&target, &classes, true) {
                err(&format!("failed to write config: {}", e));
            } else {
                info("cleared classes");
            }
        }

        "enable" => {
            if let Err(e) = set_enabled(&target, true, true) {
                err(&format!("failed to enable script: {}", e));
            } else {
                info(&format!("enabled {}", SCRIPT_ID));
            }
        }

        "disable" => {
            if let Err(e) = set_enabled(&target, false, true) {
                err(&format!("failed to disable script: {}", e));
            } else {
                info(&format!("disabled {}", SCRIPT_ID));
            }
        }

        "enabled" => match get_enabled(&target) {
            Ok(Some(true)) => println!("true"),
            Ok(Some(false)) => println!("false"),
            Ok(None) => println!("(unset)"),
            Err(e) => err(&format!("failed to read enabled flag: {}", e)),
        },

        "reconfigure" => {
            reload_kwin_config(&target);
        }

        "wrap" => {
            // wrap <ClassName>|--auto [--dry-run] [--no-enable] [--no-reconfigure] -- <command...>
            let mut dry_run = false;
            let mut no_enable = false;
            let mut no_reconf = false;

            let class_or_auto = match args.get(i) {
                Some(s) => s.clone(),
                None => {
                    err("wrap requires <ClassName>|--auto and '-- <command...>'");
                    usage();
                    return;
                }
            };
            i += 1;

            let mut class_name: Option<String> = None;
            let mut auto = false;

            if class_or_auto == "--auto" {
                auto = true;
            } else {
                class_name = Some(class_or_auto);
            }

            while i < args.len() {
                match args[i].as_str() {
                    "--dry-run" => dry_run = true,
                    "--no-enable" => no_enable = true,
                    "--no-reconfigure" => no_reconf = true,
                    "--" => {
                        i += 1;
                        break;
                    }
                    _ => {
                        err(&format!("unknown wrap option: {}", args[i]));
                        return;
                    }
                }
                i += 1;
            }

            if i >= args.len() {
                err("wrap: missing command after '--'");
                return;
            }

            let cmd_argv: Vec<String> = args[i..].to_vec();
            let argv0 = cmd_argv.get(0).cloned().unwrap_or_default();

            let final_class = if auto {
                auto_class_from_argv0(&argv0)
            } else {
                class_name.unwrap_or_else(|| "App".to_string())
            };

            let key = class_key(&final_class);
            if key.is_empty() {
                err("wrap: empty class name");
                return;
            }

            // Ensure the class exists in config (preserve spelling).
            let mut classes = get_classes(&target).unwrap_or_default();
            let exists = classes.iter().any(|c| class_key(c) == key);

            if dry_run {
                info(&format!(
                    "[dry-run] would ensure integration for class: {}",
                    final_class
                ));
                if !no_enable {
                    info("[dry-run] would enable script");
                }
                if !no_reconf {
                    info("[dry-run] would request KWin reconfigure");
                }
                info(&format!("[dry-run] would exec: {:?}", cmd_argv));
                return;
            }

            if !exists {
                classes.push(final_class.clone());
                if let Err(e) = set_classes(&target, &classes, false) {
                    err(&format!("wrap: failed to write class list: {}", e));
                    return;
                }
            }

            if !no_enable {
                let _ = set_enabled(&target, true, false);
            }

            if !no_reconf {
                reload_kwin_config(&target);
            }

            // Exec the command
            let mut c = Command::new(&cmd_argv[0]);
            if cmd_argv.len() > 1 {
                c.args(&cmd_argv[1..]);
            }

            if let Err(e) = exec_replace(c) {
                err(&format!("exec failed: {}", e));
            }
        }

        _ => {
            usage();
        }
    }
}

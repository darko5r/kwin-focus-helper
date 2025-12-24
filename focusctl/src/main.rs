use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const GROUP_NAME: &str = "Script-kwin-focus-helper";
const KEY_NAME: &str = "forceFocusClasses";

const SCRIPT_ID: &str = "kwin-focus-helper";
const PLUGINS_GROUP: &str = "Plugins";

// ----------------------------- user/session helpers -----------------------------

#[derive(Debug, Clone)]
struct TargetUser {
    uid: u32,
    gid: u32,
    username: String,
    home: PathBuf,
}

#[derive(Debug, Clone, Default)]
struct SessionEnv {
    xdg_runtime_dir: Option<String>,
    dbus_session_bus_address: Option<String>,
    display: Option<String>,
    wayland_display: Option<String>,
    xauthority: Option<String>,
    session_type: Option<String>, // "x11" or "wayland"
}

fn is_root() -> bool {
    matches!(current_euid(), Ok(0))
}

/// Current effective uid without libc (via `id -u`).
fn current_euid() -> io::Result<u32> {
    let out = Command::new("id")
        .args(["-u"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !out.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "id -u failed"));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim()
        .parse::<u32>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "id -u parse failed"))
}

fn user_by_uid(uid: u32) -> io::Result<TargetUser> {
    let passwd = fs::read_to_string("/etc/passwd")?;
    for line in passwd.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        // name:pw:uid:gid:gecos:home:shell
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 7 {
            continue;
        }
        let p_uid: u32 = match parts[2].parse() {
            Ok(x) => x,
            Err(_) => continue,
        };
        if p_uid == uid {
            let gid: u32 = parts[3].parse().unwrap_or(0);
            let name = parts[0].to_string();
            let home = PathBuf::from(parts[5]);
            return Ok(TargetUser {
                uid,
                gid,
                username: name,
                home,
            });
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("uid {} not found in /etc/passwd", uid),
    ))
}

fn user_by_name(name: &str) -> io::Result<TargetUser> {
    let passwd = fs::read_to_string("/etc/passwd")?;
    for line in passwd.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 7 {
            continue;
        }
        if parts[0] == name {
            let uid: u32 = parts[2].parse().unwrap_or(0);
            let gid: u32 = parts[3].parse().unwrap_or(0);
            let home = PathBuf::from(parts[5]);
            return Ok(TargetUser {
                uid,
                gid,
                username: name.to_string(),
                home,
            });
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("user '{}' not found in /etc/passwd", name),
    ))
}

fn loginctl_show(session_id: &str, prop: &str) -> io::Result<String> {
    let out = Command::new("loginctl")
        .args(["show-session", session_id, "-p", prop, "--value"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;

    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("loginctl show-session {} {} failed", session_id, prop),
        ));
    }

    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Detect active graphical session UID (Active=yes, Class=user, Type=x11|wayland).
fn detect_active_graphical_uid() -> io::Result<u32> {
    let out = Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;

    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "loginctl list-sessions failed",
        ));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let sid = line.split_whitespace().next().unwrap_or("");
        if sid.is_empty() {
            continue;
        }

        let active = loginctl_show(sid, "Active").unwrap_or_default();
        if active.trim() != "yes" {
            continue;
        }

        let class = loginctl_show(sid, "Class").unwrap_or_default();
        if class.trim() != "user" {
            continue;
        }

        let stype = loginctl_show(sid, "Type").unwrap_or_default();
        let stype = stype.trim();
        if stype != "x11" && stype != "wayland" {
            continue;
        }

        let uid_s = loginctl_show(sid, "User").unwrap_or_default();
        if let Ok(uid) = uid_s.trim().parse::<u32>() {
            return Ok(uid);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no active graphical user session detected",
    ))
}

/// Detect active session id for a uid (must be Active=yes, Class=user, Type=x11|wayland).
fn detect_session_id_for_uid(uid: u32) -> io::Result<String> {
    let out = Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;

    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "loginctl list-sessions failed",
        ));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let sid = line.split_whitespace().next().unwrap_or("");
        if sid.is_empty() {
            continue;
        }

        let s_uid = loginctl_show(sid, "User").unwrap_or_default();
        let s_uid: u32 = match s_uid.trim().parse() {
            Ok(x) => x,
            Err(_) => continue,
        };
        if s_uid != uid {
            continue;
        }

        let active = loginctl_show(sid, "Active").unwrap_or_default();
        if active.trim() != "yes" {
            continue;
        }

        let class = loginctl_show(sid, "Class").unwrap_or_default();
        if class.trim() != "user" {
            continue;
        }

        let stype = loginctl_show(sid, "Type").unwrap_or_default();
        let stype = stype.trim();
        if stype != "x11" && stype != "wayland" {
            continue;
        }

        return Ok(sid.to_string());
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no active graphical session found for uid {}", uid),
    ))
}

/// If you're already inside a session, prefer env variables rather than loginctl guessing.
fn session_env_from_current_process() -> Option<SessionEnv> {
    let xdg = env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty());
    let dbus = env::var("DBUS_SESSION_BUS_ADDRESS").ok().filter(|s| !s.is_empty());

    if xdg.is_none() && dbus.is_none() {
        return None;
    }

    let stype = env::var("XDG_SESSION_TYPE").ok().filter(|s| !s.is_empty());
    let display = env::var("DISPLAY").ok().filter(|s| !s.is_empty());
    let wdisp = env::var("WAYLAND_DISPLAY").ok().filter(|s| !s.is_empty());
    let xauth = env::var("XAUTHORITY").ok().filter(|s| !s.is_empty());

    Some(SessionEnv {
        xdg_runtime_dir: xdg,
        dbus_session_bus_address: dbus,
        display,
        wayland_display: wdisp,
        xauthority: xauth,
        session_type: stype,
    })
}

fn run_user_dir(uid: u32) -> PathBuf {
    PathBuf::from("/run/user").join(uid.to_string())
}

fn session_env_for_uid(uid: u32, user_home: &Path) -> io::Result<SessionEnv> {
    // If /run/user/<uid> doesn't exist, there is no real logind user runtime.
    let rundir = run_user_dir(uid);
    if !rundir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} does not exist (no user session?)", rundir.display()),
        ));
    }

    let sid = detect_session_id_for_uid(uid)?;
    let xdg = loginctl_show(&sid, "XDG_RUNTIME_DIR").ok();
    let dbus = loginctl_show(&sid, "DBUS_SESSION_BUS_ADDRESS").ok();
    let stype = loginctl_show(&sid, "Type").ok(); // x11/wayland

    let stype_s = stype.clone().unwrap_or_default();

    // Auto-detect WAYLAND_DISPLAY if Wayland: first wayland-* socket present.
    let mut wayland_display: Option<String> = None;
    if stype_s.trim() == "wayland" {
        if let Ok(rd) = fs::read_dir(&rundir) {
            for ent in rd.flatten() {
                if let Some(name) = ent.file_name().to_str().map(|s| s.to_string()) {
                    if name.starts_with("wayland-") {
                        wayland_display = Some(name);
                        break;
                    }
                }
            }
        }
        if wayland_display.is_none() {
            // Fallback guess
            wayland_display = Some("wayland-0".to_string());
        }
    }

    // Auto-detect DISPLAY for X11: :0 default.
    let display = if stype_s.trim() == "x11" {
        Some(":0".to_string())
    } else {
        None
    };

    let xauth = if stype_s.trim() == "x11" {
        Some(user_home.join(".Xauthority").to_string_lossy().to_string())
    } else {
        None
    };

    Ok(SessionEnv {
        xdg_runtime_dir: xdg.filter(|s| !s.is_empty()),
        dbus_session_bus_address: dbus.filter(|s| !s.is_empty()),
        display,
        wayland_display,
        xauthority: xauth,
        session_type: stype.filter(|s| !s.is_empty()),
    })
}

fn apply_session_env(cmd: &mut Command, sess: &SessionEnv) {
    if let Some(x) = &sess.xdg_runtime_dir {
        cmd.env("XDG_RUNTIME_DIR", x);
    }
    if let Some(x) = &sess.dbus_session_bus_address {
        cmd.env("DBUS_SESSION_BUS_ADDRESS", x);
    }
    if let Some(x) = &sess.session_type {
        cmd.env("XDG_SESSION_TYPE", x);
    }
    if let Some(x) = &sess.display {
        cmd.env("DISPLAY", x);
    }
    if let Some(x) = &sess.wayland_display {
        cmd.env("WAYLAND_DISPLAY", x);
    }
    if let Some(x) = &sess.xauthority {
        cmd.env("XAUTHORITY", x);
    }
}

// ----------------------------- class matching helpers -----------------------------

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

fn dedupe_by_key_keep_first(v: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for s in v {
        let k = class_key(&s);
        if k.is_empty() {
            continue;
        }
        if seen.insert(k) {
            out.push(s);
        }
    }
    out
}

// ----------------------------- kwinrc parsing -----------------------------

#[derive(Debug)]
struct ScriptConfig {
    group_header_index: Option<usize>,
    value_line_index: Option<usize>,
    value: String,
}

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

// ----------------------------- kwinrc IO -----------------------------

fn config_path_for(user: &TargetUser) -> PathBuf {
    user.home.join(".config/kwinrc")
}

fn read_kwinrc_for(user: &TargetUser) -> io::Result<String> {
    fs::read_to_string(config_path_for(user))
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

fn get_classes_for(user: &TargetUser) -> io::Result<Vec<String>> {
    let contents = read_kwinrc_for(user)?;
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let cfg = extract_script_config(&lines);

    if cfg.value.is_empty() {
        return Ok(Vec::new());
    }

    Ok(dedupe_by_key_keep_first(parse_classes(&cfg.value)))
}

fn set_classes_for(user: &TargetUser, new_classes: &[String]) -> io::Result<()> {
    let path = config_path_for(user);
    let contents = read_kwinrc_for(user).unwrap_or_default();

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
    Ok(())
}

fn get_enabled_for(user: &TargetUser) -> io::Result<Option<bool>> {
    let contents = read_kwinrc_for(user)?;
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let (_hdr, _val, enabled) = extract_plugins_enabled(&lines);
    Ok(enabled)
}

fn set_enabled_for(user: &TargetUser, enabled: bool) -> io::Result<()> {
    let path = config_path_for(user);
    let contents = read_kwinrc_for(user).unwrap_or_default();

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
    Ok(())
}

// ----------------------------- KWin reconfigure -----------------------------

fn reload_kwin_config_with_env(sess: &SessionEnv) {
    let cmds: &[(&str, [&str; 3])] = &[
        ("qdbus6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus-qt6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus-qt5", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus", ["org.kde.KWin", "/KWin", "reconfigure"]),
    ];

    for (prog, args) in cmds {
        let mut c = Command::new(prog);
        c.args(args);
        apply_session_env(&mut c, sess);

        if let Ok(status) = c.status() {
            if status.success() {
                eprintln!("focusctl: requested KWin reconfigure via {}", prog);
                return;
            }
        }
    }

    eprintln!(
        "focusctl: could not call qdbus/qdbus6; run manually inside session:\n\
         \tqdbus org.kde.KWin /KWin reconfigure"
    );
}

// ----------------------------- UX helpers -----------------------------

fn print_class_keys(classes: Vec<String>) {
    if classes.is_empty() {
        println!("(no forced classes configured)");
        return;
    }
    for c in classes {
        println!("{:<24} -> {}", c, class_key(&c));
    }
}

fn usage() {
    eprintln!("kwin-focus-helper / focusctl");
    eprintln!();
    eprintln!("Global options:");
    eprintln!("  --uid <uid>        Target this uid's KWin config/session");
    eprintln!("  --user <name>      Target this user's KWin config/session");
    eprintln!("  --session-auto     Auto-detect active graphical session user (root-friendly)");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  focusctl list-classes [--keys|-k]");
    eprintln!("  focusctl list-keys");
    eprintln!("  focusctl add-class <window-class>");
    eprintln!("  focusctl remove-class <window-class>");
    eprintln!("  focusctl set-classes <c1;c2;c3>");
    eprintln!("  focusctl clear");
    eprintln!("  focusctl enable");
    eprintln!("  focusctl disable");
    eprintln!("  focusctl enabled");
    eprintln!("  focusctl reconfigure");
    eprintln!();
    eprintln!("Integration wrappers:");
    eprintln!("  focusctl wrap <ClassName>|--auto [--dry-run] [--no-enable] [--no-reconfigure] -- <command...>");
    eprintln!("Notes:");
    eprintln!("  - Matching is case-insensitive and ignores trailing '.desktop'.");
    eprintln!("  - Stored/display names preserve your spelling (e.g. ProcletChrome).");
    eprintln!("  - list-keys shows stored value -> normalized match key.");
}

fn split_double_dash(args: &[String]) -> (Vec<String>, Vec<String>) {
    if let Some(pos) = args.iter().position(|s| s == "--") {
        (args[..pos].to_vec(), args[pos + 1..].to_vec())
    } else {
        (args.to_vec(), Vec::new())
    }
}

fn auto_class_from_cmd(cmd0: &str) -> String {
    let base = Path::new(cmd0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(cmd0);

    let s = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();

    if s.is_empty() {
        return "FocusApp".to_string();
    }

    let mut out = String::new();
    let mut it = s.chars();
    if let Some(first) = it.next() {
        out.push(first.to_ascii_uppercase());
        out.push_str(it.as_str());
    }
    out.push_str("App");
    out
}

/// Decide target user.
/// Priority:
/// 1) explicit --uid / --user
/// 2) if root and --session-auto => active graphical uid
/// 3) otherwise current uid
fn resolve_target_user(
    explicit_uid: Option<u32>,
    explicit_user: Option<String>,
    session_auto: bool,
) -> io::Result<TargetUser> {
    if let Some(u) = explicit_uid {
        return user_by_uid(u);
    }
    if let Some(name) = explicit_user {
        return user_by_name(&name);
    }

    if session_auto && is_root() {
        let uid = detect_active_graphical_uid()?;
        return user_by_uid(uid);
    }

    let uid = current_euid()?;
    user_by_uid(uid)
}

/// Choose session env:
/// 1) if current process already has session env -> use it (best when running inside desktop)
/// 2) else use loginctl-based session env for target uid
fn resolve_session_env(target: &TargetUser) -> io::Result<SessionEnv> {
    if let Some(sess) = session_env_from_current_process() {
        return Ok(sess);
    }
    session_env_for_uid(target.uid, &target.home)
}

// ----------------------------- wrapper logic -----------------------------

fn ensure_integration(
    user: &TargetUser,
    sess: &SessionEnv,
    class_name: &str,
    no_enable: bool,
    no_reconf: bool,
) -> io::Result<()> {
    if !no_enable {
        let _ = set_enabled_for(user, true);
    }

    let input = class_name.trim().to_string();
    let ikey = class_key(&input);
    if ikey.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "class is empty"));
    }

    let mut classes = get_classes_for(user).unwrap_or_default();
    let exists = classes.iter().any(|c| class_key(c) == ikey);
    if !exists {
        classes.push(input);
        classes = dedupe_by_key_keep_first(classes);
        set_classes_for(user, &classes)?;
    }

    if !no_reconf {
        reload_kwin_config_with_env(sess);
    }

    Ok(())
}

fn exec_command_as(
    user: &TargetUser,
    sess: &SessionEnv,
    cmdv: &[String],
) -> io::Result<()> {
    if cmdv.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing command after --"));
    }

    let mut c = Command::new(&cmdv[0]);
    if cmdv.len() > 1 {
        c.args(&cmdv[1..]);
    }

    c.current_dir(&user.home);
    c.env("HOME", &user.home);
    apply_session_env(&mut c, sess);

    // If root and target user is not root, drop privileges for GUI launch.
    if cfg!(unix) && is_root() && user.uid != 0 {
        c.uid(user.uid);
        c.gid(user.gid);
    }

    #[cfg(unix)]
    {
        let err = c.exec();
        Err(err)
    }

    #[cfg(not(unix))]
    {
        let status = c.status()?;
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }
}

fn handle_wrap(
    argv: Vec<String>,
    explicit_uid: Option<u32>,
    explicit_user: Option<String>,
    session_auto: bool,
) {
    let (left, right_cmd) = split_double_dash(&argv);

    if right_cmd.is_empty() {
        eprintln!("focusctl: wrap requires '-- <command...>'");
        usage();
        return;
    }

    let mut dry_run = false;
    let mut no_enable = false;
    let mut no_reconf = false;
    let mut auto = false;
    let mut class_name: Option<String> = None;

    let mut i = 0;
    while i < left.len() {
        match left[i].as_str() {
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--no-enable" => {
                no_enable = true;
                i += 1;
            }
            "--no-reconfigure" => {
                no_reconf = true;
                i += 1;
            }
            "--auto" => {
                auto = true;
                i += 1;
            }
            x if !x.starts_with('-') && class_name.is_none() && !auto => {
                class_name = Some(left[i].clone());
                i += 1;
            }
            _ => {
                eprintln!("focusctl: unknown wrap option/arg: {}", left[i]);
                return;
            }
        }
    }

    let class = if auto {
        auto_class_from_cmd(&right_cmd[0])
    } else {
        match class_name {
            Some(c) => c,
            None => {
                eprintln!("focusctl: wrap requires <ClassName> or --auto");
                return;
            }
        }
    };

    let user = match resolve_target_user(explicit_uid, explicit_user, session_auto) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("focusctl: failed to resolve target user: {}", e);
            return;
        }
    };

    let sess = match resolve_session_env(&user) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "focusctl: failed to resolve session env for uid {} ({}): {}",
                user.uid, user.username, e
            );
            eprintln!("hint: target uid must have an active graphical session (check /run/user/<uid>).");
            return;
        }
    };

    if dry_run {
        eprintln!("focusctl: [dry-run] target user: {} (uid={})", user.username, user.uid);
        eprintln!("focusctl: [dry-run] class: {}", class);
        eprintln!("focusctl: [dry-run] session env: {:?}", sess);
        eprintln!("focusctl: [dry-run] exec: {:?}", right_cmd);
        return;
    }

    if let Err(e) = ensure_integration(&user, &sess, &class, no_enable, no_reconf) {
        eprintln!("focusctl: failed to ensure integration: {}", e);
        return;
    }

    if let Err(e) = exec_command_as(&user, &sess, &right_cmd) {
        eprintln!("focusctl: exec failed: {}", e);
        std::process::exit(127);
    }
}

// ----------------------------- command handling -----------------------------

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
        return;
    }

    // Global options (must be before command)
    let mut explicit_uid: Option<u32> = None;
    let mut explicit_user: Option<String> = None;
    let mut session_auto = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--uid" => {
                let v = args.get(i + 1).cloned();
                if v.is_none() {
                    eprintln!("focusctl: --uid requires a value");
                    return;
                }
                explicit_uid = v.unwrap().parse::<u32>().ok();
                i += 2;
            }
            "--user" => {
                let v = args.get(i + 1).cloned();
                if v.is_none() {
                    eprintln!("focusctl: --user requires a value");
                    return;
                }
                explicit_user = Some(v.unwrap());
                i += 2;
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

    if i >= args.len() {
        usage();
        return;
    }

    let cmd = args[i].clone();
    let rest = args[(i + 1)..].to_vec();

    let user = resolve_target_user(explicit_uid, explicit_user.clone(), session_auto)
        .unwrap_or_else(|_| user_by_uid(current_euid().unwrap_or(0)).unwrap());

    match cmd.as_str() {
        "list-classes" => {
            let show_keys = matches!(rest.get(0).map(|s| s.as_str()), Some("--keys") | Some("-k"));
            match get_classes_for(&user) {
                Ok(classes) => {
                    if show_keys {
                        print_class_keys(classes);
                    } else if classes.is_empty() {
                        println!("(no forced classes configured)");
                    } else {
                        for c in classes {
                            println!("{}", c);
                        }
                    }
                }
                Err(e) => eprintln!("focusctl: failed to read config: {}", e),
            }
        }

        "list-keys" => match get_classes_for(&user) {
            Ok(classes) => print_class_keys(classes),
            Err(e) => eprintln!("focusctl: failed to read config: {}", e),
        },

        "add-class" => {
            let class = match rest.get(0) {
                Some(c) => c.clone(),
                None => {
                    eprintln!("focusctl: add-class requires <window-class>");
                    return;
                }
            };

            let input = class.trim().to_string();
            let ikey = class_key(&input);
            if ikey.is_empty() {
                eprintln!("focusctl: class is empty");
                return;
            }

            let mut classes = get_classes_for(&user).unwrap_or_default();
            let exists = classes.iter().any(|c| class_key(c) == ikey);

            if !exists {
                classes.push(input);
                classes = dedupe_by_key_keep_first(classes);
                if let Err(e) = set_classes_for(&user, &classes) {
                    eprintln!("focusctl: failed to write config: {}", e);
                } else if let Ok(sess) = resolve_session_env(&user) {
                    reload_kwin_config_with_env(&sess);
                    eprintln!("focusctl: added class");
                } else {
                    eprintln!("focusctl: added class (no session env for reconfigure)");
                }
            } else {
                eprintln!("focusctl: class already present (case-insensitive / .desktop-insensitive)");
            }
        }

        "remove-class" => {
            let class = match rest.get(0) {
                Some(c) => c.clone(),
                None => {
                    eprintln!("focusctl: remove-class requires <window-class>");
                    return;
                }
            };

            let target_key = class_key(&class);
            if target_key.is_empty() {
                eprintln!("focusctl: class is empty");
                return;
            }

            let mut classes = get_classes_for(&user).unwrap_or_default();
            let before = classes.len();
            classes.retain(|c| class_key(c) != target_key);

            if classes.len() == before {
                eprintln!("focusctl: class not found (case-insensitive / .desktop-insensitive)");
                return;
            }

            if let Err(e) = set_classes_for(&user, &classes) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else if let Ok(sess) = resolve_session_env(&user) {
                reload_kwin_config_with_env(&sess);
                eprintln!("focusctl: removed class");
            } else {
                eprintln!("focusctl: removed class (no session env for reconfigure)");
            }
        }

        "set-classes" => {
            let spec = match rest.get(0) {
                Some(s) => s.clone(),
                None => {
                    eprintln!("focusctl: set-classes requires a list like 'a;b;c'");
                    return;
                }
            };

            let classes = dedupe_by_key_keep_first(parse_classes(&spec));
            if let Err(e) = set_classes_for(&user, &classes) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else if let Ok(sess) = resolve_session_env(&user) {
                reload_kwin_config_with_env(&sess);
                eprintln!("focusctl: set classes");
            } else {
                eprintln!("focusctl: set classes (no session env for reconfigure)");
            }
        }

        "clear" => {
            let classes: Vec<String> = Vec::new();
            if let Err(e) = set_classes_for(&user, &classes) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else if let Ok(sess) = resolve_session_env(&user) {
                reload_kwin_config_with_env(&sess);
                eprintln!("focusctl: cleared classes");
            } else {
                eprintln!("focusctl: cleared classes (no session env for reconfigure)");
            }
        }

        "enable" => {
            if let Err(e) = set_enabled_for(&user, true) {
                eprintln!("focusctl: failed to enable script: {}", e);
            } else if let Ok(sess) = resolve_session_env(&user) {
                reload_kwin_config_with_env(&sess);
                eprintln!("focusctl: enabled {}", SCRIPT_ID);
            } else {
                eprintln!("focusctl: enabled {} (no session env for reconfigure)", SCRIPT_ID);
            }
        }

        "disable" => {
            if let Err(e) = set_enabled_for(&user, false) {
                eprintln!("focusctl: failed to disable script: {}", e);
            } else if let Ok(sess) = resolve_session_env(&user) {
                reload_kwin_config_with_env(&sess);
                eprintln!("focusctl: disabled {}", SCRIPT_ID);
            } else {
                eprintln!("focusctl: disabled {} (no session env for reconfigure)", SCRIPT_ID);
            }
        }

        "enabled" => match get_enabled_for(&user) {
            Ok(Some(true)) => println!("true"),
            Ok(Some(false)) => println!("false"),
            Ok(None) => println!("(unset)"),
            Err(e) => eprintln!("focusctl: failed to read enabled flag: {}", e),
        },

        "reconfigure" => {
            if let Ok(sess) = resolve_session_env(&user) {
                reload_kwin_config_with_env(&sess);
            } else {
                reload_kwin_config_with_env(&SessionEnv::default());
            }
        }

        "wrap" => handle_wrap(rest, explicit_uid, explicit_user, session_auto),

        _ => usage(),
    }
}

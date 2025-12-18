use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const GROUP_NAME: &str = "Script-kwin-focus-helper";
const KEY_NAME: &str = "forceFocusClasses";

const SCRIPT_ID: &str = "kwin-focus-helper";
const PLUGINS_GROUP: &str = "Plugins";

fn config_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("kwinrc");
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/kwinrc")
}

fn parse_classes(value: &str) -> Vec<String> {
    value
        .split(|c| c == ';' || c == ',' || c == ' ' || c == '\t' || c == '\n' || c == '\r')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn join_classes(classes: &[String]) -> String {
    // Use ';' because KWin configs commonly store lists like this.
    classes.join(";")
}

fn read_kwinrc() -> io::Result<String> {
    fs::read_to_string(config_path())
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

#[derive(Debug)]
struct ScriptConfig {
    group_header_index: Option<usize>,
    value_line_index: Option<usize>,
    value: String,
}

/// Finds `[Script-kwin-focus-helper]` group and `forceFocusClasses=...` within it.
/// Correctly tracks group boundaries.
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

fn reload_kwin_config() {
    // Plasma 5/6: possible binaries
    let cmds: &[(&str, [&str; 3])] = &[
        ("qdbus6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus-qt6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus-qt5", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus", ["org.kde.KWin", "/KWin", "reconfigure"]),
    ];

    for (prog, args) in cmds {
        if let Ok(status) = Command::new(prog).args(args).status() {
            if status.success() {
                eprintln!("focusctl: requested KWin reconfigure via {}", prog);
                return;
            }
        }
    }

    eprintln!(
        "focusctl: could not call qdbus/qdbus6; you may need to run manually:\n\
         \tqdbus org.kde.KWin /KWin reconfigure"
    );
}

fn get_classes() -> io::Result<Vec<String>> {
    let contents = read_kwinrc()?;
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let cfg = extract_script_config(&lines);

    if cfg.value.is_empty() {
        return Ok(Vec::new());
    }

    Ok(parse_classes(&cfg.value))
}

fn set_classes(new_classes: &[String], do_reconfigure: bool) -> io::Result<()> {
    let path = config_path();
    let contents = read_kwinrc().unwrap_or_default();

    let mut lines: Vec<String> = if contents.is_empty() {
        Vec::new()
    } else {
        contents.lines().map(|s| s.to_string()).collect()
    };

    let cfg = extract_script_config(&lines);

    let joined = join_classes(new_classes);
    let new_line = format!("{}={}", KEY_NAME, joined);

    match (cfg.group_header_index, cfg.value_line_index) {
        (Some(_hdr), Some(val_idx)) => {
            // Replace existing value
            lines[val_idx] = new_line;
        }
        (Some(hdr_idx), None) => {
            // Insert just after header
            lines.insert(hdr_idx + 1, new_line);
        }
        (None, _) => {
            // Append new group at end
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
        reload_kwin_config();
    }

    Ok(())
}

fn get_enabled() -> io::Result<Option<bool>> {
    let contents = read_kwinrc()?;
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let (_hdr, _val, enabled) = extract_plugins_enabled(&lines);
    Ok(enabled)
}

fn set_enabled(enabled: bool, do_reconfigure: bool) -> io::Result<()> {
    let path = config_path();
    let contents = read_kwinrc().unwrap_or_default();

    let mut lines: Vec<String> = if contents.is_empty() {
        Vec::new()
    } else {
        contents.lines().map(|s| s.to_string()).collect()
    };

    let (hdr_idx, val_idx, _cur) = extract_plugins_enabled(&lines);

    let key = format!("{}Enabled", SCRIPT_ID);
    let new_line = format!("{}={}", key, if enabled { "true" } else { "false" });

    match (hdr_idx, val_idx) {
        (Some(_h), Some(v)) => {
            lines[v] = new_line;
        }
        (Some(h), None) => {
            lines.insert(h + 1, new_line);
        }
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
        reload_kwin_config();
    }

    Ok(())
}

fn usage() {
    eprintln!("kwin-focus-helper / focusctl");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  focusctl list-classes");
    eprintln!("  focusctl add-class <window-class>");
    eprintln!("  focusctl remove-class <window-class>");
    eprintln!("  focusctl set-classes <c1;c2;c3>");
    eprintln!("  focusctl clear");
    eprintln!("  focusctl enable");
    eprintln!("  focusctl disable");
    eprintln!("  focusctl enabled");
    eprintln!("  focusctl reconfigure");
    eprintln!();
    eprintln!("Notes:");
    eprintln!("  - Classes can be separated by ';' or ',' or whitespace.");
    eprintln!("  - Wayland apps may show '*.desktop' (e.g. google-chrome.desktop).");
}

fn main() {
    let mut args = env::args();
    let _prog = args.next();

    let cmd = match args.next() {
        Some(c) => c,
        None => {
            usage();
            return;
        }
    };

    match cmd.as_str() {
        "list-classes" => match get_classes() {
            Ok(classes) => {
                if classes.is_empty() {
                    println!("(no forced classes configured)");
                } else {
                    for c in classes {
                        println!("{}", c);
                    }
                }
            }
            Err(e) => eprintln!("focusctl: failed to read config: {}", e),
        },

        "add-class" => {
            let class = match args.next() {
                Some(c) => c,
                None => {
                    eprintln!("focusctl: add-class requires <window-class>");
                    return;
                }
            };

            let mut classes = get_classes().unwrap_or_default();
            if !classes.iter().any(|c| c == &class) {
                classes.push(class);
                if let Err(e) = set_classes(&classes, true) {
                    eprintln!("focusctl: failed to write config: {}", e);
                } else {
                    eprintln!("focusctl: added class");
                }
            } else {
                eprintln!("focusctl: class already present");
            }
        }

        "remove-class" => {
            let class = match args.next() {
                Some(c) => c,
                None => {
                    eprintln!("focusctl: remove-class requires <window-class>");
                    return;
                }
            };

            let mut classes = get_classes().unwrap_or_default();
            let before = classes.len();
            classes.retain(|c| c != &class);

            if classes.len() == before {
                eprintln!("focusctl: class not found");
                return;
            }

            if let Err(e) = set_classes(&classes, true) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else {
                eprintln!("focusctl: removed class");
            }
        }

        "set-classes" => {
            let spec = match args.next() {
                Some(s) => s,
                None => {
                    eprintln!("focusctl: set-classes requires a list like 'a;b;c'");
                    return;
                }
            };

            let classes = parse_classes(&spec);
            if let Err(e) = set_classes(&classes, true) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else {
                eprintln!("focusctl: set classes");
            }
        }

        "clear" => {
            let classes: Vec<String> = Vec::new();
            if let Err(e) = set_classes(&classes, true) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else {
                eprintln!("focusctl: cleared classes");
            }
        }

        "enable" => {
            if let Err(e) = set_enabled(true, true) {
                eprintln!("focusctl: failed to enable script: {}", e);
            } else {
                eprintln!("focusctl: enabled {}", SCRIPT_ID);
            }
        }

        "disable" => {
            if let Err(e) = set_enabled(false, true) {
                eprintln!("focusctl: failed to disable script: {}", e);
            } else {
                eprintln!("focusctl: disabled {}", SCRIPT_ID);
            }
        }

        "enabled" => match get_enabled() {
            Ok(Some(true)) => println!("true"),
            Ok(Some(false)) => println!("false"),
            Ok(None) => println!("(unset)"),
            Err(e) => eprintln!("focusctl: failed to read enabled flag: {}", e),
        },

        "reconfigure" => {
            reload_kwin_config();
        }

        _ => usage(),
    }
}

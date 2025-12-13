use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

const GROUP_NAME: &str = "Script-kwin-focus-helper";
const KEY_NAME: &str = "forceFocusClasses";

fn config_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("kwinrc");
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/kwinrc")
}

fn parse_classes(value: &str) -> Vec<String> {
    value
        .split(|c| c == ';' || c == ',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn join_classes(classes: &[String]) -> String {
    classes.join(";")
}

fn read_kwinrc() -> io::Result<String> {
    fs::read_to_string(config_path())
}

fn write_kwinrc(contents: &str) -> io::Result<()> {
    fs::write(config_path(), contents)
}

struct ScriptConfig {
    has_group: bool,
    group_index: Option<usize>,
    value_index: Option<usize>,
    value: String,
}

fn extract_script_config(lines: &[String]) -> ScriptConfig {
    let mut has_group = false;
    let mut group_index = None;
    let mut value_index = None;
    let mut value = String::new();

    let target_header = format!("[{}]", GROUP_NAME);

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == target_header {
                has_group = true;
                group_index = Some(i);
            } else {
                // leaving group
            }
            continue;
        }

        if has_group && trimmed.starts_with(&(KEY_NAME.to_string() + "=")) {
            value_index = Some(i);
            let v = &trimmed[KEY_NAME.len() + 1..];
            value = v.to_string();
        }
    }

    ScriptConfig {
        has_group,
        group_index,
        value_index,
        value,
    }
}

fn reload_kwin_config() {
    // Try qdbus6 first, then qdbus, ignore errors
    let cmds = [
        ("qdbus6", ["org.kde.KWin", "/KWin", "reconfigure"]),
        ("qdbus", ["org.kde.KWin", "/KWin", "reconfigure"]),
    ];

    for (prog, args) in cmds {
        if let Ok(status) = Command::new(prog).args(&args).status() {
            if status.success() {
                eprintln!("focusctl: requested KWin reconfigure via {}", prog);
                return;
            }
        }
    }

    eprintln!(
        "focusctl: could not call qdbus/qdbus6; you may need to run:\n\
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

fn set_classes(new_classes: &[String]) -> io::Result<()> {
    let contents = read_kwinrc().unwrap_or_default();
    let mut lines: Vec<String> = if contents.is_empty() {
        Vec::new()
    } else {
        contents.lines().map(|s| s.to_string()).collect()
    };

    let mut cfg = extract_script_config(&lines);
    let joined = join_classes(new_classes);
    let new_line = format!("{}={}", KEY_NAME, joined);

    if cfg.has_group {
        if let Some(idx) = cfg.value_index {
            // Replace existing value line
            lines[idx] = new_line;
        } else if let Some(header_idx) = cfg.group_index {
            // Insert just after group header
            lines.insert(header_idx + 1, new_line);
        } else {
            // Group but no header index? Append at end as fallback
            lines.push(format!("[{}]", GROUP_NAME));
            lines.push(new_line);
        }
    } else {
        // Append new group
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[{}]", GROUP_NAME));
        lines.push(new_line);
    }

    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }

    write_kwinrc(&out)?;
    reload_kwin_config();
    Ok(())
}

fn usage() {
    eprintln!("Usage:");
    eprintln!("  focusctl list-classes");
    eprintln!("  focusctl add-class <window-class>");
    eprintln!("  focusctl remove-class <window-class>");
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
        "list-classes" => {
            match get_classes() {
                Ok(classes) => {
                    if classes.is_empty() {
                        println!("(no forced classes configured)");
                    } else {
                        for c in classes {
                            println!("{}", c);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("focusctl: failed to read config: {}", e);
                }
            }
        }

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
                classes.push(class.clone());
                if let Err(e) = set_classes(&classes) {
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

            if let Err(e) = set_classes(&classes) {
                eprintln!("focusctl: failed to write config: {}", e);
            } else {
                eprintln!("focusctl: removed class");
            }
        }

        _ => {
            usage();
        }
    }
}

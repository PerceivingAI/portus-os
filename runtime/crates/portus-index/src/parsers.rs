use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessStat {
    pub pid: u32,
    pub comm: String,
    pub ppid: u32,
    pub start_ticks: u64,
}

pub fn parse_proc_stat(input: &str) -> Option<ProcessStat> {
    let open = input.find('(')?;
    let close = input.rfind(')')?;
    if close <= open {
        return None;
    }
    let pid = input[..open].trim().parse::<u32>().ok()?;
    let comm = input[open + 1..close].to_string();
    let remainder = input.get(close + 1..)?.trim();
    let fields = remainder.split_whitespace().collect::<Vec<_>>();
    // The first token after ')' is stat field 3 (state). Field 4 is ppid and
    // field 22 is the process start time in clock ticks since boot.
    if fields.len() <= 19 {
        return None;
    }
    let ppid = fields[1].parse::<u32>().ok()?;
    let start_ticks = fields[19].parse::<u64>().ok()?;
    Some(ProcessStat {
        pid,
        comm,
        ppid,
        start_ticks,
    })
}

pub fn parse_status_identity(input: &str) -> Option<(u32, u32)> {
    let mut uid = None;
    let mut gid = None;
    for line in input.lines() {
        if let Some(value) = line.strip_prefix("Uid:") {
            uid = value.split_whitespace().next()?.parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("Gid:") {
            gid = value.split_whitespace().next()?.parse::<u32>().ok();
        }
        if uid.is_some() && gid.is_some() {
            break;
        }
    }
    uid.zip(gid)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopEntry {
    pub name: String,
    pub executable: String,
    pub executable_basename: String,
    pub terminal: bool,
}

pub fn parse_desktop_entry(input: &str) -> Option<DesktopEntry> {
    let mut in_desktop = false;
    let mut kind = None;
    let mut name = None;
    let mut exec = None;
    let mut hidden = false;
    let mut terminal = false;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Type" => kind = Some(value.trim().to_string()),
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            "Terminal" => terminal = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    if kind.as_deref() != Some("Application") || hidden {
        return None;
    }
    let name = name?;
    let raw_exec = exec?;
    let executable = first_exec_token(&raw_exec)?;
    let executable_basename = executable
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())?
        .to_string();
    Some(DesktopEntry {
        name,
        executable,
        executable_basename,
        terminal,
    })
}

fn first_exec_token(value: &str) -> Option<String> {
    let mut chars = value.trim().chars().peekable();
    let quoted = matches!(chars.peek(), Some('"'));
    if quoted {
        chars.next();
    }
    let mut token = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            token.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if quoted {
            if ch == '"' {
                break;
            }
        } else if ch.is_whitespace() {
            break;
        }
        token.push(ch);
    }
    if token.is_empty() || token.starts_with('%') {
        None
    } else {
        Some(token)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenRcService {
    pub name: String,
    pub state: String,
    pub runlevel: Option<String>,
}

pub fn parse_openrc_status(input: &str) -> Vec<OpenRcService> {
    let mut runlevel = None;
    let mut services = Vec::new();
    for raw in input.lines() {
        let line = raw.trim();
        if let Some(value) = line.strip_prefix("Runlevel:") {
            runlevel = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("Dynamic Runlevel:") {
            runlevel = Some(value.trim().to_string());
            continue;
        }
        let Some(open) = line.rfind('[') else {
            continue;
        };
        let Some(close) = line.rfind(']') else {
            continue;
        };
        if close <= open {
            continue;
        }
        let name = line[..open].trim().trim_end_matches('|').trim();
        let state = line[open + 1..close].trim();
        if name.is_empty() || state.is_empty() {
            continue;
        }
        services.push(OpenRcService {
            name: name.to_string(),
            state: state.replace(' ', "_"),
            runlevel: runlevel.clone(),
        });
    }
    services
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct I3Workspace {
    pub num: i64,
    pub name: String,
    pub visible: bool,
    pub focused: bool,
    pub urgent: bool,
    pub output: String,
}

pub fn parse_i3_workspaces(input: &str) -> Option<Vec<I3Workspace>> {
    serde_json::from_str(input).ok()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct I3Display {
    pub name: String,
    pub active: bool,
    pub primary: bool,
    pub current_workspace: Option<String>,
}

pub fn parse_i3_outputs(input: &str) -> Option<Vec<I3Display>> {
    let values: Vec<Value> = serde_json::from_str(input).ok()?;
    let mut outputs = Vec::new();
    for value in values {
        let name = value.get("name")?.as_str()?.to_string();
        outputs.push(I3Display {
            name,
            active: value
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            primary: value
                .get("primary")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            current_workspace: value
                .get("current_workspace")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        });
    }
    Some(outputs)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct I3TreePlacement {
    pub xid: u64,
    pub workspace: String,
}

pub fn parse_i3_tree_placements(input: &str) -> Option<Vec<I3TreePlacement>> {
    let root: Value = serde_json::from_str(input).ok()?;
    let mut output = Vec::new();
    walk_i3_tree(&root, None, &mut output);
    Some(output)
}

fn walk_i3_tree(value: &Value, workspace: Option<&str>, output: &mut Vec<I3TreePlacement>) {
    let node_type = value.get("type").and_then(Value::as_str);
    let own_workspace = if node_type == Some("workspace") {
        value.get("name").and_then(Value::as_str).or(workspace)
    } else {
        workspace
    };
    if let (Some(xid), Some(workspace)) =
        (value.get("window").and_then(Value::as_u64), own_workspace)
    {
        output.push(I3TreePlacement {
            xid,
            workspace: workspace.to_string(),
        });
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = value.get(key).and_then(Value::as_array) {
            for child in children {
                walk_i3_tree(child, own_workspace, output);
            }
        }
    }
}

pub fn parse_xprop_client_list(input: &str) -> Vec<u64> {
    let Some((_, values)) = input.split_once('#') else {
        return Vec::new();
    };
    values
        .split(',')
        .filter_map(|raw| parse_xid(raw.trim()))
        .collect()
}

fn parse_xid(value: &str) -> Option<u64> {
    let value = value.trim();
    value
        .strip_prefix("0x")
        .and_then(|raw| u64::from_str_radix(raw, 16).ok())
        .or_else(|| value.parse::<u64>().ok())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WindowProperties {
    pub pid: Option<u32>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub title: Option<String>,
    pub hidden: bool,
}

pub fn parse_xprop_window(input: &str) -> WindowProperties {
    let mut properties = WindowProperties::default();
    for line in input.lines() {
        if line.starts_with("_NET_WM_PID") {
            properties.pid = line
                .split_once('=')
                .and_then(|(_, value)| value.trim().parse::<u32>().ok());
        } else if line.starts_with("WM_CLASS") {
            if let Some((_, value)) = line.split_once('=') {
                let values = quoted_values(value);
                properties.instance = values.first().cloned();
                properties.class = values.get(1).cloned();
            }
        } else if line.starts_with("_NET_WM_NAME") || line.starts_with("WM_NAME") {
            if properties.title.is_none() {
                properties.title = line
                    .split_once('=')
                    .and_then(|(_, value)| quoted_values(value).first().cloned());
            }
        } else if line.starts_with("_NET_WM_STATE") && line.contains("_NET_WM_STATE_HIDDEN") {
            properties.hidden = true;
        }
    }
    properties
}

fn quoted_values(input: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_quote = false;
    let mut escaped = false;
    let mut current = String::new();
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if in_quote && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            if in_quote {
                values.push(current.clone());
                current.clear();
            }
            in_quote = !in_quote;
        } else if in_quote {
            current.push(ch);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_stat_parser_handles_spaces_and_parentheses_in_comm() {
        let input = "42 (demo worker (x)) S 7 1 1 0 0 0 0 0 0 0 0 0 0 0 0 20 0 1 98765 1000";
        let parsed = parse_proc_stat(input).unwrap();
        assert_eq!(parsed.pid, 42);
        assert_eq!(parsed.comm, "demo worker (x)");
        assert_eq!(parsed.ppid, 7);
        assert_eq!(parsed.start_ticks, 98765);
    }

    #[test]
    fn status_parser_uses_real_uid_gid_fields_only() {
        let parsed = parse_status_identity(
            "Name:\tdemo\nUid:\t1000\t1000\t1000\t1000\nGid:\t100\t100\t100\t100\n",
        )
        .unwrap();
        assert_eq!(parsed, (1000, 100));
    }

    #[test]
    fn desktop_parser_keeps_only_application_name_and_executable_identity() {
        let parsed = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Demo\nExec=\"/usr/bin/demo-app\" --url %U\nTerminal=false\n",
        )
        .unwrap();
        assert_eq!(parsed.name, "Demo");
        assert_eq!(parsed.executable, "/usr/bin/demo-app");
        assert_eq!(parsed.executable_basename, "demo-app");
        assert!(!parsed.terminal);
        assert!(
            parse_desktop_entry("[Desktop Entry]\nType=Application\nName=X\nExec=x\nHidden=true\n")
                .is_none()
        );
    }

    #[test]
    fn openrc_parser_tracks_runlevels_and_states() {
        let parsed = parse_openrc_status(
            "Runlevel: default\n dbus                       [  started  ]\n sshd                       [  stopped  ]\nDynamic Runlevel: hotplugged\n net.lo                     [  started  ]\n",
        );
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].runlevel.as_deref(), Some("default"));
        assert_eq!(parsed[0].state, "started");
        assert_eq!(parsed[2].runlevel.as_deref(), Some("hotplugged"));
    }

    #[test]
    fn i3_parsers_extract_workspace_display_and_window_placement() {
        let workspaces = parse_i3_workspaces(
            r#"[{"num":1,"name":"DEV","visible":true,"focused":true,"urgent":false,"output":"HDMI-1"}]"#,
        )
        .unwrap();
        assert_eq!(workspaces[0].name, "DEV");
        let outputs = parse_i3_outputs(
            r#"[{"name":"HDMI-1","active":true,"primary":true,"current_workspace":"DEV"}]"#,
        )
        .unwrap();
        assert_eq!(outputs[0].current_workspace.as_deref(), Some("DEV"));
        let placements = parse_i3_tree_placements(
            r#"{"type":"root","nodes":[{"type":"workspace","name":"DEV","nodes":[{"type":"con","window":123,"nodes":[],"floating_nodes":[]}],"floating_nodes":[]}],"floating_nodes":[]}"#,
        )
        .unwrap();
        assert_eq!(
            placements,
            vec![I3TreePlacement {
                xid: 123,
                workspace: "DEV".into()
            }]
        );
    }

    #[test]
    fn xprop_parsers_extract_only_bounded_window_properties() {
        assert_eq!(
            parse_xprop_client_list("_NET_CLIENT_LIST(WINDOW): window id # 0x2a, 0x2b"),
            vec![42, 43]
        );
        let properties = parse_xprop_window(
            "_NET_WM_PID(CARDINAL) = 42\nWM_CLASS(STRING) = \"demo\", \"Demo\"\n_NET_WM_NAME(UTF8_STRING) = \"Private title\"\n_NET_WM_STATE(ATOM) = _NET_WM_STATE_HIDDEN\n",
        );
        assert_eq!(properties.pid, Some(42));
        assert_eq!(properties.instance.as_deref(), Some("demo"));
        assert_eq!(properties.class.as_deref(), Some("Demo"));
        assert_eq!(properties.title.as_deref(), Some("Private title"));
        assert!(properties.hidden);
    }
}

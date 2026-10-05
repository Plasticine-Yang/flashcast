//! GNOME 的 Alt+Space 窗口菜单冲突。只读检测与确认后的修改分离。
//! 接口依据 GNOME Control Center 的 cc-application-shortcut-dialog.c。
use std::collections::HashMap;
use std::path::Path;

use gio::glib::{variant::ToVariant, Variant};
use gio::prelude::*;

use crate::hotkey::HotkeySpec;
use crate::shortcut::HotkeyConflictReport;
use crate::SessionType;

const MENU_SCHEMA: &str = "org.gnome.desktop.wm.keybindings";
const APP_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts.application";
const MENU: &str = "activate-window-menu";
const APP_PATH: &str = "/org/gnome/settings-daemon/global-shortcuts/dev.flashcast.launcher/";
const SERVICE: &str = "org.freedesktop.impl.portal.desktop.gnome";
const OBJECT: &str = "/org/gnome/globalshortcuts";
const INTERFACE: &str = "org.gnome.GlobalShortcutsRebind";
type Bindings = Vec<(String, HashMap<String, Variant>)>;

#[derive(serde::Serialize, serde::Deserialize)]
struct Backup {
    before_menu: Vec<String>,
    after_menu: Vec<String>,
    before_bindings: String,
    after_bindings: String,
}

fn applicable(session: SessionType, spec: &HotkeySpec) -> bool {
    session == SessionType::Wayland
        && spec.canonical() == "Alt+Space"
        && std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .any(|s| s.eq_ignore_ascii_case("gnome"))
}

fn settings(schema: &str, path: Option<&str>) -> Result<gio::Settings, String> {
    let schema = gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup(schema, true))
        .ok_or_else(|| "当前 GNOME 缺少快捷键设置，需在系统设置中手动处理。".to_string())?;
    Ok(gio::Settings::new_full(
        &schema,
        None::<&gio::SettingsBackend>,
        path,
    ))
}

fn alt_space(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "<alt>space" | "<mod1>space"
    )
}

fn read_bindings(app: &gio::Settings) -> Result<Bindings, String> {
    app.value("shortcuts")
        .get()
        .ok_or_else(|| "无法读取 Flashcast 的系统绑定，请手动检查。".into())
}

fn accelerator_label(raw: &str) -> String {
    let mut rest = raw;
    let mut parts = Vec::new();
    while let Some(value) = rest.strip_prefix('<') {
        let Some((modifier, remaining)) = value.split_once('>') else {
            return raw.into();
        };
        parts.push(if modifier.eq_ignore_ascii_case("mod1") {
            "Alt"
        } else {
            modifier
        });
        rest = remaining;
    }
    parts.push(rest);
    HotkeySpec::parse(&parts.join("+"))
        .map(|spec| spec.canonical())
        .unwrap_or_else(|_| raw.into())
}

fn effective(bindings: &Bindings) -> Option<String> {
    bindings
        .iter()
        .find(|(id, _)| id == "summon")
        .and_then(|(_, props)| props.get("shortcuts"))
        .and_then(|value| value.get::<Vec<String>>())
        .filter(|keys| !keys.is_empty())
        .map(|keys| {
            keys.iter()
                .map(|key| accelerator_label(key))
                .collect::<Vec<_>>()
                .join(" / ")
        })
}

fn load_backup(path: &Path) -> Option<Backup> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn matches_after(backup: &Backup, menu: &gio::Settings, app: &gio::Settings) -> bool {
    menu.strv(MENU)
        .iter()
        .map(|s| s.as_str())
        .eq(backup.after_menu.iter().map(String::as_str))
        && app.value("shortcuts").print(true).as_str() == backup.after_bindings
}

fn bus() -> Result<gio::DBusConnection, String> {
    gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).map_err(|e| e.to_string())
}

fn can_rebind() -> bool {
    bus()
        .and_then(|bus| {
            bus.call_sync(
                Some(SERVICE),
                OBJECT,
                "org.freedesktop.DBus.Introspectable",
                "Introspect",
                None,
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1500,
                None::<&gio::Cancellable>,
            )
            .map_err(|e| e.to_string())
        })
        .ok()
        .and_then(|v| v.get::<(String,)>())
        .is_some_and(|(xml,)| xml.contains(INTERFACE))
}

fn rebind(bindings: &Variant) -> Result<(), String> {
    let parameters =
        Variant::tuple_from_iter(["dev.flashcast.launcher".to_variant(), bindings.clone()]);
    bus()?
        .call_sync(
            Some(SERVICE),
            OBJECT,
            INTERFACE,
            "RebindShortcuts",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
        )
        .map_err(|e| format!("系统未接受新的绑定：{e}"))?;
    Ok(())
}

pub fn inspect(session: SessionType, spec: &HotkeySpec, backup: &Path) -> HotkeyConflictReport {
    if !applicable(session, spec) {
        return HotkeyConflictReport::default();
    }
    let read = || -> Result<HotkeyConflictReport, String> {
        let menu = settings(MENU_SCHEMA, None)?;
        let app = settings(APP_SCHEMA, Some(APP_PATH))?;
        let bindings = read_bindings(&app)?;
        let effective = effective(&bindings);
        let occupied = menu.strv(MENU).iter().any(|s| alt_space(s));
        let matches = bindings
            .iter()
            .find(|(id, _)| id == "summon")
            .and_then(|(_, props)| props.get("shortcuts"))
            .and_then(|v| v.get::<Vec<String>>())
            .is_some_and(|keys| keys.iter().any(|s| alt_space(s)));
        let status = if occupied {
            "conflict"
        } else if !matches {
            "mismatch"
        } else {
            "clear"
        };
        let can_resolve = effective.is_some()
            && menu.is_writable(MENU)
            && app.is_writable("shortcuts")
            && can_rebind();
        Ok(HotkeyConflictReport {
            status: status.into(),
            can_resolve,
            can_undo: load_backup(backup).is_some_and(|b| matches_after(&b, &menu, &app)),
            effective,
            message: (!can_resolve && status != "clear")
                .then(|| "当前系统不支持自动修改，或尚未授权 Flashcast。请按下方步骤处理。".into()),
        })
    };
    read().unwrap_or_else(|message| HotkeyConflictReport {
        status: "unknown".into(),
        message: Some(message),
        ..Default::default()
    })
}

fn write(
    menu: &gio::Settings,
    app: &gio::Settings,
    keys: &[String],
    bindings: &Variant,
) -> Result<(), String> {
    menu.set_strv(MENU, keys).map_err(|e| e.to_string())?;
    app.set_value("shortcuts", bindings)
        .map_err(|e| e.to_string())?;
    gio::Settings::sync();
    rebind(bindings)?;
    if !menu
        .strv(MENU)
        .iter()
        .map(|s| s.as_str())
        .eq(keys.iter().map(String::as_str))
        || app.value("shortcuts") != *bindings
    {
        return Err("系统设置未能保存，请手动处理。".into());
    }
    Ok(())
}

pub fn resolve(
    session: SessionType,
    spec: &HotkeySpec,
    path: &Path,
    undo: bool,
) -> Result<HotkeyConflictReport, String> {
    if !applicable(session, spec) {
        return Err("仅 GNOME Wayland 的 Alt+Space 支持自动处理。".into());
    }
    let report = inspect(session, spec, path);
    if !report.can_resolve {
        return Err(report
            .message
            .unwrap_or_else(|| "系统设置不允许修改，请手动处理。".into()));
    }
    let menu = settings(MENU_SCHEMA, None)?;
    let app = settings(APP_SCHEMA, Some(APP_PATH))?;
    let current_menu: Vec<String> = menu.strv(MENU).iter().map(ToString::to_string).collect();
    let current_bindings = app.value("shortcuts");
    if undo {
        let backup = load_backup(path).ok_or("没有可撤销的系统修改。")?;
        if !matches_after(&backup, &menu, &app) {
            return Err("系统快捷键已被再次修改，无法安全撤销。请在系统设置中手动处理。".into());
        }
        let before = Variant::parse(None, &backup.before_bindings).map_err(|e| e.to_string())?;
        if let Err(error) = write(&menu, &app, &backup.before_menu, &before) {
            let restored = write(&menu, &app, &current_menu, &current_bindings);
            return Err(format!(
                "撤销失败：{error}。{}",
                if restored.is_ok() {
                    "已保留修改后的绑定"
                } else {
                    "请检查系统快捷键设置"
                }
            ));
        }
        std::fs::remove_file(path).map_err(|e| format!("系统已恢复，但撤销记录无法清理：{e}"))?;
    } else {
        if !matches!(report.status.as_str(), "conflict" | "mismatch") {
            return Err("当前没有需要处理的冲突，请重新检测。".into());
        }
        let after_menu: Vec<_> = current_menu
            .iter()
            .filter(|s| !alt_space(s))
            .cloned()
            .collect();
        let mut bindings = read_bindings(&app)?;
        let (_, props) = bindings
            .iter_mut()
            .find(|(id, _)| id == "summon")
            .ok_or("请先授权 Flashcast 的全局快捷键。")?;
        props.insert("shortcuts".into(), vec!["<Alt>space"].to_variant());
        let after = bindings.to_variant();
        let backup = Backup {
            before_menu: current_menu.clone(),
            after_menu: after_menu.clone(),
            before_bindings: current_bindings.print(true).to_string(),
            after_bindings: after.print(true).to_string(),
        };
        // 在首次修改前持久化旧值；备份失败不改变系统。禁止把它写进同步工作区。
        std::fs::create_dir_all(path.parent().ok_or("无法找到本机备份目录。")?)
            .map_err(|e| e.to_string())?;
        let temporary = path.with_extension("tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec(&backup).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        if menu.value(MENU) != current_menu.to_variant()
            || app.value("shortcuts") != current_bindings
        {
            let _ = std::fs::remove_file(path);
            return Err("系统快捷键已被再次修改，请重新检测后再处理。".into());
        }
        if let Err(error) = write(&menu, &app, &after_menu, &after) {
            let restored = write(&menu, &app, &current_menu, &current_bindings);
            if restored.is_ok() {
                let _ = std::fs::remove_file(path);
            }
            return Err(format!(
                "无法自动解除冲突：{error}。{}",
                if restored.is_ok() {
                    "已恢复原来的系统设置"
                } else {
                    "恢复未完成，请检查系统设置"
                }
            ));
        }
    }
    Ok(inspect(session, spec, path))
}

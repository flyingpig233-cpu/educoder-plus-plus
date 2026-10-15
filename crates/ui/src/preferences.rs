use crate::{AppWindow, Controller, EnvironmentRow};
use educoder_core::{
    cache::CacheStore,
    runner::{Toolchains, inspect_toolchains},
};
use serde::{Deserialize, Serialize};
use slint::{ModelRc, VecModel};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub theme_mode: i32,
    pub accent: i32,
    pub toolchains: Toolchains,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme_mode: 0,
            accent: 0,
            toolchains: Toolchains::default(),
        }
    }
}

fn read_ui(app: &AppWindow) -> Preferences {
    Preferences {
        theme_mode: app.get_theme_mode(),
        accent: app.get_accent_index(),
        toolchains: Toolchains {
            c: app.get_c_compiler().to_string(),
            cpp: app.get_cpp_compiler().to_string(),
            rust: app.get_rust_compiler().to_string(),
            python: app.get_python_interpreter().to_string(),
            javascript: app.get_js_interpreter().to_string(),
            java: app.get_java_runtime().to_string(),
            javac: app.get_java_compiler().to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_defaults_to_system_and_old_preferences_migrate_without_losing_tools() {
        assert_eq!(Preferences::default().theme_mode, 0);
        let tools = Toolchains {
            c: "custom-cc".into(),
            ..Toolchains::default()
        };
        let legacy: Preferences =
            serde_json::from_value(serde_json::json!({"dark":false,"accent":2,"toolchains":tools}))
                .unwrap();
        assert_eq!(legacy.theme_mode, 0);
        assert_eq!(legacy.accent, 2);
        assert_eq!(legacy.toolchains.c, "custom-cc");
        let mut selected = legacy;
        selected.theme_mode = 2;
        assert_eq!(
            serde_json::from_value::<Preferences>(serde_json::to_value(selected).unwrap())
                .unwrap()
                .theme_mode,
            2
        );
    }
}

impl Controller {
    pub fn initialize(self: &Arc<Self>) {
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            if let Ok(store) = CacheStore::new("preferences")
                && let Some(saved) = store
                    .get::<Preferences>("settings", std::time::Duration::MAX)
                    .await
            {
                let _ = ui.upgrade_in_event_loop(move |app| {
                    let prefs = saved.value;
                    app.set_theme_mode(prefs.theme_mode.clamp(0, 2));
                    if prefs.theme_mode != 0 {
                        app.set_dark_mode(prefs.theme_mode == 2);
                    }
                    app.set_accent_index(prefs.accent.clamp(0, 2));
                    app.set_c_compiler(prefs.toolchains.c.clone().into());
                    app.set_cpp_compiler(prefs.toolchains.cpp.clone().into());
                    app.set_rust_compiler(prefs.toolchains.rust.clone().into());
                    app.set_python_interpreter(prefs.toolchains.python.clone().into());
                    app.set_js_interpreter(prefs.toolchains.javascript.clone().into());
                    app.set_java_runtime(prefs.toolchains.java.clone().into());
                    app.set_java_compiler(prefs.toolchains.javac.clone().into());
                    *c.preferences.lock().expect("preferences") = prefs;
                });
            }
        });
    }

    pub(super) fn save_preferences(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else { return };
        let prefs = read_ui(&app);
        *self.preferences.lock().expect("preferences") = prefs.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let result = match CacheStore::new("preferences") {
                Ok(store) => store.put("settings", &prefs).await,
                Err(error) => Err(error),
            };
            let _ = ui.upgrade_in_event_loop(move |app| {
                if result.is_ok() {
                    app.set_notice("设置已保存。".into());
                } else {
                    app.set_error_message("设置保存失败，本次修改仍然有效。".into());
                }
            });
        });
    }

    pub(super) fn inspect_environment(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else { return };
        if app.get_checking_environment() {
            return;
        }
        app.set_checking_environment(true);
        let tools = read_ui(&app).toolchains;
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let statuses = inspect_toolchains(&tools).await;
            let _ = ui.upgrade_in_event_loop(move |app| {
                app.set_environments(ModelRc::new(VecModel::from(
                    statuses
                        .into_iter()
                        .map(|s| EnvironmentRow {
                            language: s.language.into(),
                            available: s.available,
                            detail: s.version.into(),
                        })
                        .collect::<Vec<_>>(),
                )));
                app.set_checking_environment(false);
            });
        });
    }
}

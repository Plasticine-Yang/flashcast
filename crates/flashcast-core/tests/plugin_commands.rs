mod support;
use flashcast_core::{ActionStatus, QueryScope};
use support::{cleanup, fast_settings, official_host_restarted};

#[test]
fn keyword_is_a_result_and_commands_keep_an_empty_query_inside_the_page() {
    let device = support::unique_dir("commands");
    let host = official_host_restarted(&device, fast_settings());
    let home = host.query("bookmark");
    assert!(home.scope.is_home());
    let entry = home
        .items
        .iter()
        .find(|i| i.id == "flashcast.plugin.chrome-bookmarks")
        .unwrap();
    assert_eq!(host.execute(entry).status, ActionStatus::Done);
    assert_eq!(host.snapshot().input, "");
    assert!(!host.query("").scope.is_home());
    assert!(host.back().restored);
    assert_eq!(host.snapshot().input, "bookmark");
    assert_eq!(host.snapshot().scope, QueryScope::Home);
    cleanup(&device);
}

#[test]
fn command_defaults_overrides_disable_restore_and_conflicts_are_persisted_safely() {
    let device = support::unique_dir("command-settings");
    let workspace = support::unique_dir("command-workspace");
    let host = official_host_restarted(&device, fast_settings());
    host.init_workspace(&workspace).unwrap();
    let commands = host.plugin_commands();
    let c = commands
        .iter()
        .find(|c| c.plugin_id == "clipboard")
        .unwrap();
    assert_eq!(
        c.default_shortcut,
        if c.platform == "macos" {
            "Control+Command+C"
        } else {
            "Ctrl+Alt+C"
        }
    );
    assert!(!c.enabled);
    host.set_command_shortcut(&c.id, Some("Ctrl+Alt+V".into()))
        .unwrap();
    host.set_plugin_enabled("clipboard", true).unwrap();
    let before = host.settings();
    assert!(host
        .set_command_shortcut(&c.id, Some(before.hotkey.clone()))
        .is_err());
    assert_eq!(host.settings(), before);
    assert!(host
        .set_command_shortcut("unknown", Some("Ctrl+V".into()))
        .is_err());
    assert!(host
        .set_command_shortcut(&c.id, Some("bad-key".into()))
        .is_err());
    assert_eq!(host.settings(), before);
    let restarted = official_host_restarted(&device, fast_settings());
    assert_eq!(
        restarted
            .plugin_commands()
            .into_iter()
            .find(|v| v.id == c.id)
            .unwrap()
            .shortcut,
        "Ctrl+Alt+V"
    );
    host.set_command_shortcut(&c.id, Some(String::new()))
        .unwrap();
    assert_eq!(
        host.plugin_commands()
            .into_iter()
            .find(|v| v.id == c.id)
            .unwrap()
            .shortcut,
        ""
    );
    host.set_command_shortcut(&c.id, None).unwrap();
    assert_eq!(
        host.plugin_commands()
            .into_iter()
            .find(|v| v.id == c.id)
            .unwrap()
            .shortcut,
        c.default_shortcut
    );
    host.set_plugin_enabled("clipboard", false).unwrap();
    assert_eq!(
        host.execute_plugin_command(&c.id).status,
        ActionStatus::Failed
    );
    cleanup(&workspace);
    cleanup(&device);
}

#[test]
fn command_namespaces_and_global_identity_are_checked_at_registration() {
    use flashcast_core::{
        FeaturePlugin, Keyword, PluginCommand, PluginError, PluginManifest, PluginRegistry,
        PluginScope, SearchContext, SearchItem,
    };
    use std::sync::Arc;
    struct Commands {
        manifest: PluginManifest,
        id: String,
    }
    impl FeaturePlugin for Commands {
        fn manifest(&self) -> PluginManifest {
            self.manifest.clone()
        }
        fn commands(&self) -> Vec<PluginCommand> {
            let mut c = PluginCommand::open_page(&self.manifest);
            c.id = self.id.clone();
            vec![c]
        }
        fn contributes_to_home(&self) -> bool {
            false
        }
        fn search(&self, _: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
            Ok(vec![])
        }
        fn take_scope(&self, _: &Keyword) -> Option<Box<dyn PluginScope>> {
            None
        }
    }
    let registry = PluginRegistry::new();
    let custom = |plugin: &str, id: &str| {
        Arc::new(Commands {
            manifest: PluginManifest::feature(plugin, plugin, env!("CARGO_PKG_VERSION"))
                .with_keywords([plugin]),
            id: id.into(),
        })
    };
    assert!(registry
        .try_register(custom("example", "flashcast.plugin.other"))
        .is_err());
    assert!(registry
        .try_register(custom("example", "flashcast.plugin.example.notes"))
        .is_ok());
    assert!(registry
        .try_register(custom("example.notes", "flashcast.plugin.example.notes"))
        .is_err());
    assert_eq!(registry.commands().len(), 1);
}

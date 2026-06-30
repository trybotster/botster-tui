use std::fs;
use std::path::Path;

use botster_core::{
    ExtensionRuntime, PackageManifest, RunnableEntrypointInjectionKind,
    RunnableEntrypointInjectionTarget, RunnableEntrypointKind, RunnableEntrypointLaunchMode,
    RunnableEntrypointWorkingDirectory, validate_package_runnable_entrypoints,
};

#[test]
fn package_manifest_declares_terminal_app_foreground_stdio() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../botster-package.json");
    let manifest_json =
        fs::read_to_string(&manifest_path).expect("read root botster-package.json manifest");
    let manifest: PackageManifest =
        serde_json::from_str(&manifest_json).expect("deserialize botster package manifest");

    validate_package_runnable_entrypoints(&manifest).expect("validate runnable entrypoints");

    assert_eq!(manifest.name, "botster-tui");
    assert_eq!(manifest.entrypoints.len(), 1);
    assert_eq!(manifest.entrypoints[0].runtime, ExtensionRuntime::Lua);
    assert_eq!(manifest.entrypoints[0].path, "plugin.lua");
    assert_eq!(manifest.runnable_entrypoints.len(), 1);

    let entrypoint = &manifest.runnable_entrypoints[0];
    assert_eq!(entrypoint.id, "tui");
    assert_eq!(entrypoint.kind, RunnableEntrypointKind::TerminalApp);
    assert_eq!(
        entrypoint.launch_mode,
        RunnableEntrypointLaunchMode::ForegroundStdio
    );
    assert_eq!(entrypoint.command, "target/debug/botster-tui");
    assert!(entrypoint.args.is_empty());
    assert_eq!(
        entrypoint.working_directory,
        Some(RunnableEntrypointWorkingDirectory::PackageRoot)
    );

    for required_kind in [
        RunnableEntrypointInjectionKind::HubConnection,
        RunnableEntrypointInjectionKind::DataDir,
        RunnableEntrypointInjectionKind::HubSocket,
    ] {
        assert!(
            entrypoint
                .injections
                .iter()
                .any(|injection| injection.kind == required_kind && injection.required),
            "missing required injection {required_kind:?}"
        );
    }

    assert!(entrypoint.injections.iter().any(|injection| {
        injection.kind == RunnableEntrypointInjectionKind::HubSocket
            && injection.required
            && injection.target
                == RunnableEntrypointInjectionTarget::Environment {
                    name: "BOTSTER_HUB_SOCKET".to_string(),
                }
    }));
    assert!(entrypoint.injections.iter().any(|injection| {
        injection.kind == RunnableEntrypointInjectionKind::DataDir
            && injection.required
            && injection.target
                == RunnableEntrypointInjectionTarget::Environment {
                    name: "BOTSTER_HUB_DATA_DIR".to_string(),
                }
    }));
    assert!(
        entrypoint
            .environment
            .iter()
            .any(|requirement| requirement.name == "BOTSTER_HUB_SOCKET" && !requirement.required)
    );
    assert!(
        entrypoint
            .environment
            .iter()
            .any(|requirement| requirement.name == "BOTSTER_HUB_DATA_DIR" && requirement.required)
    );
}

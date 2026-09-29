use super::*;

pub(super) fn package_diagnostic_text(
    diagnostic: &botster_hub_client::DaemonPackageDiagnostic,
) -> String {
    format!("{}:{}", diagnostic.kind, diagnostic.message)
}

pub(super) fn package_name_from_payload(payload: &Option<Value>) -> Option<String> {
    payload
        .as_ref()
        .and_then(|value| value.get("package_name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn session_id_from_payload(payload: &Option<Value>) -> Option<String> {
    payload
        .as_ref()
        .and_then(|value| value.get("session_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(super) fn package_entrypoint_from_payload(payload: &Option<Value>) -> Option<(String, String)> {
    let value = payload.as_ref()?;
    let package_name = value.get("package_name")?.as_str()?.to_string();
    let entrypoint_id = value.get("entrypoint_id")?.as_str()?.to_string();
    Some((package_name, entrypoint_id))
}

pub(super) fn navigation_open_payload(payload: &Option<Value>) -> Option<(String, String, String)> {
    let value = payload.as_ref()?;
    let package_name = value.get("package_name")?.as_str()?.to_string();
    let surface_id = value.get("surface_id")?.as_str()?.to_string();
    let route_id = value.get("route_id")?.as_str()?.to_string();
    Some((package_name, surface_id, route_id))
}

pub(super) fn package_name_and_pin_from_payload(
    payload: &Option<Value>,
) -> Option<(String, DaemonPackagePin)> {
    let value = payload.as_ref()?;
    let package_name = value.get("package_name")?.as_str()?.to_string();
    let pin = serde_json::from_value(value.get("pin")?.clone()).ok()?;
    Some((package_name, pin))
}

pub(super) fn package_text(package: &DaemonPackage) -> String {
    format!(
        "{} {} classification={} state={} capabilities={} provider_profile_admitted={} availability={} surfaces={}",
        package.package_name,
        package.version,
        package.classification,
        package.state,
        capability_text(&package.requested_capabilities),
        package.provider_profile_admitted,
        availability_state_text(package.availability.state),
        package.surfaces.len()
    )
}

pub(super) fn package_surface_nodes(package: &DaemonPackage, package_index: usize) -> Vec<UiNode> {
    package
        .surfaces
        .iter()
        .enumerate()
        .map(|(surface_index, surface)| {
            node(
                UiNodeKind::Text,
                &format!("tui-package-{package_index}-surface-{surface_index}"),
                json!({
                    "text": format!(
                        "surface: package={} {}",
                        package.package_name,
                        package_surface_text(surface)
                    )
                }),
            )
        })
        .collect()
}

pub(super) fn package_surface_text(surface: &PackageSurfaceDescriptor) -> String {
    let supports = if surface.supports.is_empty() {
        "none".to_string()
    } else {
        surface
            .supports
            .iter()
            .map(|operation| match operation {
                PackageSurfaceOperation::Render => "render",
                PackageSurfaceOperation::Action => "action",
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "id={} kind={} title={} supports={supports}",
        surface.id,
        match surface.kind {
            PackageSurfaceKind::App => "app",
            PackageSurfaceKind::Settings => "settings",
            PackageSurfaceKind::DashboardWidget => "dashboard_widget",
            PackageSurfaceKind::Diagnostics => "diagnostics",
        },
        surface.title
    )
}

pub(super) fn package_availability_nodes(package: &DaemonPackage, index: usize) -> Vec<UiNode> {
    let mut nodes = Vec::new();
    for (reason_index, reason) in package.availability.reasons.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-availability-reason-{reason_index}"),
            json!({ "text": format!("package blocked: {}", availability_reason_text(reason)) }),
        ));
    }
    for (dependency_index, dependency) in package.dependency_availability.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-dependency-{dependency_index}"),
            json!({
                "text": format!(
                    "dependency: id={} package={} state={}",
                    dependency.id,
                    dependency.package_name,
                    availability_state_text(dependency.state)
                )
            }),
        ));
        for (reason_index, reason) in dependency.reasons.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!(
                    "tui-package-{index}-dependency-{dependency_index}-reason-{reason_index}"
                ),
                json!({ "text": format!("dependency blocked: {}", availability_reason_text(reason)) }),
            ));
        }
    }
    for (feature_index, feature) in package.feature_availability.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-feature-{feature_index}"),
            json!({
                "text": format!(
                    "feature: id={} state={}",
                    feature.id,
                    availability_state_text(feature.state)
                )
            }),
        ));
        for (reason_index, reason) in feature.reasons.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-package-{index}-feature-{feature_index}-reason-{reason_index}"),
                json!({ "text": format!("feature blocked: {}", availability_reason_text(reason)) }),
            ));
        }
    }
    nodes
}

pub(super) fn route_text(route: &DaemonPackageRouteDescriptor) -> String {
    let mut parts = vec![
        format!("package={}", route.package_name),
        format!("route_id={}", route.route_id),
        format!("path={}", route.route_path),
        format!("target={}", route.target.kind),
        format!("enabled={}", route.enabled),
        format!("blocked={}", route.blocked),
        format!("supports_settings={}", route.supports_settings),
    ];
    if let Some(surface_id) = &route.surface_id {
        parts.push(format!("surface_id={surface_id}"));
    }
    if let Some(target_surface_id) = &route.target.surface_id {
        parts.push(format!("target_surface_id={target_surface_id}"));
    }
    if let Some(app_id) = &route.app_id {
        parts.push(format!("app_id={app_id}"));
    }
    parts.join(" ")
}

pub(super) fn package_action_nodes(package: &DaemonPackage, index: usize) -> Vec<UiNode> {
    vec![
        button(
            &format!("tui-package-{index}-show"),
            "Show",
            "botster.tui.package.show",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-enable"),
            "Enable",
            "botster.tui.package.enable",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-disable"),
            "Disable",
            "botster.tui.package.disable",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-remove"),
            "Remove",
            "botster.tui.package.remove",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-update-status"),
            "Update status",
            "botster.tui.package.update_status",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-logs"),
            "Logs",
            "botster.tui.package.logs",
            json!({ "package_name": package.package_name }),
        ),
    ]
}

/// Records of a plugin log page shown under its package, newest last.
pub(super) const SHOWN_PLUGIN_LOG_RECORDS: usize = 10;

pub(super) fn plugin_log_nodes(logs: &DaemonPluginLogs, index: usize) -> Vec<UiNode> {
    let shown = logs.records.len().min(SHOWN_PLUGIN_LOG_RECORDS);
    let mut nodes = vec![node(
        UiNodeKind::Text,
        &format!("tui-package-{index}-logs-summary"),
        json!({
            "text": format!(
                "logs: {} · {} records read · showing the last {shown} · next_seq={} first_available_seq={}",
                logs.package_name,
                logs.records.len(),
                logs.next_seq,
                logs.first_available_seq
            )
        }),
    )];
    for record in &logs.records[logs.records.len() - shown..] {
        let dropped = if record.dropped_before > 0 {
            format!(" ({} dropped before)", record.dropped_before)
        } else {
            String::new()
        };
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-log-{}", record.seq),
            json!({
                "text": format!("log #{} {}: {}{dropped}", record.seq, record.level, record.message)
            }),
        ));
    }
    nodes
}

pub(super) fn entrypoint_action_nodes(
    package: &DaemonPackage,
    package_index: usize,
    entrypoint: &botster_hub_client::DaemonPackageRunnableEntrypoint,
    entrypoint_index: usize,
) -> Vec<UiNode> {
    let payload = json!({
        "package_name": package.package_name,
        "entrypoint_id": entrypoint.id,
    });
    vec![
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-start"),
            "Start",
            "botster.tui.entrypoint.start",
            payload.clone(),
        ),
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-stop"),
            "Stop",
            "botster.tui.entrypoint.stop",
            payload.clone(),
        ),
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-restart"),
            "Restart",
            "botster.tui.entrypoint.restart",
            payload.clone(),
        ),
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-status"),
            "Status",
            "botster.tui.entrypoint.status",
            payload,
        ),
    ]
}

pub(super) fn available_package_text(package: &DaemonAvailablePackage) -> String {
    let mut parts = vec![
        format!("entry_id={}", package.entry_id),
        format!("package={}", package.package_name),
        format!("version={}", package.version),
        format!("classification={}", package.classification),
        format!("source_kind={}", package.source_kind),
        format!("source_label={}", package.source_label),
        format!("first_party={}", package.first_party),
        format!("state={}", package.state),
        format!(
            "capabilities={}",
            capability_text(&package.requested_capabilities)
        ),
        format!(
            "compatibility={}:{}",
            package.compatibility.result, package.compatibility.botster_requirement
        ),
    ];
    if !package.compatibility.diagnostics.is_empty() {
        parts.push(format!(
            "compatibility_diagnostics={}",
            package.compatibility.diagnostics.join(",")
        ));
    }
    if let Some(pin) = &package.pin {
        parts.push(format!("pin={}", pin_text(pin)));
    }
    parts.join(" ")
}

pub(super) fn app_text(app: &DaemonApp) -> String {
    format!(
        "package={} app={} entrypoint={} kind={} launch_mode={} lifecycle={}",
        app.package_name,
        app.app_id,
        app.entrypoint_id,
        app.kind,
        app.launch_mode,
        app.lifecycle_state
    )
}

pub(super) fn app_launch_target_text(app: &DaemonApp) -> String {
    let mut parts = vec![format!("kind={}", app.launch_target.kind)];
    match app.launch_target.local_url.as_deref() {
        Some(local_url) => {
            parts.push(format!("local_url={local_url}"));
            parts.push("open=copy URL or open it in a browser".to_string());
        }
        None if app.kind == "web_app" || app.launch_target.kind == "web_app" => {
            parts.push("local_url=unavailable".to_string());
            parts.push("open=blocked or not launched by hub".to_string());
        }
        None => {
            parts.push("local_url=not_applicable".to_string());
            parts.push("open=use hub-provided terminal app action when available".to_string());
        }
    }
    parts.join(" ")
}

pub(super) fn navigation_entry_text(entry: &DaemonPackageNavigationEntry) -> String {
    let mut parts = vec![
        format!("package={}", entry.package_name),
        format!("item_id={}", entry.item_id),
        format!("label={}", entry.label),
        format!("route_id={}", entry.route_id),
        format!("path={}", entry.route_path),
        format!("target={}", entry.target.kind),
        format!("source={}", entry.source.kind),
        format!("enabled={}", entry.enabled),
        format!("blocked={}", entry.blocked),
    ];
    if let Some(description) = &entry.description {
        parts.push(format!("description={description}"));
    }
    if let Some(icon) = &entry.icon {
        parts.push(format!("icon={icon}"));
    }
    if let Some(surface_id) = &entry.target.surface_id {
        parts.push(format!("target_surface_id={surface_id}"));
    }
    if let Some(surface_id) = &entry.source.surface_id {
        parts.push(format!("source_surface_id={surface_id}"));
    }
    if let Some(entrypoint_id) = &entry.target.entrypoint_id {
        parts.push(format!("target_entrypoint_id={entrypoint_id}"));
    }
    if let Some(entrypoint_id) = &entry.source.entrypoint_id {
        parts.push(format!("source_entrypoint_id={entrypoint_id}"));
    }
    parts.join(" ")
}

pub(super) fn navigation_open_payload_for_entry(
    entry: &DaemonPackageNavigationEntry,
) -> Option<Value> {
    if entry.target.kind != "plugin_surface" && entry.target.kind != "settings" {
        return None;
    }
    let surface_id = entry
        .target
        .surface_id
        .as_ref()
        .or(entry.source.surface_id.as_ref())?;
    Some(json!({
        "package_name": entry.package_name,
        "surface_id": surface_id,
        "route_id": entry.route_id,
    }))
}

pub(super) fn navigation_blocked_text(entry: &DaemonPackageNavigationEntry) -> String {
    let mut parts = vec![
        format!("label={}", entry.label),
        format!("route_id={}", entry.route_id),
        format!("enabled={}", entry.enabled),
        format!("blocked={}", entry.blocked),
    ];
    if entry.diagnostics.is_empty() {
        parts.push("diagnostics=none".to_string());
    } else {
        parts.push(format!(
            "diagnostics={}",
            entry
                .diagnostics
                .iter()
                .map(package_diagnostic_text)
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    parts.join(" ")
}

pub(super) fn navigation_unsupported_text(entry: &DaemonPackageNavigationEntry) -> String {
    let mut parts = vec![
        format!("label={}", entry.label),
        format!("route_id={}", entry.route_id),
        format!("target={}", entry.target.kind),
    ];
    if let Some(surface_id) = &entry.target.surface_id {
        parts.push(format!("target_surface_id={surface_id}"));
    }
    if let Some(entrypoint_id) = &entry.target.entrypoint_id {
        parts.push(format!("target_entrypoint_id={entrypoint_id}"));
    }
    parts.push("open=unsupported in botster-tui".to_string());
    parts.join(" ")
}

pub(super) fn action_state_nodes(
    actions: &[botster_hub_client::DaemonPackageActionState],
    label: &str,
    id_prefix: &str,
) -> Vec<UiNode> {
    actions
        .iter()
        .enumerate()
        .map(|(action_index, action)| {
            node(
                UiNodeKind::Text,
                &format!("{id_prefix}-action-{action_index}"),
                json!({ "text": format!("{label}: {}", action_state_text(action)) }),
            )
        })
        .collect()
}

pub(super) fn action_state_text(action: &botster_hub_client::DaemonPackageActionState) -> String {
    let mut parts = vec![
        format!("action_id={}", action.action_id),
        format!("status={}", action_status_text(action.status)),
    ];
    if let Some(reason) = &action.reason {
        parts.push(format!("reason={reason}"));
    }
    if !action.diagnostics.is_empty() {
        parts.push(format!(
            "diagnostics={}",
            action
                .diagnostics
                .iter()
                .map(package_diagnostic_text)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if !action.required_references.is_empty() {
        parts.push(format!(
            "required_references={}",
            action
                .required_references
                .iter()
                .map(|reference| format!("{}:{}", reference.kind, reference.key))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(request) = &action.request {
        parts.push(format!("request={}", action_request_text(request)));
    }
    parts.join(" ")
}

pub(super) fn action_status_text(
    status: botster_hub_client::DaemonPackageActionStatus,
) -> &'static str {
    match status {
        botster_hub_client::DaemonPackageActionStatus::Available => "available",
        botster_hub_client::DaemonPackageActionStatus::Blocked => "blocked",
        botster_hub_client::DaemonPackageActionStatus::Unavailable => "unavailable",
    }
}

pub(super) fn action_request_text(
    request: &botster_hub_client::DaemonPackageActionRequest,
) -> String {
    let mut parts = vec![format!("type={}", request.request_type)];
    if let Some(package_name) = &request.package_name {
        parts.push(format!("package={package_name}"));
    }
    if let Some(entry_id) = &request.entry_id {
        parts.push(format!("entry_id={entry_id}"));
    }
    if let Some(entrypoint_id) = &request.entrypoint_id {
        parts.push(format!("entrypoint_id={entrypoint_id}"));
    }
    if let Some(pin) = &request.pin {
        parts.push(format!("pin={}", pin_text(pin)));
    }
    if request.registry_path.is_some() {
        parts.push("registry_path=provided".to_string());
    }
    parts.join(",")
}

pub(super) fn install_plan_nodes(plan: &DaemonPackageInstallPlan) -> Vec<UiNode> {
    let mut nodes = vec![node(
        UiNodeKind::Text,
        "tui-install-plan-summary",
        json!({
            "text": format!(
                "install plan: package={} mutates_registry={} starts_entrypoints={} {}",
                plan.entry.package_name,
                plan.mutates_registry,
                plan.starts_entrypoints,
                available_package_text(&plan.entry)
            )
        }),
    )];
    for (index, effect) in plan.effects.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-install-plan-effect-{index}"),
            json!({ "text": format!("install effect: {}:{}", effect.kind, effect.message) }),
        ));
    }
    for (index, diagnostic) in plan.diagnostics.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-install-plan-diagnostic-{index}"),
            json!({ "text": format!("install diagnostic: {}", package_diagnostic_text(diagnostic)) }),
        ));
    }
    nodes
}

pub(super) fn update_status_nodes(status: &DaemonPackageUpdateStatus) -> Vec<UiNode> {
    let mut text = format!(
        "update status: package={} update_available={} reload_required={} restart_required={}",
        status.package_name,
        status.update_available,
        status.reload_required,
        status.restart_required
    );
    if let Some(pin) = &status.pin {
        text.push_str(&format!(" pin={}", pin_text(pin)));
    }
    let mut nodes = vec![node(
        UiNodeKind::Text,
        "tui-update-status-summary",
        json!({ "text": text }),
    )];
    if let Some(pin) = &status.pin {
        nodes.push(button(
            "tui-update-status-preview",
            "Preview update",
            "botster.tui.package.update_preview",
            json!({ "package_name": status.package_name, "pin": pin }),
        ));
        nodes.push(button(
            "tui-update-status-apply",
            "Apply update",
            "botster.tui.package.update_apply",
            json!({ "package_name": status.package_name, "pin": pin }),
        ));
    }
    for (index, diagnostic) in status.diagnostics.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-update-status-diagnostic-{index}"),
            json!({ "text": format!("update diagnostic: {}", package_diagnostic_text(diagnostic)) }),
        ));
    }
    nodes
}

pub(super) fn availability_state_text(state: DaemonPackageAvailabilityState) -> &'static str {
    match state {
        DaemonPackageAvailabilityState::Available => "available",
        DaemonPackageAvailabilityState::Blocked => "blocked",
    }
}

pub(super) fn availability_reason_text(reason: &DaemonPackageAvailabilityReason) -> String {
    let mut parts = vec![
        format!("reason={}", reason.reason),
        format!("action={}", reason.action),
    ];
    if let Some(package_name) = &reason.package_name {
        parts.push(format!("package={package_name}"));
    }
    if let Some(capability) = &reason.capability {
        parts.push(format!(
            "capability={}",
            capability_text(std::slice::from_ref(capability))
        ));
    }
    if let Some(requirement) = &reason.requirement {
        parts.push(format!("requirement={requirement}"));
    }
    parts.join(" ")
}

pub(super) fn pin_text(pin: &DaemonPackagePin) -> String {
    let mut parts = vec![
        format!("revision={}", pin.revision),
        format!("update_policy={}", pin.update_policy),
    ];
    if let Some(branch) = &pin.branch {
        parts.push(format!("branch={branch}"));
    }
    if let Some(tag) = &pin.tag {
        parts.push(format!("tag={tag}"));
    }
    if let Some(rev) = &pin.rev {
        parts.push(format!("rev={rev}"));
    }
    if let Some(checksum) = &pin.checksum {
        parts.push(format!("checksum={checksum}"));
    }
    parts.join(",")
}

pub(super) fn entrypoint_text(
    entrypoint: &botster_hub_client::DaemonPackageRunnableEntrypoint,
) -> String {
    let process = &entrypoint.process;
    let mut parts = vec![
        format!("id={}", entrypoint.id),
        format!("kind={}", entrypoint.kind),
        format!("state={}", process.state),
    ];
    if !process.diagnostics.is_empty() {
        let diagnostics = process
            .diagnostics
            .iter()
            .map(|diagnostic| format!("{}:{}", diagnostic.kind, diagnostic.message))
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("diagnostics={diagnostics}"));
    }
    if let Some(pid) = process.pid {
        parts.push(format!("pid={pid}"));
    }
    if let Some(started_at) = process.started_at {
        parts.push(format!("started_at={started_at}"));
    }
    if let Some(exited_at) = process.exited_at {
        parts.push(format!("exited_at={exited_at}"));
    }
    if let Some(exit_status) = &process.exit_status {
        parts.push(format!("exit_status={exit_status}"));
    }
    parts.join(",")
}

pub(super) fn capability_text(capabilities: &[botster_hub_client::DaemonCapability]) -> String {
    if capabilities.is_empty() {
        return "none".to_string();
    }

    capabilities
        .iter()
        .map(|capability| match &capability.scope {
            Some(scope) => format!("{}:{scope}", capability.surface),
            None => capability.surface.clone(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

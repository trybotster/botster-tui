use super::*;

impl TuiApp {
    #[cfg(test)]
    pub(super) fn record_request(&mut self, request: &DaemonRequest) {
        match request {
            DaemonRequest::Status => self.observed_requests.push(ObservedRequest::Status),
            DaemonRequest::ReadPluginLogs {
                package_name,
                after_seq,
            } => self
                .observed_requests
                .push(ObservedRequest::ReadPluginLogs {
                    package_name: package_name.clone(),
                    after_seq: *after_seq,
                }),
            DaemonRequest::ResolveQuarantine { target } => self
                .observed_requests
                .push(ObservedRequest::ResolveQuarantine(target.clone())),
            DaemonRequest::ListApps => self.observed_requests.push(ObservedRequest::ListApps),
            DaemonRequest::ListPackageNavigation => self
                .observed_requests
                .push(ObservedRequest::ListPackageNavigation),
            DaemonRequest::ListPackages => {
                self.observed_requests.push(ObservedRequest::ListPackages)
            }
            DaemonRequest::ShowPackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::ShowPackage(package_name.clone())),
            DaemonRequest::SetPackageConfiguration {
                package_name,
                values,
            } => self
                .observed_requests
                .push(ObservedRequest::SetPackageConfiguration {
                    package_name: package_name.clone(),
                    values: values.clone(),
                }),
            DaemonRequest::EnablePackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::EnablePackage(package_name.clone())),
            DaemonRequest::DisablePackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::DisablePackage(package_name.clone())),
            DaemonRequest::RemovePackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::RemovePackage(package_name.clone())),
            DaemonRequest::CheckPackageUpdate { package_name } => self
                .observed_requests
                .push(ObservedRequest::CheckPackageUpdate(package_name.clone())),
            DaemonRequest::PreviewPackageUpdate { package_name, pin } => self
                .observed_requests
                .push(ObservedRequest::PreviewPackageUpdate {
                    package_name: package_name.clone(),
                    pin: pin.clone(),
                }),
            DaemonRequest::ApplyPackageUpdate { package_name, pin } => {
                self.observed_requests
                    .push(ObservedRequest::ApplyPackageUpdate {
                        package_name: package_name.clone(),
                        pin: pin.clone(),
                    })
            }
            DaemonRequest::StartPackageEntrypoint {
                package_name,
                entrypoint_id,
                ..
            } => self
                .observed_requests
                .push(ObservedRequest::StartPackageEntrypoint {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::StopPackageEntrypoint {
                package_name,
                entrypoint_id,
            } => self
                .observed_requests
                .push(ObservedRequest::StopPackageEntrypoint {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::RestartPackageEntrypoint {
                package_name,
                entrypoint_id,
            } => self
                .observed_requests
                .push(ObservedRequest::RestartPackageEntrypoint {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::PackageEntrypointStatus {
                package_name,
                entrypoint_id,
            } => self
                .observed_requests
                .push(ObservedRequest::PackageEntrypointStatus {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::PluginSurfaceRender {
                package_name,
                surface_id,
                ..
            } => self
                .observed_requests
                .push(ObservedRequest::PluginSurfaceRender {
                    package_name: package_name.clone(),
                    surface_id: surface_id.clone(),
                }),
            DaemonRequest::PluginSurfaceAction {
                package_name,
                request,
            } => self
                .observed_requests
                .push(ObservedRequest::PluginSurfaceAction {
                    package_name: package_name.clone(),
                    request: request.clone(),
                }),
            DaemonRequest::Attach {
                session_id,
                subscription_id,
            } => self.observed_requests.push(ObservedRequest::Attach {
                session_id: session_id.clone(),
                subscription_id: subscription_id.clone(),
            }),
            DaemonRequest::Detach {
                session_id,
                subscription_id,
            } => self.observed_requests.push(ObservedRequest::Detach {
                session_id: session_id.clone(),
                subscription_id: subscription_id.clone(),
            }),
            DaemonRequest::ShutdownSession { session_id } => self
                .observed_requests
                .push(ObservedRequest::ShutdownSession(session_id.clone())),
            DaemonRequest::RemoveSession { session_id } => self
                .observed_requests
                .push(ObservedRequest::RemoveSession(session_id.clone())),
            DaemonRequest::ReadScreen { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadScreen(session_id.clone())),
            DaemonRequest::ReadModeFlags { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadModeFlags(session_id.clone())),
            DaemonRequest::CaptureSnapshot { session_id } => self
                .observed_requests
                .push(ObservedRequest::CaptureSnapshot(session_id.clone())),
            DaemonRequest::ListSpawnTargets => self
                .observed_requests
                .push(ObservedRequest::ListSpawnTargets),
            DaemonRequest::ListSessionTypesForTarget { target_id } => {
                self.observed_requests
                    .push(ObservedRequest::ListSessionTypesForTarget {
                        target_id: target_id.clone(),
                    })
            }
            DaemonRequest::ShowSessionTypeDefinition { session_type_id } => self
                .observed_requests
                .push(ObservedRequest::ShowSessionTypeDefinition(
                    session_type_id.clone(),
                )),
            DaemonRequest::CreateSessionType { .. } => self
                .observed_requests
                .push(ObservedRequest::CreateSessionType),
            DaemonRequest::UpdateSessionType { .. } => self
                .observed_requests
                .push(ObservedRequest::UpdateSessionType),
            DaemonRequest::DeleteSessionType {
                source,
                session_type_id,
            } => self
                .observed_requests
                .push(ObservedRequest::DeleteSessionType {
                    source: source.clone(),
                    session_type_id: session_type_id.clone(),
                }),
            DaemonRequest::SpawnSessionType {
                session_type_id,
                session_id,
                request,
            } => self
                .observed_requests
                .push(ObservedRequest::SpawnSessionType {
                    session_type_id: session_type_id.clone(),
                    session_id: session_id.clone(),
                    target_id: request.target_id.clone(),
                }),
            DaemonRequest::Spawn {
                session_id,
                command,
            } => self.observed_requests.push(ObservedRequest::Spawn {
                session_id: session_id.clone(),
                command: command.clone(),
            }),
            DaemonRequest::SubscribeEvents {
                subscription_id,
                owner,
                name,
                subjects,
            } => self
                .observed_requests
                .push(ObservedRequest::SubscribeEvents {
                    subscription_id: subscription_id.clone(),
                    owner: owner.clone(),
                    name: name.clone(),
                    subjects: subjects.clone(),
                }),
            DaemonRequest::UnsubscribeEvents { subscription_id } => {
                self.observed_requests
                    .push(ObservedRequest::UnsubscribeEvents {
                        subscription_id: subscription_id.clone(),
                    })
            }
            _ => {}
        }
    }

    /// Apply one host-control response to read models and diagnostics.
    ///
    /// Terminal-stream events never travel in responses on v9; the terminal
    /// plane is the only source of OUTPUT, snapshots, attach state, and results.
    pub(super) fn apply_response(&mut self, response: DaemonResponse) {
        self.record_diagnostics(response.diagnostics);

        if let Some(error) = response.error {
            let quarantine_changed = error_may_create_quarantine(&error.code, &error.operation);
            self.record_diagnostics(error.diagnostics);
            self.error = Some(if error.code == "not_attached" {
                // Core refused input, resize or a guarded write from a client
                // with no attachment; nothing reached the session.
                format!(
                    "not attached: {} (operation={}); nothing reached the session",
                    error.message, error.operation
                )
            } else {
                format!(
                    "{} (code={} operation={})",
                    error.message, error.code, error.operation
                )
            });
            if quarantine_changed {
                // Only Status lists quarantines; show the new one now.
                self.refresh_status();
            }
            return;
        }

        if let Some(status) = response.status {
            self.connection_error = None;
            self.clear_connection_diagnostics();
            self.schema_version = Some(status.schema_version);
            self.compatibility = Some(status.compatibility);
            self.software = Some(status.software);
            self.record_diagnostics(status.diagnostics);
            self.status = format!("connected ({})", status.lifecycle_state);
            self.package_count = status.package_count;
            self.enabled_package_count = status.enabled_package_count;
            self.quarantines = status.quarantines;
            self.hub_counters = status.observability;
        }

        if matches!(response.kind, DaemonResponseKind::PluginLogs)
            && let Some(logs) = response.plugin_logs.clone()
        {
            self.plugin_logs.insert(logs.package_name.clone(), logs);
        }
        if matches!(response.kind, DaemonResponseKind::QuarantineResolved) {
            // The quarantine list lives only in Status, so read it again. A
            // package resolution's package list is applied by its reply.
            self.action_feedback = Some("quarantine resolved".to_string());
            self.refresh_status();
        }
        if matches!(
            response.kind,
            DaemonResponseKind::Packages | DaemonResponseKind::PackageDecision
        ) {
            self.packages = response.packages;
            self.sync_notice_subscriptions();
        }
        if matches!(response.kind, DaemonResponseKind::Apps) {
            self.apps = response.apps;
        }
        if matches!(response.kind, DaemonResponseKind::PackageNavigation) {
            self.package_navigation = response.package_navigation;
        }
        if matches!(response.kind, DaemonResponseKind::SpawnTargets) {
            self.spawn_targets = response.spawn_targets;
            self.spawn_targets_loaded = true;
        }
        if matches!(response.kind, DaemonResponseKind::AvailablePackages) {
            self.available_packages = response.available_packages;
        }
        if matches!(response.kind, DaemonResponseKind::PackageInstallPlan) {
            self.install_plan = response.install_plan;
        }
        if matches!(response.kind, DaemonResponseKind::PackageUpdateStatus) {
            self.update_status = response.update_status;
        }
        if matches!(response.kind, DaemonResponseKind::PackageDecision) {
            self.package_decision = response.package_decision;
        }
        if matches!(response.kind, DaemonResponseKind::PluginSurface)
            && let Some(surface) = response.plugin_surface
        {
            match normalize_plugin_surface(surface) {
                Ok(surface) => {
                    let owner_changed = self.plugin_surface.as_ref().is_none_or(|current| {
                        current.package_name != surface.package_name
                            || current.surface_id != surface.surface_id
                    });
                    if owner_changed {
                        self.plugin_presentation = renderer::PresentationState::default();
                        self.plugin_action_result = None;
                        self.pending_plugin_request = None;
                        // Surface replacement drops prior surface-demanded generations.
                        self.drop_entity_options_subscriptions();
                        self.entity_options_invalid_fields.clear();
                    }
                    self.plugin_surface = Some(surface);
                    self.sync_entity_options_subscriptions();
                }
                Err(error) => {
                    self.error = Some(format!("plugin surface render: {error}"));
                }
            }
        }
        if matches!(response.kind, DaemonResponseKind::PluginActionResult)
            && let Some(result) = response.plugin_action_result
        {
            self.apply_plugin_action_result(result);
        }
    }
}

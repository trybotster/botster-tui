use super::*;

impl TuiApp {
    pub(super) fn open_package_navigation(
        &mut self,
        package_name: String,
        surface_id: String,
        route_id: String,
    ) {
        self.error = None;
        self.action_feedback = Some(format!(
            "navigation open requested: {package_name} {route_id}"
        ));
        self.submit_apply(DaemonRequest::PluginSurfaceRender {
            package_name,
            surface_id,
            payload: json!({}),
        });
    }

    pub(super) fn launch_target_options(&self) -> Vec<LaunchTargetOption> {
        let mut options: BTreeMap<String, LaunchTargetOption> = BTreeMap::new();
        for target in &self.spawn_targets {
            if !target.enabled {
                continue;
            }
            options.insert(
                target.target_id.clone(),
                LaunchTargetOption {
                    target_id: target.target_id.clone(),
                    label: target.label.clone(),
                },
            );
        }
        options.into_values().collect()
    }

    pub(super) fn begin_target_first_spawn(&mut self) {
        self.error = None;
        self.session_type_form = None;
        if let Some(failure) = &self.spawn_targets_failure {
            self.error = Some(format!("launch targets failed to load: {failure}"));
            return;
        }
        // Before the target list loads, the dialog opens and fills in when the
        // ListSpawnTargets reply arrives.
        if self.spawn_targets_loaded && self.launch_target_options().is_empty() {
            self.error =
                Some("no launch targets available (no enabled admitted spawn targets)".to_string());
            return;
        }
        self.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickTarget,
        });
        self.action_feedback = Some("select a launch target".to_string());
    }

    pub(super) fn spawn_pick_target(&mut self, target_id: &str) {
        let Some(target) = self
            .launch_target_options()
            .into_iter()
            .find(|target| target.target_id == target_id)
        else {
            self.error = Some(format!("launch target not found: {target_id}"));
            return;
        };
        // Clear any prior picker rows before the list request so a failed
        // load cannot leave selectable stale rows from a previous target.
        self.error = None;
        self.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickTarget,
        });
        self.submit(
            DaemonRequest::ListSessionTypesForTarget {
                target_id: target.target_id.clone(),
            },
            PendingReply::ListForTarget {
                target_id: target.target_id,
                target_label: target.label,
            },
            REQUEST_DEADLINE,
        );
    }

    /// ListSessionTypesForTarget completed. Flow-local only: the entity store
    /// is not touched.
    pub(super) fn apply_list_for_target(
        &mut self,
        target_id: &str,
        target_label: &str,
        response: DaemonResponse,
    ) {
        let picking = matches!(
            self.target_first_spawn.as_ref().map(|flow| &flow.step),
            Some(TargetFirstSpawnStep::PickTarget)
        );
        if !picking {
            return;
        }
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
            self.error = Some(format!(
                "{} (code={} operation={})",
                error.message, error.code, error.operation
            ));
            self.action_feedback = Some(format!(
                "session types for {target_label} unavailable; pick another target or cancel"
            ));
            return;
        }
        self.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickSessionType {
                target_id: target_id.to_string(),
                target_label: target_label.to_string(),
                session_types: response.session_types,
            },
        });
        self.action_feedback = Some(format!("select a session type for {target_label}"));
    }

    pub(super) fn execute_spawn_session_type(
        &mut self,
        session_type_id: &str,
        request: DaemonSessionTypeRequest,
    ) {
        self.error = None;
        self.target_first_spawn = None;
        let session_id = format!("btui-{}", short_suffix());
        self.pending_sessions
            .insert(session_id.clone(), SessionRow::pending(session_id.clone()));
        self.set_selected_session(Some(session_id.clone()));
        self.rebuild_session_rows();
        self.action_feedback = Some(format!("spawn pending: {session_id} via {session_type_id}"));
        self.submit(
            DaemonRequest::SpawnSessionType {
                session_type_id: session_type_id.to_string(),
                session_id: session_id.clone(),
                request,
            },
            PendingReply::Spawn { session_id },
            REQUEST_DEADLINE,
        );
    }

    pub(super) fn open_session_type_edit(&mut self, session_type_id: &str) {
        self.error = None;
        self.action_feedback = Some(format!("loading authoring definition: {session_type_id}"));
        self.submit(
            DaemonRequest::ShowSessionTypeDefinition {
                session_type_id: session_type_id.to_string(),
            },
            PendingReply::ShowSessionTypeDefinition {
                session_type_id: session_type_id.to_string(),
            },
            REQUEST_DEADLINE,
        );
    }

    pub(super) fn apply_show_session_type_definition(
        &mut self,
        session_type_id: &str,
        response: DaemonResponse,
    ) {
        if let Some(error) = response.error.clone() {
            self.apply_response(response);
            self.error = Some(format!("{}: {}", error.code, error.message));
            return;
        }
        let definition = response.session_type_definition.clone();
        self.apply_response(response);
        match definition {
            Some(editable) => {
                self.session_type_form = Some(SessionTypeFormDraft::from_authoring(editable));
                self.action_feedback = Some(format!("edit ready: {session_type_id}"));
            }
            None => {
                self.error =
                    Some("show_session_type_definition returned no definition".to_string());
            }
        }
    }

    pub(super) fn delete_session_type(&mut self, session_type_id: &str) {
        let Some(entity) = self
            .session_type_entities
            .entities
            .get(session_type_id)
            .cloned()
        else {
            self.error = Some(format!("session type not found: {session_type_id}"));
            return;
        };
        if !entity.editable {
            self.error = Some(format!("session type is not editable: {session_type_id}"));
            return;
        }
        // Prefer source_name (owning source) over target_id (eligibility stamp),
        // matching Hub show_session_type_definition mutation source construction.
        let source = match entity.source.as_str() {
            "device" => DaemonSessionTypeMutationSource::Device,
            "repo" => DaemonSessionTypeMutationSource::Repo {
                target_id: if !entity.source_name.is_empty() {
                    entity.source_name.clone()
                } else {
                    entity.target_id.clone()
                },
            },
            other => {
                self.error = Some(format!("cannot delete session type source: {other}"));
                return;
            }
        };
        self.action_feedback = Some(format!("delete requested: {session_type_id}"));
        self.submit_apply(DaemonRequest::DeleteSessionType {
            source,
            session_type_id: entity.id.clone(),
        });
    }

    pub(super) fn submit_session_type_form(&mut self) {
        let Some(form) = self.session_type_form.clone() else {
            return;
        };
        if form.id.trim().is_empty()
            || form.label.trim().is_empty()
            || form.role.trim().is_empty()
            || form.interaction.trim().is_empty()
            || form.lifecycle.trim().is_empty()
            || form.command.trim().is_empty()
        {
            if let Some(form) = self.session_type_form.as_mut() {
                form.error = Some(
                    "id, label, role, interaction, lifecycle, and command are required".to_string(),
                );
            }
            return;
        }
        let source = match mutation_source_from_form(&form) {
            Ok(source) => source,
            Err(error) => {
                if let Some(form) = self.session_type_form.as_mut() {
                    form.error = Some(error);
                }
                return;
            }
        };
        let definition = match definition_from_session_type_form(&form) {
            Ok(definition) => definition,
            Err(error) => {
                if let Some(form) = self.session_type_form.as_mut() {
                    form.error = Some(error);
                }
                return;
            }
        };
        let request = match form.mode {
            SessionTypeFormMode::Create => DaemonRequest::CreateSessionType { source, definition },
            SessionTypeFormMode::Edit => DaemonRequest::UpdateSessionType { source, definition },
        };
        self.action_feedback = Some(match form.mode {
            SessionTypeFormMode::Create => "create session type requested".to_string(),
            SessionTypeFormMode::Edit => "update session type requested".to_string(),
        });
        self.submit(request, PendingReply::SessionTypeForm, REQUEST_DEADLINE);
    }

    pub(super) fn spawn_pick_session_type(&mut self, session_type_id: &str) {
        let Some(flow) = self.target_first_spawn.as_ref() else {
            return;
        };
        let TargetFirstSpawnStep::PickSessionType {
            target_id,
            target_label,
            session_types,
        } = &flow.step
        else {
            return;
        };
        let Some(session_type) = session_types
            .iter()
            .find(|session_type| session_type.session_type_id == session_type_id)
            .cloned()
        else {
            self.error = Some(format!("session type not found: {session_type_id}"));
            return;
        };
        if !session_type.available {
            self.error = Some(format!(
                "session type unavailable: {}{}",
                session_type.session_type_id,
                if session_type.diagnostics.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", session_type.diagnostics.join("; "))
                }
            ));
            return;
        }
        let needs_prompt = session_type.context_keys.iter().any(|key| key == "prompt");
        if needs_prompt {
            self.target_first_spawn = Some(TargetFirstSpawnFlow {
                step: TargetFirstSpawnStep::Prompt {
                    target_id: target_id.clone(),
                    target_label: target_label.clone(),
                    session_type_id: session_type_id.to_string(),
                    prompt: String::new(),
                },
            });
            self.action_feedback = Some("enter prompt context".to_string());
            return;
        }
        self.execute_spawn_session_type(
            session_type_id,
            DaemonSessionTypeRequest {
                target_id: Some(target_id.clone()),
                ..DaemonSessionTypeRequest::default()
            },
        );
    }

    pub(super) fn submit_target_first_spawn(&mut self) {
        let Some(flow) = self.target_first_spawn.clone() else {
            return;
        };
        match flow.step {
            TargetFirstSpawnStep::Prompt {
                target_id,
                session_type_id,
                prompt,
                ..
            } => {
                let mut request = DaemonSessionTypeRequest {
                    target_id: Some(target_id.clone()),
                    ..DaemonSessionTypeRequest::default()
                };
                if !prompt.trim().is_empty() {
                    request.context.prompt = Some(prompt.trim().to_string());
                }
                self.execute_spawn_session_type(&session_type_id, request);
            }
            _ => {
                self.error = Some("spawn form is incomplete".to_string());
            }
        }
    }

    pub(super) fn apply_spawn_flow_values(&mut self, values: &UiFormValues) {
        let Some(flow) = self.target_first_spawn.as_mut() else {
            return;
        };
        if let TargetFirstSpawnStep::Prompt { prompt, .. } = &mut flow.step
            && let Some(value) = values.0.get("spawn_prompt").and_then(Value::as_str)
        {
            *prompt = value.to_string();
        }
    }
}

impl TuiApp {
    /// Ask the Hub to start an ended session again under the same session id.
    /// Offered only when the Hub advertised `session_restart` at Hello.
    pub(super) fn restart_session(&mut self, session_id: &str) {
        if !self.hub_offers_restart {
            self.error = Some("restart unavailable: this Hub does not offer it".to_string());
            return;
        }
        if !self.restarting_sessions.insert(session_id.to_string()) {
            return;
        }
        self.error = None;
        self.action_feedback = Some(format!("restarting: {session_id}"));
        self.submit(
            DaemonRequest::RestartSession {
                session_id: session_id.to_string(),
            },
            PendingReply::Restart {
                session_id: session_id.to_string(),
            },
            REQUEST_DEADLINE,
        );
    }
}

/// Whether the Hub's Hello offers `session_restart`.
pub(super) fn host_offers_restart(compatibility: &DaemonCompatibility) -> bool {
    compatibility
        .features
        .iter()
        .any(|feature| feature == FEATURE_SESSION_RESTART)
}

/// What the Hub's refusal of a restart means for the operator.
pub(super) fn restart_refusal_text(
    session_id: &str,
    error: &botster_hub_client::DaemonOperatorError,
) -> String {
    let advice = match error.code.as_str() {
        "restart_not_ready" => "; the Hub cannot release the session yet, retry shortly",
        "restart_not_ended" => "; the session is still running",
        "restart_record_unavailable" => "; the Hub holds no restart record for it",
        "restart_environment_not_retained" => "; its environment was not kept",
        _ => "",
    };
    format!(
        "restart refused for {session_id}: {} (code={}){advice}",
        error.message, error.code
    )
}

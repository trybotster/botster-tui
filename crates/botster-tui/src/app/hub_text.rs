use super::*;

/// Operator errors whose failure the Hub records as a quarantine: a package
/// mutation whose rollback failed, and a repository session-types write whose
/// outcome is unknown.
pub(super) fn error_may_create_quarantine(code: &str, operation: &str) -> bool {
    code == "package_compensation_failed" || operation == "repo_session_type"
}

/// The resolution target for a listed quarantine.
pub(super) fn quarantine_target(quarantine: &DaemonQuarantine) -> DaemonQuarantineTarget {
    match quarantine {
        DaemonQuarantine::Package { package_name, .. } => DaemonQuarantineTarget::Package {
            package_name: package_name.clone(),
        },
        DaemonQuarantine::RepositorySessionTypes { root, .. } => {
            DaemonQuarantineTarget::RepositorySessionTypes { root: root.clone() }
        }
    }
}

pub(super) fn quarantine_target_text(target: &DaemonQuarantineTarget) -> String {
    match target {
        DaemonQuarantineTarget::Package { package_name } => format!("package {package_name}"),
        DaemonQuarantineTarget::RepositorySessionTypes { root } => {
            format!("session types at {}", root.display())
        }
    }
}

pub(super) fn quarantine_text(quarantine: &DaemonQuarantine) -> String {
    match quarantine {
        DaemonQuarantine::Package {
            package_name,
            original,
            compensation,
            durable,
            loaded,
            quarantined_at_ms,
        } => {
            let mut text = format!("package {package_name}");
            if *quarantined_at_ms == 0 {
                text.push_str(": no failure record");
            } else {
                text.push_str(&format!(
                    ": {original}; compensation failed: {compensation}"
                ));
            }
            if *loaded {
                text.push_str(" · loaded but inert");
            }
            if !*durable {
                text.push_str(" · not durable: lasts until the Hub restarts");
            }
            text
        }
        DaemonQuarantine::RepositorySessionTypes {
            root,
            cause,
            detail,
            ..
        } => format!("session types at {}: {cause}: {detail}", root.display()),
    }
}

/// The protocol 11 event and quarantine counters that are not zero.
pub(super) fn hub_counters_text(counters: &DaemonObservabilityCounters) -> Option<String> {
    let values = [
        (
            "event_replacements_stranded",
            counters.event_replacements_stranded,
        ),
        (
            "event_deliveries_generation_unloaded",
            counters.event_deliveries_generation_unloaded,
        ),
        (
            "event_deliveries_package_unloaded",
            counters.event_deliveries_package_unloaded,
        ),
        (
            "event_deliveries_handler_absent",
            counters.event_deliveries_handler_absent,
        ),
        ("event_stage_overlaps", counters.event_stage_overlaps),
        ("events_stranded", counters.events_stranded),
        (
            "package_quarantines_not_durable",
            counters.package_quarantines_not_durable,
        ),
    ];
    let shown = values
        .iter()
        .filter(|(_, value)| *value != 0)
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    (!shown.is_empty()).then(|| format!("hub counters: {}", shown.join(" ")))
}

pub(super) fn admit_terminal_hello(ack: &DaemonHelloAck) -> DaemonTransportResult<()> {
    let requirement = tui_terminal_compatibility_requirement();
    let Some(terminal_compatibility) = ack.terminal_compatibility.as_ref() else {
        return Err(terminal_hello_error(
            "hello ack omitted terminal_compatibility",
        ));
    };
    ensure_terminal_compatible(&requirement, terminal_compatibility)
        .map_err(|error| terminal_hello_error_with_diagnostic(error.diagnostic))
}

pub(super) fn terminal_hello_error(reason: &str) -> DaemonTransportError {
    terminal_hello_error_with_diagnostic(format!(
        "botster-tui is incompatible with the terminal protocol: {reason}"
    ))
}

pub(super) fn terminal_hello_error_with_diagnostic(diagnostic: String) -> DaemonTransportError {
    DaemonTransportError::Compatibility(DaemonCompatibilityError {
        diagnostic: diagnostic.clone(),
        diagnostics: vec![DaemonDiagnostic::compatibility_mismatch(diagnostic)],
    })
}

pub(super) fn tui_compatibility_requirement() -> DaemonCompatibilityRequirement {
    DaemonCompatibilityRequirement {
        protocol: PROTOCOL.to_string(),
        protocol_version: botster_hub_client::PROTOCOL_VERSION,
        required_features: vec![
            FEATURE_SESSIONS.to_string(),
            FEATURE_PACKAGE_NAVIGATION.to_string(),
            FEATURE_PLUGIN_SURFACE_RENDER.to_string(),
            FEATURE_PLUGIN_SURFACE_ACTION.to_string(),
            FEATURE_TERMINAL_READBACK.to_string(),
            FEATURE_SESSION_ENTITY_SUBSCRIPTIONS.to_string(),
            FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS.to_string(),
            FEATURE_UNIX_TERMINAL_ADAPTER.to_string(),
            FEATURE_TERMINAL_SUBSCRIPTION_CLOSED.to_string(),
            FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS.to_string(),
        ],
        minimum_conformance_fixture_revision: MINIMUM_CONFORMANCE_FIXTURE_REVISION,
        client_name: "botster-tui".to_string(),
    }
}

pub(super) fn tui_terminal_compatibility_requirement() -> TerminalCompatibilityRequirement {
    let mut requirement = TerminalCompatibilityRequirement::for_ready_then_history_attach();
    requirement.client_name = "botster-tui".to_string();
    requirement
}

pub(super) fn diagnostic_text(diagnostic: &DaemonDiagnostic) -> String {
    let label = match diagnostic.kind {
        DaemonDiagnosticKind::Connected => "connected",
        DaemonDiagnosticKind::Disconnected => "disconnected",
        DaemonDiagnosticKind::CompatibilityMismatch => "compatibility_mismatch",
        DaemonDiagnosticKind::UnsupportedFeature => "unsupported_feature",
        DaemonDiagnosticKind::TerminalStreamUnavailable => "terminal_stream_unavailable",
        DaemonDiagnosticKind::WorkerCompatibility => "worker_compatibility",
        DaemonDiagnosticKind::ActionFailure => "action_failure",
        DaemonDiagnosticKind::DaemonStartupFailure => "daemon_startup_failure",
        DaemonDiagnosticKind::Backpressure => "backpressure",
    };
    let mut parts = vec![label.to_string()];
    if let Some(operation) = &diagnostic.operation {
        parts.push(format!("operation={operation}"));
    }
    if let Some(feature) = &diagnostic.feature {
        parts.push(format!("feature={feature}"));
    }
    if let Some(message) = &diagnostic.message {
        parts.push(message.clone());
    }
    parts.join("; ")
}

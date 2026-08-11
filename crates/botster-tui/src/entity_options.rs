//! Entity-backed `ui.select` options: multi-family generation store and
//! materialization of `options_source` into kit-ready static SelectOption slots.
//!
//! Projection policy lives in `botster-ui-contract`. This module owns TUI client
//! subscription generation discipline (mirroring SessionEntityState) and the
//! app-side realization step that happens before kit handoff.

use std::collections::{BTreeMap, BTreeSet};

use botster_hub_client::DaemonEntityFrame;
use botster_ui_contract::{
    EntityFamilyStore, EntityOption, EntityOptionsFrame, EntityRecordItem, UiAuthoredNodeId,
    UiBindIf, UiBindList, UiChild, UiConditional, UiEntityOptionsSource, UiNode, UiNodeId,
    UiNodeKind, apply_entity_options_frame, collect_entity_option_families,
    project_entity_options_from_store,
};
use serde_json::{Map, Value};

/// Families already owned by process-wide TUI subscriptions (navigator / settings).
/// Entity-options demand reuses those subscriptions instead of opening a second one.
pub fn is_process_wide_entity_family(family: &str) -> bool {
    matches!(family, "session" | "session_type")
}

/// Per-family generation + record map for options projection.
#[derive(Clone, Debug, Default)]
pub struct EntityOptionsFamilyState {
    pub subscription_id: Option<String>,
    pub has_snapshot: bool,
    pub snapshot_seq: Option<u64>,
    pub records: BTreeMap<String, Map<String, Value>>,
}

impl EntityOptionsFamilyState {
    pub fn begin_generation(&mut self, subscription_id: String) {
        self.subscription_id = Some(subscription_id);
        self.has_snapshot = false;
        self.snapshot_seq = None;
        self.records.clear();
    }

    pub fn matches(&self, subscription_id: &str, entity_type: &str, family: &str) -> bool {
        entity_type == family && self.subscription_id.as_deref() == Some(subscription_id)
    }

    /// Classify a delta against the current generation.
    ///
    /// - `Accept` — matching generation, snapshot held, strictly advancing seq
    /// - `Ignore` — foreign generation / non-advancing seq (do not mutate)
    /// - `NeedsRecovery` — matching generation but no snapshot yet, or a sequence
    ///   hole after a snapshot. Production drain must resubscribe for a fresh
    ///   authoritative snapshot.
    pub fn classify_delta(
        &self,
        subscription_id: &str,
        entity_type: &str,
        family: &str,
        snapshot_seq: u64,
    ) -> DeltaDisposition {
        if !self.matches(subscription_id, entity_type, family) {
            return DeltaDisposition::Ignore;
        }
        if !self.has_snapshot {
            return DeltaDisposition::NeedsRecovery(
                "delta before first authoritative snapshot".to_string(),
            );
        }
        match self.snapshot_seq {
            Some(current) if snapshot_seq == current + 1 => DeltaDisposition::Accept,
            Some(current) if snapshot_seq > current + 1 => DeltaDisposition::NeedsRecovery(
                format!("sequence gap: current={current} observed={snapshot_seq}"),
            ),
            // Non-advancing or older seq under the active generation: ignore.
            Some(_) | None => DeltaDisposition::Ignore,
        }
    }
}

/// Production delta disposition for entity-options generation discipline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaDisposition {
    Accept,
    Ignore,
    NeedsRecovery(String),
}

/// Multi-family entity-options store with per-family generation discipline.
#[derive(Clone, Debug, Default)]
pub struct EntityOptionsStore {
    families: BTreeMap<String, EntityOptionsFamilyState>,
}

impl EntityOptionsStore {
    pub fn family(&self, entity_type: &str) -> Option<&EntityOptionsFamilyState> {
        self.families.get(entity_type)
    }

    pub fn family_mut(&mut self, entity_type: &str) -> &mut EntityOptionsFamilyState {
        self.families.entry(entity_type.to_string()).or_default()
    }

    pub fn begin_generation(&mut self, entity_type: &str, subscription_id: String) {
        self.family_mut(entity_type)
            .begin_generation(subscription_id);
    }

    pub fn drop_family(&mut self, entity_type: &str) {
        self.families.remove(entity_type);
    }

    pub fn retain_families(&mut self, wanted: &BTreeSet<String>) {
        self.families.retain(|family, _| wanted.contains(family));
    }

    /// Apply a production DaemonEntityFrame with generation/sequence gates.
    ///
    /// - `Ok(true)` — family map mutated
    /// - `Ok(false)` — ignored (no generation, foreign subscription, non-advancing seq)
    /// - `Err` — matching generation needs recovery (pre-snapshot delta, sequence gap,
    ///   or subscription Error). Callers must `begin_generation` + resubscribe.
    pub fn apply_daemon_frame(&mut self, frame: DaemonEntityFrame) -> Result<bool, String> {
        let family_key = entity_type_of(&frame).to_string();
        let Some(family) = self.families.get_mut(&family_key) else {
            // No begin_generation for this family — ignore foreign frames.
            return Ok(false);
        };
        match frame {
            DaemonEntityFrame::Snapshot {
                subscription_id,
                entity_type,
                snapshot_seq,
                items,
                resync_reason: _,
            } => {
                if !family.matches(&subscription_id, &entity_type, &family_key) {
                    return Ok(false);
                }
                let options_frame = EntityOptionsFrame::Snapshot {
                    entity_type: family_key.clone(),
                    snapshot_seq,
                    items: items
                        .into_iter()
                        .map(daemon_item_to_record)
                        .collect::<Result<Vec<_>, _>>()?,
                    resync_reason: None,
                };
                let mut single = EntityFamilyStore::new();
                single.insert(family_key.clone(), family.records.clone());
                apply_entity_options_frame(&mut single, &options_frame);
                family.records = single.remove(&family_key).unwrap_or_default();
                family.has_snapshot = true;
                family.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Upsert {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
                entity,
            } => {
                match family.classify_delta(
                    &subscription_id,
                    &entity_type,
                    &family_key,
                    snapshot_seq,
                ) {
                    DeltaDisposition::Ignore => return Ok(false),
                    DeltaDisposition::NeedsRecovery(reason) => {
                        return Err(format!(
                            "entity options gap recovery required for {family_key}: {reason}"
                        ));
                    }
                    DeltaDisposition::Accept => {}
                }
                let fields = value_to_fields(entity, &id)?;
                let options_frame = EntityOptionsFrame::Upsert {
                    entity_type: family_key.clone(),
                    id,
                    fields,
                    seq: snapshot_seq,
                };
                let mut single = EntityFamilyStore::new();
                single.insert(family_key.clone(), family.records.clone());
                apply_entity_options_frame(&mut single, &options_frame);
                family.records = single.remove(&family_key).unwrap_or_default();
                family.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Patch {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
                patch,
            } => {
                match family.classify_delta(
                    &subscription_id,
                    &entity_type,
                    &family_key,
                    snapshot_seq,
                ) {
                    DeltaDisposition::Ignore => return Ok(false),
                    DeltaDisposition::NeedsRecovery(reason) => {
                        return Err(format!(
                            "entity options gap recovery required for {family_key}: {reason}"
                        ));
                    }
                    DeltaDisposition::Accept => {}
                }
                let fields = value_to_fields(patch, &id)?;
                let options_frame = EntityOptionsFrame::Patch {
                    entity_type: family_key.clone(),
                    id,
                    fields,
                    seq: snapshot_seq,
                };
                let mut single = EntityFamilyStore::new();
                single.insert(family_key.clone(), family.records.clone());
                apply_entity_options_frame(&mut single, &options_frame);
                family.records = single.remove(&family_key).unwrap_or_default();
                family.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Remove {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
            } => {
                match family.classify_delta(
                    &subscription_id,
                    &entity_type,
                    &family_key,
                    snapshot_seq,
                ) {
                    DeltaDisposition::Ignore => return Ok(false),
                    DeltaDisposition::NeedsRecovery(reason) => {
                        return Err(format!(
                            "entity options gap recovery required for {family_key}: {reason}"
                        ));
                    }
                    DeltaDisposition::Accept => {}
                }
                let options_frame = EntityOptionsFrame::Remove {
                    entity_type: family_key.clone(),
                    id,
                    seq: snapshot_seq,
                };
                let mut single = EntityFamilyStore::new();
                single.insert(family_key.clone(), family.records.clone());
                apply_entity_options_frame(&mut single, &options_frame);
                family.records = single.remove(&family_key).unwrap_or_default();
                family.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Error {
                subscription_id,
                entity_type,
                code,
                message,
            } => {
                if !family.matches(&subscription_id, &entity_type, &family_key) {
                    return Ok(false);
                }
                Err(format!(
                    "entity options subscription error for {entity_type}: code={code} message={message}"
                ))
            }
        }
    }

    /// Apply a pure contract timeline frame without generation gates (fixture path).
    #[cfg(test)]
    pub fn apply_contract_frame(&mut self, frame: &EntityOptionsFrame) -> bool {
        let entity_type = match frame {
            EntityOptionsFrame::Snapshot { entity_type, .. }
            | EntityOptionsFrame::Upsert { entity_type, .. }
            | EntityOptionsFrame::Patch { entity_type, .. }
            | EntityOptionsFrame::Remove { entity_type, .. } => entity_type.clone(),
        };
        let family = self.family_mut(&entity_type);
        let mut single = EntityFamilyStore::new();
        single.insert(entity_type.clone(), family.records.clone());
        apply_entity_options_frame(&mut single, frame);
        family.records = single.remove(&entity_type).unwrap_or_default();
        match frame {
            EntityOptionsFrame::Snapshot { snapshot_seq, .. } => {
                family.has_snapshot = true;
                family.snapshot_seq = Some(*snapshot_seq);
            }
            EntityOptionsFrame::Upsert { seq, .. }
            | EntityOptionsFrame::Patch { seq, .. }
            | EntityOptionsFrame::Remove { seq, .. } => {
                family.snapshot_seq = Some(*seq);
            }
        }
        true
    }

    /// Seed a family generation for pure fixture application without a live subscribe.
    #[cfg(test)]
    pub fn seed_family_for_fixture(&mut self, entity_type: &str, subscription_id: &str) {
        self.begin_generation(entity_type, subscription_id.to_string());
        if let Some(family) = self.families.get_mut(entity_type) {
            family.has_snapshot = true;
            family.snapshot_seq = Some(0);
        }
    }

    pub fn as_family_store(&self) -> EntityFamilyStore {
        self.families
            .iter()
            .map(|(entity_type, state)| (entity_type.clone(), state.records.clone()))
            .collect()
    }

    /// Merge process-wide family maps (session / session_type) into a projection store.
    pub fn projection_store_with_process_wide(
        &self,
        process_wide: &EntityFamilyStore,
    ) -> EntityFamilyStore {
        let mut store = self.as_family_store();
        for (family, records) in process_wide {
            store.insert(family.clone(), records.clone());
        }
        store
    }
}

fn entity_type_of(frame: &DaemonEntityFrame) -> &str {
    match frame {
        DaemonEntityFrame::Snapshot { entity_type, .. }
        | DaemonEntityFrame::Upsert { entity_type, .. }
        | DaemonEntityFrame::Patch { entity_type, .. }
        | DaemonEntityFrame::Remove { entity_type, .. }
        | DaemonEntityFrame::Error { entity_type, .. } => entity_type,
    }
}

fn daemon_item_to_record(item: Value) -> Result<EntityRecordItem, String> {
    let id = item
        .get("id")
        .or_else(|| item.get("session_uuid"))
        .or_else(|| item.get("session_type_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| "entity options snapshot item missing id".to_string())?
        .to_string();
    let fields = value_to_fields(item, &id)?;
    Ok(EntityRecordItem { id, fields })
}

fn value_to_fields(value: Value, id: &str) -> Result<Map<String, Value>, String> {
    let mut fields = match value {
        Value::Object(map) => map,
        other => {
            return Err(format!(
                "entity options record fields must be an object, got {other}"
            ));
        }
    };
    if !fields.contains_key("id") {
        fields.insert("id".to_string(), Value::String(id.to_string()));
    }
    Ok(fields)
}

/// Compact TUI label: ordered present display fields joined with a middle dot.
pub fn compact_entity_option_label(option: &EntityOption, display_fields: &[String]) -> String {
    let mut parts = Vec::new();
    for field in display_fields {
        if let Some(value) = option.metadata.get(field)
            && !value.is_empty()
        {
            parts.push(value.as_str());
        }
    }
    if parts.is_empty() {
        if option.label.is_empty() {
            option.value.clone()
        } else {
            option.label.clone()
        }
    } else {
        parts.join(" · ")
    }
}

const SELECTION_INVALID_ERROR: &str = "Selected value is no longer available";

/// Realize every entity-backed Select under `root` into static options slots.
/// Clears invalid drafts and stamps a visible field error when selection is invalid.
pub fn materialize_entity_options_selects(
    root: &mut UiNode,
    store: &EntityFamilyStore,
    drafts: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    materialize_entity_options_node(root, store, drafts)
}

fn materialize_entity_options_node(
    node: &mut UiNode,
    store: &EntityFamilyStore,
    drafts: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    if node.kind == UiNodeKind::Select
        && let Some(source_value) = node.props.get("options_source").cloned()
    {
        realize_entity_options_select(node, source_value, store, drafts)?;
    }
    for child in &mut node.children {
        materialize_entity_options_child(child, store, drafts)?;
    }
    for children in node.slots.values_mut() {
        for child in children {
            materialize_entity_options_child(child, store, drafts)?;
        }
    }
    Ok(())
}

fn materialize_entity_options_child(
    child: &mut UiChild,
    store: &EntityFamilyStore,
    drafts: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    match child {
        UiChild::Node(node) => materialize_entity_options_node(node, store, drafts),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => {
            materialize_entity_options_node(node, store, drafts)
        }
        UiChild::BindList(UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            materialize_entity_options_node(item_template, store, drafts)?;
            if let Some(template) = empty_template {
                materialize_entity_options_node(template, store, drafts)?;
            }
            Ok(())
        }
        UiChild::BindIf(UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(UiBindIf::PresentationIf { node, .. }) => {
            materialize_entity_options_node(node, store, drafts)
        }
    }
}

fn realize_entity_options_select(
    node: &mut UiNode,
    source_value: Value,
    store: &EntityFamilyStore,
    drafts: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    let descriptor: UiEntityOptionsSource = serde_json::from_value(source_value)
        .map_err(|error| format!("options_source deserialize failed: {error}"))?;
    let field_name = node
        .props
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let selection = drafts
        .get(&field_name)
        .and_then(Value::as_str)
        .or_else(|| node.props.get("selected").and_then(Value::as_str))
        .map(str::to_string);
    let projection = project_entity_options_from_store(&descriptor, store, selection.as_deref());

    // Realized Select requires exactly one of: non-empty `options` slot, or
    // `options_source`. Empty projections therefore keep the producer prop.
    if projection.options.is_empty() {
        node.slots.remove("options");
        if !projection.selection_valid {
            if !field_name.is_empty() {
                drafts.remove(&field_name);
            }
            node.props.remove("selected");
            node.props.insert(
                "error".to_string(),
                Value::String(SELECTION_INVALID_ERROR.to_string()),
            );
        }
        return Ok(());
    }

    node.props.remove("options_source");
    if !projection.selection_valid {
        if !field_name.is_empty() {
            drafts.remove(&field_name);
        }
        node.props.remove("selected");
        node.props.insert(
            "error".to_string(),
            Value::String(SELECTION_INVALID_ERROR.to_string()),
        );
    } else if let Some(selected) = selection {
        node.props
            .insert("selected".to_string(), Value::String(selected));
        // Clear stale invalidation error if present.
        if node.props.get("error").and_then(Value::as_str) == Some(SELECTION_INVALID_ERROR) {
            node.props.remove("error");
        }
    }

    let base_id = node
        .id
        .as_ref()
        .and_then(UiAuthoredNodeId::as_literal)
        .map(|id| id.0.clone())
        .unwrap_or_else(|| "entity-options-select".to_string());

    let options: Vec<UiChild> = projection
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let label = compact_entity_option_label(option, &descriptor.display_fields);
            let option_id = format!("{base_id}-option-{index}");
            UiChild::Node(Box::new(UiNode {
                kind: UiNodeKind::SelectOption,
                id: Some(UiAuthoredNodeId::Literal(UiNodeId(option_id))),
                props: [
                    ("value".to_string(), Value::String(option.value.clone())),
                    ("label".to_string(), Value::String(label)),
                ]
                .into_iter()
                .collect(),
                children: Vec::new(),
                slots: BTreeMap::new(),
            }))
        })
        .collect();
    node.slots.insert("options".to_string(), options);
    Ok(())
}

/// Families demanded by a plugin surface body for options_source producers.
pub fn demanded_entity_option_families(body: &UiNode) -> BTreeSet<String> {
    collect_entity_option_families(body).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use botster_ui_contract::{
        EntityOptionsProjection, UiEntityOptionsKind, apply_entity_options_frames,
        conformance_fixtures_json, project_entity_options_from_store,
    };
    use serde_json::json;

    fn sample_descriptor() -> UiEntityOptionsSource {
        UiEntityOptionsSource {
            kind: UiEntityOptionsKind::EntityOptions,
            source: "/session".to_string(),
            value_field: "session_uuid".to_string(),
            display_fields: vec![
                "label".to_string(),
                "lifecycle_class".to_string(),
                "session_type".to_string(),
                "spawn_point".to_string(),
            ],
            order: vec!["label".to_string(), "session_uuid".to_string()],
            r#where: BTreeMap::from([(
                "lifecycle_class".to_string(),
                Value::String("current".to_string()),
            )]),
            exclude: None,
        }
    }

    #[test]
    fn generation_rejects_stale_subscription_and_out_of_order_seq() {
        let mut store = EntityOptionsStore::default();
        store.begin_generation("session", "gen-1".to_string());
        assert!(
            store
                .apply_daemon_frame(DaemonEntityFrame::Snapshot {
                    subscription_id: "gen-1".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 1,
                    items: vec![json!({
                        "id": "sess-a",
                        "session_uuid": "sess-a",
                        "label": "A",
                        "lifecycle_class": "current"
                    })],
                    resync_reason: None,
                })
                .expect("snapshot ok")
        );
        assert!(
            !store
                .apply_daemon_frame(DaemonEntityFrame::Upsert {
                    subscription_id: "stale-gen".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 99,
                    id: "sess-b".to_string(),
                    entity: json!({
                        "id": "sess-b",
                        "session_uuid": "sess-b",
                        "label": "B",
                        "lifecycle_class": "current"
                    }),
                })
                .expect("stale rejected as non-mutate")
        );
        assert!(
            !store
                .apply_daemon_frame(DaemonEntityFrame::Upsert {
                    subscription_id: "gen-1".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 1,
                    id: "sess-c".to_string(),
                    entity: json!({
                        "id": "sess-c",
                        "session_uuid": "sess-c",
                        "label": "C",
                        "lifecycle_class": "current"
                    }),
                })
                .expect("non-increasing seq rejected")
        );
        store.begin_generation("session", "gen-2".to_string());
        assert!(
            !store
                .apply_daemon_frame(DaemonEntityFrame::Upsert {
                    subscription_id: "gen-1".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 2,
                    id: "sess-d".to_string(),
                    entity: json!({
                        "id": "sess-d",
                        "session_uuid": "sess-d",
                        "label": "D",
                        "lifecycle_class": "current"
                    }),
                })
                .expect("old generation ignored after begin_generation")
        );
        let pre_snapshot = store
            .apply_daemon_frame(DaemonEntityFrame::Upsert {
                subscription_id: "gen-2".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 1,
                id: "sess-e".to_string(),
                entity: json!({
                    "id": "sess-e",
                    "session_uuid": "sess-e",
                    "label": "E",
                    "lifecycle_class": "current"
                }),
            })
            .expect_err("delta before snapshot requires recovery");
        assert!(
            pre_snapshot.contains("gap recovery") || pre_snapshot.contains("before first"),
            "{pre_snapshot}"
        );
        assert!(store.family("session").unwrap().records.is_empty());

        // Snapshot then sequence hole (1 → 3) requires recovery, not silent apply.
        assert!(
            store
                .apply_daemon_frame(DaemonEntityFrame::Snapshot {
                    subscription_id: "gen-2".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 1,
                    items: vec![json!({
                        "id": "sess-a",
                        "session_uuid": "sess-a",
                        "label": "A",
                        "lifecycle_class": "current"
                    })],
                    resync_reason: None,
                })
                .expect("snapshot ok")
        );
        let gap = store
            .apply_daemon_frame(DaemonEntityFrame::Upsert {
                subscription_id: "gen-2".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 3,
                id: "sess-gap".to_string(),
                entity: json!({
                    "id": "sess-gap",
                    "session_uuid": "sess-gap",
                    "label": "Gap",
                    "lifecycle_class": "current"
                }),
            })
            .expect_err("sequence hole requires recovery");
        assert!(gap.contains("sequence gap"), "{gap}");
        assert!(
            !store
                .family("session")
                .unwrap()
                .records
                .contains_key("sess-gap")
        );

        // Production recovery: begin_generation + authoritative snapshot replaces state.
        store.begin_generation("session", "gen-3".to_string());
        assert!(
            store
                .apply_daemon_frame(DaemonEntityFrame::Snapshot {
                    subscription_id: "gen-3".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 1,
                    items: vec![json!({
                        "id": "sess-recovered",
                        "session_uuid": "sess-recovered",
                        "label": "Recovered",
                        "lifecycle_class": "current"
                    })],
                    resync_reason: None,
                })
                .expect("recovery snapshot applies")
        );
        assert!(
            store
                .family("session")
                .unwrap()
                .records
                .contains_key("sess-recovered")
        );
        assert_eq!(
            store.family("session").unwrap().subscription_id.as_deref(),
            Some("gen-3")
        );
    }

    #[test]
    fn shared_fixture_timeline_matches_contract_projector() {
        let conformance = conformance_fixtures_json();
        let fixture = &conformance["entity_options_reactive_timeline"];
        let descriptor: UiEntityOptionsSource =
            serde_json::from_value(fixture["descriptor"].clone()).expect("descriptor");
        let selection = fixture["selection"].as_str();
        let timeline = fixture["timeline"].as_array().expect("timeline");

        let mut pure_store = EntityFamilyStore::new();
        let mut tui_store = EntityOptionsStore::default();

        for step in timeline {
            let frames: Vec<EntityOptionsFrame> =
                serde_json::from_value(step["frames"].clone()).expect("frames");
            for frame in &frames {
                let entity_type = match frame {
                    EntityOptionsFrame::Snapshot { entity_type, .. }
                    | EntityOptionsFrame::Upsert { entity_type, .. }
                    | EntityOptionsFrame::Patch { entity_type, .. }
                    | EntityOptionsFrame::Remove { entity_type, .. } => entity_type.as_str(),
                };
                if tui_store.family(entity_type).is_none() {
                    tui_store.seed_family_for_fixture(entity_type, "fixture");
                }
                apply_entity_options_frame(&mut pure_store, frame);
                tui_store.apply_contract_frame(frame);
            }
            let expected: EntityOptionsProjection =
                serde_json::from_value(step["expected_projection"].clone()).expect("expected");
            let pure = project_entity_options_from_store(&descriptor, &pure_store, selection);
            let via_tui = project_entity_options_from_store(
                &descriptor,
                &tui_store.as_family_store(),
                selection,
            );
            assert_eq!(pure, expected, "pure projector at {}", step["name"]);
            assert_eq!(via_tui, expected, "tui store at {}", step["name"]);
            assert_eq!(
                pure_store,
                tui_store.as_family_store(),
                "store parity at {}",
                step["name"]
            );
        }
        let _ = apply_entity_options_frames; // keep import used if timeline empty in future
        let _ = sample_descriptor;
    }

    #[test]
    fn materialize_builds_options_and_invalidates_selection() {
        let mut store = EntityOptionsStore::default();
        store.seed_family_for_fixture("session", "fixture");
        store.apply_contract_frame(&EntityOptionsFrame::Snapshot {
            entity_type: "session".to_string(),
            snapshot_seq: 1,
            items: vec![EntityRecordItem {
                id: "sess-alpha".to_string(),
                fields: [
                    ("id".to_string(), json!("sess-alpha")),
                    ("session_uuid".to_string(), json!("sess-alpha")),
                    ("label".to_string(), json!("Alpha")),
                    ("lifecycle_class".to_string(), json!("current")),
                    ("session_type".to_string(), json!("agent")),
                    ("spawn_point".to_string(), json!("local")),
                ]
                .into_iter()
                .collect(),
            }],
            resync_reason: None,
        });

        let mut root = UiNode {
            kind: UiNodeKind::Select,
            id: Some(UiAuthoredNodeId::Literal(UiNodeId(
                "session-select".to_string(),
            ))),
            props: [
                ("name".to_string(), json!("session")),
                ("label".to_string(), json!("Session")),
                (
                    "options_source".to_string(),
                    serde_json::to_value(sample_descriptor()).unwrap(),
                ),
            ]
            .into_iter()
            .collect(),
            children: Vec::new(),
            slots: BTreeMap::new(),
        };
        let mut drafts = BTreeMap::from([("session".to_string(), json!("sess-alpha"))]);
        materialize_entity_options_selects(&mut root, &store.as_family_store(), &mut drafts)
            .expect("materialize");
        assert!(!root.props.contains_key("options_source"));
        let options = root.slots.get("options").expect("options slot");
        assert_eq!(options.len(), 1);
        match &options[0] {
            UiChild::Node(option) => {
                assert_eq!(option.props.get("value"), Some(&json!("sess-alpha")));
                let label = option.props.get("label").and_then(Value::as_str).unwrap();
                assert!(label.contains("Alpha"), "{label}");
                assert!(label.contains("current"), "{label}");
            }
            other => panic!("expected option node, got {other:?}"),
        }
        assert_eq!(drafts.get("session"), Some(&json!("sess-alpha")));

        // Invalidate selection by removing the only option.
        store.apply_contract_frame(&EntityOptionsFrame::Remove {
            entity_type: "session".to_string(),
            id: "sess-alpha".to_string(),
            seq: 2,
        });
        let mut root = UiNode {
            kind: UiNodeKind::Select,
            id: Some(UiAuthoredNodeId::Literal(UiNodeId(
                "session-select".to_string(),
            ))),
            props: [
                ("name".to_string(), json!("session")),
                ("label".to_string(), json!("Session")),
                (
                    "options_source".to_string(),
                    serde_json::to_value(sample_descriptor()).unwrap(),
                ),
            ]
            .into_iter()
            .collect(),
            children: Vec::new(),
            slots: BTreeMap::new(),
        };
        materialize_entity_options_selects(&mut root, &store.as_family_store(), &mut drafts)
            .expect("materialize invalid");
        assert!(!drafts.contains_key("session"));
        assert_eq!(
            root.props.get("error").and_then(Value::as_str),
            Some(SELECTION_INVALID_ERROR)
        );
    }

    #[test]
    fn materialize_session_source_empty_store_keeps_producer_prop() {
        let mut root = UiNode {
            kind: UiNodeKind::Select,
            id: Some(UiAuthoredNodeId::Literal(UiNodeId(
                "entity-options-select".into(),
            ))),
            props: [
                ("name".into(), json!("option")),
                ("label".into(), json!("Option")),
                (
                    "options_source".into(),
                    json!({
                        "$kind": "entity_options",
                        "source": "/session",
                        "value_field": "session_uuid",
                        "display_fields": ["lifecycle_class"],
                        "order": ["session_uuid"],
                        "where": { "lifecycle_class": "current" }
                    }),
                ),
            ]
            .into_iter()
            .collect(),
            children: Vec::new(),
            slots: BTreeMap::new(),
        };
        let mut drafts = BTreeMap::new();
        materialize_entity_options_selects(&mut root, &EntityFamilyStore::new(), &mut drafts)
            .expect("materialize empty store");
        // Empty projection keeps options_source so realized xor validation passes.
        assert!(root.props.contains_key("options_source"));
        assert!(!root.slots.contains_key("options"));
        root.validate_realized()
            .expect("empty entity-options select keeps producer prop for realized xor");
    }
}

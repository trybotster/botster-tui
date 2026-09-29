use super::*;

pub(super) fn plugin_surface_body_node(surface: &DaemonPluginSurface) -> Result<UiNode, String> {
    // Authored validation owns binding context and descendant-key diagnostics.
    // Renderer capabilities still inspect only concrete trees because bound prop
    // sentinels are materialized in plugin_surface_render_root.
    let body = &surface.ui_tree_snapshot.body;
    body.validate().map_err(|error| {
        format!(
            "plugin surface {}:{} failed UiNode validate: {error}",
            surface.package_name, surface.surface_id
        )
    })?;
    if !node_requires_binding_materialization(body) {
        renderer::tui_capabilities()
            .validate_node(body)
            .map_err(|error| {
                format!(
                    "plugin surface {}:{} unsupported TUI primitive: {error}",
                    surface.package_name, surface.surface_id
                )
            })?;
    }
    Ok(body.clone())
}

pub(super) fn normalize_plugin_surface(
    surface: DaemonPluginSurface,
) -> Result<DaemonPluginSurface, String> {
    let snapshot = &surface.ui_tree_snapshot;
    if snapshot.package_name != surface.package_name || snapshot.surface_id != surface.surface_id {
        return Err(format!(
            "plugin surface {}:{} ui_tree_snapshot identity mismatch",
            surface.package_name, surface.surface_id
        ));
    }
    Ok(surface)
}

pub(super) fn materialize_plugin_surface(
    root: &UiNode,
    session_entities: &SessionEntityState,
    entity_options_store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid_entity_option_fields: &BTreeSet<String>,
) -> Result<UiNode, String> {
    let mut materialized = if node_requires_binding_materialization(root) {
        let rows = session_entities.binding_rows()?;
        materialize_binding_node(root, &rows, None, None, false)?
    } else {
        root.clone()
    };
    // Realize entity-backed selects before kit validation / hit-map render.
    let mut draft_view = drafts.clone();
    materialize_entity_options_selects(&mut materialized, entity_options_store, &mut draft_view)?;
    // Re-apply invalid-field errors for fields already cleared by reconcile.
    stamp_entity_option_invalid_errors(&mut materialized, invalid_entity_option_fields);
    reject_duplicate_realized_node_ids(&materialized)?;
    Ok(materialized)
}

pub(super) fn stamp_entity_option_invalid_errors(
    node: &mut UiNode,
    invalid_fields: &BTreeSet<String>,
) {
    if node.kind == UiNodeKind::Select
        && let Some(name) = node.props.get("name").and_then(Value::as_str)
        && invalid_fields.contains(name)
        && !node.props.contains_key("error")
    {
        node.props.insert(
            "error".to_string(),
            Value::String("Selected value is no longer available".to_string()),
        );
    }
    for child in &mut node.children {
        stamp_entity_option_invalid_errors_child(child, invalid_fields);
    }
    for children in node.slots.values_mut() {
        for child in children {
            stamp_entity_option_invalid_errors_child(child, invalid_fields);
        }
    }
}

pub(super) fn stamp_entity_option_invalid_errors_child(
    child: &mut UiChild,
    invalid_fields: &BTreeSet<String>,
) {
    match child {
        UiChild::Node(node)
        | UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. }) => {
            stamp_entity_option_invalid_errors(node, invalid_fields);
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            stamp_entity_option_invalid_errors(item_template, invalid_fields);
            if let Some(template) = empty_template {
                stamp_entity_option_invalid_errors(template, invalid_fields);
            }
        }
    }
}

pub(super) fn collect_invalid_entity_option_fields(
    node: &UiNode,
    store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid: &mut BTreeSet<String>,
) {
    if node.kind == UiNodeKind::Select
        && let Some(source) = node.props.get("options_source")
        && let Ok(descriptor) =
            serde_json::from_value::<botster_ui_contract::UiEntityOptionsSource>(source.clone())
    {
        let field_name = node
            .props
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !field_name.is_empty() {
            let selection = drafts.get(field_name).and_then(Value::as_str);
            let projection = botster_ui_contract::project_entity_options_from_store(
                &descriptor,
                store,
                selection,
            );
            if !projection.selection_valid {
                invalid.insert(field_name.to_string());
            }
        }
    }
    for child in &node.children {
        collect_invalid_entity_option_fields_child(child, store, drafts, invalid);
    }
    for children in node.slots.values() {
        for child in children {
            collect_invalid_entity_option_fields_child(child, store, drafts, invalid);
        }
    }
}

pub(super) fn collect_invalid_entity_option_fields_child(
    child: &UiChild,
    store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid: &mut BTreeSet<String>,
) {
    match child {
        UiChild::Node(node)
        | UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. }) => {
            collect_invalid_entity_option_fields(node, store, drafts, invalid);
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            collect_invalid_entity_option_fields(item_template, store, drafts, invalid);
            if let Some(template) = empty_template {
                collect_invalid_entity_option_fields(template, store, drafts, invalid);
            }
        }
    }
}

pub(super) fn node_requires_binding_materialization(node: &UiNode) -> bool {
    matches!(
        node.id,
        Some(UiAuthoredNodeId::Bind(_) | UiAuthoredNodeId::BindListDescendant(_))
    ) || node.props.values().any(value_contains_binding)
        || node
            .children
            .iter()
            .chain(node.slots.values().flatten())
            .any(child_requires_binding_materialization)
}

pub(super) fn child_requires_binding_materialization(child: &UiChild) -> bool {
    match child {
        UiChild::Node(node)
        | UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
            node_requires_binding_materialization(node)
        }
        UiChild::BindList(_) | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { .. }) => {
            true
        }
    }
}

pub(super) fn value_contains_binding(value: &Value) -> bool {
    match value {
        Value::Object(values) => {
            (values.len() == 1 && values.get("$bind").and_then(Value::as_str).is_some())
                || values.values().any(value_contains_binding)
        }
        Value::Array(values) => values.iter().any(value_contains_binding),
        _ => false,
    }
}

pub(super) fn materialize_binding_node(
    source: &UiNode,
    session_rows: &[Value],
    item: Option<&Value>,
    row_id: Option<&UiNodeId>,
    bound_id_allowed: bool,
) -> Result<UiNode, String> {
    let mut node = source.clone();
    let mut descendant_row_id = row_id.cloned();
    node.id = match source.id.as_ref() {
        None => None,
        Some(UiAuthoredNodeId::Literal(id)) => Some(UiAuthoredNodeId::Literal(id.clone())),
        Some(UiAuthoredNodeId::Bind(binding)) => {
            if !bound_id_allowed {
                return Err(
                    "bound node id is only supported on a direct BindList item template root"
                        .to_string(),
                );
            }
            let value = resolve_item_binding(&binding.path, item)?;
            let id = value
                .as_str()
                .ok_or_else(|| "bound node id did not resolve to a string".to_string())?;
            if id.trim().is_empty() {
                return Err("bound node id resolved to a blank string".to_string());
            }
            let id = UiNodeId(id.to_string());
            descendant_row_id = Some(id.clone());
            Some(UiAuthoredNodeId::Literal(id))
        }
        Some(UiAuthoredNodeId::BindListDescendant(descendant_id)) => {
            let row_id = descendant_row_id.as_ref().ok_or_else(|| {
                "bound list descendant id requires a realized item template root id".to_string()
            })?;
            let id = realize_bind_list_descendant_id(&row_id.0, descendant_id.key())
                .map_err(|error| format!("bound list descendant id failed: {error}"))?;
            Some(UiAuthoredNodeId::Literal(id))
        }
    };
    for value in node.props.values_mut() {
        *value = materialize_binding_value(value, item)?;
    }
    node.children = materialize_binding_children(
        &source.children,
        session_rows,
        item,
        descendant_row_id.as_ref(),
    )?;
    node.slots = source
        .slots
        .iter()
        .map(|(name, children)| {
            materialize_binding_children(children, session_rows, item, descendant_row_id.as_ref())
                .map(|children| (name.clone(), children))
        })
        .collect::<Result<_, _>>()?;
    Ok(node)
}

pub(super) fn materialize_binding_children(
    children: &[UiChild],
    session_rows: &[Value],
    item: Option<&Value>,
    row_id: Option<&UiNodeId>,
) -> Result<Vec<UiChild>, String> {
    let mut materialized = Vec::new();
    for child in children {
        match child {
            UiChild::Node(node) => materialized.push(UiChild::Node(Box::new(
                materialize_binding_node(node, session_rows, item, row_id, false)?,
            ))),
            UiChild::Conditional(UiConditional::When { condition, node }) => {
                materialized.push(UiChild::Conditional(UiConditional::When {
                    condition: condition.clone(),
                    node: Box::new(materialize_binding_node(
                        node,
                        session_rows,
                        item,
                        row_id,
                        false,
                    )?),
                }));
            }
            UiChild::Conditional(UiConditional::Hidden { condition, node }) => {
                materialized.push(UiChild::Conditional(UiConditional::Hidden {
                    condition: condition.clone(),
                    node: Box::new(materialize_binding_node(
                        node,
                        session_rows,
                        item,
                        row_id,
                        false,
                    )?),
                }));
            }
            UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { predicate, node }) => {
                materialized.push(UiChild::BindIf(
                    botster_ui_contract::UiBindIf::PresentationIf {
                        predicate: predicate.clone(),
                        node: Box::new(materialize_binding_node(
                            node,
                            session_rows,
                            item,
                            row_id,
                            false,
                        )?),
                    },
                ));
            }
            UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { path, node }) => {
                let value = resolve_item_binding(path, item)?;
                if binding_truthy(value) {
                    materialized.push(UiChild::Node(Box::new(materialize_binding_node(
                        node,
                        session_rows,
                        item,
                        row_id,
                        false,
                    )?)));
                }
            }
            UiChild::BindList(botster_ui_contract::UiBindList::BindList {
                source,
                r#where,
                item_template,
                empty_template,
            }) => {
                if source != "/session" {
                    return Err(format!("unsupported binding source {source:?}"));
                }
                let reference = session_binding_reference_row();
                for field in r#where.keys() {
                    if !reference.contains_key(field) {
                        return Err(format!(
                            "unsupported /session where field {field:?}; the entity was not treated as unavailable"
                        ));
                    }
                }
                let matching = session_rows
                    .iter()
                    .filter(|row| {
                        r#where
                            .iter()
                            .all(|(field, expected)| row.get(field) == Some(expected))
                    })
                    .collect::<Vec<_>>();
                if matching.is_empty() {
                    if let Some(empty_template) = empty_template {
                        materialized.push(UiChild::Node(Box::new(materialize_binding_node(
                            empty_template,
                            session_rows,
                            None,
                            None,
                            false,
                        )?)));
                    }
                } else {
                    for row in matching {
                        materialized.push(UiChild::Node(Box::new(materialize_binding_node(
                            item_template,
                            session_rows,
                            Some(row),
                            None,
                            true,
                        )?)));
                    }
                }
            }
        }
    }
    Ok(materialized)
}

pub(super) fn reject_duplicate_realized_node_ids(root: &UiNode) -> Result<(), String> {
    collect_realized_node_ids(root).map(|_| ())
}

pub(super) enum RealizedChildCondition {
    When(UiCondition),
    Hidden(UiCondition),
    Presentation(botster_ui_contract::UiPresentationPredicate),
}

pub(super) fn collect_realized_node_ids(
    node: &UiNode,
) -> Result<std::collections::BTreeSet<String>, String> {
    let mut realized = std::collections::BTreeSet::new();
    if let Some(UiAuthoredNodeId::Literal(id)) = &node.id {
        realized.insert(id.0.clone());
    }

    let mut children = Vec::new();
    for child in node.children.iter().chain(node.slots.values().flatten()) {
        let (ids, condition) = collect_realized_child_ids(child)?;
        reject_realized_node_id_overlap(&realized, &ids)?;
        children.push((ids, condition));
    }
    for (index, (left_ids, left_condition)) in children.iter().enumerate() {
        for (right_ids, right_condition) in children.iter().skip(index + 1) {
            if !realized_children_are_exclusive(left_condition, right_condition) {
                reject_realized_node_id_overlap(left_ids, right_ids)?;
            }
        }
    }
    for (ids, _) in children {
        realized.extend(ids);
    }
    Ok(realized)
}

pub(super) fn collect_realized_child_ids(
    child: &UiChild,
) -> Result<
    (
        std::collections::BTreeSet<String>,
        Option<RealizedChildCondition>,
    ),
    String,
> {
    match child {
        UiChild::Node(node)
        | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. }) => {
            collect_realized_node_ids(node).map(|ids| (ids, None))
        }
        UiChild::Conditional(UiConditional::When { condition, node }) => {
            collect_realized_node_ids(node)
                .map(|ids| (ids, Some(RealizedChildCondition::When(condition.clone()))))
        }
        UiChild::Conditional(UiConditional::Hidden { condition, node }) => {
            collect_realized_node_ids(node)
                .map(|ids| (ids, Some(RealizedChildCondition::Hidden(condition.clone()))))
        }
        UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { predicate, node }) => {
            collect_realized_node_ids(node).map(|ids| {
                (
                    ids,
                    Some(RealizedChildCondition::Presentation(predicate.clone())),
                )
            })
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            let mut ids = collect_realized_node_ids(item_template)?;
            if let Some(empty_template) = empty_template {
                ids.extend(collect_realized_node_ids(empty_template)?);
            }
            Ok((ids, None))
        }
    }
}

pub(super) fn realized_children_are_exclusive(
    left: &Option<RealizedChildCondition>,
    right: &Option<RealizedChildCondition>,
) -> bool {
    match (left, right) {
        (Some(RealizedChildCondition::When(left)), Some(RealizedChildCondition::When(right))) => {
            conditions_are_distinct_on_one_axis(left, right)
        }
        (Some(RealizedChildCondition::When(left)), Some(RealizedChildCondition::Hidden(right)))
        | (Some(RealizedChildCondition::Hidden(left)), Some(RealizedChildCondition::When(right))) => {
            left == right
        }
        (
            Some(RealizedChildCondition::Presentation(left)),
            Some(RealizedChildCondition::Presentation(right)),
        ) => presentation_predicates_are_exclusive(left, right),
        _ => false,
    }
}

pub(super) fn presentation_predicates_are_exclusive(
    left: &botster_ui_contract::UiPresentationPredicate,
    right: &botster_ui_contract::UiPresentationPredicate,
) -> bool {
    match (left, right) {
        (
            botster_ui_contract::UiPresentationPredicate::Equals {
                key: left_key,
                value: left_value,
            },
            botster_ui_contract::UiPresentationPredicate::Equals {
                key: right_key,
                value: right_value,
            },
        ) => left_key == right_key && left_value != right_value,
        _ => false,
    }
}

pub(super) fn conditions_are_distinct_on_one_axis(left: &UiCondition, right: &UiCondition) -> bool {
    condition_axis_count(left) == 1
        && condition_axis_count(right) == 1
        && ((left.width.is_some() && right.width.is_some() && left.width != right.width)
            || (left.height.is_some() && right.height.is_some() && left.height != right.height)
            || (left.pointer.is_some() && right.pointer.is_some() && left.pointer != right.pointer)
            || (left.orientation.is_some()
                && right.orientation.is_some()
                && left.orientation != right.orientation)
            || (left.keyboard_occluded.is_some()
                && right.keyboard_occluded.is_some()
                && left.keyboard_occluded != right.keyboard_occluded))
}

pub(super) fn condition_axis_count(condition: &UiCondition) -> usize {
    usize::from(condition.width.is_some())
        + usize::from(condition.height.is_some())
        + usize::from(condition.pointer.is_some())
        + usize::from(condition.orientation.is_some())
        + usize::from(condition.keyboard_occluded.is_some())
}

pub(super) fn reject_realized_node_id_overlap(
    left: &std::collections::BTreeSet<String>,
    right: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if let Some(id) = left.intersection(right).next() {
        return Err(format!("duplicate materialized node id {id:?}"));
    }
    Ok(())
}

pub(super) fn materialize_binding_value(
    value: &Value,
    item: Option<&Value>,
) -> Result<Value, String> {
    match value {
        Value::Object(values)
            if values.len() == 1 && values.get("$bind").and_then(Value::as_str).is_some() =>
        {
            let path = values
                .get("$bind")
                .and_then(Value::as_str)
                .expect("guarded binding path");
            Ok(resolve_item_binding(path, item)?.clone())
        }
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                materialize_binding_value(value, item).map(|value| (key.clone(), value))
            })
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        Value::Array(values) => values
            .iter()
            .map(|value| materialize_binding_value(value, item))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Ok(value.clone()),
    }
}

pub(super) fn resolve_item_binding<'a>(
    path: &str,
    item: Option<&'a Value>,
) -> Result<&'a Value, String> {
    let relative = path.strip_prefix("@/").ok_or_else(|| {
        if path.starts_with('/') {
            format!("unsupported absolute binding path {path:?}")
        } else {
            format!("unsupported binding path {path:?}")
        }
    })?;
    let item = item.ok_or_else(|| format!("item-relative binding {path:?} has no current row"))?;
    item.pointer(&format!("/{relative}"))
        .ok_or_else(|| format!("binding path {path:?} is missing from the current session row"))
}

pub(super) fn binding_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

pub(super) fn iframe_unsupported_diagnostic(surface: &DaemonPluginSurface) -> Option<String> {
    let iframe = find_iframe_node(&surface.ui_tree_snapshot.body)?;
    let title = iframe
        .props
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("untitled");
    let src = iframe
        .props
        .get("src")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let sandbox = iframe
        .props
        .get("sandbox")
        .map(compact_json)
        .unwrap_or_else(|| "default".to_string());
    Some(format!(
        "plugin surface iframe unsupported: package={} surface={} title={} src={} sandbox={} open=copy URL or open it in a browser",
        surface.package_name, surface.surface_id, title, src, sandbox
    ))
}

pub(super) fn find_iframe_node(node: &UiNode) -> Option<&UiNode> {
    if node.kind == UiNodeKind::Iframe {
        return Some(node);
    }
    node.children
        .iter()
        .chain(node.slots.values().flatten())
        .find_map(find_iframe_child)
}

pub(super) fn find_iframe_child(child: &UiChild) -> Option<&UiNode> {
    match child {
        UiChild::Node(node) => find_iframe_node(node),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => find_iframe_node(node),
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => find_iframe_node(item_template)
            .or_else(|| empty_template.as_deref().and_then(find_iframe_node)),
        UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
            find_iframe_node(node)
        }
    }
}

pub(super) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

pub(super) fn plugin_action_result_text(result: &UiActionResult) -> String {
    let mut parts = vec![
        format!("state={:?}", result.state),
        format!("request_id={}", result.request_id.0),
    ];
    if !result.form_errors.is_empty() {
        parts.push(format!("form_errors={}", result.form_errors.join(" | ")));
    }
    if let Some(error) = &result.error {
        parts.push(format!("error={error}"));
    }
    parts.join(" ")
}

pub(super) fn plugin_surface_render_root(
    surface: &DaemonPluginSurface,
    result: Option<&UiActionResult>,
    session_entities: &SessionEntityState,
    entity_options_store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid_entity_option_fields: &BTreeSet<String>,
) -> UiNode {
    if let Some(diagnostic) = iframe_unsupported_diagnostic(surface) {
        return node(
            UiNodeKind::Text,
            "tui-plugin-surface-iframe-unsupported",
            json!({ "text": diagnostic }),
        );
    }
    let root = match plugin_surface_body_node(surface) {
        Ok(root) => root,
        Err(error) => {
            return node(
                UiNodeKind::Text,
                "tui-plugin-surface-invalid",
                json!({ "text": format!("plugin surface render: {error}") }),
            );
        }
    };
    let mut root = match materialize_plugin_surface(
        &root,
        session_entities,
        entity_options_store,
        drafts,
        invalid_entity_option_fields,
    ) {
        Ok(root) => root,
        Err(error) => {
            return node(
                UiNodeKind::Text,
                "tui-plugin-surface-binding-invalid",
                json!({ "text": format!("plugin surface binding: {error}") }),
            );
        }
    };
    if let Some(result) = result {
        apply_plugin_result_errors(&mut root, result);
    }
    validated_materialized_plugin_surface_node(surface, root)
}

pub(super) fn validated_materialized_plugin_surface_node(
    surface: &DaemonPluginSurface,
    root: UiNode,
) -> UiNode {
    if let Err(error) = root.validate_realized() {
        return node(
            UiNodeKind::Text,
            "tui-plugin-surface-materialized-invalid",
            json!({
                "text": format!(
                    "plugin surface render: plugin surface {}:{} failed UiNode validate: {error}",
                    surface.package_name, surface.surface_id
                )
            }),
        );
    }
    if let Err(error) = renderer::tui_capabilities().validate_realized_node(&root) {
        return node(
            UiNodeKind::Text,
            "tui-plugin-surface-materialized-unsupported",
            json!({
                "text": format!(
                    "plugin surface render: plugin surface {}:{} unsupported TUI primitive: {error}",
                    surface.package_name, surface.surface_id
                )
            }),
        );
    }
    root
}

pub(super) fn apply_plugin_result_errors(root_node: &mut UiNode, result: &UiActionResult) {
    let field_error = root_node
        .id
        .as_ref()
        .and_then(UiAuthoredNodeId::as_literal)
        .and_then(|id| result.field_errors.get(&id.0))
        .or_else(|| {
            root_node
                .props
                .get("name")
                .and_then(Value::as_str)
                .and_then(|name| result.field_errors.get(name))
        });
    if let Some(messages) = field_error {
        root_node
            .props
            .insert("error".to_string(), Value::String(messages.join(" | ")));
    }
    if root_node.kind == UiNodeKind::Form && !result.form_errors.is_empty() {
        let form_id = root_node
            .id
            .as_ref()
            .and_then(UiAuthoredNodeId::as_literal)
            .map_or("plugin-form", |id| id.0.as_str());
        root_node.children.insert(
            0,
            child(node(
                UiNodeKind::Text,
                &format!("{form_id}-result-error"),
                json!({ "text": format!("error: {}", result.form_errors.join(" | ")) }),
            )),
        );
    }
    for child in root_node
        .children
        .iter_mut()
        .chain(root_node.slots.values_mut().flatten())
    {
        apply_plugin_result_errors_to_child(child, result);
    }
}

#[cfg(test)]
pub(super) fn static_child_node(child: &UiChild) -> Option<&UiNode> {
    match child {
        UiChild::Node(node) => Some(node),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => Some(node),
        UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => Some(node),
        UiChild::BindList(_) => None,
    }
}

pub(super) fn apply_plugin_result_errors_to_child(child: &mut UiChild, result: &UiActionResult) {
    match child {
        UiChild::Node(node) => apply_plugin_result_errors(node, result),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => {
            apply_plugin_result_errors(node, result);
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            apply_plugin_result_errors(item_template, result);
            if let Some(empty_template) = empty_template {
                apply_plugin_result_errors(empty_template, result);
            }
        }
        UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
            apply_plugin_result_errors(node, result);
        }
    }
}

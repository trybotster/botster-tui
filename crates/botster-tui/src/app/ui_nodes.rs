use super::*;

pub(super) fn node(kind: UiNodeKind, id: &str, props: Value) -> UiNode {
    UiNode {
        kind,
        id: Some(UiNodeId(id.to_string()).into()),
        props: props.as_object().cloned().unwrap_or_default(),
        children: Vec::new(),
        slots: BTreeMap::new(),
    }
}

pub(super) fn child(node: UiNode) -> UiChild {
    UiChild::Node(Box::new(node))
}

pub(super) fn responsive_child(width: UiWidthClass, node: UiNode) -> UiChild {
    UiChild::Conditional(UiConditional::When {
        condition: UiCondition {
            width: Some(width),
            ..UiCondition::default()
        },
        node: Box::new(node),
    })
}

pub(super) fn button(id: &str, label: &str, action_id: &str, payload: Value) -> UiNode {
    node(
        UiNodeKind::Button,
        id,
        json!({
            "label": label,
            "action": {
                "id": action_id,
                "payload": payload
            }
        }),
    )
}

pub(super) fn workspace_button(
    id: &str,
    label: &str,
    action_id: &str,
    payload: Value,
    toolbar_overflow: &str,
    tone: Option<&str>,
) -> UiNode {
    let mut control = button(id, label, action_id, payload);
    control.props.insert(
        "toolbar_overflow".to_string(),
        Value::String(toolbar_overflow.to_string()),
    );
    if let Some(tone) = tone {
        control
            .props
            .insert("tone".to_string(), Value::String(tone.to_string()));
    }
    control
}

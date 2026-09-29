use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PackageConfigurationField {
    pub(super) key: String,
    pub(super) field_type: String,
    pub(super) label: String,
    pub(super) required: bool,
    pub(super) order: Option<i64>,
    pub(super) options: Vec<PackageConfigurationOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PackageConfigurationOption {
    pub(super) value: String,
    pub(super) label: String,
}

pub(super) fn package_configuration_fields(
    package: &DaemonPackage,
) -> Vec<PackageConfigurationField> {
    let Some(schema) = &package.configuration.schema else {
        return Vec::new();
    };
    let Some(fields) = schema.get("fields").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut parsed = fields
        .iter()
        .filter_map(package_configuration_field)
        .collect::<Vec<_>>();
    parsed.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then_with(|| left.key.cmp(&right.key))
    });
    parsed
}

pub(super) fn package_configuration_field(value: &Value) -> Option<PackageConfigurationField> {
    let key = value.get("key").and_then(Value::as_str)?.to_string();
    let field_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unsupported")
        .to_string();
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| key.clone());
    let required = value
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or_default();
    let order = value.get("order").and_then(Value::as_i64);
    let options = value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let value = option.get("value").and_then(Value::as_str)?.to_string();
            let label = option
                .get("label")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| value.clone());
            Some(PackageConfigurationOption { value, label })
        })
        .collect();

    Some(PackageConfigurationField {
        key,
        field_type,
        label,
        required,
        order,
        options,
    })
}

pub(super) fn package_config_field_name(package_name: &str, key: &str) -> String {
    format!("{PACKAGE_CONFIG_FIELD_PREFIX}:{package_name}:{key}")
}

pub(super) fn package_configuration_field_label(field: &PackageConfigurationField) -> String {
    if field.required {
        format!("{} *", field.label)
    } else {
        field.label.clone()
    }
}

pub(super) fn package_configuration_field_error(
    package: &DaemonPackage,
    key: &str,
) -> Option<String> {
    if package
        .configuration
        .missing_required
        .iter()
        .any(|missing| missing == key)
    {
        return Some("required configuration value is missing".to_string());
    }
    None
}

pub(super) fn package_configuration_diagnostic_text(
    diagnostic: &botster_hub_client::DaemonPackageDiagnostic,
) -> String {
    format!("{}:{}", diagnostic.kind, diagnostic.message)
}

pub(super) fn package_configuration_submit_value(
    field: &PackageConfigurationField,
    draft: &Value,
) -> Option<Value> {
    match field.field_type.as_str() {
        "boolean" => Some(json!({
            "type": "boolean",
            "value": draft.as_bool().unwrap_or_default()
        })),
        "select" => Some(json!({
            "type": "select",
            "value": draft.as_str().unwrap_or_default()
        })),
        "multiline_text" => Some(json!({
            "type": "multiline_text",
            "value": draft.as_str().unwrap_or_default()
        })),
        "secret" => draft.as_bool().unwrap_or_default().then(|| {
            json!({
                "type": "secret",
                "state": "write_only"
            })
        }),
        "string" | "path" | "url" => Some(json!({
            "type": field.field_type,
            "value": draft.as_str().unwrap_or_default()
        })),
        _ => None,
    }
}

pub(super) fn configuration_value_text(value: Option<&Value>) -> String {
    value
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn configuration_value_bool(value: Option<&Value>) -> bool {
    value
        .and_then(|value| value.get("value"))
        .and_then(Value::as_bool)
        .unwrap_or_default()
}

pub(super) fn configuration_secret_state(value: Option<&Value>) -> &'static str {
    match value
        .and_then(|value| value.get("state"))
        .and_then(Value::as_str)
    {
        Some("redacted") => "redacted",
        Some("write_only") => "write-only",
        _ => "unset",
    }
}

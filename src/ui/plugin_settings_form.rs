use eframe::egui;
use serde_json::{Map, Number, Value};

use crate::api::{PluginSettingsFieldKind, PluginSettingsSchemaManifest};
use crate::config::AgentConfig;

pub struct PluginSettingsForm;

impl PluginSettingsForm {
    pub fn render(
        ui: &mut egui::Ui,
        config: &mut AgentConfig,
        plugin_id: &str,
        schema: &PluginSettingsSchemaManifest,
    ) {
        let values = plugin_settings_object_mut(config, plugin_id);

        for field in &schema.fields {
            ensure_default_value(values, field);

            ui.group(|ui| {
                ui.label(&field.title);
                render_field(ui, values, field);
                if let Some(help) = &field.help {
                    ui.label(egui::RichText::new(help).small().weak());
                }
            });
            ui.add_space(8.0);
        }
    }
}

fn render_field(
    ui: &mut egui::Ui,
    values: &mut Map<String, Value>,
    field: &crate::api::PluginSettingsFieldManifest,
) {
    match field.kind {
        PluginSettingsFieldKind::Boolean => {
            let current = values
                .get(&field.key)
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut next = current;
            if ui.checkbox(&mut next, "Enabled").changed() {
                values.insert(field.key.clone(), Value::Bool(next));
            }
        }
        PluginSettingsFieldKind::Text
        | PluginSettingsFieldKind::Path
        | PluginSettingsFieldKind::Secret => {
            let mut text = values
                .get(&field.key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if ui.text_edit_singleline(&mut text).changed() {
                values.insert(field.key.clone(), Value::String(text));
            }
        }
        PluginSettingsFieldKind::Multiline => {
            let mut text = values
                .get(&field.key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if ui.text_edit_multiline(&mut text).changed() {
                values.insert(field.key.clone(), Value::String(text));
            }
        }
        PluginSettingsFieldKind::Number => {
            let current = values
                .get(&field.key)
                .map(number_to_string)
                .unwrap_or_default();
            let mut text = current;
            if ui.text_edit_singleline(&mut text).changed() {
                if let Some(parsed) = parse_number_value(&text) {
                    values.insert(field.key.clone(), parsed);
                } else if text.trim().is_empty() {
                    values.remove(&field.key);
                }
            }
        }
        PluginSettingsFieldKind::Select => {
            let mut selected = values
                .get(&field.key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            egui::ComboBox::from_id_salt(format!("plugin_field_{}", field.key))
                .selected_text(
                    field
                        .options
                        .iter()
                        .find(|option| option.value == selected)
                        .map(|option| option.label.clone())
                        .unwrap_or_else(|| selected.clone()),
                )
                .show_ui(ui, |ui| {
                    for option in &field.options {
                        ui.selectable_value(
                            &mut selected,
                            option.value.clone(),
                            option.label.clone(),
                        );
                    }
                });

            values.insert(field.key.clone(), Value::String(selected));
        }
    }
}

fn plugin_settings_object_mut<'a>(
    config: &'a mut AgentConfig,
    plugin_id: &str,
) -> &'a mut Map<String, Value> {
    let entry = config
        .plugin_settings
        .entry(plugin_id.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("plugin settings object")
}

fn ensure_default_value(
    values: &mut Map<String, Value>,
    field: &crate::api::PluginSettingsFieldManifest,
) {
    if values.contains_key(&field.key) {
        return;
    }

    if let Some(default_value) = &field.default_value {
        values.insert(field.key.clone(), default_value.clone());
        return;
    }

    let fallback = match field.kind {
        PluginSettingsFieldKind::Boolean => Value::Bool(false),
        PluginSettingsFieldKind::Number => Value::Number(Number::from(0)),
        _ => Value::String(String::new()),
    };
    values.insert(field.key.clone(), fallback);
}

fn number_to_string(value: &Value) -> String {
    if let Some(number) = value.as_i64() {
        return number.to_string();
    }
    if let Some(number) = value.as_u64() {
        return number.to_string();
    }
    if let Some(number) = value.as_f64() {
        if number.fract() == 0.0 {
            return format!("{}", number as i64);
        }
        return number.to_string();
    }
    String::new()
}

fn parse_number_value(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('.') {
        let parsed = trimmed.parse::<f64>().ok()?;
        return Number::from_f64(parsed).map(Value::Number);
    }
    if let Ok(parsed) = trimmed.parse::<i64>() {
        return Some(Value::Number(Number::from(parsed)));
    }
    if let Ok(parsed) = trimmed.parse::<u64>() {
        return Some(Value::Number(Number::from(parsed)));
    }
    None
}

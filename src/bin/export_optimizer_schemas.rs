//! Emit optimizer DTO schemas for reviewing the bundled OpenAPI contract.
//! Usage: cargo run --bin export_optimizer_schemas > optimizer-schemas.json
use schemars::{schema_for, JsonSchema};
use serde_json::{Map, Value};

fn component_refs(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if let Some(Value::String(reference)) = fields.get_mut("$ref") {
                if let Some(name) = reference.strip_prefix("#/definitions/") {
                    *reference = format!("#/components/schemas/{name}");
                }
            }
            for child in fields.values_mut() {
                component_refs(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                component_refs(child);
            }
        }
        _ => {}
    }
}

fn add<T: JsonSchema>(name: &str, components: &mut Map<String, Value>) {
    let root = schema_for!(T);
    for (name, definition) in root.definitions {
        let mut value = serde_json::to_value(definition).unwrap();
        component_refs(&mut value);
        components.insert(name, value);
    }
    let mut value = serde_json::to_value(root.schema).unwrap();
    value.as_object_mut().unwrap().remove("title");
    component_refs(&mut value);
    components.insert(name.into(), value);
}

fn main() {
    use kobayashi::server::api::{OptimizeRequest, OptimizeResponse, OptimizeStatusResponse};
    let mut components = Map::new();
    add::<OptimizeRequest>("OptimizeRequest", &mut components);
    add::<OptimizeResponse>("OptimizeResponse", &mut components);
    add::<OptimizeStatusResponse>("OptimizeStatusResponse", &mut components);
    println!("{}", serde_json::to_string_pretty(&components).unwrap());
}

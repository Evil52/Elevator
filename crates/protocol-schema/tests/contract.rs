use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use protocol_schema::{
    SEMANTIC_ONLY, invalid_fixtures, schema_json, valid_fixtures, validate_json,
};
use serde_json::Value;

const MESSAGE_KINDS: [&str; 12] = [
    "hello",
    "car_snapshot",
    "call_floor",
    "command_result",
    "alarm_page",
    "ack_alarm",
    "inject_fault",
    "recover_car",
    "sim_control",
    "event_log_chunk",
    "read_log",
    "ping",
];

fn protocol_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/v1")
}

fn pretty(value: &Value) -> String {
    format!("{}\n", serde_json::to_string_pretty(value).unwrap())
}

fn read(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn file_names(dir: &PathBuf) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

fn schema_value() -> Value {
    serde_json::from_str(&schema_json()).unwrap_or(Value::Null)
}

fn count_maximum(value: &Value, limit: u64) -> usize {
    match value {
        Value::Object(map) => {
            let own = usize::from(map.get("maximum").and_then(Value::as_u64) == Some(limit));
            own + map.values().map(|v| count_maximum(v, limit)).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(|v| count_maximum(v, limit)).sum(),
        _ => 0,
    }
}

#[test]
fn there_is_a_valid_fixture_for_every_message_kind() {
    let names: BTreeSet<String> = valid_fixtures().into_iter().map(|(name, _)| name).collect();
    for kind in MESSAGE_KINDS {
        assert!(names.contains(kind), "no valid fixture named {kind}");
    }
}

#[test]
fn the_invalid_fixtures_cover_the_known_failure_shapes() {
    let names: BTreeSet<String> = invalid_fixtures()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for expected in [
        "time_over_json_limit",
        "negative_floor",
        "floor_over_u8",
        "unknown_version",
        "unknown_message_kind",
        "unknown_enum_value",
        "missing_field",
        "extra_field",
        "fractional_position",
        "string_instead_of_number",
        "page_with_too_many_items",
        "page_with_too_few_items",
        "null_envelope",
        "two_messages_at_once",
    ] {
        assert!(
            names.contains(expected),
            "no invalid fixture named {expected}"
        );
    }
}

#[test]
fn the_schema_file_is_up_to_date() {
    let generated = schema_json();
    assert_ne!(generated.len(), 0, "schema is empty");
    assert_eq!(read(&protocol_dir().join("schema.json")), generated);
}

#[test]
fn the_fixture_files_are_up_to_date() {
    for (dir, fixtures) in [("valid", valid_fixtures()), ("invalid", invalid_fixtures())] {
        let path = protocol_dir().join("fixtures").join(dir);
        let expected: BTreeSet<String> = fixtures
            .iter()
            .map(|(name, _)| format!("{name}.json"))
            .collect();
        assert_ne!(expected.len(), 0, "no {dir} fixtures");
        assert_eq!(file_names(&path), expected, "files in fixtures/{dir}");
        for (name, value) in &fixtures {
            assert_eq!(
                read(&path.join(format!("{name}.json"))),
                pretty(value),
                "fixtures/{dir}/{name}.json"
            );
        }
    }
}

#[test]
fn the_schema_carries_the_json_integer_limit_for_every_time_field() {
    assert_eq!(count_maximum(&schema_value(), (1 << 53) - 1), 3);
}

#[test]
fn every_valid_fixture_passes_the_schema_and_rust_validation() {
    let fixtures = valid_fixtures();
    assert!(fixtures.len() >= MESSAGE_KINDS.len(), "too few fixtures");
    let validator = jsonschema::validator_for(&schema_value()).unwrap();
    for (name, value) in fixtures {
        assert!(validator.is_valid(&value), "schema rejects {name}");
        let envelope = validate_json(&value).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(serde_json::to_value(envelope).unwrap(), value, "{name}");
    }
}

#[test]
fn every_invalid_fixture_is_rejected() {
    let fixtures = invalid_fixtures();
    assert_ne!(fixtures.len(), 0, "no invalid fixtures");
    let validator = jsonschema::validator_for(&schema_value()).unwrap();
    for (name, value) in fixtures {
        assert!(validate_json(&value).is_err(), "rust accepts {name}");
        if !SEMANTIC_ONLY.contains(&name.as_str()) {
            assert!(!validator.is_valid(&value), "schema accepts {name}");
        }
    }
}

#[test]
fn integer_bounds_and_object_shape_agree_across_validators() {
    let validator = jsonschema::validator_for(&schema_value()).unwrap();
    let mut value = valid_fixtures()
        .into_iter()
        .find(|(n, _)| n == "call_floor")
        .unwrap()
        .1;
    value["message"]["callFloor"]["floor"] = serde_json::from_str("5e0").unwrap();
    assert!(validator.is_valid(&value));
    assert!(validate_json(&value).is_ok());
    value["seq"] = Value::from(u64::from(u32::MAX) + 1);
    assert!(!validator.is_valid(&value));
    value["seq"] = Value::from(0);
    value["message"]["callFloor"] = serde_json::json!([7, 5, "down"]);
    assert!(validate_json(&value).is_err());
}

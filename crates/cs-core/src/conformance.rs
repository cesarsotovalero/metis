use super::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn schema() -> jsonschema::Validator {
    jsonschema::validator_for(&serde_json::from_str::<Value>(EVENT_SCHEMA).unwrap()).unwrap()
}
fn event() -> Value {
    serde_json::from_str(
        include_str!("../../../fixtures/lifecycle.jsonl")
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn all_fixture_shapes_validate_and_round_trip() {
    let validator = schema();
    for fixture in [
        include_str!("../../../fixtures/ordinary.jsonl"),
        include_str!("../../../fixtures/cached.jsonl"),
        include_str!("../../../fixtures/interrupted.jsonl"),
        include_str!("../../../fixtures/lifecycle.jsonl"),
        include_str!("../../../fixtures/event-contract.jsonl"),
    ] {
        for line in fixture.lines() {
            let raw: Value = serde_json::from_str(line).unwrap();
            assert!(validator.is_valid(&raw), "invalid schema fixture: {line}");
            let e: EventEnvelope = serde_json::from_value(raw).unwrap();
            e.validate().unwrap();
            let round_trip: EventEnvelope =
                serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
            assert_eq!(e, round_trip);
        }
    }
    // Every schema variant has an actual Rust round-trip fixture, including all families.
    let kinds = include_str!("../../../fixtures/event-contract.jsonl")
        .lines()
        .map(|line| {
            serde_json::from_str::<EventEnvelope>(line)
                .unwrap()
                .payload
                .kind()
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    let raw_schema: Value = serde_json::from_str(EVENT_SCHEMA).unwrap();
    let declared = raw_schema["properties"]["payload"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["properties"]["kind"]["const"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(kinds, declared);
}
#[test]
fn schema_and_runtime_reject_malformed_and_private_fields() {
    let validator = schema();
    let mut cases = Vec::new();
    for version in [json!(0), json!(2), json!(65536)] {
        let mut value = event();
        value["schema_version"] = version;
        cases.push(value);
    }
    for field in [
        "event_id",
        "session_id",
        "producer_id",
        "producer_epoch",
        "task_id",
        "turn_id",
        "correlation_id",
    ] {
        for invalid in [
            json!(""),
            json!("x".repeat(129)),
            json!("embedded\ncontrol"),
            json!(7),
        ] {
            let mut value = event();
            value[field] = invalid;
            cases.push(value);
        }
    }
    for field in ["prompt", "source", "arguments", "output", "extensions"] {
        let mut value = event();
        value[field] = json!("private body");
        cases.push(value);
        let mut value = event();
        value["payload"][field] = json!("private body");
        cases.push(value);
    }
    for payload in [
        json!({"kind":"unrecognized_future_event"}),
        json!({"kind":"request_cancelled","request_id":""}),
        json!({"kind":"tool_completed","tool_id":"tool-1","exit_code":2147483648_u64}),
        json!({"kind":"queue_overflow","dropped_events":0}),
        json!({"kind":"artifact_stored","artifact_id":"a","content_hash":"not-a-hash","byte_length":0,"capture_mode":"metadata"}),
        json!({"kind":"task_outcome_recorded","task_id":"t","outcome":"successful_maybe","evidence_kind":"observed"}),
        json!({"kind":"context_snapshot_observed","snapshot_id":"s","tokens":{"provenance":"reported","value":1,"source":"fixture","secret":"body"}}),
    ] {
        let mut value = event();
        value["payload"] = payload;
        cases.push(value);
    }
    for sequence in [json!(-1), json!(9223372036854775808_u64)] {
        let mut value = event();
        value["producer_sequence"] = sequence;
        cases.push(value);
    }
    for value in cases {
        assert!(!validator.is_valid(&value), "schema accepted {value}");
        assert!(
            serde_json::from_value::<EventEnvelope>(value.clone())
                .map(|e| e.validate().is_err())
                .unwrap_or(true),
            "runtime accepted {value}"
        );
    }
}
#[test]
fn semantic_identity_and_capability_checks() {
    for payload in [
        json!({"kind":"session_resumed","previous_epoch":"epoch-1"}),
        json!({"kind":"task_started","task_id":"different-task"}),
        json!({"kind":"adapter_capability_changed","capability":"tool_output_replace","status":"verified","agent_version":"v1","evidence_ref":null}),
    ] {
        let mut value = event();
        value["payload"] = payload;
        value["task_id"] = json!("task-1");
        assert!(serde_json::from_value::<EventEnvelope>(value)
            .unwrap()
            .validate()
            .is_err());
    }
    // Upper-bound counters and nullable fields remain lossless through schema + Serde.
    let mut value = event();
    value["payload"] = json!({"kind":"usage_observed","request_id":"r","mode":"cumulative","usage":{"total_tokens":u64::MAX}});
    let e: EventEnvelope = serde_json::from_value(value).unwrap();
    e.validate().unwrap();
}

#[test]
fn every_required_payload_field_is_enforced() {
    let raw_schema: Value = serde_json::from_str(EVENT_SCHEMA).unwrap();
    let validator = schema();
    for line in include_str!("../../../fixtures/event-contract.jsonl").lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        let shape = raw_schema["properties"]["payload"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|shape| shape["properties"]["kind"]["const"] == value["payload"]["kind"])
            .unwrap();
        for required in shape["required"].as_array().unwrap() {
            let mut missing = value.clone();
            missing["payload"]
                .as_object_mut()
                .unwrap()
                .remove(required.as_str().unwrap());
            assert!(!validator.is_valid(&missing));
            assert!(serde_json::from_value::<EventEnvelope>(missing).is_err());
        }
    }
}

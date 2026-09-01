//! The IPC contract: what Rust puts on the wire against what `api.ts` declares.
//!
//! TBC-0007 names fixture drift as the failure most likely to bite and least
//! likely to be noticed — the visual suite mocks this boundary, so it passes
//! against a reality that no longer exists. These assertions are copied from
//! `packages/shared/src/ipc.ts` by hand, which is the point: the field lists are
//! an independent source of truth, and adding a field in Rust without adding it
//! there turns this red.
//!
//! Driven through `tauri::test`, so the response goes through Tauri's own
//! serialiser rather than `serde_json` directly. No window, no WebView2.

mod common;

use std::sync::Arc;

use common::real_apps;
use serde_json::Value;
use takyon_lib::entry::{Action, EntryId};
use takyon_lib::frecency::Frecency;
use takyon_lib::query::{Pipeline, QueryResult};
use takyon_lib::sources::recents::RecentsSource;
use takyon_lib::sources::system::SystemSource;
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::Manager;

/// `interface QueryResult` in `packages/shared/src/ipc.ts`.
const QUERY_RESULT_KEYS: [&str; 3] = ["seq", "entries", "indexing"];
/// `interface Entry`. The optionals must be absent rather than null when Rust
/// has none, or the frontend's `field?: T` types become `T | null`.
const ENTRY_REQUIRED: [&str; 5] = ["id", "title", "kind", "score", "actions"];
const ENTRY_OPTIONAL: [&str; 3] = ["subtitle", "icon", "version"];
/// `type EntryKind`.
const ENTRY_KINDS: [&str; 8] = [
    "app",
    "file",
    "folder",
    "clip",
    "calc",
    "recent",
    "system",
    "systemTask",
];
/// `interface Action`.
const ACTION_REQUIRED: [&str; 2] = ["id", "label"];
const ACTION_OPTIONAL: [&str; 1] = ["accelerator"];

/// A copy of `lib.rs`'s handler, minus the window resize.
///
/// The real one is pinned to the `Wry` runtime, so a mock app cannot call it
/// (TBC-0007). Its body is two lines over `Pipeline`; what this test is actually
/// about is the shape of what `QueryResult` serialises to.
#[tauri::command]
fn query(q: String, seq: u64, pipeline: tauri::State<'_, Arc<Pipeline>>) -> QueryResult {
    pipeline.query(&q, seq)
}

#[tauri::command]
fn actions_for(entry_id: String, pipeline: tauri::State<'_, Arc<Pipeline>>) -> Vec<Action> {
    pipeline.actions_for(&EntryId(entry_id))
}

/// A mock app with one window and a Pipeline over the real application walk.
///
/// Usage is in memory: this asserts the shape of a response, not persistence,
/// and an on-disk `frecency.db` would outlive the `App` that manages it.
fn mock_palette() -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![query, actions_for])
        .build(mock_context(noop_assets()))
        .expect("mock app");

    let (apps, icons) = real_apps();
    let frecency = Arc::new(Frecency::open(None).unwrap());
    app.manage(Arc::new(Pipeline::new(
        apps,
        Arc::new(RecentsSource::new()),
        Arc::new(SystemSource::new()),
        icons,
        frecency,
    )));

    let webview = tauri::WebviewWindowBuilder::new(&app, "palette", Default::default())
        .build()
        .expect("mock window");
    (app, webview)
}

fn call(webview: &tauri::WebviewWindow<MockRuntime>, cmd: &str, body: Value) -> Value {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: body.into(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .expect("command was rejected")
    .deserialize()
    .expect("response was not JSON")
}

#[test]
fn v0_3_a_query_response_carries_exactly_the_fields_api_ts_declares() {
    let (_app, webview) = mock_palette();
    let response = call(&webview, "query", serde_json::json!({ "q": "e", "seq": 1u64 }));

    let object = response.as_object().expect("QueryResult is an object");
    let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut expected = QUERY_RESULT_KEYS;
    expected.sort_unstable();
    assert_eq!(keys, expected, "QueryResult drifted from ipc.ts");

    assert!(object["seq"].is_u64());
    assert!(object["indexing"].is_boolean());

    let entries = object["entries"].as_array().expect("entries is an array");
    assert!(!entries.is_empty(), "nothing matched, so nothing was checked");
    eprintln!("  {} entries checked", entries.len());

    for entry in entries {
        let fields = entry.as_object().expect("Entry is an object");
        for key in ENTRY_REQUIRED {
            assert!(fields.contains_key(key), "Entry is missing {key}: {entry}");
        }
        for (key, value) in fields {
            assert!(
                ENTRY_REQUIRED.contains(&key.as_str()) || ENTRY_OPTIONAL.contains(&key.as_str()),
                "Entry carries {key}, which ipc.ts does not declare"
            );
            // An optional the frontend reads as `field?: T` must be absent, not
            // null: `"subtitle": null` types as `string | null` and breaks the
            // `?.` the Palette uses on it.
            assert!(!value.is_null(), "{key} serialised as null rather than absent");
        }

        assert!(fields["id"].is_string());
        assert!(fields["title"].is_string());
        assert!(fields["score"].is_number());
        let kind = fields["kind"].as_str().expect("kind is a string");
        assert!(ENTRY_KINDS.contains(&kind), "unknown EntryKind {kind:?}");
        for action in fields["actions"].as_array().expect("actions is an array") {
            assert!(action.is_string(), "actions carries ids only, not objects");
        }
    }
}

#[test]
fn v0_3_the_action_menu_response_matches_api_ts() {
    let (_app, webview) = mock_palette();
    let result = call(&webview, "query", serde_json::json!({ "q": "e", "seq": 1u64 }));
    let id = result["entries"][0]["id"].as_str().expect("an Entry").to_string();

    let actions = call(&webview, "actions_for", serde_json::json!({ "entryId": id }));
    let actions = actions.as_array().expect("Action[]");
    assert!(!actions.is_empty(), "{id} offered no actions");
    eprintln!("  {} actions for {id}", actions.len());

    for action in actions {
        let fields = action.as_object().expect("Action is an object");
        for key in ACTION_REQUIRED {
            assert!(fields.contains_key(key), "Action is missing {key}: {action}");
        }
        for (key, value) in fields {
            assert!(
                ACTION_REQUIRED.contains(&key.as_str()) || ACTION_OPTIONAL.contains(&key.as_str()),
                "Action carries {key}, which ipc.ts does not declare"
            );
            assert!(!value.is_null(), "{key} serialised as null rather than absent");
            assert!(value.is_string());
        }
    }
}

/// `api.ts` sends `{ entryId }` and Tauri maps it onto an `entry_id` parameter.
///
/// The convention, not the production signature — the handler here is a copy, so
/// a rename in `lib.rs` would not turn this red. What it pins is that the
/// mapping exists at all, which every camelCase argument in `api.ts` relies on.
/// v0.4: a Calc Entry reaches the wire in the shape `ipc.ts` declares.
///
/// The machine-independent half of the contract test above. A calculation is the
/// one Entry that does not depend on what is installed, so this asserts values
/// and not only field names.
#[test]
fn v0_4_a_calculation_reaches_the_wire_as_a_calc_entry() {
    let (_app, webview) = mock_palette();
    let response = call(&webview, "query", serde_json::json!({ "q": "12*1.18", "seq": 1u64 }));

    let entries = response["entries"].as_array().expect("entries is an array");
    let calc = &entries[0];
    assert_eq!(calc["kind"].as_str(), Some("calc"), "{calc}");
    assert_eq!(calc["title"].as_str(), Some("14.16"));
    assert_eq!(calc["subtitle"].as_str(), Some("12*1.18"));
    assert_eq!(calc["id"].as_str(), Some("calc:14.16"));
    // Ids only, never objects, and the one it carries is what the Palette's Enter
    // handler sends back.
    assert_eq!(
        calc["actions"].as_array().map(Vec::as_slice),
        Some([Value::from("copy_answer")].as_slice())
    );
    // No icon key: an answer has no file to extract one from, and a `null` here
    // would break the `icon?: string` the row reads with `?.`.
    assert!(calc.get("icon").is_none(), "a calculation shipped an icon: {calc}");
}

/// The menu Rust hands back for an id no Source holds an index for.
///
/// Every other Kind is found by lookup; a calculation is not. This is the one
/// menu that would silently come back empty if that branch were dropped.
#[test]
fn v0_4_the_action_menu_for_a_calculation_is_not_empty() {
    let (_app, webview) = mock_palette();
    let response = call(&webview, "actions_for", serde_json::json!({ "entryId": "calc:14.16" }));

    let actions = response.as_array().expect("actions_for returns an array");
    assert_eq!(actions.len(), 1, "{response}");
    assert_eq!(actions[0]["id"].as_str(), Some("copy_answer"));
    assert_eq!(actions[0]["label"].as_str(), Some("Copy answer"));
    assert_eq!(actions[0]["accelerator"].as_str(), Some("Enter"));
}

#[test]
fn v0_3_camel_case_argument_names_reach_snake_case_parameters() {
    let (_app, webview) = mock_palette();
    let response = call(&webview, "actions_for", serde_json::json!({ "entryId": "nope" }));
    assert!(
        response.as_array().is_some(),
        "actions_for did not accept entryId: {response}"
    );
}

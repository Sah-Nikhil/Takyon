//! The real updater plugin, end to end, against a feed on 127.0.0.1.
//!
//! Drives `update_check` and `update_install` through `tauri::test` with the
//! plugin registered, so the check, the download and the minisign check are the
//! plugin's own. The payload is `scripts/fixtures/updater/payload.txt`, signed by
//! a throwaway key. It is not an installer, so a valid signature ends in
//! "invalid updater binary format" and nothing is ever run. Plain HTTP is allowed
//! by the test's config only (`dangerousInsecureTransportProtocol`).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use serde_json::{json, Value};
use takyon_lib::update::Pending;
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::Manager;

fn fixture(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/fixtures/updater");
    std::fs::read(dir.join(name)).expect("updater fixture")
}

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap().trim().to_string()
}

/// Serves `latest.json` and the payload until the test process exits.
fn serve(payload: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let manifest = json!({
        "version": "9.9.9",
        "notes": "Fixture notes.",
        "pub_date": "2026-10-28T00:00:00Z",
        "platforms": {
            "windows-x86_64": { "url": format!("{base}/payload"), "signature": text("payload.txt.sig") },
            "darwin-aarch64": { "url": format!("{base}/payload"), "signature": text("payload.txt.sig") }
        }
    })
    .to_string()
    .into_bytes();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let body = if request.starts_with("GET /latest.json") {
                &manifest
            } else {
                &payload
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    format!("{base}/latest.json")
}

fn app(endpoint: &str, pubkey: &str) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
    app_with(endpoint, pubkey, false)
}

/// `with_prefs`: an in-memory `settings.db`, so checks persist and feed the notice.
fn app_with(
    endpoint: &str,
    pubkey: &str,
    with_prefs: bool,
) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        json!({
            "pubkey": pubkey,
            "endpoints": [endpoint],
            "dangerousInsecureTransportProtocol": true
        }),
    );
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            takyon_lib::update::update_check,
            takyon_lib::update::update_install,
            takyon_lib::update::update_notice,
            takyon_lib::update::update_auto_check,
            takyon_lib::update::set_update_auto_check
        ])
        .build(context)
        .expect("mock app");
    app.manage(Pending::default());
    if with_prefs {
        app.manage(std::sync::Arc::new(
            takyon_lib::prefs::Prefs::open(None).expect("in-memory settings.db"),
        ));
    }
    let webview = tauri::WebviewWindowBuilder::new(&app, "settings", Default::default())
        .build()
        .expect("mock window");
    // The app must outlive the test; it owns the async runtime the commands use.
    std::mem::forget(app);
    webview
}

fn invoke(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<Value, Value> {
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
    .map(|r| r.deserialize().expect("JSON"))
}

const CHANNEL: &str = "__CHANNEL__:7";

#[test]
fn updater_a_newer_feed_reads_as_available_with_its_notes() {
    let webview = app(&serve(fixture("payload.txt")), &text("signer.pub"));
    let found = invoke(&webview, "update_check", json!({})).expect("check");
    assert_eq!(
        found,
        json!({ "state": "available", "version": "9.9.9", "notes": "Fixture notes." })
    );
}

/// Signature passes, then the plugin refuses the payload as no installer:
/// verification ran on the real bytes, and nothing was launched.
#[test]
fn updater_a_correctly_signed_download_passes_verification() {
    let webview = app(&serve(fixture("payload.txt")), &text("signer.pub"));
    invoke(&webview, "update_check", json!({})).expect("check");
    let refusal = invoke(&webview, "update_install", json!({ "onProgress": CHANNEL }))
        .expect_err("a text file installed");
    assert_eq!(
        refusal,
        json!("Update failed: invalid updater binary format")
    );
}

/// One byte changed in transit: refused before anything runs, and said so.
#[test]
fn updater_a_tampered_download_is_refused() {
    let mut tampered = fixture("payload.txt");
    tampered[0] ^= 1;
    let webview = app(&serve(tampered), &text("signer.pub"));
    invoke(&webview, "update_check", json!({})).expect("check");
    let refusal = invoke(&webview, "update_install", json!({ "onProgress": CHANNEL }))
        .expect_err("tampered download accepted");
    assert_eq!(
        refusal,
        json!("The download failed its signature check. Nothing was installed.")
    );
}

/// The release a build trusts is the key it was built with, not whoever signed.
#[test]
fn updater_a_download_signed_by_another_key_is_refused() {
    let webview = app(&serve(fixture("payload.txt")), &text("stranger.pub"));
    invoke(&webview, "update_check", json!({})).expect("check");
    let refusal = invoke(&webview, "update_install", json!({ "onProgress": CHANNEL }))
        .expect_err("foreign signature accepted");
    assert_eq!(
        refusal,
        json!("The download failed its signature check. Nothing was installed.")
    );
}

#[test]
fn updater_an_unreachable_feed_is_worded_for_a_person() {
    // Bound then dropped: nothing listens on this port.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let webview = app(
        &format!("http://127.0.0.1:{port}/latest.json"),
        &text("signer.pub"),
    );
    let refusal = invoke(&webview, "update_check", json!({})).expect_err("checked nothing");
    assert_eq!(
        refusal,
        json!("Couldn't reach the update feed. Check the connection and try again.")
    );
}

/// Live: the real HTTPS path (Schannel, system proxy) against a real feed.
/// `TAKYON_UPDATE_FEED=<latest.json URL> cargo test --test updater -- --ignored`.
/// Before the first signed release the feed is a 404, which must read as
/// unreachable; afterwards it must parse. A TLS failure prints in the log line.
#[test]
#[ignore = "network; set TAKYON_UPDATE_FEED"]
fn updater_live_feed_answers_over_https() {
    let Ok(feed) = std::env::var("TAKYON_UPDATE_FEED") else {
        return eprintln!("TAKYON_UPDATE_FEED unset; skipped");
    };
    let webview = app(&feed, &text("signer.pub"));
    match invoke(&webview, "update_check", json!({})) {
        Ok(found) => assert!(found["state"] == "available" || found["state"] == "current"),
        Err(refusal) => assert_eq!(
            refusal,
            json!("Couldn't reach the update feed. Check the connection and try again.")
        ),
    }
}

/// The Palette's row comes from what a check stored, never from a request of
/// its own (ADR-0040). The only test here that writes the process-wide notice.
#[test]
fn updater_a_check_that_finds_an_update_puts_it_in_the_palette() {
    let webview = app_with(&serve(fixture("payload.txt")), &text("signer.pub"), true);
    assert_eq!(
        invoke(&webview, "update_notice", json!({})),
        Ok(Value::Null)
    );
    invoke(&webview, "update_check", json!({})).expect("check");
    assert_eq!(
        invoke(&webview, "update_notice", json!({})),
        Ok(json!({ "kind": "available", "version": "9.9.9" }))
    );
}

/// On by default (the user's choice for ADR-0040), and the switch persists.
#[test]
fn updater_the_daily_check_is_on_until_switched_off() {
    let webview = app_with(&serve(fixture("payload.txt")), &text("signer.pub"), true);
    assert_eq!(
        invoke(&webview, "update_auto_check", json!({})),
        Ok(json!(true))
    );
    invoke(&webview, "set_update_auto_check", json!({ "on": false })).expect("write");
    assert_eq!(
        invoke(&webview, "update_auto_check", json!({})),
        Ok(json!(false))
    );
}

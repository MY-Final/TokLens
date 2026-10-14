//! 合成数据回归：URL 规则、请求体、围栏剥离、坏 JSON / 未知字段 / 越界拒绝、凭据语义、提示词。
//!
//! 全部纯函数化，不联网、不依赖真实凭据、不依赖当前时间。

use super::config::{
    BASE_URL_MAX_LEN, LlmConfigDto, LlmConfigPatch, apply_patch, normalize_base_url,
    normalize_model,
};
use super::credential::{CredentialStore, apply_secret, has_secret};
use super::http::{
    build_generation_body, build_ping_body, chat_completions_url, extract_content, extract_usage,
    finish_generated_spec, fnv1a64, strip_code_fence,
};
use super::prompt::system_prompt;
use crate::panels::get_panel_catalog;
use crate::settings::LlmConfig;
use crate::usage::client::Client;
use serde_json::json;

// ---------------------------------------------------------------------------------------------
// Base URL / 模型名规范化
// ---------------------------------------------------------------------------------------------

#[test]
fn base_url_only_allows_https_or_local_http_and_strips_trailing_slash() {
    assert_eq!(
        normalize_base_url("https://api.deepseek.com").unwrap(),
        "https://api.deepseek.com"
    );
    assert_eq!(
        normalize_base_url("  https://api.openai.com/v1/  ").unwrap(),
        "https://api.openai.com/v1"
    );
    assert_eq!(
        normalize_base_url("https://host/path///").unwrap(),
        "https://host/path"
    );
    assert_eq!(
        normalize_base_url("http://localhost:11434/v1").unwrap(),
        "http://localhost:11434/v1"
    );
    assert_eq!(
        normalize_base_url("http://127.0.0.1:8000").unwrap(),
        "http://127.0.0.1:8000"
    );

    for bad in [
        "",
        "   ",
        "not a url",
        "ftp://host",
        "file:///etc/passwd",
        "http://evil.example.com",
        "http://0.0.0.0",
        "http://192.168.1.1",
        "https://user:pass@host",
        "https://host?x=1",
        "https://host#frag",
    ] {
        assert!(normalize_base_url(bad).is_err(), "expected reject: {bad}");
    }

    let too_long = format!("https://example.com/{}", "a".repeat(BASE_URL_MAX_LEN));
    assert!(normalize_base_url(&too_long).is_err());
}

#[test]
fn model_name_is_trimmed_and_bounded() {
    assert_eq!(
        normalize_model("  deepseek-chat  ").unwrap(),
        "deepseek-chat"
    );
    assert!(normalize_model("   ").is_err());
    assert!(normalize_model(&"m".repeat(super::config::MODEL_MAX_LEN + 1)).is_err());
}

#[test]
fn config_patch_keeps_omitted_fields_and_clears_on_empty() {
    let current = LlmConfig {
        base_url: Some("https://api.deepseek.com".into()),
        model: Some("deepseek-chat".into()),
    };

    let unchanged = apply_patch(&current, &LlmConfigPatch::default()).unwrap();
    assert_eq!(unchanged, current);

    let patch = LlmConfigPatch {
        model: Some("  deepseek-reasoner ".into()),
        base_url: None,
    };
    let next = apply_patch(&current, &patch).unwrap();
    assert_eq!(next.base_url.as_deref(), Some("https://api.deepseek.com"));
    assert_eq!(next.model.as_deref(), Some("deepseek-reasoner"));

    let patch = LlmConfigPatch {
        base_url: Some("   ".into()),
        model: Some(String::new()),
    };
    assert_eq!(apply_patch(&current, &patch).unwrap(), LlmConfig::default());

    // 非法值拒绝且不改写既有配置。
    let patch = LlmConfigPatch {
        base_url: Some("http://evil.example.com".into()),
        model: None,
    };
    assert!(apply_patch(&current, &patch).is_err());
}

#[test]
fn config_dto_never_contains_the_key() {
    let dto = LlmConfigDto::new(
        &LlmConfig {
            base_url: Some("https://api.deepseek.com".into()),
            model: Some("m".into()),
        },
        true,
    );
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({ "baseUrl": "https://api.deepseek.com", "model": "m", "hasKey": true })
    );
}

// ---------------------------------------------------------------------------------------------
// 请求构造 / 响应清洗
// ---------------------------------------------------------------------------------------------

#[test]
fn request_bodies_match_openai_contract() {
    let body = build_generation_body("deepseek-chat", "sys", "usr");
    assert_eq!(body["model"], json!("deepseek-chat"));
    assert_eq!(body["messages"][0]["role"], json!("system"));
    assert_eq!(body["messages"][0]["content"], json!("sys"));
    assert_eq!(body["messages"][1]["role"], json!("user"));
    assert_eq!(body["messages"][1]["content"], json!("usr"));
    assert_eq!(body["temperature"], json!(0.2));
    assert_eq!(body["max_tokens"], json!(1024));

    let ping = build_ping_body("m");
    assert_eq!(ping["max_tokens"], json!(1));
    assert_eq!(ping["messages"][0]["role"], json!("user"));
    assert_eq!(ping["messages"][0]["content"], json!("ping"));

    assert_eq!(
        chat_completions_url("https://api.deepseek.com"),
        "https://api.deepseek.com/chat/completions"
    );
}

#[test]
fn code_fence_is_stripped_for_json_and_plain() {
    assert_eq!(strip_code_fence("{\"a\":1}"), "{\"a\":1}");
    assert_eq!(strip_code_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
    assert_eq!(strip_code_fence("```\n[1,2]\n```"), "[1,2]");
    assert_eq!(strip_code_fence("  ```json\n{}\n```  "), "{}");
}

#[test]
fn response_content_and_usage_are_extracted() {
    let response = json!({
        "choices": [{ "message": { "role": "assistant", "content": "hi" } }],
        "usage": { "prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4 },
    });
    assert_eq!(extract_content(&response).unwrap(), "hi");
    let usage = extract_usage(&response).unwrap();
    assert_eq!(usage.prompt_tokens, Some(3));
    assert_eq!(usage.completion_tokens, Some(1));
    assert_eq!(usage.total_tokens, Some(4));

    assert!(extract_content(&json!({ "choices": [] })).is_err());
    assert!(extract_usage(&json!({ "id": "x" })).is_none());
}

// ---------------------------------------------------------------------------------------------
// 规格清洗 + 权威校验
// ---------------------------------------------------------------------------------------------

const VALID: &str = r#"{
  "specVersion": 1,
  "id": "p_ai_1",
  "type": "ranking",
  "title": "模型排行",
  "query": { "range": { "kind": "preset", "key": "30d" }, "timeBasis": "request", "client": null },
  "typeOptions": { "dimension": "model", "limit": 5 },
  "layout": { "colSpan": 6, "rowSpan": 1, "order": 0 },
  "source": { "origin": "manual" }
}"#;

#[test]
fn valid_generated_spec_is_accepted_and_marked_ai() {
    let spec = finish_generated_spec(&format!("```json\n{VALID}\n```"), &Client::ALL, "seed")
        .expect("valid spec");
    assert_eq!(spec.id, "p_ai_1");
    let value = serde_json::to_value(&spec).unwrap();
    assert_eq!(value["type"], json!("ranking"));
    assert_eq!(value["source"]["origin"], json!("ai"));
}

#[test]
fn missing_fields_are_repaired_within_policy_and_id_is_deterministic() {
    let minimal = r#"{ "type": "stat", "query": { "range": { "kind": "followPage" } } }"#;
    let seed = "看上周每个模型用了多少 token";
    let spec = finish_generated_spec(minimal, &Client::ALL, seed).expect("repaired spec");
    let value = serde_json::to_value(&spec).unwrap();
    assert_eq!(value["specVersion"], json!(1));
    assert_eq!(value["source"]["origin"], json!("ai"));
    let expected = format!("p_ai_{:016x}", fnv1a64(seed.as_bytes()));
    assert_eq!(value["id"], json!(expected));

    let again = finish_generated_spec(minimal, &Client::ALL, seed).expect("repaired spec");
    assert_eq!(serde_json::to_value(&again).unwrap()["id"], json!(expected));
}

#[test]
fn invalid_generated_specs_are_rejected_with_readable_errors() {
    let enabled = Client::ALL;

    assert!(
        finish_generated_spec("not json", &enabled, "s")
            .unwrap_err()
            .contains("合法 JSON")
    );
    assert!(finish_generated_spec("[]", &enabled, "s").is_err());

    // 目录之外的顶层字段（如 SQL / 路径）必须被拒绝。
    let unknown = r#"{ "specVersion":1, "id":"p", "type":"stat", "query":{"range":{"kind":"followPage"}}, "sql":"drop table" }"#;
    assert!(finish_generated_spec(unknown, &enabled, "s").is_err());

    let unknown_option = r#"{ "specVersion":1, "id":"p", "type":"stat", "query":{"range":{"kind":"followPage"}}, "typeOptions":{"bogus":1} }"#;
    assert!(finish_generated_spec(unknown_option, &enabled, "s").is_err());

    let out_of_bounds = r#"{ "specVersion":1, "id":"p", "type":"ranking", "query":{"range":{"kind":"followPage"}}, "typeOptions":{"limit":9999} }"#;
    assert!(finish_generated_spec(out_of_bounds, &enabled, "s").is_err());

    let bad_client = r#"{ "specVersion":1, "id":"p", "type":"stat", "query":{"range":{"kind":"followPage"},"client":"nope"} }"#;
    assert!(finish_generated_spec(bad_client, &enabled, "s").is_err());

    let bad_basis = r#"{ "specVersion":1, "id":"p", "type":"token_trend", "query":{"range":{"kind":"followPage"},"timeBasis":"request"} }"#;
    assert!(finish_generated_spec(bad_basis, &enabled, "s").is_err());

    let future_version =
        r#"{ "specVersion":99, "id":"p", "type":"stat", "query":{"range":{"kind":"followPage"}} }"#;
    assert!(finish_generated_spec(future_version, &enabled, "s").is_err());
}

// ---------------------------------------------------------------------------------------------
// 目录即契约 / 凭据语义
// ---------------------------------------------------------------------------------------------

#[test]
fn system_prompt_embeds_the_catalog_and_rules() {
    let prompt = system_prompt(&get_panel_catalog());
    for marker in [
        "token_trend",
        "session_table",
        "activity_calendar",
        "单一事实来源",
        "specVersion=1",
        "禁止包含 SQL",
        "只输出 JSON",
    ] {
        assert!(prompt.contains(marker), "prompt missing marker: {marker}");
    }
}

#[derive(Default)]
struct MemoryStore {
    secret: std::sync::Mutex<Option<String>>,
}

impl CredentialStore for MemoryStore {
    fn set(&self, secret: &str) -> Result<(), String> {
        *self.secret.lock().unwrap() = Some(secret.to_string());
        Ok(())
    }
    fn clear(&self) -> Result<(), String> {
        *self.secret.lock().unwrap() = None;
        Ok(())
    }
    fn has(&self) -> Result<bool, String> {
        Ok(self.secret.lock().unwrap().is_some())
    }
}

#[test]
fn credential_is_write_only_and_empty_clears() {
    let store = MemoryStore::default();
    assert!(!has_secret(&store));

    apply_secret(&store, "  sk-secret  ").expect("set");
    assert!(has_secret(&store));
    assert_eq!(store.secret.lock().unwrap().as_deref(), Some("sk-secret"));

    apply_secret(&store, "   ").expect("clear by whitespace");
    assert!(!has_secret(&store));

    apply_secret(&store, "k2").expect("set");
    apply_secret(&store, "").expect("clear by empty");
    assert!(!has_secret(&store));
}

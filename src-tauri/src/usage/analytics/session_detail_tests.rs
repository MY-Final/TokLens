use super::*;
use crate::usage::client::Client;

fn fixture() -> UsageStore {
    let store = UsageStore::open_in_memory().expect("synthetic store");
    store.conn.execute_batch(
        "INSERT INTO sessions(id,client,client_session_id,started_at_ms,total_tokens,
         semantics_id,total_basis,message_coverage,source_kind,cost_source,imported_at_ms,updated_at_ms)
         VALUES('s','codex','same',1,999,'codex_v1','client_cumulative','partial','measured','none',1,1),
               ('other','claude_code','same',1,7,'claude_v1','sum_of_messages','none','measured','none',1,1);
         INSERT INTO messages(id,session_id,client,dedup_key,seq,created_at_ms,total_tokens,
         semantics_id,source_kind,cost_source,imported_at_ms,updated_at_ms,unknown_relations,is_sidechain)
         VALUES('a','s','codex','first',1,2,0,'codex_v1','measured','none',1,1,NULL,0),
               ('b','s','codex','next-day',2,86400002,10,'codex_v1','estimated','none',1,1,'reasoning',1),
               ('c','s','codex','unknown',NULL,NULL,NULL,'codex_v1','derived','none',1,1,NULL,0);",
    ).expect("synthetic metadata");
    store
}

#[test]
fn all_days_and_unknown_values_remain_distinct_from_zero() {
    let store = fixture();
    let detail = store
        .session_detail(
            &AnalyticsFilter {
                client: Some(Client::Codex),
                from_ms: Some(100),
                to_ms: Some(200),
                project_path: Some("outside-list"),
            },
            "same",
            100,
            0,
        )
        .expect("detail ignores list filters");
    assert_eq!(detail.session.total_tokens, Some(999));
    assert_eq!(detail.session.messages, 3);
    assert_eq!(
        detail
            .requests
            .iter()
            .map(|r| r.dedup_key.as_str())
            .collect::<Vec<_>>(),
        ["first", "next-day", "unknown"]
    );
    assert_eq!(detail.requests[0].total_tokens, Some(0));
    assert_eq!(detail.requests[2].total_tokens, None);
    assert_eq!(detail.requests[1].source_kind, "estimated");
    assert!(detail.requests[1].is_sidechain);
    assert_eq!(detail.requests[1].unknown_relations, ["reasoning"]);
    assert_eq!(
        detail.request_coverage,
        RequestCoverageDetail {
            total: 3,
            timed: 2,
            untimed: 1,
            with_usage: 2,
            without_usage: 1,
            estimated: 1,
            partial: true,
        }
    );
}

#[test]
fn pagination_keeps_full_coverage_and_client_identity() {
    let store = fixture();
    let filter = AnalyticsFilter {
        client: Some(Client::Codex),
        ..Default::default()
    };
    let detail = store.session_detail(&filter, "same", 1, 1).expect("page");
    assert_eq!(detail.requests.len(), 1);
    assert_eq!(detail.requests[0].dedup_key, "next-day");
    assert_eq!(detail.request_coverage.total, 3);
    let other = store
        .session_detail(
            &AnalyticsFilter {
                client: Some(Client::ClaudeCode),
                ..Default::default()
            },
            "same",
            100,
            0,
        )
        .expect("other client");
    assert_eq!(other.session.total_tokens, Some(7));
    assert!(other.requests.is_empty());
    assert_eq!(other.request_coverage.total, 0);
    assert!(matches!(
        store.session_detail(&filter, "missing", 1, 0),
        Err(UsageError::InvalidInput(_))
    ));
    assert!(matches!(
        store.session_detail(&AnalyticsFilter::default(), "same", 1, 0),
        Err(UsageError::InvalidInput(_))
    ));
}

#[test]
fn only_metadata_columns_exist_and_stable_order_is_paginated() {
    let store = fixture();
    let mut insert = store
        .conn
        .prepare(
            "INSERT INTO messages(id,session_id,client,dedup_key,seq,created_at_ms,
         semantics_id,source_kind,cost_source,imported_at_ms,updated_at_ms)
         VALUES(?1,'s','codex',?1,1,2,'codex_v1','measured','none',1,1)",
        )
        .expect("insert");
    for index in 0..205 {
        insert
            .execute([format!("stable-{index:03}")])
            .expect("synthetic request");
    }
    let detail = store
        .session_detail(
            &AnalyticsFilter {
                client: Some(Client::Codex),
                ..Default::default()
            },
            "same",
            1000,
            0,
        )
        .expect("capped page");
    assert_eq!(detail.requests.len(), 200);
    assert_eq!(detail.requests[0].dedup_key, "first");
    assert_eq!(detail.requests[1].dedup_key, "stable-000");
    let columns = store
        .conn
        .prepare("SELECT name FROM pragma_table_info('messages')")
        .expect("metadata schema")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("column names")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("columns");
    for forbidden in ["body", "content", "prompt", "text"] {
        assert!(!columns.iter().any(|name| name == forbidden));
    }
}

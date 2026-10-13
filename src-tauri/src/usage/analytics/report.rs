//! Complete request-time report from a single SQLite read snapshot.
use super::AnalyticsFilter;
use crate::usage::{
    report::{METRICS_VERSION, ReportMetadata, ReportRow, StatisticsReport},
    store::{UsageError, UsageStore},
};
use rusqlite::params;

impl UsageStore {
    pub fn statistics_report(
        &self,
        filter: &AnalyticsFilter<'_>,
        include_estimated: bool,
        generated_at_ms: i64,
    ) -> Result<StatisticsReport, UsageError> {
        let to_ms = filter.to_ms.ok_or_else(|| {
            UsageError::InvalidInput("Report requires an exclusive end time".into())
        })?;
        let valid = |ms: i64| (-62_167_219_200_000..253_402_300_800_000).contains(&ms);
        if !valid(to_ms)
            || filter
                .from_ms
                .is_some_and(|from| !valid(from) || from >= to_ms)
        {
            return Err(UsageError::InvalidInput("Invalid report date range".into()));
        }
        let client = filter.client.map(|c| c.as_str());
        let tx = self.conn.unchecked_transaction()?;
        let untimed_requests = tx.query_row("SELECT COUNT(*) FROM messages m JOIN sessions s ON s.id=m.session_id WHERE m.created_at_ms IS NULL AND (?1 IS NULL OR m.client=?1) AND (?2 IS NULL OR s.project_path=?2)", params![client, filter.project_path], |r| r.get(0))?;
        let excluded_estimated_requests = if include_estimated {
            0
        } else {
            tx.query_row("SELECT COUNT(*) FROM messages m JOIN sessions s ON s.id=m.session_id WHERE m.created_at_ms IS NOT NULL AND (?1 IS NULL OR m.created_at_ms>=?1) AND m.created_at_ms<?2 AND (?3 IS NULL OR m.client=?3) AND (?4 IS NULL OR s.project_path=?4) AND m.source_kind='estimated'", params![filter.from_ms,to_ms,client,filter.project_path], |r| r.get(0))?
        };
        let rows = {
            let mut stmt = tx.prepare("SELECT date(m.created_at_ms / 1000.0, 'unixepoch'), m.client, s.model, s.project_path, m.source_kind, COUNT(*), SUM(m.input_tokens), SUM(m.output_tokens), SUM(m.reasoning_tokens), SUM(m.cache_read_tokens), SUM(m.cache_write_tokens), SUM(m.total_tokens), SUM(m.total_tokens IS NULL), COUNT(m.input_tokens), COUNT(m.output_tokens), COUNT(m.reasoning_tokens), COUNT(m.cache_read_tokens), COUNT(m.cache_write_tokens), COUNT(m.total_tokens) FROM messages m JOIN sessions s ON s.id=m.session_id WHERE m.created_at_ms IS NOT NULL AND (?1 IS NULL OR m.created_at_ms>=?1) AND m.created_at_ms<?2 AND (?3 IS NULL OR m.client=?3) AND (?4 IS NULL OR s.project_path=?4) AND (?5 OR m.source_kind!='estimated') GROUP BY 1,2,3,4,5 ORDER BY 1,2,3,4,5")?;
            let records = stmt.query_map(
                params![
                    filter.from_ms,
                    to_ms,
                    client,
                    filter.project_path,
                    include_estimated
                ],
                |r| {
                    let requests = r.get::<_, i64>(5)?;
                    let mut unknown_token_fields = Vec::new();
                    for (index, name) in [
                        "inputTokens",
                        "outputTokens",
                        "reasoningTokens",
                        "cacheReadTokens",
                        "cacheWriteTokens",
                        "totalTokens",
                    ]
                    .iter()
                    .enumerate()
                    {
                        if r.get::<_, i64>(13 + index)? < requests {
                            unknown_token_fields.push((*name).into());
                        }
                    }
                    Ok(ReportRow {
                        date: r.get(0)?,
                        client: r.get(1)?,
                        model: r.get(2)?,
                        project: r.get(3)?,
                        source_kind: r.get(4)?,
                        requests,
                        input_tokens: r.get(6)?,
                        output_tokens: r.get(7)?,
                        reasoning_tokens: r.get(8)?,
                        cache_read_tokens: r.get(9)?,
                        cache_write_tokens: r.get(10)?,
                        total_tokens: r.get(11)?,
                        requests_without_usage: r.get(12)?,
                        unknown_token_fields,
                    })
                },
            )?;
            let rows = records.take(100_001).collect::<Result<Vec<_>, _>>()?;
            if rows.len() > 100_000 {
                return Err(UsageError::InvalidInput(
                    "报告分组超过 100000 行，请缩短日期范围或选择单个客户端后重试。".into(),
                ));
            }
            rows
        };
        tx.commit()?;
        Ok(StatisticsReport { metadata: ReportMetadata {schema_version:1,metrics_version:METRICS_VERSION.into(),from_ms:filter.from_ms,to_ms,client:client.map(str::to_owned),include_estimated,date_basis:"UTC".into(),model_basis:"session_main_model".into(),generated_at_ms,untimed_requests,excluded_estimated_requests,notes:vec!["Grok CLI requests represent recorded rounds, not individual model calls; request dates use the native round end time (turnEndedAt).".into(), "Token sums include known values only; unknownTokenFields marks partially missing fields. Untimed requests are excluded and counted independently of the date range.".into()]}, rows })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{client::Client, report::ReportFormat};
    fn fixture() -> UsageStore {
        let store = UsageStore::open_in_memory().expect("synthetic database");
        store.conn.execute_batch("INSERT INTO sessions (id,client,client_session_id,model,project_path,semantics_id,total_basis,message_coverage,source_kind,cost_source,imported_at_ms,updated_at_ms) VALUES ('private-session','codex','private-id','main-model','project','test','sum_of_messages','complete','measured','none',0,0),('other','grok','other','grok-model',NULL,'test','sum_of_messages','complete','measured','none',0,0);").expect("sessions");
        for (index, time, kind, input, total) in [
            (0, Some(0), "measured", Some(0), Some(0)),
            (1, Some(0), "measured", None, None),
            (2, Some(86_400_000), "derived", Some(7), Some(7)),
            (3, Some(86_400_000), "estimated", Some(9), Some(9)),
            (4, None, "measured", Some(1), Some(1)),
            (5, Some(501_i64 * 86_400_000), "measured", Some(2), Some(2)),
        ] {
            store.conn.execute("INSERT INTO messages (id,session_id,client,dedup_key,created_at_ms,model,semantics_id,source_kind,input_tokens,total_tokens,source_ref,cost_source,imported_at_ms,updated_at_ms) VALUES (?1,'private-session','codex',?1,?2,'request-model','test',?3,?4,?5,'private-source','none',0,0)",params![index.to_string(),time,kind,input,total]).expect("message");
        }
        store.conn.execute("INSERT INTO messages (id,session_id,client,dedup_key,created_at_ms,semantics_id,source_kind,cost_source,imported_at_ms,updated_at_ms) VALUES ('grok','other','grok','grok',0,'test','measured','none',0,0)",[]).expect("grok message");
        store
    }
    #[test]
    fn statistics_report_filters_and_preserves_unknown_zero_sources_and_long_ranges() {
        let store = fixture();
        let filter = AnalyticsFilter {
            from_ms: Some(0),
            to_ms: Some(502 * 86_400_000),
            client: Some(Client::Codex),
            project_path: None,
        };
        let report = store
            .statistics_report(&filter, false, 123)
            .expect("report");
        assert_eq!(report.rows.len(), 3);
        assert_eq!(report.rows[0].date, "1970-01-01");
        assert_eq!(report.rows[1].date, "1970-01-02");
        assert_eq!(report.rows[0].input_tokens, Some(0));
        assert_eq!(report.rows[0].output_tokens, None);
        assert_eq!(report.rows[0].requests, 2);
        assert_eq!(report.rows[0].requests_without_usage, 1);
        assert!(
            report.rows[0]
                .unknown_token_fields
                .contains(&"inputTokens".into())
        );
        assert_eq!(report.metadata.untimed_requests, 1);
        assert_eq!(report.metadata.excluded_estimated_requests, 1);
        assert_eq!(report.rows[1].model.as_deref(), Some("main-model"));
        assert_eq!(report.rows[1].source_kind, "derived");
        let included = store
            .statistics_report(&filter, true, 123)
            .expect("estimates");
        assert_eq!(included.rows.len(), 4);
        let boundary = store
            .statistics_report(
                &AnalyticsFilter {
                    to_ms: Some(86_400_000),
                    ..filter
                },
                false,
                0,
            )
            .expect("boundary");
        assert_eq!(boundary.rows.len(), 1);
        for format in [
            ReportFormat::Json,
            ReportFormat::Csv,
            ReportFormat::Markdown,
        ] {
            let rendered = report.render(format).expect("render");
            assert!(!rendered.contains("private-session"));
            assert!(!rendered.contains("private-id"));
            assert!(!rendered.contains("private-source"));
            assert!(!rendered.contains("request-model"));
        }
        assert!(
            store
                .statistics_report(
                    &AnalyticsFilter {
                        from_ms: Some(1),
                        to_ms: Some(1),
                        ..filter
                    },
                    false,
                    0
                )
                .is_err()
        );
    }
    #[test]
    fn statistics_report_safe_rendering_and_empty_metadata() {
        let store = fixture();
        let filter = AnalyticsFilter {
            from_ms: Some(1),
            to_ms: Some(2),
            ..Default::default()
        };
        let report = store
            .statistics_report(&filter, false, 0)
            .expect("empty report");
        assert!(report.rows.is_empty());
        for format in [
            ReportFormat::Csv,
            ReportFormat::Json,
            ReportFormat::Markdown,
        ] {
            assert!(
                report
                    .render(format)
                    .expect("empty render")
                    .contains(METRICS_VERSION)
            );
        }
        store
            .conn
            .execute(
                "UPDATE sessions SET model=?1,project_path=?2 WHERE client='codex'",
                params![" \t=HYPERLINK(\"bad\")", "<script>|`x`\nnext"],
            )
            .expect("hostile metadata");
        let report = store
            .statistics_report(
                &AnalyticsFilter {
                    to_ms: Some(1),
                    client: Some(Client::Codex),
                    ..Default::default()
                },
                false,
                0,
            )
            .expect("report");
        let csv = report.render(ReportFormat::Csv).expect("csv");
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("' \t=HYPERLINK(\"\"bad\"\")"));
        assert!(csv.ends_with("\r\n"));
        let md = report.render(ReportFormat::Markdown).expect("markdown");
        assert!(md.contains("&lt;script&gt;&#124;&#96;x&#96; next"));
        let value: serde_json::Value =
            serde_json::from_str(&report.render(ReportFormat::Json).expect("json"))
                .expect("valid json");
        assert!(value["rows"][0]["outputTokens"].is_null());
        assert_eq!(value["rows"][0]["inputTokens"], 0);
    }
}

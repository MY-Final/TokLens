//! Atomic imports: watermark, session snapshot and message details.

use super::writes::{upsert_message_conn, upsert_session_conn, upsert_source_file_conn};
use super::{ImportSummary, SourceFile, UpsertOutcome, UsageError, UsageStore};
use crate::usage::model::{MessageRecord, SessionRecord};
use rusqlite::params;

impl UsageStore {
    /// 覆盖式导入一个源文件的解析结果：先写水位，再写会话快照，然后**删除该源文件此前写入的明细**，
    /// 最后写入本次解析结果。
    ///
    /// 与 `import_session` 的差别：文件被截断/替换后，旧明细必须消失，否则会残留源文件中已不存在的记录。
    /// 单事务提交，中断时整体回滚、源文件水位不前进，因此下次运行会安全地重新解析。
    pub fn replace_file_import(
        &self,
        file: &SourceFile,
        session: &SessionRecord,
        messages: &[MessageRecord],
    ) -> Result<ImportSummary, UsageError> {
        if file.client != session.client {
            return Err(UsageError::InvalidInput(format!(
                "source file client {} does not match session client {}",
                file.client, session.client
            )));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let file_id = upsert_source_file_conn(&transaction, file)?;
        let outcome = upsert_session_conn(&transaction, session)?;
        let pruned = transaction.execute(
            "DELETE FROM messages WHERE source_file_id = ?1",
            params![file_id],
        )?;
        let mut summary = ImportSummary {
            session: Some(outcome),
            messages_pruned: pruned,
            ..ImportSummary::default()
        };
        for message in messages {
            if message.client_session_id != session.client_session_id {
                return Err(UsageError::InvalidInput(format!(
                    "message session {} does not match session {}",
                    message.client_session_id, session.client_session_id
                )));
            }
            match upsert_message_conn(&transaction, message, Some(file_id))? {
                UpsertOutcome::Inserted => summary.messages_inserted += 1,
                UpsertOutcome::Updated => summary.messages_updated += 1,
                UpsertOutcome::SkippedStale => summary.messages_skipped_stale += 1,
            }
        }
        transaction.commit()?;
        Ok(summary)
    }

    /// 一次性导入一个会话（源文件水位 + 会话 + 请求明细）。单事务保证原子性：
    /// 中断不会留下半截数据，重跑幂等。
    pub fn import_session(
        &self,
        file: &SourceFile,
        session: &SessionRecord,
        messages: &[MessageRecord],
    ) -> Result<ImportSummary, UsageError> {
        if file.client != session.client {
            return Err(UsageError::InvalidInput(format!(
                "source file client {} does not match session client {}",
                file.client, session.client
            )));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let file_id = upsert_source_file_conn(&transaction, file)?;
        let outcome = upsert_session_conn(&transaction, session)?;
        let mut summary = ImportSummary {
            session: Some(outcome),
            ..ImportSummary::default()
        };
        if outcome != UpsertOutcome::SkippedStale {
            for message in messages {
                if message.client_session_id != session.client_session_id {
                    return Err(UsageError::InvalidInput(format!(
                        "message session {} does not match session {}",
                        message.client_session_id, session.client_session_id
                    )));
                }
                match upsert_message_conn(&transaction, message, Some(file_id))? {
                    UpsertOutcome::Inserted => summary.messages_inserted += 1,
                    UpsertOutcome::Updated => summary.messages_updated += 1,
                    UpsertOutcome::SkippedStale => summary.messages_skipped_stale += 1,
                }
            }
        }
        transaction.commit()?;
        Ok(summary)
    }
}

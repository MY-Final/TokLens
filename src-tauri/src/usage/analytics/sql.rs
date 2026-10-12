//! Shared token aggregate columns and row mapping.

use super::TokenSums;

pub(super) const SUM_COLUMNS: &str = "COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), \
     COALESCE(SUM(reasoning_tokens), 0), COALESCE(SUM(cache_read_tokens), 0), \
     COALESCE(SUM(cache_write_tokens), 0), COALESCE(SUM(total_tokens), 0), COALESCE(SUM(CASE WHEN client IN ('claude_code', 'dsh', 'kimi_code', 'mimocode', 'pi_desktop', 'copilot', 'mcode') THEN cache_write_tokens ELSE 0 END), 0)";

pub(super) fn read_sums(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<TokenSums> {
    Ok(TokenSums {
        input_tokens: row.get(offset)?,
        output_tokens: row.get(offset + 1)?,
        reasoning_tokens: row.get(offset + 2)?,
        cache_read_tokens: row.get(offset + 3)?,
        cache_write_tokens: row.get(offset + 4)?,
        total_tokens: row.get(offset + 5)?,
        counted_cache_write_tokens: row.get(offset + 6)?,
    })
}

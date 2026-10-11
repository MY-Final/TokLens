//! UTC timestamp parsing and system clock access.

/// 解析客户端时间戳（ISO 8601 UTC，`YYYY-MM-DDTHH:MM:SS[.fff]Z`）。
///
/// 只支持本机实际出现的 UTC `Z` 形式；其它形式返回 `None`（不猜测时区偏移）。
pub fn parse_iso8601_utc_ms(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if *bytes.last()? != b'Z' {
        return None;
    }
    let year: i64 = value.get(0..4)?.parse().ok()?;
    let month: i64 = value.get(5..7)?.parse().ok()?;
    let day: i64 = value.get(8..10)?.parse().ok()?;
    let hour: i64 = value.get(11..13)?.parse().ok()?;
    let minute: i64 = value.get(14..16)?.parse().ok()?;
    let second: i64 = value.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis: i64 = if bytes.len() > 20 {
        if bytes[19] != b'.' {
            return None;
        }
        let fraction = value.get(20..value.len() - 1)?;
        if fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut digits = fraction.to_string();
        while digits.len() < 3 {
            digits.push('0');
        }
        digits.parse().ok()?
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000) + millis)
}

/// Howard Hinnant 的 `days_from_civil`：返回 1970-01-01 以来的天数。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// 当前时间（Unix 毫秒）。取不到系统时间时返回 0，不做 panic。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_iso8601_utc_with_and_without_millis() {
        assert_eq!(parse_iso8601_utc_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso8601_utc_ms("2026-10-11T07:37:14.576Z"),
            Some(1_791_704_234_576)
        );
        assert_eq!(
            parse_iso8601_utc_ms("2026-05-24T12:55:06.571Z"),
            Some(1_779_627_306_571)
        );
        assert_eq!(
            parse_iso8601_utc_ms("2024-02-29T00:00:00Z"),
            Some(1_709_164_800_000)
        );
    }

    #[test]
    fn rejects_unsupported_timestamp_forms() {
        for value in [
            "",
            "2026-10-11",
            "2026-10-11T07:37:14+08:00",
            "2026-10-11T07:37:14.123456Z",
            "2026-13-01T00:00:00Z",
            "2026-10-11T25:00:00Z",
            "not-a-date-at-all-here",
        ] {
            assert_eq!(parse_iso8601_utc_ms(value), None, "should reject {value}");
        }
    }
}

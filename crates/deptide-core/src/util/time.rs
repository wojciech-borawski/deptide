use chrono::{DateTime, Local, SecondsFormat, Utc};

pub fn now_ms() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

pub fn iso_timestamp(moment: DateTime<Utc>) -> String {
    moment.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn file_stamp(moment: DateTime<Utc>) -> String {
    moment
        .with_timezone(&Local)
        .format("%Y-%m-%dT%H-%M-%S")
        .to_string()
}

pub fn format_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{}.{:01}s", seconds, (duration_ms % 1000) / 100)
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshDuration {
    pub days: u32,
    pub seconds: i64,
}

impl FreshDuration {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let Some(days_raw) = raw.strip_suffix('d') else {
            return Err(Self::error());
        };
        if days_raw.is_empty() || days_raw.contains('.') {
            return Err(Self::error());
        }
        let days: u32 = days_raw.parse().map_err(|_| Self::error())?;
        if !(1..=366).contains(&days) {
            return Err(Self::error());
        }
        Ok(Self {
            days,
            seconds: i64::from(days) * 24 * 60 * 60,
        })
    }

    fn error() -> String {
        "--fresh expects a whole-day duration from 1d to 366d".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessCommand {
    Scrape,
    Crawl,
    Embed,
    Ingest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshnessRequest {
    pub command: FreshnessCommand,
    pub every_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum FreshAction {
    List { json: bool },
    RunNow { id: Uuid, json: bool },
    History { id: Uuid, limit: usize, json: bool },
}

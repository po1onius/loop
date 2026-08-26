use crate::MessageContract;
use serde::{Deserialize, Serialize};

pub const EVENT_SEARCH_REFRESH_ROUTING_KEY: &str = "event.search.refresh.v1";
pub const EVENT_SEARCH_REFRESH_MESSAGE_TYPE: &str = "com.loop.event.search.refresh.v1";

/// 通知搜索投影重新读取指定活动的最新状态。
///
/// 消息不携带活动快照：consumer 总是从 PostgreSQL 读取当前数据，因此重复投递
/// 和乱序投递都不会让旧版本覆盖新版本。活动不存在或不再公开时会删除索引文档。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventSearchRefreshV1 {
    pub event_id: i64,
}

impl MessageContract for EventSearchRefreshV1 {
    const MESSAGE_TYPE: &'static str = EVENT_SEARCH_REFRESH_MESSAGE_TYPE;
    const ROUTING_KEY: &'static str = EVENT_SEARCH_REFRESH_ROUTING_KEY;

    fn subject(&self) -> String {
        format!("events/{}", self.event_id)
    }

    fn validate_data(&self) -> Result<(), &'static str> {
        if self.event_id <= 0 {
            return Err("event_id must be positive");
        }
        Ok(())
    }
}

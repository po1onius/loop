use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const EVENT_CONTENT_VERSION_V1: i32 = 1;

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "dto.ts")]
pub enum EventStatus {
    Draft,
    Published,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "dto.ts")]
pub enum EventTextColor {
    Accent,
    Warning,
    Success,
    Muted,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "dto.ts")]
pub enum EventTextMark {
    Bold,
    Italic,
    Underline,
    Color { value: EventTextColor },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "dto.ts")]
pub enum EventInlineNode {
    Text {
        text: String,
        #[serde(default)]
        marks: Vec<EventTextMark>,
    },
    Hashtag {
        text: String,
        tag_id: Option<String>,
    },
    Mention {
        user_id: String,
        label: String,
    },
    Link {
        text: String,
        url: String,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct EventContentImage {
    pub asset_id: String,
    pub width: i32,
    pub height: i32,
    pub alt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "feature_type", rename_all = "snake_case")]
#[ts(export_to = "dto.ts")]
pub enum EventFeatureBlock {
    Schedule {
        title: String,
        items: Vec<String>,
    },
    Location {
        title: String,
        address: Option<String>,
    },
    Notice {
        title: String,
        items: Vec<String>,
    },
    Ticket {
        title: String,
        description: String,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "dto.ts")]
pub enum EventContentBlock {
    Heading {
        id: String,
        level: i32,
        children: Vec<EventInlineNode>,
    },
    Paragraph {
        id: String,
        children: Vec<EventInlineNode>,
    },
    Quote {
        id: String,
        children: Vec<EventInlineNode>,
    },
    Image {
        id: String,
        item: EventContentImage,
        caption: Option<String>,
    },
    ImageGrid {
        id: String,
        items: Vec<EventContentImage>,
    },
    Divider {
        id: String,
    },
    Feature {
        id: String,
        feature: EventFeatureBlock,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct EventContentDoc {
    pub version: i32,
    pub blocks: Vec<EventContentBlock>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct CreateEventRequest {
    pub title: String,
    pub content: EventContentDoc,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct CreateEventDraftRequest {
    pub title: Option<String>,
    pub content: Option<EventContentDoc>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct UpdateEventDraftRequest {
    pub title: String,
    pub content: EventContentDoc,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct EventResp {
    pub event_id: String,
    pub creator_id: String,
    pub title: String,
    pub status: EventStatus,
    pub content_version: i32,
    pub content: EventContentDoc,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct ListEventsResp {
    pub items: Vec<EventResp>,
    pub next_offset: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct CreateMediaUploadRequest {
    pub mime_type: String,
    pub byte_size: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct PresignedHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct CreateMediaUploadResp {
    pub asset_id: String,
    pub storage_key: String,
    pub upload_method: String,
    pub upload_url: String,
    pub upload_headers: Vec<PresignedHeader>,
    pub expires_in: i32,
    pub public_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct MediaAssetResp {
    pub asset_id: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: String,
    pub public_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct CompleteMediaUploadResp {
    pub asset: MediaAssetResp,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct MediaDownloadUrlResp {
    pub asset_id: String,
    pub download_method: String,
    pub download_url: String,
    pub download_headers: Vec<PresignedHeader>,
    pub expires_in: i32,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct LoginRequest {
    pub account: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct LoginResp {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub refresh_exp: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct SetTokenRequest {
    pub user_id: String,
    pub num: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct RegisterRequest {
    pub username: String,
    pub account: String,
    pub pwd: String,
    pub verify_code: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct VerifyCodeRequest {
    pub account: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "dto.ts")]
pub struct VerifyCodeResp {}

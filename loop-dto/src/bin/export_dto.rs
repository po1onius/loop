use std::path::PathBuf;

use loop_dto::{
    CreateEventRequest, EventContentBlock, EventContentDoc, EventContentImage, EventFeatureBlock,
    EventInlineNode, EventResp, EventStatus, EventTextColor, EventTextMark, ListEventsResp,
    LoginRequest, LoginResp, RefreshTokenRequest, RegisterRequest, SetTokenRequest,
    VerifyCodeRequest, VerifyCodeResp,
};
use ts_rs::{Config, TS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let export_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("loop-dto should be under repo root")
        .join("app")
        .join("lib");

    let cfg = Config::new().with_out_dir(export_dir);

    EventStatus::export(&cfg)?;
    EventTextColor::export(&cfg)?;
    EventTextMark::export(&cfg)?;
    EventInlineNode::export(&cfg)?;
    EventContentImage::export(&cfg)?;
    EventFeatureBlock::export(&cfg)?;
    EventContentBlock::export(&cfg)?;
    EventContentDoc::export(&cfg)?;
    CreateEventRequest::export(&cfg)?;
    EventResp::export(&cfg)?;
    ListEventsResp::export(&cfg)?;
    LoginRequest::export(&cfg)?;
    LoginResp::export(&cfg)?;
    RefreshTokenRequest::export(&cfg)?;
    RegisterRequest::export(&cfg)?;
    SetTokenRequest::export(&cfg)?;
    VerifyCodeRequest::export(&cfg)?;
    VerifyCodeResp::export(&cfg)?;

    Ok(())
}

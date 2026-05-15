use std::path::PathBuf;

use loop_dto::{
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

    LoginRequest::export(&cfg)?;
    LoginResp::export(&cfg)?;
    RefreshTokenRequest::export(&cfg)?;
    RegisterRequest::export(&cfg)?;
    SetTokenRequest::export(&cfg)?;
    VerifyCodeRequest::export(&cfg)?;
    VerifyCodeResp::export(&cfg)?;

    Ok(())
}

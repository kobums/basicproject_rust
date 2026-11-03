// 이미지 업로드 (직접 관리하는 파일, gomachine 은 만들지 않는다). gomachine README "인증 API 계약" 과
// basicproject_go global/upload 와 같은 규칙이다.
//
//   POST /api/upload/index  (Authorization: Bearer, multipart 파일 field "upload")
//     → 200 {filename, url}   url = /uploads/{filename}
//     → 400 파일 없음 / 허용되지 않는 형식 / 5MB 초과 / multipart 아님, 401 토큰 없음, 413 본문 20MB 초과
//   GET /uploads/{filename}  저장한 파일 정적 서빙 (tower-http ServeDir)
//
// 파일은 {UPLOAD_PATH (기본 webdata)}/uploads/ 에 서버가 만든 임의 이름 + 내용 기준 확장자로 저장한다
// (클라이언트 파일 이름은 쓰지 않는다). 정적 서빙되는 경로라 확장자 화이트리스트 + 매직 바이트 검사로
// 이미지 밖의 파일(.html, .svg 등)을 막는다. 에러 본문은 인증 API 와 같은 {status, message, fieldErrors, timestamp}.

use std::path::{Path, PathBuf};

use axum::extract::Multipart;
use axum::extract::multipart::MultipartRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;

use crate::auth_api::{MSG_AUTH_REQUIRED, error_json};
use crate::support::Session;

/// multipart 파일 field 이름.
pub const FIELD: &str = "upload";
/// 이미지 최대 크기 (5MB, basicproject_go upload.MaxImageSize).
pub const MAX_IMAGE_SIZE: usize = 5 << 20;
/// 요청 본문 최대 크기 (20MB, basicproject_go services.BodyLimit). main.rs 가 /api 전체에 건다.
pub const BODY_LIMIT: usize = 20 << 20;
/// UPLOAD_PATH 기본값 (basicproject_go config.UploadPath 와 같다). 파일은 그 아래 uploads/ 에 둔다.
pub const DEFAULT_UPLOAD_PATH: &str = "webdata";
/// 정적 서빙 경로.
pub const URL_PREFIX: &str = "/uploads";

pub const MSG_NO_FILE: &str = "파일이 없습니다.";
pub const MSG_TOO_LARGE: &str = "이미지 크기는 5MB 이하여야 합니다.";
pub const MSG_BAD_EXT: &str = "허용되지 않는 파일 형식입니다. (jpg, jpeg, png, gif, webp 만 가능)";
pub const MSG_NOT_IMAGE: &str = "이미지 파일만 업로드할 수 있습니다.";
pub const MSG_BAD_REQUEST: &str = "잘못된 요청입니다.";
pub const MSG_BODY_TOO_LARGE: &str = "요청 본문이 너무 큽니다.";
pub const MSG_SAVE_FAILED: &str = "파일을 저장할 수 없습니다.";

const ALLOWED_EXT: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

/// 업로드 디렉터리: {UPLOAD_PATH}/uploads (UPLOAD_PATH 가 비어 있으면 webdata).
pub fn upload_dir(upload_path: Option<&str>) -> PathBuf {
    let root = upload_path.map(str::trim).filter(|v| !v.is_empty()).unwrap_or(DEFAULT_UPLOAD_PATH);
    Path::new(root).join("uploads")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UploadResponse {
    pub filename: String,
    pub url: String,
}

/// POST /upload/index 경로. dir 은 저장 디렉터리 (없으면 첫 업로드 때 만든다).
pub fn routes<S>(dir: PathBuf) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route(
        "/upload/index",
        post(
            move |session: Option<Session>, multipart: Result<Multipart, MultipartRejection>| async move {
                upload(&dir, session, multipart).await
            },
        ),
    )
}

/// 파일 이름 확장자 (소문자)가 허용 목록에 있는지.
pub fn allowed_extension(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ALLOWED_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

/// 내용(매직 바이트)으로 판별한 저장 확장자. 이미지가 아니면 None.
pub fn sniff_image(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some("gif")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// 업로드 파일 검사 (basicproject_go upload.ValidateImage): 크기, 확장자, 내용. 저장 확장자를 돌려준다.
pub fn validate_image(filename: &str, data: &[u8]) -> Result<&'static str, &'static str> {
    if data.len() > MAX_IMAGE_SIZE {
        return Err(MSG_TOO_LARGE);
    }
    if !allowed_extension(filename) {
        return Err(MSG_BAD_EXT);
    }
    sniff_image(data).ok_or(MSG_NOT_IMAGE)
}

/// 서버가 만드는 저장 이름 (임의 128 비트 hex + 확장자).
fn new_filename(ext: &str) -> Option<String> {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).ok()?;
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{hex}.{ext}"))
}

// 실패 = (HTTP 상태, 메시지).
type Failure = (StatusCode, &'static str);

fn multipart_error(status: StatusCode) -> Failure {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        (status, MSG_BODY_TOO_LARGE)
    } else {
        (StatusCode::BAD_REQUEST, MSG_BAD_REQUEST)
    }
}

/// "upload" field 의 파일 이름과 내용. 크기가 MAX_IMAGE_SIZE 를 넘으면 그 자리에서 멈춘다.
async fn read_file(mut multipart: Multipart) -> Result<(String, Vec<u8>), Failure> {
    while let Some(mut field) = multipart.next_field().await.map_err(|e| multipart_error(e.status()))? {
        if field.name() != Some(FIELD) {
            continue;
        }
        let Some(filename) = field.file_name().map(str::to_string) else {
            // 파일이 아닌 텍스트 field
            return Err((StatusCode::BAD_REQUEST, MSG_NO_FILE));
        };
        let mut data = Vec::new();
        while let Some(chunk) = field.chunk().await.map_err(|e| multipart_error(e.status()))? {
            if data.len() + chunk.len() > MAX_IMAGE_SIZE {
                return Err((StatusCode::BAD_REQUEST, MSG_TOO_LARGE));
            }
            data.extend_from_slice(&chunk);
        }
        if data.is_empty() {
            return Err((StatusCode::BAD_REQUEST, MSG_NO_FILE));
        }
        return Ok((filename, data));
    }
    Err((StatusCode::BAD_REQUEST, MSG_NO_FILE))
}

async fn upload(dir: &Path, session: Option<Session>, multipart: Result<Multipart, MultipartRejection>) -> Response {
    if session.is_none() {
        return error_json(StatusCode::UNAUTHORIZED, MSG_AUTH_REQUIRED);
    }
    let Ok(multipart) = multipart else {
        return error_json(StatusCode::BAD_REQUEST, MSG_BAD_REQUEST);
    };
    let (original, data) = match read_file(multipart).await {
        Ok(v) => v,
        Err((status, msg)) => return error_json(status, msg),
    };
    let ext = match validate_image(&original, &data) {
        Ok(ext) => ext,
        Err(msg) => return error_json(StatusCode::BAD_REQUEST, msg),
    };
    let Some(filename) = new_filename(ext) else {
        return error_json(StatusCode::INTERNAL_SERVER_ERROR, MSG_SAVE_FAILED);
    };
    let path = dir.join(&filename);
    let saved = async {
        tokio::fs::create_dir_all(dir).await?;
        tokio::fs::write(&path, &data).await
    };
    if let Err(e) = saved.await {
        tracing::error!(error = %e, path = %path.display(), "upload save failed");
        return error_json(StatusCode::INTERNAL_SERVER_ERROR, MSG_SAVE_FAILED);
    }
    let url = format!("{URL_PREFIX}/{filename}");
    (StatusCode::OK, Json(UploadResponse { filename, url })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn dir_from_env() {
        assert_eq!(upload_dir(None), Path::new("webdata/uploads"));
        assert_eq!(upload_dir(Some(" ")), Path::new("webdata/uploads"));
        assert_eq!(upload_dir(Some("/data")), Path::new("/data/uploads"));
    }

    #[test]
    fn image_checks() {
        assert_eq!(validate_image("a.PNG", PNG), Ok("png"));
        // 확장자와 내용이 달라도 내용 기준 확장자로 저장한다
        assert_eq!(validate_image("a.jpg", PNG), Ok("png"));
        assert_eq!(validate_image("a.jpeg", &[0xFF, 0xD8, 0xFF, 0xE0]), Ok("jpg"));
        assert_eq!(validate_image("a.gif", b"GIF89a...."), Ok("gif"));
        assert_eq!(validate_image("a.webp", b"RIFF\0\0\0\0WEBPVP8 "), Ok("webp"));
        assert_eq!(validate_image("a.html", PNG), Err(MSG_BAD_EXT));
        assert_eq!(validate_image("a.svg", b"<svg/>"), Err(MSG_BAD_EXT));
        assert_eq!(validate_image("noext", PNG), Err(MSG_BAD_EXT));
        assert_eq!(validate_image("a.png", b"<html><script>"), Err(MSG_NOT_IMAGE));
        let big = vec![0xFFu8; MAX_IMAGE_SIZE + 1];
        assert_eq!(validate_image("a.jpg", &big), Err(MSG_TOO_LARGE));
    }

    #[test]
    fn random_names() {
        let a = new_filename("png").unwrap();
        assert_eq!(a.len(), 32 + 4);
        assert!(a.ends_with(".png"));
        assert_ne!(a, new_filename("png").unwrap());
    }
}

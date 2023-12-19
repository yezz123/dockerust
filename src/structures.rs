use std::collections::HashMap;

#[derive(serde::Serialize)]
#[allow(non_camel_case_types)]
pub enum DockerErrorMessageType {
    BLOB_UNKNOWN,
    BLOB_UPLOAD_INVALID,
    BLOB_UPLOAD_UNKNOWN,
    DIGEST_INVALID,
    MANIFEST_BLOB_UNKNOWN,
    MANIFEST_INVALID,
    MANIFEST_UNKNOWN,
    MANIFEST_UNVERIFIED,
    NAME_INVALID,
    NAME_UNKNOWN,
    SIZE_INVALID,
    TAG_INVALID,
    UNAUTHORIZED,
    DENIED,
    UNSUPPORTED,
}

#[derive(serde::Serialize)]
pub struct DockerError {
    pub code: DockerErrorMessageType,
    pub message: String,
    pub detail: HashMap<String, String>,
}

#[derive(serde::Serialize)]
pub struct DockerErrorResponse {
    errors: Vec<DockerError>,
}

impl DockerErrorResponse {
    pub fn new_simple(code: DockerErrorMessageType, msg: &str) -> Self {
        Self {
            errors: vec![
                DockerError {
                    code,
                    message: msg.to_string(),
                    detail: HashMap::new(),
                }
            ]
        }
    }
}

#[derive(serde::Serialize)]
pub struct DockerTagsList {
    pub name: String,
    pub tags: Vec<String>,
}

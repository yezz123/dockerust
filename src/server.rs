use actix_web::body::SizedStream;
use actix_web::http::Method;
use actix_web::web::Data;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpResponseBuilder, HttpServer};
use base64::{engine::general_purpose as b64decoder, Engine as _};
use futures::StreamExt;
use jsonwebtoken::{encode, Validation};
use regex::Regex;
use std::cmp::min;
use std::collections::HashSet;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::str::FromStr;
use uuid::Uuid;

use crate::api::{
    DockerCatalog, DockerErrorMessageType, DockerErrorResponse, DockerTagsList,
};
use crate::constants::AUTH_TOKENS_DURATION;
use crate::storage::{clean_storage, get_docker_images_list, BlobReference, DockerImage};
use crate::docker::DockerManifestOrManifestList;
use crate::read_file_stream::ReadFileStream;
use crate::utils::{create_empty_file, sha256sum, sha256sum_str, time};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Credentials {
    pub user_name: String,
    pub password_hash: String,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub storage_path: PathBuf,
    pub listen_address: String,
    pub access_url: String,
    pub app_secret: String,
    pub credentials: Vec<Credentials>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct InvalidAuthResponse {
    details: &'static str,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct AuthResponse {
    token: String,
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct JWTClaims {
    user: Option<String>,
    timeout: u64,
}

impl ServerConfig {
    pub fn need_auth(&self) -> bool {
        !self.credentials.is_empty()
    }

    fn get_encoding_secret(&self) -> jsonwebtoken::EncodingKey {
        jsonwebtoken::EncodingKey::from_secret(self.app_secret.as_ref())
    }

    fn get_decoding_secret(&self) -> jsonwebtoken::DecodingKey {
        jsonwebtoken::DecodingKey::from_secret(self.app_secret.as_ref())
    }

    fn jwt_algorithm(&self) -> jsonwebtoken::Algorithm {
        jsonwebtoken::Algorithm::HS512
    }

    fn get_auth_validation_algorithm(&self) -> jsonwebtoken::Validation {
        let mut val = Validation::new(self.jwt_algorithm());
        val.validate_exp = false;
        val.required_spec_claims = HashSet::new();
        val
    }

    pub fn check_auth(&self, user: &str, password: &str) -> bool {
        for cred in &self.credentials {
            if cred.user_name.eq(user)
                && bcrypt::verify(password, &cred.password_hash).unwrap_or(false)
            {
                return true;
            }
        }

        false
    }
}

fn ok_or_internal_error<E>(r: Result<HttpResponse, E>) -> HttpResponse
where
    E: Error,
{
    match r {
        Ok(e) => e,
        Err(e) => {
            println!("Error! {}", e);
            HttpResponse::InternalServerError().body("500 Internal Server Error")
        }
    }
}

fn request_auth(conf: &ServerConfig, error: Option<&'static str>) -> HttpResponse {
    let realm = format!("{}/token", conf.access_url);
    let service = conf
        .access_url
        .split("://")
        .last()
        .unwrap_or("dockerust");

    let complement = match error {
        None => "".to_string(),
        Some(e) => format!(",error=\"{}\"", e),
    };

    HttpResponse::Unauthorized()
        .insert_header((
            "WWW-Authenticate",
            format!(
                "Bearer realm=\"{}\",service=\"{}\",scope=\"access\"{}",
                realm, service, complement
            ),
        ))
        .json(DockerErrorResponse::new_simple(
            DockerErrorMessageType::UNAUTHORIZED,
            "please authenticate",
        ))
}

fn check_auth(
    req: &HttpRequest,
    conf: &ServerConfig,
    user: &mut Option<String>,
) -> Option<HttpResponse> {
    if !conf.need_auth() {
        *user = Some("anonymous".to_string());
        return None;
    }

    let auth_part: String = req
        .headers()
        .get("authorization")
        .map(|s| s.to_str().unwrap_or(""))
        .unwrap_or("")
        .to_string()
        .replace("Bearer ", "");

    if auth_part.is_empty() {
        return Some(request_auth(conf, None));
    }

    let token = jsonwebtoken::decode::<JWTClaims>(
        &auth_part,
        &conf.get_decoding_secret(),
        &conf.get_auth_validation_algorithm(),
    );

    let token = match token {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to decode JWT token: {}", e);
            return Some(request_auth(conf, None));
        }
    };

    if token.claims.timeout < time() {
        return Some(request_auth(conf, Some("invalid_token")));
    }

    if let Some(id) = token.claims.user {
        *user = Some(id);
    }

    None
}

fn insufficient_authorizations(conf: &ServerConfig) -> HttpResponse {
    request_auth(conf, Some("insufficient_scope"))
}

async fn get_auth_token(config: web::Data<ServerConfig>, r: HttpRequest) -> HttpResponse {
    ok_or_internal_error::<std::io::Error>((move || {
        let mut user = None;

        let auth_part: String = r
            .headers()
            .get("authorization")
            .map(|s| s.to_str().unwrap_or(""))
            .unwrap_or("")
            .to_string()
            .replace("Basic ", "");

        if !auth_part.is_empty() {
            let decoded = b64decoder::STANDARD.decode(auth_part).unwrap_or_default();
            let decoded = String::from_utf8_lossy(&decoded);
            let split: Vec<&str> = decoded.splitn(2, ':').collect();

            let username = split.first().unwrap_or(&"");
            let password = split.get(1).unwrap_or(&"");

            if config.check_auth(username, password) {
                user = Some(username.to_string());
            } else {
                return Ok(HttpResponse::Unauthorized()
                    .insert_header(("www-authenticate", "Basic realm=\"dockerust\""))
                    .json(InvalidAuthResponse {
                        details: "incorrect username or password",
                    }));
            }
        }

        let claim = JWTClaims {
            user,
            timeout: time() + AUTH_TOKENS_DURATION,
        };

        let token = encode(
            &jsonwebtoken::Header::new(config.jwt_algorithm()),
            &claim,
            &config.get_encoding_secret(),
        )
        .map_err(|_| std::io::Error::new(ErrorKind::Other, "failed to encode token"))?;

        Ok(HttpResponse::Ok().json(AuthResponse {
            access_token: token.to_string(),
            token,
            expires_in: AUTH_TOKENS_DURATION,
        }))
    })())
}

async fn not_found() -> HttpResponse {
    HttpResponse::NotFound().body("404 Not Found")
}

async fn base(config: web::Data<ServerConfig>, r: HttpRequest) -> HttpResponse {
    let mut user = None;
    if let Some(e) = check_auth(&r, &config, &mut user) {
        return e;
    }
    HttpResponse::Ok().finish()
}

#[derive(serde::Deserialize)]
struct CatalogRequest {
    n: Option<usize>,
    last: Option<String>,
}

async fn catalog(req: web::Query<CatalogRequest>, conf: web::Data<ServerConfig>) -> HttpResponse {
    let images = match get_docker_images_list(&conf.storage_path) {
        Ok(images) => images,
        Err(e) => {
            eprintln!("Failed to get the list of images! {:?}", e);
            return HttpResponse::InternalServerError().json("500 Internal Error");
        }
    };

    if images.is_empty() {
        return HttpResponse::Ok().json(DockerCatalog {
            repositories: vec![],
        });
    }

    let start = match &req.last {
        None => 0,
        Some(s) => images
            .iter()
            .position(|f| f.eq(s))
            .map(|f| f + 1)
            .unwrap_or(0),
    };
    let end = start + req.n.unwrap_or(images.len() + 1);

    HttpResponse::Ok().json(DockerCatalog {
        repositories: images[min(start, images.len() - 1)..min(images.len(), end)].to_vec(),
    })
}

fn get_tags_list(image: &DockerImage) -> std::io::Result<HttpResponse> {
    if !image.image_path().exists() {
        return Ok(
            HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::NAME_UNKNOWN,
                "repository name not known to registry",
            )),
        );
    }

    let tags = image.tags_list()?;

    Ok(HttpResponse::Ok().json(DockerTagsList {
        name: image.image.to_string(),
        tags,
    }))
}

async fn serve_blob(
    blob_ref: &BlobReference,
    image: &DockerImage,
    content_type: &str,
) -> std::io::Result<HttpResponse> {
    let blob_path = blob_ref.data_path(&image.storage_path);

    if !blob_path.exists() {
        return Ok(
            HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::BLOB_UNKNOWN,
                "blob not found",
            )),
        );
    }

    let blob_len = blob_path.metadata()?.len();

    let mut response = HttpResponse::Ok();
    response
        .content_type(content_type)
        .insert_header(("Docker-Content-Digest", blob_ref.to_digest()))
        .insert_header(("Etag", blob_ref.to_digest()));

    Ok(response.body(SizedStream::new(blob_len, ReadFileStream::new(&blob_path)?)))
}

async fn get_manifest(image: &DockerImage, image_ref: &str) -> std::io::Result<HttpResponse> {
    // Requested hash is included in the request
    let blob_ref = if image_ref.starts_with("sha256") {
        BlobReference::from_str(image_ref)?
    }
    // We must find ourselves the blob to load
    else {
        let manifest_path = image.manifest_tag_link_path(image_ref);

        if !manifest_path.exists() {
            return Ok(
                HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                    DockerErrorMessageType::MANIFEST_UNKNOWN,
                    "manifest unknown",
                )),
            );
        }

        BlobReference::from_file(&manifest_path)?
    };

    if !image.manifests_revision_list()?.contains(&blob_ref) {
        return Ok(
            HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::MANIFEST_BLOB_UNKNOWN,
                "manifest blob not attached to manifest",
            )),
        );
    }

    // Load manifest to get its type
    let manifest: DockerManifestOrManifestList = serde_json::from_str(&std::fs::read_to_string(
        blob_ref.data_path(&image.storage_path),
    )?)?;

    serve_blob(&blob_ref, image, &manifest.mediaType).await
}

async fn put_manifest(
    image: &DockerImage,
    image_ref: &str,
    mut payload: web::Payload,
    conf: &ServerConfig,
) -> std::io::Result<HttpResponse> {
    // Get manifest data
    let mut bytes = web::BytesMut::new();
    while let Some(item) = payload.next().await {
        bytes.extend_from_slice(&item.map_err(|_| {
            std::io::Error::new(ErrorKind::Other, "Failed to read a chunk of data")
        })?);
    }

    let manifest = String::from_utf8(bytes.as_ref().to_vec()).map_err(|_| {
        std::io::Error::new(
            ErrorKind::Other,
            "Failed to turn the manifest into a string",
        )
    })?;

    let blob_ref = BlobReference::from_sha256sum(sha256sum_str(&manifest)?);

    // Write manifest
    let blob_path = blob_ref.data_path(&conf.storage_path);
    create_empty_file(&blob_path)?;
    std::fs::write(blob_path, manifest)?;

    // Write references to manifest
    let mut list = vec![image.manifest_revision_path(&blob_ref)];

    // Add a tag only if it is not a valid digest
    if !BlobReference::is_valid_reference(image_ref) {
        list.push(image.manifest_tag_link_path(image_ref));
    }

    for manifest_path in list {
        create_empty_file(&manifest_path)?;
        std::fs::write(manifest_path, blob_ref.to_digest())?;
    }

    let location = format!(
        "{}/v2/{}/manifests/{}",
        conf.access_url,
        image.image,
        blob_ref.to_digest()
    );

    Ok(HttpResponse::Created()
        .insert_header(("Docker-Content-Digest", blob_ref.to_digest()))
        .insert_header(("location", location))
        .finish())
}

async fn delete_manifest(
    image: &DockerImage,
    digest: &str,
    conf: &ServerConfig,
) -> std::io::Result<HttpResponse> {
    let blob = BlobReference::from_str(digest)?;

    if !image.manifests_revision_list()?.contains(&blob) {
        return Ok(
            HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::MANIFEST_BLOB_UNKNOWN,
                "manifest blob not attached to manifest",
            )),
        );
    }

    // Remove tags
    for tag in image.get_tags_attached_to_manifest_blob(&blob)? {
        std::fs::remove_dir_all(image.tags_path().join(tag))?;
    }

    // Remove reference
    std::fs::remove_file(image.manifest_revision_path(&blob))?;

    // Run garbage collector
    clean_storage(&conf.storage_path)?;

    Ok(HttpResponse::Accepted().finish())
}

async fn get_blob(image: &DockerImage, digest: &str) -> std::io::Result<HttpResponse> {
    // Requested hash is included in the request
    serve_blob(
        &BlobReference::from_str(digest)?,
        image,
        "application/octet-stream",
    )
    .await
}

async fn delete_blob(_image: &DockerImage, _digest: &str) -> std::io::Result<HttpResponse> {
    Ok(
        HttpResponse::MethodNotAllowed().json(DockerErrorResponse::new_simple(
            DockerErrorMessageType::UNSUPPORTED,
            "blobs are automatically garbage collected",
        )),
    )
}

fn blob_upload_response(
    mut res: HttpResponseBuilder,
    image: &DockerImage,
    uuid: &str,
    config: &ServerConfig,
) -> std::io::Result<HttpResponse> {
    let location = format!(
        "{}/v2/{}/blobs/uploads/{}",
        config.access_url, &image.image, uuid
    );

    let offset = match std::fs::metadata(image.upload_storage_path(uuid))?.len() {
        0 => 0,
        s => s - 1,
    };

    Ok(res
        .insert_header(("Range", format!("0-{}", offset)))
        .insert_header(("Location", location))
        .insert_header(("Docker-Upload-Uuid", uuid))
        .finish())
}

async fn start_blob_upload(
    image: &DockerImage,
    config: &ServerConfig,
) -> std::io::Result<HttpResponse> {
    let uuid = Uuid::new_v4().to_string();
    let path = image.upload_storage_path(&uuid);

    create_empty_file(&path)?;

    blob_upload_response(HttpResponse::Accepted(), image, &uuid, config)
}

fn blob_upload_status(
    image: &DockerImage,
    uuid: &str,
    config: &ServerConfig,
) -> std::io::Result<HttpResponse> {
    if !image.upload_storage_path(uuid).exists() {
        return Ok(
            HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::BLOB_UNKNOWN,
                "blob unknown",
            )),
        );
    }

    blob_upload_response(HttpResponse::NoContent(), image, uuid, config)
}

async fn process_blob_upload(
    image: &DockerImage,
    uuid: &str,
    mut payload: web::Payload,
) -> std::io::Result<Option<HttpResponse>> {
    let payload_path = image.upload_storage_path(uuid);

    if !payload_path.exists() {
        return Ok(Some(HttpResponse::NotFound().json(
            DockerErrorResponse::new_simple(DockerErrorMessageType::BLOB_UNKNOWN, "blob unknown"),
        )));
    }

    // Open file
    let mut file = OpenOptions::new()
        .append(true)
        .open(image.upload_storage_path(uuid))?;

    while let Some(chunk) = payload.next().await {
        match chunk {
            Ok(c) => {
                file.write_all(&c)?;
            }
            Err(e) => {
                eprintln!("Failed to read from blob upload request! {:?}", e);
                return Ok(Some(
                    HttpResponse::InternalServerError().json("500 Internal Server Error"),
                ));
            }
        }
    }

    file.flush()?;
    drop(file);

    Ok(None)
}

async fn blob_upload_patch(
    image: &DockerImage,
    uuid: &str,
    config: &ServerConfig,
    payload: web::Payload,
) -> std::io::Result<HttpResponse> {
    if let Some(res) = process_blob_upload(image, uuid, payload).await? {
        return Ok(res);
    }

    blob_upload_response(HttpResponse::Accepted(), image, uuid, config)
}

async fn blob_upload_finish(
    image: &DockerImage,
    uuid: &str,
    config: &ServerConfig,
    payload: web::Payload,
    digest: &str,
) -> std::io::Result<HttpResponse> {
    // Process last chunk
    if let Some(res) = process_blob_upload(image, uuid, payload).await? {
        return Ok(res);
    }

    // Process chunk digest
    let computed_digest = format!("sha256:{}", sha256sum(&image.upload_storage_path(uuid))?);
    if !computed_digest.eq(digest) {
        return Ok(
            HttpResponse::BadRequest().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::DIGEST_INVALID,
                "invalid digest",
            )),
        );
    }

    // Move blob to its destination
    let dest = BlobReference::from_str(digest)?.data_path(&config.storage_path);
    create_empty_file(&dest)?;
    std::fs::rename(image.upload_storage_path(uuid), &dest)?;

    let end_of_blob_range = std::fs::metadata(&dest)?.len() - 1;

    let location = format!("{}/v2/{}/blobs/{}", config.access_url, &image.image, digest);

    Ok(HttpResponse::Created()
        .insert_header(("Content-Range", format!("0-{}", end_of_blob_range)))
        .insert_header(("Docker-Content-Digest", digest))
        .insert_header(("Location", location))
        .finish())
}

fn cancel_blob_upload(image: &DockerImage, uuid: &str) -> std::io::Result<HttpResponse> {
    if !image.upload_storage_path(uuid).exists() {
        return Ok(
            HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::BLOB_UNKNOWN,
                "blob unknown",
            )),
        );
    }

    std::fs::remove_file(image.upload_storage_path(uuid))?;

    Ok(HttpResponse::NoContent()
        .insert_header(("content-length", "0"))
        .finish())
}

#[derive(serde::Deserialize)]
struct RequestQuery {
    digest: Option<String>,
}

async fn requests_dispatcher(
    r: HttpRequest,
    config: web::Data<ServerConfig>,
    payload: web::Payload,
    query: web::Query<RequestQuery>,
) -> HttpResponse {
    let mut user = None;
    if let Some(e) = check_auth(&r, &config, &mut user) {
        return e;
    }

    let parts = r.uri().path().split('/').skip(2).collect::<Vec<_>>();
    if parts.len() < 3 {
        return not_found().await;
    }

    // Get tags list `/v2/<name>/tags/list`
    if r.uri().path().ends_with("/tags/list") {
        let image = DockerImage::new(&config.storage_path, &parts[..parts.len() - 2].join("/"));

        return ok_or_internal_error(get_tags_list(&image));
    }
    // Manifest manipulation `/v2/<name>/manifests/<reference>`
    else if parts[parts.len() - 2].eq("manifests") {
        let image = DockerImage::new(&config.storage_path, &parts[..parts.len() - 2].join("/"));
        let image_ref = parts.last().unwrap();

        // Get manifest
        match *r.method() {
            Method::GET => return ok_or_internal_error(get_manifest(&image, image_ref).await),
            Method::HEAD => return ok_or_internal_error(get_manifest(&image, image_ref).await),
            Method::PUT => {
                if user.is_none() {
                    return insufficient_authorizations(&config);
                }

                return ok_or_internal_error(
                    put_manifest(&image, image_ref, payload, &config).await,
                );
            }
            Method::DELETE => {
                if user.is_none() {
                    return insufficient_authorizations(&config);
                }

                return ok_or_internal_error(delete_manifest(&image, image_ref, &config).await);
            }
            _ => {}
        }
    }
    // Blobs manipulation `/v2/<name>/blobs/<digest>`
    else if parts[parts.len() - 2].eq("blobs") {
        let image = DockerImage::new(&config.storage_path, &parts[..parts.len() - 2].join("/"));
        let digest = parts.last().unwrap();

        match *r.method() {
            Method::GET => return ok_or_internal_error(get_blob(&image, digest).await),
            Method::HEAD => return ok_or_internal_error(get_blob(&image, digest).await),
            Method::DELETE => {
                if user.is_none() {
                    return insufficient_authorizations(&config);
                }

                return ok_or_internal_error(delete_blob(&image, digest).await);
            }
            _ => {}
        }
    }
    // Request blobs upload
    else if r.uri().path().ends_with("/blobs/uploads/") {
        if user.is_none() {
            return insufficient_authorizations(&config);
        }

        return ok_or_internal_error(
            start_blob_upload(
                &DockerImage::new(&config.storage_path, &parts[..parts.len() - 3].join("/")),
                &config,
            )
            .await,
        );
    }
    // Manage blogs upload
    else if parts[parts.len() - 3] == "blobs" && parts[parts.len() - 2] == "uploads" {
        if user.is_none() {
            return insufficient_authorizations(&config);
        }

        let image = DockerImage::new(&config.storage_path, &parts[..parts.len() - 3].join("/"));
        let uuid = parts.last().unwrap_or(&"");

        if !Regex::new(r"^[0-9a-zA-Z\-]+$").unwrap().is_match(uuid) {
            return HttpResponse::BadRequest().json("Invalid UUID !");
        }

        match *r.method() {
            Method::GET => return ok_or_internal_error(blob_upload_status(&image, uuid, &config)),
            Method::PATCH => {
                return ok_or_internal_error(
                    blob_upload_patch(&image, uuid, &config, payload).await,
                )
            }
            Method::PUT => {
                return ok_or_internal_error(
                    blob_upload_finish(
                        &image,
                        uuid,
                        &config,
                        payload,
                        query.digest.as_ref().unwrap_or(&String::new()),
                    )
                    .await,
                )
            }
            Method::DELETE => return ok_or_internal_error(cancel_blob_upload(&image, uuid)),
            _ => {}
        }
    }

    not_found().await
}

pub async fn start(config: ServerConfig) -> std::io::Result<()> {
    let listen_address = config.listen_address.to_string();
    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(config.clone()))
            .route("/token", web::to(get_auth_token))
            .route("/v2/", web::get().to(base))
            .route("/v2/_catalog", web::get().to(catalog))
            .route("/v2/{tail:.*}", web::to(requests_dispatcher))
            .route("{tail:.*}", web::to(not_found))
    })
    .bind(listen_address)?
    .run()
    .await
}


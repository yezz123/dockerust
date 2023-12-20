use std::path::PathBuf;

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use actix_web::http::Method;

use crate::structures::{DockerErrorMessageType, DockerErrorResponse, DockerTagsList};
use crate::storage::{BlobReference, DockerImage};
use crate::read_file_stream::ReadFileStream;

#[derive(Clone)]
pub struct ServerConfig {
    pub storage_path: PathBuf,
    pub listen_address: String,
}


fn ok_or_internal_error(r: std::io::Result<HttpResponse>) -> HttpResponse {
    match r {
        Ok(e) => e,
        Err(e) => {
            println!("Error! {}", e);
            HttpResponse::InternalServerError().body("500 Internal Server Error")
        }
    }
}

fn not_found() -> HttpResponse {
    HttpResponse::NotFound().body("404 Not Found")
}

fn base() -> HttpResponse {
    HttpResponse::Ok().finish()
}

fn get_tags_list(image: &DockerImage) -> std::io::Result<HttpResponse> {
    if !image.image_path().exists() {
        return Ok(HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
            DockerErrorMessageType::NAME_UNKNOWN,
            "repository name not known to registry",
        )));
    }

    let mut tags = vec![];
    for dir in std::fs::read_dir(image.tags_path())? {
        let dir = dir?;
        tags.push(dir.file_name().to_string_lossy().to_string());
    }

    Ok(HttpResponse::Ok().json(DockerTagsList {
        name: image.image.to_string(),
        tags,
    }))
}

async fn get_manifest(image: &DockerImage, image_ref: &str, send: bool) -> std::io::Result<HttpResponse> {
    // Requested hash is included in the request
    let blob_ref = if image_ref.starts_with("sha256") {
        BlobReference::from_str(image_ref)?
    }

    // We must find ourselves the blob to load
    else {
        let manifest_path = image.manifest_link_path(image_ref);

        if !manifest_path.exists() {
            return Ok(HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
                DockerErrorMessageType::MANIFEST_UNKNOWN,
                "manifest unknown",
            )));
        }

        BlobReference::from_file(&manifest_path)?
    };

    let manifest_path = blob_ref.data_path(&image.storage_path, &blob_ref);

    Ok(HttpResponse::Ok()
        .content_type("application/vnd.docker.distribution.manifest.v2+json")
        .header("Docker-Content-Digest", blob_ref.to_digest())
        .header("Etag", blob_ref.to_digest())
        .body(match send {
            true => std::fs::read_to_string(manifest_path)?,
            false => "".to_string()
        }))
}

async fn get_digest(image: &DockerImage, digest: &str, send: bool) -> std::io::Result<HttpResponse> {
    // Requested hash is included in the request
    let blob_ref = BlobReference::from_str(digest)?;

    let blob_path = blob_ref.data_path(&image.storage_path, &blob_ref);

    let mut response = HttpResponse::Ok();
    response.content_type("application/octet-stream")
        .header("Docker-Content-Digest", blob_ref.to_digest())
        .header("Etag", blob_ref.to_digest());

    if !send {
        return Ok(response.finish());
    }

    Ok(response.streaming(ReadFileStream::new(&blob_path)?))
}


async fn requests_dispatcher(r: HttpRequest, config: web::Data<ServerConfig>) -> HttpResponse {
    let parts = r.uri().path().split("/").skip(2).collect::<Vec<_>>();
    if parts.len() < 3 {
        return not_found();
    }

    // Get tags list `/v2/<name>/tags/list`
    if r.uri().path().ends_with("/tags/list") {
        let image = DockerImage::new(
            &config.storage_path,
            &parts[..parts.len() - 2].join("/"),
        );

        return ok_or_internal_error(get_tags_list(&image));
    }


    // Manifest manipulation `/v2/<name>/manifests/<reference>`
    if parts[parts.len() - 2].eq("manifests") {
        let image = DockerImage::new(
            &config.storage_path,
            &parts[..parts.len() - 2].join("/"),
        );
        let image_ref = parts.last().unwrap();

        // Get manifest
        match r.method() {
            &Method::GET => {
                return ok_or_internal_error(get_manifest(&image, image_ref, true).await);
            }
            &Method::HEAD => {
                return ok_or_internal_error(get_manifest(&image, image_ref, false).await);
            }
            &_ => {}
        }
    }

        // Blobs manipulation `/v2/<name>/blobs/<digest>`
    if parts[parts.len() - 2].eq("blobs") {
        let image = DockerImage::new(
            &config.storage_path,
            &parts[..parts.len() - 2].join("/"),
        );
        let digest = parts.last().unwrap();

        // Get manifest
        match r.method() {
            &Method::GET => {
                return ok_or_internal_error(get_digest(&image, digest, true).await);
            }
            &Method::HEAD => {
                return ok_or_internal_error(get_digest(&image, digest, false).await);
            }
            &_ => {}
        }
    }

    not_found()
}

pub async fn start(config: ServerConfig) -> std::io::Result<()> {
    let listen_address = config.listen_address.to_string();
    HttpServer::new(move || {
        App::new()
            .data(config.clone())
            .route("/v2/", web::get().to(base))
            .route("/v2/**", web::to(requests_dispatcher))
            .route("**", web::to(not_found))
    })
        .bind(listen_address)?
        .run()
        .await
}

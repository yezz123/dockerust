use std::path::PathBuf;
use actix_web::{HttpServer, web, HttpResponse, App, HttpRequest};
use actix_web::http::Method;
use crate::storage::{DockerImage, BlobReference};
use crate::structures::{DockerErrorResponse, DockerErrorMessageType};

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

async fn get_manifest(image: &DockerImage, image_ref: &str) -> std::io::Result<HttpResponse> {
    let manifest_path = image.manifest_link_path(image_ref);

    if !manifest_path.exists() {
        return Ok(HttpResponse::NotFound().json(DockerErrorResponse::new_simple(
            DockerErrorMessageType::MANIFEST_UNKNOWN,
            "manifest unknown",
        )));
    }

    let blob_ref = BlobReference::from_file(&manifest_path)?;
    let manifest_path = blob_ref.data_path(&image.storage_path, &blob_ref);

    Ok(HttpResponse::Ok()
        .content_type("application/vnd.docker.distribution.manifest.v2+json")
        .header("Docker-Content-Digest", blob_ref.to_digest())
        .header("Etag", blob_ref.to_digest())
        .body(std::fs::read_to_string(manifest_path)?))
}

async fn requests_dispatcher(r: HttpRequest, config: web::Data<ServerConfig>) -> HttpResponse {
    let parts = r.uri().path().split("/").skip(2).collect::<Vec<_>>();
    if parts.len() < 3 {
        return not_found();
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
                return ok_or_internal_error(get_manifest(&image, image_ref).await);
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

use std::path::PathBuf;
use actix_web::{HttpServer, web, HttpResponse, App};

pub struct ServerConfig {
    pub storage_path: PathBuf,
    pub listen_address: String,
}

fn base() -> HttpResponse {
    HttpResponse::Ok().finish()
}

pub async fn start(config: ServerConfig) -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/v2/", web::get().to(base))
    })
        .bind(&config.listen_address)?
        .run()
        .await
}

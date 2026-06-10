mod bom;
mod tree;
mod alt;
mod calc;
mod version;
mod validation;
mod app_state;
mod api;

use actix_web::{web, App, HttpServer};
use dotenvy::dotenv;
use std::env;

use crate::app_state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("PORT must be a valid number");

    let state = web::Data::new(AppState::new());

    log::info!("Starting BOM Parser server on port {}", port);

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .configure(api::configure)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

#[macro_use]
extern crate dotenv;
#[macro_use]
extern crate dotenv_codegen;

use axum::{Router, routing::get};
use std::sync::{Arc};
use tokio::sync::{Mutex}; 

mod redis_service;
mod deezer;
mod private_api_routs;
mod postgres_service;
mod s3_client;

use crate::deezer::Deezer;
use crate::private_api_routs::{get_album, get_stream, get_track_page, get_track_remix, play};

use std::env;
use dotenv::dotenv;
// use aws_sdk_s3::Client as S3Client;
use crate::postgres_service::PostgresDb;
use crate::s3_client::{S3Client};

#[derive(Clone)]
pub struct SharedState{
    deezer: Deezer,
    postgres_db: Arc<PostgresDb>,
    s3: S3Client
}

#[tokio::main]
async fn main() {
    let arl: String = String::from("0a14f4691b0e47c4f31249af0c638afa6d37d79dc7d864c8aab429b642ce362336d6c69578b4eaf539338c52ecec1567f20773422c66b9dbad9d54c2fd86fb5439140550aa94b81f17a69ac29281ee9e582872cbd1614ecf486c22c6b796b618");
    let shared_state = SharedState { 
        deezer: Deezer::new(arl),
        postgres_db: Arc::new(PostgresDb::new("postgres://admin:admin@localhost:5432/musicdb").await),
        s3: S3Client::new("http://localhost:9000", vec!["deezer"]).await
    };
    
    let app = Router::new()
        .route("/track/{id}", get(get_track_page))
        .route("/stream/{id}", get(get_stream))
        .route("/listen/{id}", get(play))
        .route("/album/{id}", get(get_album))
        .route("/mix/{id}", get(get_track_remix)).with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

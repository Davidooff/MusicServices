#[macro_use]
extern crate dotenv;
#[macro_use]
extern crate dotenv_codegen;

use axum::{Router, routing::get};
use std::sync::{Arc};
use tokio::sync::{Mutex}; 

mod redis_service;
mod deezer_domain;
mod deezer_public;
mod private_api_routs;

use crate::deezer_domain::Deezer;
use crate::private_api_routs::{get_track_page, get_track_remix};

use std::env;
use dotenv::dotenv;
use reqwest::Client;
#[derive(Clone)]
pub struct SharedState{
    deezer: Deezer
}

#[tokio::main]
async fn main() {
    // match dotenv() {
    //     Ok(res) => {
    //         println!("Env loaded from environment variables.!");  
    //     }x
    //     Err(err) =>  panic!("{}", err.to_string())
    // }
    
    let arl: String = String::from("3e186b93f5a516e8e9682386d8ab7810da0d4f4c764de7e1332400a2411a1646b6de227c75989a18562a37723e2ae02aa88b41b87d6520a375aa583ae092669be267ee4a6dbbb84dccd1e2b2abcdf9d760fd7b3bd6b358f72b2b7c851f770527");
    let shared_state = SharedState { deezer: Deezer::new(arl) };
    
    let app = Router::new()
        .route("/track/{search_query}", get(get_track_page))
        .route("/mix/{id}", get(get_track_remix)).with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

use axum::extract::{Path, State};
use axum::http::{Method, StatusCode};
use axum::Json;
use axum::response::IntoResponse;
use serde::Serialize;
use crate::SharedState;

#[derive(Serialize)]
struct TrackRemixBodyQuery<'a> {
    sng_id: &'a str,
    start_with_input_track: bool,
}

pub async fn get_track_remix(Path(id): Path<String>, State(state): State<SharedState>) -> Result<impl IntoResponse, StatusCode> {
    let deezer = state.deezer.clone();
    let req_body = format!("{{\"sng_id\": {}, \"start_with_input_track\": true}}", id);
    
    let res = deezer.call(
        Method::POST, 
        "song.getSearchTrackMix",
        true, 
        Some(req_body)
    ).await.map_err(|e| { 
        println!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR 
    })?;
    
    Ok(Json(res))
}

pub async fn get_track_page(Path(id): Path<String>, State(state): State<SharedState>) -> Result<impl IntoResponse, StatusCode> {
    let deezer = state.deezer.clone();
    
    let req_body = format!("{{\"sng_id\": {}, \"start_with_input_track: true\"}}", id);

    let res = deezer.call(
        Method::POST,
        "deezer.pageTrack",
        true,
        Some(req_body)
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(res))
}

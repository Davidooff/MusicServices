use axum::body::Body;
use axum::extract::{FromRef, Path, State};
use axum::http::{header, Method, Response, StatusCode};
use axum::Json;
use axum::response::IntoResponse;
use futures::{SinkExt, Stream, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;
use crate::deezer::{Album, AlbumHeader, ApiError, Deezer, SongFormat};
use crate::SharedState;
use tokio_stream::wrappers::ReceiverStream;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use tokio::join;
use tokio_util::io::{ReaderStream, StreamReader};
use crate::postgres_service::{AlbumInput, AuthorInput, PostgresDb, TrackInput};

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
    let res = deezer.get_track_page(&id).await
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(res))
}

// pub(crate) type BlowfishCbcDec = cbc::Decryptor<Blowfish>;

pub async fn get_stream(
    Path(id): Path<String>,
    State(state): State<SharedState>,
) -> Result<Response<Body>, StatusCode> {
    let deezer = state.deezer.clone();
    let stream = deezer.get_stream(id, None).await.map_err(|e| {
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let body = Body::from_stream(stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"combined_data.flac\"",
        )
        .body(body)
        .map_err(|e| {
            eprintln!("Failed to build response: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
}

impl FromRef<AlbumHeader> for AlbumInput {
    fn from_ref(input: &AlbumHeader) -> Self {
        Self{
            id: input.alb_id.parse::<i32>().unwrap(),
            title: input.alb_title.clone(),
            img: input.img.clone(),
        }
    }
}

pub async fn get_album_and_add_to_db(id: String, state: SharedState) -> Result<Album, StatusCode> {
    let deezer = state.deezer.clone();
    let postgres = state.postgres_db.clone();

    let res = deezer.get_album(id).await
      .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)?;

    postgres.add_album_by_album(&res).await;

    Ok(res)
}


pub async fn get_album(Path(id): Path<String>, State(state): State<SharedState>) -> Result<impl IntoResponse, StatusCode> {
    Ok(Json(get_album_and_add_to_db(id, state).await?))
}

pub async fn record_listening(state: SharedState, id: i32, alb_id: Option<String>) -> Result<bool, StatusCode> {
    let postgres = state.postgres_db.clone();

    let record = postgres.record_listening(id).await.map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match record {
        true => Ok(true),
        false => {
            let deezer = state.deezer.clone();
            
            let alb_id = match alb_id {
                Some(id) => id,
                None => deezer.get_track_page(&id.to_string()).await.map_err(|e| {
                    eprintln!("{}", e);
                    StatusCode::BAD_REQUEST
                })?.alb_id
            };

            get_album_and_add_to_db(alb_id, state).await?;

            let record = postgres.record_listening(id).await.map_err(|e| {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            Ok(record)
        },
    }
}


const FLAC: &str = "flac";
const MP3: &str = "mp3";


pub async fn create_stream_from_body(body: Body, data_fromat: SongFormat) -> Result<Response<Body>, StatusCode> {
    let format = match data_fromat {
        SongFormat::FLAC => FLAC,
        SongFormat::MP3 => MP3,
    };
    
    let format = format!("attachment; filename=\"combined_data.{}\"", format);
    
    let response = Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, "application/octet-stream")
      .header(
          header::CONTENT_DISPOSITION,
          format,
      )
      .body(body)
      .map_err(|e| {
          eprintln!("Failed to build response: {:?}", e);
          StatusCode::INTERNAL_SERVER_ERROR
      })?;

    Ok(response)
}


pub async fn play(Path(id): Path<String>, State(state): State<SharedState>) -> Result<Response<Body>, StatusCode> {
    let state_c =  state.clone();
    let deezer = state.deezer.clone();
    let postgres = state.postgres_db.clone();
    let s3 = state.s3;
    let id_i =  id.parse::<i32>().map_err(|e| {
        StatusCode::BAD_REQUEST
    })?;

    match s3.try_get_song(&id).await {
        Ok(stream) => {
            let is_recorded = record_listening(state_c, id_i, None).await;

            let stream = stream.1.body.into_async_read();
            let stream = ReaderStream::new(stream);
            let body = Body::from_stream(stream);

            create_stream_from_body(body, SongFormat::FLAC).await
        }
        Err(()) => {
            let track_data = deezer.get_track_page(&id).await.map_err(|e| {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            
            let (stream, record) = join!{
                deezer.get_stream(id, Some(track_data.track_token)),
                record_listening(state_c.clone(), id_i, Some(track_data.alb_id))
            };
            
            let stream = stream.map_err(|e| {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            
            let body = Body::from_stream(stream);

            create_stream_from_body(body, SongFormat::FLAC).await
        }
    }
    

}
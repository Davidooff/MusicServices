use std::io;
use std::pin::Pin;
use std::sync::Arc;
use aws_sdk_s3::primitives::ByteStream;
use aws_smithy_types::body::SdkBody;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, Response, StatusCode};
use axum::Json;
use axum::response::IntoResponse;
use bytes::Bytes;
use futures::{stream, TryFutureExt, TryStream};
use futures::TryStreamExt;
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use crate::{SharedState};
use crate::postgres_service::{AuthorInput, TrackInput};
use tokio_util::io::ReaderStream;

#[derive(Deserialize, Debug)]
pub struct SearchParams {
    q: String,
    limit: String,
    offset: String,
}

#[axum::debug_handler]
pub async fn search(Query(params): Query<SearchParams>, State(state): State<Arc<SharedState>>) -> Result<impl IntoResponse, StatusCode> {
    let soundcloud = state.soundcloud_api.clone();

    let search_res = soundcloud.search(&params.q, &params.offset, &params.limit).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(search_res))
}

#[axum::debug_handler]
pub async fn get_tracks_data(Path(ids): Path<String>, State(state): State<Arc<SharedState>>) -> Result<impl IntoResponse, StatusCode> {
    let soundcloud = state.soundcloud_api.clone();
    let postgre = state.postgres_db.clone();

    // The original `get_track_data` is fine
    let tracks_data = soundcloud.get_track_data(ids.as_str()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for track in &tracks_data {
        let ti = TrackInput::from(track);
        let ai = AuthorInput::from(track);

        // This call remains the same
        postgre.add_track(&ti, &ai).await.expect("failed to add track");
    }

    Ok(Json(tracks_data))
}

#[axum::debug_handler]
pub async fn get_stream(
    Path(id): Path<String>,
    State(state): State<Arc<SharedState>>
) -> Result<Response<Body>, StatusCode> {
    let s3 = state.s3_client.clone();


    let key = format!("tracks/{}.m3u8", id);
    if let Ok(mut file) = s3.get_object().bucket("soundcloud").key(key).send().await {
        let async_read = file.body.into_async_read();
        let stream = ReaderStream::new(async_read);
        
        let body = Body::from_stream(stream);
        let response = Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/octet-stream")
          .header(
              header::CONTENT_DISPOSITION,
              "attachment; filename=\"combined_data.m3u8\"",
          )
          .body(body)
          .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        println!("response from s3");

        return Ok(response);
    }


    let soundcloud = state.soundcloud_api.clone();
    let postgre = state.postgres_db.clone();


    
    let track_data = soundcloud
        .get_track_data(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    
    
    let track = track_data.first().ok_or(StatusCode::BAD_REQUEST)?;
    
    let media_data = track.media.transcodings.first().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let ti = &TrackInput::from(track);
    let ai = &AuthorInput::from(track);
    let (url_with_chunks, added_track) = tokio::join!(
        soundcloud.get_url_to_chunks(&media_data.url, &track.track_authorization),
        postgre.add_track(ti, ai)
    );
    
    let url_with_chunks = &url_with_chunks.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    
    // If track added to db run in concurent
    let (chunk_tokens, _) = tokio::join!(
        soundcloud.get_chunks(&url_with_chunks),
        // Выполняем запись прослушивания только если добавление трека было успешным
        async {
            if added_track.is_ok() {
                postgre.record_listening(track.id).await
            } else {
                // Если трек не был добавлен, просто возвращаем Ok, чтобы не вызывать ошибку в join!
                Ok(false) 
            }
        }
    );

    let chunk_tokens: Vec<String> = chunk_tokens.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    
    let chunk_futures = futures::stream::iter(chunk_tokens.into_iter())
        .map(move |token| {
            let sc = soundcloud.clone();
            async move { sc.stream_chunk(token).await }
        });


    let (tx, rx) = mpsc::channel::<Bytes>(8);

    let key = format!("tracks/{}.m3u8", id);
    tokio::spawn(async move {
        
        let stream = ReceiverStream::from(rx);
        let collected: Vec<u8> = stream
            .fold(Vec::new(), |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                acc
            })
            .await;

        
        let byte_stream = ByteStream::from(collected);
        // PutObject can take a streaming body:
        if let Err(e) = s3.put_object()
            .bucket("soundcloud")
            .key(key)
            .body(byte_stream)
            .send()
            .await
        {
            tracing::error!("S3 upload failed: {}", e);
        }
    });


    let max_concurrency = 1;
    let tee_stream = chunk_futures
        .buffered(max_concurrency)
        .flatten()
        .map(move |chunk_result| {
            if let Some(bytes) = chunk_result.as_ref().ok() {
                let s3_copy = bytes.clone();
                let _ = tx.try_send(s3_copy);
            }
            
            chunk_result
        });

    let body = Body::from_stream(tee_stream);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"combined_data.m3u8\"",
        )
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

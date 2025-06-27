use std::io::Read;
use std::pin::Pin;
use serde_json::{self, Value};
use reqwest::{ Method, Client, Url};
use reqwest::cookie::Jar;
use std::sync::Arc;
use aws_sdk_s3::config::http::HttpResponse;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use axum::body::Body;
use axum::extract::{FromRef, Path, State};
use axum::http::{Response, StatusCode};
use blowfish::Blowfish;
use bytes::Bytes;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use tokio::sync::{mpsc, Mutex, RwLock};
use thiserror::Error;
use serde::{Deserialize, Serialize};
use futures::{Stream, StreamExt, TryStreamExt};
use futures::stream::BoxStream;
use md5;
use tokio_stream::wrappers::ReceiverStream;
use crate::postgres_service::{AlbumInput, AuthorInput, TrackInput};
use crate::SharedState;

const BASE_URL: &str = "https://www.deezer.com/ajax/gw-light.php";

const MEDIA_EL: &str = "
                {
                    \"type\": \"FULL\",
                    \"formats\": [
                        {
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"FLAC\"
                        },
                        {
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_320\"
                        },
                        {
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_128\"
                        },
                        {
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_64\"
                        },
                        {
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_MISC\"
                        }
                    ]
                }";


// #[derive(Debug)]
pub enum SongFormat {
   FLAC, MP3,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Request failed: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("Failed to parse URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Failed to parse json: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("API returned an error: {0}")]
    ApiError(Value), // For generic API errors

    #[error("A valid API token is required")]
    TokenRequired(String), // A specific, typed error for our retry logic

    #[error("A valid API token is required")]
    SdkStreamReadError(String), // A

    #[error("A valid API token is required")]
    SdkLoadError(#[from] SdkError<PutObjectError, HttpResponse>), // A specific, typed error for our retry logic
}


#[derive(Deserialize, Serialize, Debug)]
pub struct Artist {
    #[serde(rename = "ART_ID")]
    pub(crate) id: String,
    #[serde(rename = "ART_NAME")]
    pub(crate) name: String,
    #[serde(rename = "ARTIST_IS_DUMMY")]
    is_dummy: bool,
    #[serde(rename = "ART_PICTURE")]
    pub(crate) picture: String,
}




#[derive(Deserialize, Serialize, Debug)]
pub struct TrackPage {
    #[serde(rename = "SNG_ID")]
    pub(crate) id: String,
    #[serde(rename = "SNG_TITLE")]
    pub(crate) sng_title: String,
    #[serde(rename = "ARTISTS")]
    pub artists: Vec<Artist>,
    #[serde(rename = "ALB_ID")]
    pub alb_id: String,
    #[serde(rename = "ALB_TITLE")]
    pub alb_title:  String,
    #[serde(rename = "TRACK_TOKEN")]
    pub track_token: String,
    #[serde(rename = "ART_PICTURE")]
    pub picture: String,
    #[serde(rename = "DURATION")]
    pub(crate) duration: String,
}



#[derive(Deserialize, Serialize, Debug)]
pub struct AlbumSongs {
    pub data: Vec<TrackPage>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AlbumHeader {
    #[serde(rename = "ART_ID")]
    pub art_id: String,
    #[serde(rename = "ALB_ID")]
    pub alb_id: String,
    #[serde(rename = "ARTISTS")]
    pub artists: Vec<Artist>,
    #[serde(rename = "ALB_TITLE")]
    pub alb_title:  String,
    #[serde(rename = "ALB_PICTURE")]
    pub img: String,
}



#[derive(Deserialize, Serialize, Debug)]
pub struct Album{
    #[serde(rename = "DATA")]
    pub album_header: AlbumHeader,
    #[serde(rename = "SONGS")]
    pub songs: AlbumSongs,
}




fn is_empty_array(val: &Value) -> bool {
    val.as_array().map_or(false, |arr| arr.is_empty())
}

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send>>;

#[derive(Clone)]
pub struct Deezer {
    api_token: Arc<RwLock<String>>,
    license_token: Arc<RwLock<String>>,
    client: Client,
    token_update_lock: Arc<Mutex<()>>,
}

pub type BlowfishCbcDec = cbc::Decryptor<Blowfish>;

impl Deezer {
    pub fn new(
        arl: String,
    ) -> Self {
        let jar = Arc::new(Jar::default());
        let client = Client::builder().cookie_provider(Arc::clone(&jar)).build().unwrap();
        let url = Url::parse("https://www.deezer.com").unwrap();

        // Add a cookie to the jar
        jar.add_cookie_str(format!("arl={}", arl).as_str(), &url);
        Self{
            api_token: Arc::new(RwLock::new(String::new())),
            license_token: Arc::new(RwLock::new(String::new())),
            client,
            token_update_lock: Arc::new(Mutex::new(())),
        }
    }

    // The "entry point" function that users will call.
    pub async fn call(&self, method: Method, method_endpoint: &str, is_api_token_req: bool, body: Option<String>) -> Result<Value, ApiError> {
        // First attempt
        let result = self.make_request(method.clone(), method_endpoint, is_api_token_req, body.clone()).await;

        match result {
            Err(ApiError::TokenRequired(old_token)) => {
                println!("Deezer API token is invalid or expired. Attempting to refresh.");
                // The token is invalid, so we try to update it.
                // The `update_token` function will acquire a lock to prevent multiple concurrent updates.
                self.update_token(&old_token).await?;
                println!("Token refreshed successfully. Retrying original request for '{}'.", method_endpoint);

                // Retry the request with the new token.
                self.make_request(method, method_endpoint, is_api_token_req, body).await
            }
            // For any other error or success, just return the result.
            _ => result,
        }
    }

    async fn create_req_url_body(&self, track_tokens: &[&str]) -> String {
        let license_token = {
            let guard = self.license_token.read().await;
            guard.clone()
        };

        let mut media: Vec<&str> = Vec::new();

        for _ in 0..track_tokens.len() {
            media.push(MEDIA_EL);
        }

        let media =  media.join(", ");
        let track_tokens = track_tokens.join("\", \"");

        format!("
            {{\
                \"license_token\": \"{license_token}\",\
                \"media\": [ {media} ],
                \"track_tokens\": [ \"{track_tokens}\" ]
            }}
        ")
    }

    // The internal, recursive implementation.
    async fn make_request<'a>(
        &'a self,
        method: Method,
        method_endpoint: &'a str,
        is_api_token_req: bool,
        body: Option<String>,
    ) -> Result<Value, ApiError> {
        // 2. Wrap the entire function body in `Box::pin(async move { ... })`.
        //    This moves the Future onto the heap, breaking the infinite-size recursion.

        let mut token = String::new();
        let mut params = [
            ("method", method_endpoint),
            ("api_version", "1.0"),
            ("input", "3"),
            ("api_token", ""), // Placeholder for the token
        ];

        println!("0 is_api_token_req: {}", is_api_token_req);


        if is_api_token_req {
            token = self.api_token.read().await.clone();
            // We need to ensure the borrowed `token` string lives long enough.
            // It's already cloned, so it's owned by this stack frame, which is fine.
            params[3] = ("api_token", &token);
        }

        let url_p = Url::parse_with_params(BASE_URL, params)?; // Pass a slice reference
        let mut req_builder = self.client.request(method.clone(), url_p.clone());

        if let Some(some_body) = body.clone() {
            req_builder = req_builder.body(some_body);
        }
        let req = req_builder.build()?;

        let res: Value = self.client.execute(req).await?.json().await?;
        // println!("{:?}", res);

        if let Some(error) = res.get("error") {
            if !is_empty_array(&error) {
                if error.get("VALID_TOKEN_REQUIRED").is_some() {
                    // Refresh token, THEN make the recursive call.
                    return Err(ApiError::TokenRequired(token));
                }
                println!("{}", url_p);
                if let Some(body) = body{
                    println!("{}", body);

                }
                return Err(ApiError::ApiError(error.clone()));
            }
        }

        Ok(res)
    }

    pub async fn get_track_page(&self, id: &str) -> Result<TrackPage, ApiError> {
        let req_body = format!("{{\"sng_id\": \"{}\", \"start_with_input_track\": true}}", id);

        let res = self.call(
            Method::POST,
            "deezer.pageTrack",
            true,
            Some(req_body)
        ).await?;

        let data = res
            .get("results")
            .and_then(|t| t.get("DATA"))
            .ok_or(
                ApiError::ApiError(serde_json::json!({
                    "error": "Unable to parse data from track page"
                }))
            )?;

        let data: TrackPage = serde_json::from_value(data.to_owned())?;
        Ok(data)
    }
    
    pub async fn get_album(&self, id: String) -> Result<Album, ApiError> {
        let req_body = format!("{{\"alb_id\": \"{}\",\"lang\": \"en\"}}", id);

        let res = self.call(Method::POST, "deezer.pageAlbum", true, Some(req_body)).await?;
        let res = res.get("results").ok_or_else(|| ApiError::ApiError(serde_json::json!({
            "error": "Unable to parse data from album page"
        })))?;

        Ok(serde_json::from_value::<Album>(res.to_owned())?)
    }

    pub async fn get_track_url(&self, track_token: &str) -> Result<String, ApiError> {
        // We will now correctly handle the Result from this function
        let track_req_body = self.create_req_url_body(&[track_token]).await;

        println!("Request Body Sent to media.deezer.com:\n{}", track_req_body);

        // --- FIX 1: Manually add the arl cookie to the request ---
        // This is the most direct way to ensure authentication with the media subdomain.
        let jar = Arc::new(Jar::default());
        let url = "https://www.deezer.com".parse::<Url>().unwrap();


        // Build the request
        let request = self.client.post("https://media.deezer.com/v1/get_url")
            // --- FIX 2: Add Content-Type Header ---
            .header("Content-Type", "application/json")
            .body(track_req_body)
            .build()?;

        println!("Executing request to: {}", request.url());
        println!("With headers: {:?}", request.headers());

        // --- DEBUGGING: Get the raw response first ---
        let response = self.client.execute(request).await?;
        let status = response.status();
        let response_text = response.text().await?;

        println!("Response Status: {}", status);
        println!("Response Body:\n{}", response_text);

        // Now, if the status was successful, parse the text
        if !status.is_success() {
            return Err(ApiError::ApiError(serde_json::json!({
            "error": "Request to get_url failed",
            "status": status.as_u16(),
            "response": response_text
        })));
        }

        let res: Value = serde_json::from_str(&response_text)?;

        // Your original parsing logic
        let url = res
            .get("data")
            .and_then(|t| t.get(0))
            .and_then(|t| t.get("media"))
            .and_then(|t| t.get(0))
            .and_then(|t| t.get("sources"))
            .and_then(|t| t.get(0))
            .and_then(|t| t.get("url"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ApiError::ApiError(serde_json::json!({
                "error": "Media URL not found in the response JSON",
                "res": res,
            }))
            })?;

        Ok(url.to_string())
    }

    pub async fn get_stream(&self, id: &str, track_token: Option<String>) -> Result<ReceiverStream<Result<Vec<u8>, ApiError>>, ApiError> {
        let key = Self::generate_blowfish_key(id);

        let (tx, rx) = mpsc::channel(8);

        let stream: ReceiverStream<Result<Vec<u8>, ApiError>> = ReceiverStream::new(rx);
        let self_s = self.clone();
        let token = match track_token {
            Some(url) => url,
            None => self.get_track_page(id).await?.track_token
        };

        tokio::spawn(async move {
            let mut download_stream = match self_s.download_by_token(&token).await {
                Ok(stream) => stream,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            let mut byte_buffer: Vec<u8> = Vec::with_capacity(2048 * 4);
            let mut total_bytes_sent = 0usize;
            let iv = [0, 1, 2, 3, 4, 5, 6, 7];

            while let Some(chunk_result) = download_stream.next().await {
                match chunk_result {
                    Ok(network_chunk) => {
                        byte_buffer.extend_from_slice(&network_chunk);

                        while byte_buffer.len() >= 2048 {
                            let mut segment = byte_buffer.drain(..2048).collect::<Vec<u8>>();

                            let segment_index = total_bytes_sent / 2048;
                            if segment_index > 0 && segment_index % 3 == 0 {
                                let mut cbc_decryptor = BlowfishCbcDec::new_from_slices(&key, &iv)
                                  .expect("Failed to create CBC decryptor");

                                for block in segment.chunks_mut(8) {
                                    cbc_decryptor.decrypt_block_mut(block.into());
                                }
                            }

                            if tx.send(Ok(segment)).await.is_err() {
                                return;
                            }
                            total_bytes_sent += 2048;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(ApiError::from(e))).await;
                        return;
                    }
                }
            }

            if !byte_buffer.is_empty() {
                let _ = tx.send(Ok(byte_buffer)).await;
            }
        });

        Ok(stream)
    }
    pub async fn download(&self, id: &str) -> Result<BoxStream<reqwest::Result<Bytes>>, ApiError> {
        let track_page = self.get_track_page(id).await?;
        
        let track_url = self.get_track_url(&track_page.track_token).await?;
        println!("Got url: {:?}", track_url);
        
        let res = self.client.get(&track_url).send().await?.bytes_stream().boxed();
        Ok(res)
    }

    pub async fn download_by_token(&self, token: &str) -> Result<BoxStream<reqwest::Result<Bytes>>, ApiError> {
        let track_url = self.get_track_url(token).await?;
        let res = self.client.get(track_url).send().await?.bytes_stream().boxed();
        Ok(res)
    }
    
    pub fn generate_blowfish_key(track_id: &str) -> [u8; 16] {
        const SECRET: &[u8] = b"g4el58wc0zvf9na1";

        let digest = md5::compute(track_id.as_bytes());
        let md5_id = format!("{:x}", digest);

        let md5_bytes = md5_id.as_bytes();

        let mut bf_key = [0u8; 16];

        for i in 0..16 {
            bf_key[i] = md5_bytes[i] ^ md5_bytes[i + 16] ^ SECRET[i];
        }

        bf_key
    }

    pub async fn update_token(&self, old_token: &str) -> Result<(), ApiError> {
        let _lock_guard = self.token_update_lock.lock().await;

        {
            let current_token = self.api_token.read().await;
            if *current_token != old_token {
                // The token was already updated by another thread. Our work is done.
                println!("Token was already updated by another process before we started. No action needed.");
                return Ok(());
            }
        }

        let user_data = self.make_request(Method::GET, "deezer.getUserData", false, None).await?;

        println!("User Data: {:?}", user_data);
        
        let new_api_token = user_data
            .get("results")
            .and_then(|t| t.get("checkForm"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ApiError::ApiError(serde_json::json!({
                    "error": "New token (checkForm) not found in deezer.getUserData response"
                }))
            })?.to_owned();

        let new_license_token = user_data
            .get("results")
            .and_then(|t| t.get("USER"))
            .and_then(|t| t.get("OPTIONS"))
            .and_then(|t| t.get("license_token"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ApiError::ApiError(serde_json::json!({
                            "error": "New token (checkForm) not found in deezer.getUserData response"
                        }))
            })?.to_owned();

        println!("Successfully fetched new token.");

        let mut token_guard = self.api_token.write().await;
        let mut license_token_guard = self.license_token.write().await;

        if *token_guard != new_api_token {
            println!("Token in memory is the old one, updating...");
            *token_guard = new_api_token;
        } else {
            println!("Token was already updated by another process. Discarding new token.");
        }

        *license_token_guard = new_license_token;

        Ok(())
    }
}
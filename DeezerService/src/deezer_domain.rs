use serde_json::{self, Value};
use reqwest::{ Method, Client, Url};
use reqwest::cookie::Jar;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use thiserror::Error;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://www.deezer.com/ajax/gw-light.php";

const MEDIA_EL: &str = "
                {{
                    \"type\": \"FULL\",
                    \"formats\": [
                        {{
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"FLAC\"
                        }},
                        {{
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_320\"
                        }},
                        {{
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_128\"
                        }},
                        {{
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_64\"
                        }},
                        {{
                            \"cipher\": \"BF_CBC_STRIPE\",
                            \"format\": \"MP3_MISC\"
                        }}
                    ]
                }}";

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
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Artist {
    #[serde(rename = "ART_ID")]
    id: String,
    #[serde(rename = "ART_NAME")]
    name: String,
    #[serde(rename = "ARTIST_IS_DUMMY")]
    is_dummy: bool,
    #[serde(rename = "ART_PICTURE")]
    picture: String,
}


#[derive(Deserialize, Serialize, Debug)]
pub struct TrackPage {
    #[serde(rename = "SNG_ID")]
    sng_id: String,
    #[serde(rename = "SNG_TITLE")]
    sng_title: String,
    #[serde(rename = "ARTISTS")]
    artists: Vec<Artist>,
    #[serde(rename = "ALB_ID")]
    alb_id: String,
    #[serde(rename = "ALB_TITLE")]
    alb_title:  String,
    #[serde(rename = "TRACK_TOKEN")]
    track_token: String,
}


fn is_empty_array(val: &Value) -> bool {
    val.as_array().map_or(false, |arr| arr.is_empty())
}

#[derive(Clone)]
pub struct Deezer {
    api_token: Arc<RwLock<String>>,
    license_token: Arc<RwLock<String>>,
    client: Client,
    token_update_lock: Arc<Mutex<()>>,
}

impl Deezer {
    pub fn new(
        arl: String,
    ) -> Self {
        let jar = Arc::new(Jar::default());
        let client = Client::builder().cookie_provider(Arc::clone(&jar)).cookie_store(true).build().unwrap();
        let url = reqwest::Url::parse("https://www.deezer.com").unwrap();

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
            let guard = self.api_token.read().await;
            guard.clone()
        };

        let mut media: Vec<&str> = Vec::new();

        for i in 0..track_tokens.len() {
            media.push(MEDIA_EL);
        }

        let media =  media.join("\", \"");
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
        println!("{:?}", res);

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

    pub async fn get_track_page(&self, id: String) -> Result<TrackPage, ApiError> {
        let req_body = format!("{{\"sng_id\": \"{}\", \"start_with_input_track\": true}}", id);

        let res = self.call(
            Method::POST,
            "deezer.pageTrack",
            true,
            Some(req_body)
        ).await?;

        println!("{:?}", res);


        let data = res
            .get("results")
            .and_then(|t| t.get("DATA"))
            .ok_or(
                ApiError::ApiError(serde_json::json!({
                    "error": "Unable to parse data from track page"
                }))
            )?;

        println!("{:?}", data);


        let data: TrackPage = serde_json::from_value(data.to_owned())?;
        Ok(data)
    }

    pub async fn get_track_url(&self, track_token: String) -> Result<String, ApiError> {
        let track_req_body = self.create_req_url_body(&[track_token.as_str()]).await;

        let req = self.client.post("https://media.deezer.com/v1/get_url").body(track_req_body).build()?;
        let res= &self.client.execute(req).await?.json::<Value>().await?;

        let url = res
            .get("data")
            .and_then(|t| t.get(0))
            .and_then(|t| t.get("media"))
            .and_then(|t| t.get(0))
            .and_then(|t| t.get("sources"))
            .and_then(|t| t.get(0))
            .and_then(|t| t.get("url"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                ApiError::ApiError(serde_json::json!({
                    "error": "Media not found",
                    "res": res,
                }))
            })?;

        Ok(url.to_string())
    }

    // pub async fn download(&self, id: &str) -> Result<(), ApiError> {
    //     let req_body = format!("{{\"sng_id\": {}, \"start_with_input_track: true\"}}", id);
    //     let track_page = self.call(&[id]).await;
    // }

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
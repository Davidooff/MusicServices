use std::pin::Pin;
use serde_json::Value;
use reqwest::{ Method, Client, Url};
use reqwest::cookie::Jar;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use thiserror::Error;
use log::log;

const BASE_URL: &str = "https://www.deezer.com/ajax/gw-light.php";


#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Request failed: {0}")]
    RequestError(#[from] reqwest::Error), // Covers I/O, build, and transport errors

    #[error("Failed to parse URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("API returned an error: {0}")]
    ApiError(Value), // For generic API errors

    #[error("A valid API token is required")]
    TokenRequired, // A specific, typed error for our retry logic
}

fn is_empty_array(val: &Value) -> bool {
    val.as_array().map_or(false, |arr| arr.is_empty())
}


#[derive(Clone)]
pub struct Deezer {
    api_token: Arc<RwLock<String>>,
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
            client,
            token_update_lock: Arc::new(Mutex::new(())),
        }
    }

    // The "entry point" function that users will call.
    pub async fn call<'a>(&self, method: Method, method_endpoint: &str, is_api_token_req: bool, body: Option<String>) -> Result<Value, ApiError> {
        // It calls the internal recursis_empty_arrayive function with the initial retry count.
        self.call_recursive(method, method_endpoint, is_api_token_req, body, 1).await
    }

    // The internal, recursive implementation.
    fn call_recursive<'a>(
        &'a self,
        method: Method,
        method_endpoint: &'a str,
        is_api_token_req: bool,
        body: Option<String>,
        retries_left: u8,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ApiError>> + Send + 'a>> {
        // 2. Wrap the entire function body in `Box::pin(async move { ... })`.
        //    This moves the Future onto the heap, breaking the infinite-size recursion.
        Box::pin(async move {
            if retries_left == 0 {
                return Err(ApiError::TokenRequired);
            }

            let mut token = String::new();
            let mut params = [
                ("method", method_endpoint),
                ("api_version", "1.0"),
                ("input", "3"),
                ("api_token", ""), // Placeholder for the token
            ];

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
                        self.update_token(token.as_str()).await?;
                        return self.call_recursive(method, method_endpoint, is_api_token_req, body, retries_left - 1).await;
                    }
                    println!("{}", url_p);
                    if let Some(body) = body{
                        println!("{}", body);

                    }
                    return Err(ApiError::ApiError(error.clone()));
                }
            }

            Ok(res)
        })
    }

    // Note the change from &mut self to &self. This is more idiomatic.
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


        let new_token = match self.call(Method::GET, "deezer.getUserData", false, None).await {
            Ok(res_json) => {
                res_json
                    .get("results")
                    .and_then(|r| r.get("checkForm"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_owned()) // Convert &str to String
                    .ok_or_else(|| {
                        ApiError::ApiError(serde_json::json!({
                        "error": "New token (checkForm) not found in deezer.getUserData response"
                    }))
                    })?
            }
            Err(err) => return Err(err),
        };

        println!("Successfully fetched new token.");

        let mut token_guard = self.api_token.write().await;

        if *token_guard == old_token {
            println!("Token in memory is the old one, updating...");
            *token_guard = new_token;
        } else {
            println!("Token was already updated by another process. Discarding new token.");
        }

        Ok(())
    }
}
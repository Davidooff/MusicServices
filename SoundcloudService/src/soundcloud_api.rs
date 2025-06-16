use std::error::Error;
use std::io::ErrorKind;
use std::pin::Pin;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use regex::Regex;
use futures::{Stream, StreamExt, TryStreamExt};

const BASE_URL: &str = "https://api-v2.soundcloud.com";

#[derive(Deserialize, Serialize, Clone)]
pub struct FormatData {
    pub protocol: String,
    pub mime_type: String
}

#[derive(Deserialize, Serialize, Clone)]
pub struct EncodingData {
    pub url: String,
    pub preset: Option<String>,
    pub duration: u32,
    pub snipped: bool,
    pub format: FormatData,
    pub quality: String,
    pub is_legacy_transcoding: Option<bool>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Media {
    pub transcodings: Vec<EncodingData>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct User {
    pub avatar_url: String,
    pub username: String,
    pub id: i32,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct TrackData {
    pub id: i32,
    pub title: String,
    pub artwork_url: Option<String>,
    pub duration: i32,
    pub media: Media,
    pub track_authorization: String,
    pub user: User
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(untagged)] // Try to deserialize as one of the variants
pub enum PlaylistTrack {
    Full(TrackData), // Your original (but now fully optional) TrackData
    Partial { id: i32}, // A struct for the minimal objects
}

#[derive(Deserialize, Serialize, Clone)]
pub struct PlaylistData {
    pub id: i32,
    pub title: String,
    pub artwork_url:  Option<String>,
    pub duration: i32,
    pub user: User,
    pub tracks: Vec<PlaylistTrack>
}

#[derive(Deserialize, Serialize)]
pub enum SearchItem {
    Playlist(PlaylistData),
    Track(TrackData),
    User(User),
}

#[derive(Deserialize, Serialize)]
pub struct SearchResponse {
    pub collection: Vec<SearchItem>
}


#[derive(Deserialize, Serialize)]
pub struct ChunkUrl {
    pub url: String,
}

pub struct SoundCloudApi {
    client: Client,
    client_id: String,
    url_re: Regex,
}


pub type ByteStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send>>;

impl SoundCloudApi {
    pub fn new(client_id: &str) -> Self {
        Self {
            client: Client::new(),
            client_id: String::from(client_id),
            url_re: Regex::new(r#"https:?:[^\s"]+"#).unwrap()
        }
    }

    pub async fn search(&self, query: &str, offset: &str, limit: &str) -> Result<SearchResponse, Box<dyn Error>> {
        let url = Url::parse_with_params(format!("{}/search", BASE_URL).as_str(), &[
            ("q", query), ("client_id", self.client_id.as_str()),
            ("limit", limit), ("offset", offset)
        ])?;

        let req = self.client.get(url).build()?;
        let res = self.client.execute(req).await?.text().await?;

        let search_res: SearchResponse = serde_json::from_str(&res)?;
        Ok(search_res)
    }

    pub async fn get_track_data(&self, ids: &str) -> Result<Vec<TrackData>, Box<dyn Error>> {
        let url = Url::parse_with_params(format!("{}/tracks", BASE_URL).as_str(), &[
            ("ids", ids), ("client_id", self.client_id.as_str()),
        ])?;
        let req = self.client.get(url).build()?;
        let res = self.client.execute(req).await?.text().await?;
        let track: Vec<TrackData> = serde_json::from_str(&res)?;

        Ok(track)
    }

    pub async fn get_url_to_chunks(&self, url: &str, track_authorization: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let url = Url::parse_with_params(url, &[
            ("client_id", self.client_id.as_str()), ("track_authorization", track_authorization),
        ])?;
        let req = self.client.get(url).build()?;
        let res = self.client.execute(req).await?.text().await?;
        let urls: ChunkUrl = serde_json::from_str(&res)?;

        Ok(urls.url)
    }

    pub async fn get_chunks(&self, url: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let req = self.client.get(url).build()?;
        let res = self.client.execute(req).await?.text().await?;
        let urls: Vec<String> = self.url_re.find_iter(res.as_str()).map(|m| m.as_str().to_string()).collect();
        
        Ok(urls)
    }

    pub async fn stream_chunk(&self, url: String) -> ByteStream {
        // Use the ? operator to propagate errors instead of panicking.
        // If send() fails, this function will immediately return an Err.
        let response = self.client
            .get(url)
            .send()
            .await.unwrap();


        // The rest of your logic is good, just wrap the final stream in Ok().
        // Note: The error in bytes_stream is reqwest::Error. We map it to io::Error
        // to match the ByteStream definition.
        let stream = response
            .bytes_stream()
            .map_err(|e| std::io::Error::new(ErrorKind::Other, e)) // A better mapping
            .boxed();

        // Wrap the successful result in Ok()
        stream
    }
}
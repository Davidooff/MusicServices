use reqwest::Client;
use std::error::Error;
use serde::{Deserialize, Serialize};


const URL: &str = "https://api.deezer.com";

#[derive(Serialize, Deserialize)]
pub struct AlbumInfo {
    id: u32,
    title: String,
    cover: String
}

#[derive(Serialize, Deserialize)]
pub struct Artist {
    id: u32,
    name: String,
    picture: String
}

#[derive(Serialize, Deserialize)]
pub struct Track {
    id: u32,
    title: String,
    duration: u16,
    album: AlbumInfo,
    artist: Artist
}

#[derive(Serialize, Deserialize)]
struct TrackSearchRes {
    data: Vec<Track>,
}

pub async fn search_track(client: &Client, query: &str) -> Result<Vec<Track>, Box<dyn Error>> {
    let res = client.get(format!("{}/search/track", URL)).query(&[("q", query)]).send().await?.text().await?;
    let tracks: TrackSearchRes = serde_json::from_str(&res)?;
    Ok(tracks.data)
}
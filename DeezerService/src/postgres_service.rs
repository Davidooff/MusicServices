use std::time::Duration;
use axum::extract::FromRef;
use axum::response::IntoResponse;
use sqlx::{pool, FromRow, Postgres};
use sqlx::postgres::{PgHasArrayType, PgPoolOptions, PgTypeInfo};
use crate::deezer::{Album, AlbumHeader, Artist, TrackPage};


#[derive(Debug, FromRow)]
pub struct Track {
  id: i32,
  title: String,
  duration: u32,
  author_id: i32,
  album_id: i32,
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "author_input_deezer")] // Links this struct to the PG type
pub struct AuthorInput {
  pub id: i32,
  pub title: String,
  pub img: String,
}

impl From<Artist> for AuthorInput {
  fn from(value: Artist) -> Self {
    Self {
      id: value.id.parse::<i32>().unwrap(),
      title: value.name,
      img: value.picture
    }
  }
}

impl FromRef<Artist> for AuthorInput {
  fn from_ref(value: &Artist) -> Self {
    Self {
      id: value.id.parse::<i32>().unwrap(),
      title: value.name.clone(),
      img: value.picture.clone(),
    }
  }
}


impl FromRef<AlbumHeader> for AuthorInput {
  fn from_ref(value: &AlbumHeader) -> Self {
    for art in &value.artists {
      if art.id == value.art_id {
        return AuthorInput::from_ref(art);
      }
    }

    AuthorInput::from_ref(&value.artists[0])
  }
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "album_input_deezer")]
pub struct AlbumInput {
  pub id: i32,
  pub title: String,
  pub img: String,
}

impl From<AlbumHeader> for AlbumInput {
  fn from(value: AlbumHeader) -> Self {
    Self {
      id: value.alb_id.parse::<i32>().unwrap(),
      title: value.alb_title,
      img: value.img,
    }
  }
}


#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "track_input_deezer")]
pub struct TrackInput {
  pub id: i32,
  pub title: String,
  pub duration: i32,
}

impl From<TrackPage> for TrackInput {
  fn from(value: TrackPage) -> Self {
    Self {
      id: value.id.parse::<i32>().unwrap(),
      title: value.sng_title,
      duration: value.duration.parse::<i32>().unwrap()
    }
  }
}

impl FromRef<TrackPage> for TrackInput {
  fn from_ref(value: &TrackPage) -> Self {
    Self {
      id: value.id.parse::<i32>().unwrap(),
      title: value.sng_title.clone(),
      duration: value.duration.parse::<i32>().unwrap()
    }
  }
}




impl PgHasArrayType for TrackInput {
  fn array_type_info() -> PgTypeInfo {
    PgTypeInfo::with_name("_track_input_deezer")
  }
}

pub struct PostgresDb {
  pool: pool::Pool<Postgres>,
}

impl PostgresDb {
  pub async fn new(url: &str) -> Self {
    Self {
      pool: PgPoolOptions::new().connect(&url).await.unwrap(),
    }
  }

  pub async fn add_album(
    &self,
    track: &[TrackInput],
    author: &AuthorInput,
    album: &AlbumInput
  ) -> Result<(), sqlx::error::Error> {
    sqlx::query("CALL add_album_deezer($1, $2, $3)")
      .bind(author)
      .bind(album)
      .bind(track)
      .execute(&self.pool)
      .await?;

    Ok(())
  }

  pub async fn add_album_by_album(
    &self,
    album: &Album
  ) -> Result<(), sqlx::error::Error> {
    let author_input = AuthorInput::from_ref(&album.album_header);
    let album_input = AlbumInput::from_ref(&album.album_header);
    let mut tracks_input: Vec<TrackInput> = vec![];
    for track in &album.songs.data {
      tracks_input.push(TrackInput::from_ref(track));
    }
    
    sqlx::query("CALL add_album_deezer($1, $2, $3)")
      .bind(author_input)
      .bind(album_input)
      .bind(tracks_input)
      .execute(&self.pool)
      .await?;

    Ok(())
  }
  
  pub async fn record_listening(&self, id: i32) -> Result<bool, sqlx::Error> {
    let is_added: bool = sqlx::query_scalar("SELECT record_listen_deezer($1)")
      .bind(id)
      .fetch_one(&self.pool)
      .await?;
    
    Ok(is_added)
  }
}
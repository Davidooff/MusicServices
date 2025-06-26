use sqlx::postgres::{PgHasArrayType, PgPoolOptions, PgTypeInfo};
use sqlx::{pool, Postgres};
use std::error::Error;
use crate::soundcloud_api::TrackData;

// Types:
#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "author_input")] // Links this struct to the PG type
pub struct AuthorInput {
    id: i32,
    title: String,
    img: String,
}


#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "album_input")]
pub struct AlbumInput {
    id: i32,
    title: String,
    img: String,
    author_id: i32,
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "track_input")]
pub struct TrackInput {
    id: i32,
    title: String,
    duration: i32,
    img: String,
}

// CORRECTED: Implement From for a reference to avoid unnecessary clones.
impl From<&TrackData> for AuthorInput {
    fn from(track: &TrackData) -> Self {
        Self {
            id: track.user.id,
            title: track.user.username.clone(),
            img: track.artwork_url.clone().unwrap_or("".to_owned()),
        }
    }
}

// ADDED: The missing implementation, also taking a reference.
impl From<&TrackData> for TrackInput {
    fn from(track: &TrackData) -> Self {
        Self {
            id: track.id,
            title: track.title.clone(),
            duration: track.duration,
            img: track.artwork_url.clone().unwrap_or("".to_owned()),
        }
    }
}


// Tables:
#[derive(Debug, sqlx::FromRow)]
pub struct TrackTblEntry {
    id: i32,
    title: String,
    duration: i32,
    img: Option<String>,
    author_id: Option<i32>,
}

impl From<TrackData> for TrackTblEntry {
    fn from(track_data: TrackData) -> Self {
        Self {
            id: track_data.id,
            title: track_data.title,
            duration: track_data.duration,
            img: track_data.artwork_url,
            author_id: Some(track_data.user.id),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct OptionalStr(Option<String>);

impl PgHasArrayType for TrackInput {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_tracks_input")
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

    pub async fn add_track(
        &self,
        track: &TrackInput,
        author: &AuthorInput,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("CALL add_track($1, $2)")
            .bind(track)
            .bind(author)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_tracks(&self, id: &str) -> Result<TrackTblEntry, Box<dyn Error + Send + Sync>> {
        let mut track: TrackTblEntry = sqlx::query_as("SELECT * FROM tracks WHERE id=$1")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        if track.img.is_none() {
            let img: OptionalStr = sqlx::query_as("SELECT img FROM albums WHERE id=$1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
            track.img = img.0;
        }

        Ok(track)
    }
    
    pub async fn record_listening(&self, id: i32) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let was_inserted: bool = sqlx::query_scalar("SELECT record_listen_deezer($1)")
            .bind(id)       // Привязываем track_id к параметру $1
            .fetch_one(&self.pool)      // Выполняем запрос и ожидаем ровно одну строку
            .await?;              // Ожидаем завершения и обрабатываем возможные ошибки I/O

        Ok(was_inserted)
    }
}
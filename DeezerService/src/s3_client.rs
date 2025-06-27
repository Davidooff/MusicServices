use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::operation::get_object::GetObjectOutput;
use aws_smithy_types::byte_stream::ByteStream;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use axum_core::__private::tracing::log;
use futures::{StreamExt, TryStreamExt};
use futures::stream::BoxStream;
use tokio::sync::mpsc::Receiver;
use crate::deezer::{ApiError, SongFormat};
use tokio_stream::wrappers::ReceiverStream;


#[derive(Debug, Clone)] 
pub struct S3Client(Client);

impl S3Client {
  pub async fn new(url: &str, buckets: Vec<&str>) -> S3Client {
    let region = Region::new("us-east-1");

    let credentials = Credentials::new(
      "minioadmin",
      "minioadmin",
      None,
      None,
      "Static",
    );

    // Build the AWS SDK config with all necessary settings for MinIO.
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
      .region(region)
      .credentials_provider(credentials)
      .endpoint_url(url)
      .load()
      .await;

    // Create the S3 client with force_path_style enabled.
    let s3_config = aws_sdk_s3::config::Builder::from(&config)
      .force_path_style(true)
      .build();

    let client = Client::from_conf(s3_config);

    // Create buckets.
    for bucket_name in buckets {
      match client.create_bucket().bucket(bucket_name).send().await {
        Ok(_) => println!("Bucket '{}' created successfully.", bucket_name),
        Err(e) => {
          if let Some(service_error) = e.as_service_error() {
            if service_error.is_bucket_already_owned_by_you() || service_error.code() == Some("BucketAlreadyExists") {
              println!("Bucket '{}' already exists and is owned by you.", bucket_name);
            } else {
              panic!("Service error creating bucket '{}': {}", bucket_name, service_error);
            }
          } else {
            // This could be a network error like a "dispatch failure".
            panic!("Error creating bucket '{}': {}", bucket_name, e);
          }
        }
      }
    }

    S3Client(client)
  }
  
  pub async fn try_get_song(&self, id: &str) -> Result<(SongFormat, GetObjectOutput), ()> {
    // let key = format!("tracks/{}.mp3", id);
    // if let Ok(file) = self.0.get_object().bucket("soundcloud").key(key).send().await { 
    //   return Ok(file)
    // }
    let key = format!("tracks/{}.flac", id);
    
    if let Ok(file) = self.0.get_object().bucket("deezer").key(key).send().await {
      return Ok((SongFormat::FLAC, file))
    }
    
    Err(())
  }

  pub async fn save_song(&self, id: &str, format: SongFormat, data: BoxStream<'static, Result<Vec<u8>, ()>>) -> Result<(), ApiError> {
    // let data = ReceiverStream::new(data);
    let initial_vec: Vec<u8> = Vec::new();
    let data: Vec<u8> = data.try_fold(initial_vec, |mut acc, item_vec| async move {
      acc.extend(item_vec);
      Ok(acc)
    }).await.map_err(|e| {
      ApiError::SdkStreamReadError(id.to_string())
    })?;

    let format = match format {
      SongFormat::FLAC => crate::private_api_routs::FLAC,
      SongFormat::MP3 => crate::private_api_routs::MP3,
    };

    let key = format!("tracks/{}.{}", id, format);

    let byte_stream = ByteStream::from(data);
    self.0.put_object()
      .bucket("deezer")
      .key(key)
      .body(byte_stream)
      .send()
      .await?;

    Ok(())
  }
}


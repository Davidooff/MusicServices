use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::operation::get_object::GetObjectOutput;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use crate::deezer::SongFormat;

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
    
    if let Ok(file) = self.0.get_object().bucket("soundcloud").key(key).send().await {
      return Ok((SongFormat::FLAC, file))
    }
    
    Err(())
  }
}


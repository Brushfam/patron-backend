use std::time::Duration;

pub use aws_sdk_s3::Error;
use aws_sdk_s3::{
    config::{Credentials, Region},
    presigning::{PresignedRequest, PresigningConfig},
    primitives::ByteStream,
    Client,
};

use crate::config;

const EXPIRATION_TIME: Duration = Duration::from_secs(86400);

pub struct ConfiguredClient<'a> {
    config: &'a config::Storage,
    client: Client,
}

impl<'a> ConfiguredClient<'a> {
    pub async fn new(config: &'a config::Storage) -> ConfiguredClient<'a> {
        let sdk_config = aws_config::from_env()
            .endpoint_url(&config.endpoint_url)
            .region(Region::new(config.region.clone()))
            .credentials_provider(Credentials::new(
                &config.access_key_id,
                &config.secret_access_key,
                None,
                None,
                "s3-client",
            ))
            .load()
            .await;

        ConfiguredClient {
            config,
            client: Client::new(&sdk_config),
        }
    }

    pub async fn get_source_code(&self, hash: &[u8]) -> Result<PresignedRequest, Error> {
        let req = self
            .client
            .get_object()
            .bucket(&self.config.source_code_bucket)
            .key(hex::encode(hash))
            .presigned(
                PresigningConfig::builder()
                    .expires_in(EXPIRATION_TIME)
                    .build()
                    .expect("unable to build presigning config"),
            )
            .await?;

        Ok(req)
    }

    pub async fn upload_source_code<F>(&self, hash: &[u8], file: F) -> Result<(), Error>
    where
        ByteStream: From<F>,
    {
        self.client
            .put_object()
            .bucket(&self.config.source_code_bucket)
            .key(hex::encode(hash))
            .body(ByteStream::from(file))
            .send()
            .await?;

        Ok(())
    }
}

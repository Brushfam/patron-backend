use std::time::Duration;

pub use aws_sdk_s3::Error;
use aws_sdk_s3::{
    config::{Credentials, Region},
    presigning::{PresignedRequest, PresigningConfig},
    primitives::ByteStream,
    Client,
};

use crate::config;

/// Expiration time used for pre-signed URLs.
///
/// Pre-signed URLs from an S3 client can be used to
/// pass files to isolated build environments.
const EXPIRATION_TIME: Duration = Duration::from_secs(86400);

/// Configured S3 client.
pub struct ConfiguredClient<'a> {
    config: &'a config::Storage,
    client: Client,
}

impl<'a> ConfiguredClient<'a> {
    /// Create new [`ConfiguredClient`] from the provided [`Storage`] configuration.
    ///
    /// [`Storage`]: config::Storage
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

    /// Get the source code pre-signed request for the provided code hash.
    ///
    /// The pre-signed request is active for a limited duration.
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

    /// Upload source code with the provided code hash.
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

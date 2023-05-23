use std::error::Error;

use axum::async_trait;
use db::{Database, DatabaseConnection};
use hyper::body::{self, Bytes, HttpBody};
use migration::MigratorTrait;
use serde::Serialize;

pub(crate) async fn create_database() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("unable to create test database");

    migration::Migrator::up(&db, None)
        .await
        .expect("unable to run migrations");

    db
}

pub(crate) trait RequestBodyExt: Sized {
    fn from_json<B: Serialize>(val: B) -> Self;
}

impl<T> RequestBodyExt for T
where
    T: HttpBody + From<Vec<u8>>,
{
    fn from_json<B: Serialize>(val: B) -> Self {
        T::from(serde_json::to_vec(&val).expect("unable to serialize"))
    }
}

#[async_trait(?Send)]
pub(crate) trait ResponseBodyExt {
    async fn bytes(self) -> Bytes;

    async fn text(self) -> String;

    async fn json(self) -> serde_json::Value;
}

#[async_trait(?Send)]
impl<T> ResponseBodyExt for T
where
    T: HttpBody,
    T::Error: Error,
{
    async fn bytes(self) -> Bytes {
        body::to_bytes(self)
            .await
            .expect("unable to convert to bytes")
    }

    async fn text(self) -> String {
        String::from_utf8(self.bytes().await.to_vec()).expect("unable to convert to text")
    }

    async fn json(self) -> serde_json::Value {
        serde_json::from_slice(&self.bytes().await).expect("unable to convert to json")
    }
}

use std::sync::Arc;

use axum::{
    extract::{multipart::MultipartError, Multipart, State},
    http::StatusCode,
    Extension, Json,
};
use axum_derive_error::ErrorResponse;
use common::{config::Config, hash, s3};
use db::{
    sea_query::OnConflict, source_code, user, ActiveValue, ColumnTrait, DatabaseConnection, DbErr,
    EntityTrait, QueryFilter, QuerySelect, SelectExt, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use serde::Serialize;

use crate::auth::AuthenticatedUserId;

/// Errors that may occur during the source code upload process.
#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum SourceCodeUploadError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// `multipart/form-data` request handling error.
    MultipartError(MultipartError),

    /// AWS S3-related error.
    S3Error(s3::Error),

    /// Request didn't have any file uploads in it.
    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    #[display(fmt = "no file upload was found")]
    NoFileUpload,

    /// Provided archive mime type is incorrect.
    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    #[display(fmt = "incorrect file content type")]
    IncorrectContentType,

    /// Deleted user attempted to upload an archive.
    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "non-existent user")]
    NonExistentUser,
}

/// JSON response body.
#[derive(Serialize)]
pub(super) struct SourceCodeUploadResponse {
    /// Source code identifier.
    id: i64,
}

/// Upload a new source code archive for later usages in build sessions.
///
/// This route accepts a `multipart/form-data` form with a single file field
/// that contains a ZIP archive, which will later be identified by its [`blake2`](common::hash::blake2)
/// hash.
///
/// Restrictions on file upload size are currently imposed via an HTTP proxy server,
/// and not the API server itself.
pub(super) async fn upload(
    Extension(current_user): Extension<AuthenticatedUserId>,
    Extension(config): Extension<Arc<Config>>,
    State(db): State<Arc<DatabaseConnection>>,
    mut data: Multipart,
) -> Result<Json<SourceCodeUploadResponse>, SourceCodeUploadError> {
    let archive = data
        .next_field()
        .await?
        .ok_or(SourceCodeUploadError::NoFileUpload)?;

    if let Some(content_type) = archive.content_type() {
        if content_type != "application/zip" {
            return Err(SourceCodeUploadError::IncorrectContentType);
        }
    }

    let archive = archive.bytes().await?;

    db.transaction(|txn| {
        Box::pin(async move {
            let user_exists = user::Entity::find_by_id(current_user.id())
                .select_only()
                .exists(txn)
                .await?;

            if user_exists {
                let archive_hash = hash::blake2(&archive).to_vec();

                let existing_source_code = source_code::Entity::find()
                    .select_only()
                    .column(source_code::Column::Id)
                    .filter(source_code::Column::ArchiveHash.eq(&*archive_hash))
                    .into_tuple::<i64>()
                    .one(txn)
                    .await?;

                let id = if let Some(id) = existing_source_code {
                    id
                } else {
                    s3::ConfiguredClient::new(&config.storage)
                        .await
                        .upload_source_code(&archive_hash[..], archive)
                        .await?;

                    let model = source_code::Entity::insert(source_code::ActiveModel {
                        user_id: ActiveValue::Set(Some(current_user.id())),
                        archive_hash: ActiveValue::Set(archive_hash),
                        ..Default::default()
                    })
                    .on_conflict(
                        OnConflict::column(source_code::Column::ArchiveHash)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec_with_returning(txn)
                    .await?;

                    model.id
                };

                Ok(Json(SourceCodeUploadResponse { id }))
            } else {
                Err(SourceCodeUploadError::NonExistentUser)
            }
        })
    })
    .await
    .into_raw_result()
}

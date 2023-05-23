use std::sync::Arc;

use axum::{
    extract::{multipart::MultipartError, Multipart, Path, State},
    http::StatusCode,
};
use axum_derive_error::ErrorResponse;
use db::{
    build_session_token, file, sea_query::OnConflict, ActiveValue, ColumnTrait, DatabaseConnection,
    DbErr, EntityTrait, QueryFilter, QuerySelect, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum UploadFileError {
    DatabaseError(DbErr),

    #[status(StatusCode::BAD_REQUEST)]
    MultipartError(MultipartError),

    #[status(StatusCode::FORBIDDEN)]
    #[display(fmt = "invalid token provided")]
    InvalidToken,

    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    #[display(fmt = "no file upload was found")]
    NoFileUpload,
}

pub(super) async fn upload(
    State(db): State<Arc<DatabaseConnection>>,
    Path(token): Path<String>,
    mut data: Multipart,
) -> Result<(), UploadFileError> {
    let archive = data
        .next_field()
        .await?
        .ok_or(UploadFileError::NoFileUpload)?;

    let name = archive
        .name()
        .ok_or(UploadFileError::NoFileUpload)?
        .to_string();

    let text = archive.text().await?;

    db.transaction(|txn| {
        Box::pin(async move {
            let source_code_id = build_session_token::Entity::find()
                .select_only()
                .column(build_session_token::Column::SourceCodeId)
                .filter(build_session_token::Column::Token.eq(token))
                .into_tuple::<i64>()
                .one(txn)
                .await?
                .ok_or(UploadFileError::InvalidToken)?;

            file::Entity::insert(file::ActiveModel {
                source_code_id: ActiveValue::Set(source_code_id),
                name: ActiveValue::Set(name),
                text: ActiveValue::Set(text),
                ..Default::default()
            })
            .on_conflict(
                OnConflict::columns([file::Column::SourceCodeId, file::Column::Name])
                    .update_column(file::Column::Text)
                    .to_owned(),
            )
            .exec_without_returning(txn)
            .await?;

            Ok(())
        })
    })
    .await
    .into_raw_result()
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, sync::Arc};

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::assert_json;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use common_multipart_rfc7578::client::multipart;
    use db::{
        build_session, build_session_token, source_code, user, ActiveValue, DatabaseConnection,
        EntityTrait,
    };
    use tower::{Service, ServiceExt};

    async fn create_test_env(db: &DatabaseConnection) -> i64 {
        let user = user::Entity::insert(user::ActiveModel::default())
            .exec_with_returning(db)
            .await
            .expect("unable to create user");

        let source_code_id = source_code::Entity::insert(source_code::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            archive_hash: ActiveValue::Set(Vec::new()),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to create source code")
        .id;

        let build_session_id = build_session::Entity::insert(build_session::ActiveModel {
            user_id: ActiveValue::Set(Some(user.id)),
            source_code_id: ActiveValue::Set(source_code_id),
            status: ActiveValue::Set(build_session::Status::New),
            cargo_contract_version: ActiveValue::Set(String::from("3.0.0")),
            rustc_version: ActiveValue::Set(String::from("1.69.0")),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert build session")
        .id;

        build_session_token::Entity::insert(build_session_token::ActiveModel {
            build_session_id: ActiveValue::Set(build_session_id),
            source_code_id: ActiveValue::Set(source_code_id),
            token: ActiveValue::Set(String::from("testtoken")),
        })
        .exec_without_returning(db)
        .await
        .expect("unable to create a build session token");

        build_session_id
    }

    #[tokio::test]
    async fn upload_and_seal() {
        let db = create_database().await;

        let build_session_id = create_test_env(&db).await;

        let mut form = multipart::Form::default();
        form.add_reader("lib.rs", Cursor::new(b"Hello, world"));

        let mut service = crate::app_router(Arc::new(db), Arc::new(Config::new().unwrap()));

        let response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/files/upload/testtoken")
                    .header("Content-Type", form.content_type())
                    .body(Body::wrap_stream(multipart::Body::from(form)))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let response = service
            .call(
                Request::builder()
                    .method("GET")
                    .uri(format!("/files/{}", build_session_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "files": [
                "lib.rs"
            ]
        });

        let response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/files/seal/testtoken")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let mut form = multipart::Form::default();
        form.add_reader("lib.rs", Cursor::new(b"Hello, world"));

        let response = service
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/files/upload/testtoken")
                    .header("Content-Type", form.content_type())
                    .body(Body::wrap_stream(multipart::Body::from(form)))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn empty_request() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::new().unwrap()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/files/upload/testtoken")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

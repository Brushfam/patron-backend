use std::sync::Arc;

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{file, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::example_error;

/// Max count of files that can be fetched from the database.
const MAX_FILES: u64 = 1000;

/// Query string that contains an optional file path to fetch.
#[derive(Deserialize, JsonSchema)]
pub(super) struct DetailsQuery {
    /// File path.
    ///
    /// If `null`, a list of file names will be returned instead.
    #[serde(default)]
    #[schemars(example = "crate::schema::example_file")]
    file: Option<String>,
}

/// Source code file details response.
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub(super) enum DetailsResponse {
    /// Single-file contents request.
    File {
        /// Contents of a single file.
        text: String,
    },

    /// List of files request.
    List {
        /// List of related file names.
        #[schemars(example = "crate::schema::example_files")]
        files: Vec<String>,
    },
}

/// Errors that may occur during the file details request handling.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum DetailsError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// The requested file was not found.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "file not found")]
    FileNotFound,
}

/// Generate OAPI documentation for the [`details`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Retrieve source code archive file details.")
        .description(
            r#"This route conditionally returns either a single file contents
or a list of files contained within a provided source code archive."#,
        )
        .response::<200, Json<DetailsResponse>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("File not found.")
                .example(example_error(DetailsError::FileNotFound))
        })
}

/// File details request handler.
///
/// Depending on query string contents, this route may either return
/// a list of files related to the provided source code identifier,
/// or a single file inside of a source code archive.
pub(super) async fn details(
    State(db): State<Arc<DatabaseConnection>>,
    Path(source_code_id): Path<i64>,
    Query(details): Query<DetailsQuery>,
) -> Result<Json<DetailsResponse>, DetailsError> {
    let response = if let Some(file) = details.file {
        file::Entity::find()
            .select_only()
            .column(file::Column::Text)
            .filter(file::Column::SourceCodeId.eq(source_code_id))
            .filter(file::Column::Name.eq(file))
            .into_tuple::<String>()
            .one(&*db)
            .await?
            .map(|text| DetailsResponse::File { text })
            .ok_or(DetailsError::FileNotFound)?
    } else {
        file::Entity::find()
            .select_only()
            .column(file::Column::Name)
            .filter(file::Column::SourceCodeId.eq(source_code_id))
            .limit(MAX_FILES)
            .into_tuple::<String>()
            .all(&*db)
            .await
            .map(|files| DetailsResponse::List { files })?
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::assert_json;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use common::config::Config;
    use db::{file, source_code, user, ActiveValue, DatabaseConnection, EntityTrait};
    use tower::ServiceExt;

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

        file::Entity::insert(file::ActiveModel {
            source_code_id: ActiveValue::Set(source_code_id),
            name: ActiveValue::Set(String::from("lib.rs")),
            text: ActiveValue::Set(String::from("Test file")),
            ..Default::default()
        })
        .exec_without_returning(db)
        .await
        .expect("unable to create a file");

        source_code_id
    }

    #[tokio::test]
    async fn single_file() {
        let db = create_database().await;

        let source_code_id = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/files/{}?file=lib.rs", source_code_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "text": "Test file"
        })
    }

    #[tokio::test]
    async fn unknown_file() {
        let db = create_database().await;

        let source_code_id = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/files/{}?file=main.rs", source_code_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn file_list() {
        let db = create_database().await;

        let source_code_id = create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/files/{}", source_code_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "files": [
                "lib.rs"
            ]
        })
    }
}

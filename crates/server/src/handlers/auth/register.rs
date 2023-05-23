use std::sync::Arc;

use axum::{extract::State, Json};
use axum_derive_error::ErrorResponse;
use db::{
    token, user, DatabaseConnection, DbErr, EntityTrait, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use serde::Serialize;

#[derive(ErrorResponse, Display, From, Error)]
pub(super) enum UserRegistrationError {
    DatabaseError(DbErr),
}

#[derive(Serialize)]
pub(super) struct UserRegistrationResponse {
    token: String,
}

pub(super) async fn register(
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<UserRegistrationResponse>, UserRegistrationError> {
    db.transaction(|txn| {
        Box::pin(async move {
            let user =
                user::Entity::insert(<db::user::ActiveModel as std::default::Default>::default())
                    .exec_with_returning(txn)
                    .await?;

            let (model, token) = token::generate_token(user.id);

            token::Entity::insert(model)
                .exec_without_returning(txn)
                .await?;

            Ok(Json(UserRegistrationResponse { token }))
        })
    })
    .await
    .into_raw_result()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::{create_database, ResponseBodyExt};

    use assert_json::{assert_json, validators};
    use axum::{body::Body, http::Request};
    use common::config::Config;
    use db::token::TOKEN_LENGTH;
    use tower::ServiceExt;

    #[tokio::test]
    async fn register() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::new().unwrap()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/register")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "token": validators::string(|val| {
                (val.len() == TOKEN_LENGTH)
                    .then_some(())
                    .ok_or(String::from("invalid length"))
            })
        });
    }
}

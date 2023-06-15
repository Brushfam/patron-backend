use std::{array::TryFromSliceError, sync::Arc};

use aide::{transform::TransformOperation, OperationIo};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_derive_error::ErrorResponse;
use db::{contract, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect};
use derive_more::{Display, Error, From};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use sp_core::{
    crypto::{AccountId32, Ss58Codec},
    ByteArray,
};

use crate::{hex_hash::HexHash, schema::example_error};

use super::WrappedAccountId32;

/// Errors that may occur during the contract details request handling.
#[derive(ErrorResponse, Display, From, Error, OperationIo)]
#[aide(output)]
pub(super) enum ContractDetailsError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Incorrect hash size stored inside of a database
    IncorrectArchiveHash(TryFromSliceError),

    /// Owner account attached to a contract is invalid.
    #[display(fmt = "incorrect address size of an owner account")]
    IncorrectAddressSizeOfOwner,

    /// The requested contract was not found.
    #[status(StatusCode::NOT_FOUND)]
    #[display(fmt = "contract not found")]
    ContractNotFound,
}

/// Contract details response.
#[derive(Serialize, JsonSchema)]
pub struct ContractData {
    /// Related code hash.
    #[schemars(example = "crate::schema::example_hex_hash")]
    pub code_hash: HexHash,

    /// Contract owner.
    ///
    /// This field is only available is the contract
    /// was discovered after the initial activation of an event server.
    #[schemars(example = "crate::schema::example_account")]
    pub owner: Option<String>,
}

/// Generate OAPI documentation for the [`details`] handler.
pub(super) fn docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get details about the provided contract account.")
        .response::<200, Json<ContractData>>()
        .response_with::<404, Json<Value>, _>(|op| {
            op.description("Provided contract account was not found.")
                .example(example_error(ContractDetailsError::ContractNotFound))
        })
}

/// Contract details request handler.
pub(super) async fn details(
    Path(account): Path<WrappedAccountId32>,
    State(db): State<Arc<DatabaseConnection>>,
) -> Result<Json<ContractData>, ContractDetailsError> {
    let (code_hash, owner) = contract::Entity::find()
        .select_only()
        .columns([contract::Column::CodeHash, contract::Column::Owner])
        .filter(contract::Column::Address.eq(account.0.as_slice()))
        .into_tuple::<(Vec<u8>, Option<Vec<u8>>)>()
        .one(&*db)
        .await?
        .ok_or(ContractDetailsError::ContractNotFound)?;

    let owner = owner
        .map(|address| {
            Result::<_, ContractDetailsError>::Ok(
                AccountId32::new(
                    address
                        .try_into()
                        .map_err(|_| ContractDetailsError::IncorrectAddressSizeOfOwner)?,
                )
                .to_ss58check(),
            )
        })
        .transpose()?;

    Ok(Json(ContractData {
        code_hash: code_hash.as_slice().try_into()?,
        owner,
    }))
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
    use db::{code, contract, node, ActiveValue, DatabaseConnection, EntityTrait};
    use sp_core::crypto::AccountId32;
    use tower::ServiceExt;

    async fn create_test_env(db: &DatabaseConnection) {
        let node = node::Entity::insert(node::ActiveModel {
            name: ActiveValue::Set(String::from("test")),
            url: ActiveValue::Set(String::from("ws://localhost:9944")),
            schema: ActiveValue::Set(String::from("test")),
            confirmed_block: ActiveValue::Set(0),
            ..Default::default()
        })
        .exec_with_returning(db)
        .await
        .expect("unable to insert node");

        code::Entity::insert(code::ActiveModel {
            hash: ActiveValue::Set(vec![0; 32]),
            code: ActiveValue::Set(vec![1, 2, 3]),
        })
        .exec_without_returning(db)
        .await
        .expect("unable to insert code");

        contract::Entity::insert(contract::ActiveModel {
            node_id: ActiveValue::Set(node.id),
            code_hash: ActiveValue::Set(vec![0; 32]),
            address: ActiveValue::Set(vec![1; 32]),
            owner: ActiveValue::Set(Some(vec![2; 32])),
            ..Default::default()
        })
        .exec_without_returning(db)
        .await
        .expect("unable to insert contract");
    }

    #[tokio::test]
    async fn successful() {
        let db = create_database().await;

        create_test_env(&db).await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/contracts/{}", AccountId32::new([1; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_json!(response.json().await, {
            "code_hash": hex::encode([0; 32]),
            "owner": AccountId32::from([2; 32]).to_string(),
        })
    }

    #[tokio::test]
    async fn unknown() {
        let db = create_database().await;

        let response = crate::app_router(Arc::new(db), Arc::new(Config::for_tests()))
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/contracts/{}", AccountId32::new([1; 32])))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

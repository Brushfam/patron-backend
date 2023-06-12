use std::str::FromStr;

use common::rpc::subxt::utils::AccountId32;
use db::{
    node, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, TransactionErrorExt,
    TransactionTrait,
};
use derive_more::{Display, Error, From};

/// Errors that may occur during payment contract address update process.
#[derive(Debug, Display, Error, From)]
pub enum UpdateContractError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Provided account id cannot be parsed.
    #[display(fmt = "invalid account id for payment contract")]
    InvalidPaymentAddress,
}

/// Update payment contract address.
///
/// # Details
///
/// Using [`update_contract`] you can update an account id of a payment contract
/// associated with the provided node.
///
/// Consult self-hosted documentation for more information on supported smart contract ABI.
pub async fn update_contract(
    database: DatabaseConnection,
    name: String,
    payment_address: Option<String>,
) -> Result<(), UpdateContractError> {
    let payment_address = payment_address
        .as_deref()
        .map(AccountId32::from_str)
        .transpose()
        .map_err(|_| UpdateContractError::InvalidPaymentAddress)?
        .map(|addr| addr.0.to_vec());

    database
        .transaction(|txn| {
            Box::pin(async move {
                node::Entity::update_many()
                    .filter(node::Column::Name.eq(name))
                    .col_expr(node::Column::PaymentContract, payment_address.into())
                    .exec(txn)
                    .await?;

                Ok(())
            })
        })
        .await
        .into_raw_result()
}

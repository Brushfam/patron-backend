//! # Database models and interation utilities
//!
//! This crate provides definitions for database models that can be used, in conjunction
//! with [`sea_orm`], to interact with the database in a typed manner.
//!
//! Additionally, this crate provides with utilities to map transaction errors ([`TransactionErrorExt::into_raw_result`])
//! and to provide other crates with commonly used `SELECT` query utilities [`SelectExt`].

pub mod build_session;
pub mod build_session_token;
pub mod cli_token;
pub mod code;
pub mod contract;
pub mod diagnostic;
pub mod event;
pub mod file;
pub mod log;
pub mod node;
pub mod public_key;
pub mod source_code;
pub mod token;
pub mod user;

use std::error::Error;

use async_trait::async_trait;
pub use sea_orm::{
    self, sea_query, ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, Database,
    DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait, StatementBuilder, TransactionError, TransactionTrait,
    TryGetableMany,
};
pub use time::{OffsetDateTime, PrimitiveDateTime};

/// Utility methods for operating with transaction errors.
pub trait TransactionErrorExt<T, E> {
    /// Convert transaction [`Result`] into a [`Result`] with a custom error.
    ///
    /// # Example
    ///
    /// ```
    /// use std::{error::Error, fmt::{self, Display}};
    ///
    /// use db::{DbErr, TransactionError, TransactionErrorExt, TransactionTrait};
    /// use tokio::runtime::Runtime;
    ///
    /// #[derive(Debug)]
    /// enum CustomError {
    ///     DbErr(DbErr),
    ///     Custom
    /// }
    ///
    /// impl Display for CustomError {
    ///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         write!(f, "custom error")
    ///     }
    /// }
    ///
    /// impl Error for CustomError {}
    ///
    ///
    /// impl From<DbErr> for CustomError {
    ///     fn from(err: DbErr) -> Self {
    ///         Self::DbErr(err)
    ///     }
    /// }
    ///
    /// Runtime::new()
    ///     .unwrap()
    ///     .block_on(async {
    ///         let db = db::Database::connect("sqlite::memory:")
    ///             .await
    ///             .expect("unable to create test database");
    ///
    ///         let result: Result<(), TransactionError<CustomError>> = db.transaction(|txn| {
    ///             Box::pin(async move {
    ///                 Err(CustomError::Custom)
    ///             })
    ///         })
    ///         .await;
    ///
    ///         let transformed_result: Result<(), CustomError> = result.into_raw_result();
    ///     });
    /// ```
    fn into_raw_result(self) -> Result<T, E>;
}

impl<T, E> TransactionErrorExt<T, E> for Result<T, TransactionError<E>>
where
    E: Error + From<DbErr>,
{
    fn into_raw_result(self) -> Result<T, E> {
        match self {
            Ok(val) => Ok(val),
            Err(TransactionError::Connection(err)) => Err(err.into()),
            Err(TransactionError::Transaction(err)) => Err(err),
        }
    }
}

/// Utility methods for SELECT queries.
#[async_trait]
pub trait SelectExt {
    /// Check if at least one record that satisfies a query.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Determine if at least one record of entity exists in the database
    /// let exists = Entity::find()
    ///     .select_only()
    ///     .exists(&db)
    ///     .await?;
    /// ```
    async fn exists<C: ConnectionTrait + Send>(self, db: &C) -> Result<bool, DbErr>;
}

#[async_trait]
impl<T> SelectExt for T
where
    T: QueryTrait<QueryStatement = sea_query::SelectStatement> + Send,
{
    async fn exists<C: ConnectionTrait + Send>(self, db: &C) -> Result<bool, DbErr> {
        use sea_query::{Expr, Query};

        let mut query = self.into_query();

        // Fix failing tests with SQLite by returning at least some expr
        query.expr(1);

        let stmt = StatementBuilder::build(
            Query::select().expr(Expr::exists(query)),
            &db.get_database_backend(),
        );

        db.query_one(stmt).await?.unwrap().try_get_by_index(0)
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{
        prelude::*,
        sea_query::{self, ColumnDef, Iden, Table},
        Database, QuerySelect,
    };

    use crate::SelectExt;

    #[derive(Iden)]
    enum TestVals {
        Table,
        Id,
    }

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "test_vals")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}

    #[tokio::test]
    async fn exists() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("unable to create test database");

        let table = Table::create()
            .table(TestVals::Table)
            .col(
                ColumnDef::new(TestVals::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .to_owned();

        let builder = db.get_database_backend();
        db.execute(builder.build(&table)).await.unwrap();

        let exists = Entity::find().select_only().exists(&db).await.unwrap();

        assert!(!exists);

        Entity::insert(<ActiveModel as std::default::Default>::default())
            .exec_without_returning(&db)
            .await
            .unwrap();

        let exists = Entity::find().select_only().exists(&db).await.unwrap();

        assert!(exists);
    }
}

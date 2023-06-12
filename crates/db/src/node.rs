//! Supported network instance.
//!
//! This model represents a single network with information about an RPC node,
//! its schema, last confirmed block for event client and optionally a payment contract
//! that can be used to acquire membership fees.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "nodes")]
pub struct Model {
    /// Unique node identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Node name.
    pub name: String,

    /// RPC node WebSocket URL.
    pub url: String,

    /// Node schema used to communicate with an RPC node.
    pub schema: String,

    /// Payment contract address.
    ///
    /// [`None`] if node doesn't provide such a contract.
    pub payment_contract: Option<Vec<u8>>,

    /// Last confirmed block that was discovered by an event client.
    ///
    /// `confirmed_block` value is used to catch-up to missed blocks if
    /// any such blocks are present.
    pub confirmed_block: i64,
}

/// Node model relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::contract::Entity")]
    Contracts,
}

impl Related<super::contract::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Contracts.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

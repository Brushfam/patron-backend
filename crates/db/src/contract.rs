//! A single smart contract model instance.
//!
//! This model is used to store information about discovered contracts.

use sea_orm::entity::prelude::*;

/// Smart contract information model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "contracts")]
pub struct Model {
    /// Unique contract identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related contract code hash.
    pub code_hash: Vec<u8>,

    /// Related contract node identifier.
    pub node_id: i64,

    /// Related contract address.
    pub address: Vec<u8>,

    /// Contract owner, if the contract was
    /// discovered via propagated node events.
    pub owner: Option<Vec<u8>>,
}

/// Smart contract model relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::code::Entity",
        from = "Column::CodeHash",
        to = "super::code::Column::Hash"
    )]
    Code,

    #[sea_orm(
        belongs_to = "super::node::Entity",
        from = "Column::NodeId",
        to = "super::node::Column::Id"
    )]
    Node,
}

impl Related<super::code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Code.def()
    }
}

impl Related<super::node::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Node.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

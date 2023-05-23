use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "contracts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub code_hash: Vec<u8>,
    pub node_id: i64,
    pub address: Vec<u8>,
    pub owner: Option<Vec<u8>>,
}

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

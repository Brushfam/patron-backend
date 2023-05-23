use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "codes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub hash: Vec<u8>,
    pub code: Vec<u8>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::contract::Entity")]
    Contracts,

    #[sea_orm(has_many = "super::build_session::Entity")]
    BuildSessions,
}

impl Related<super::contract::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Contracts.def()
    }
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BuildSessions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

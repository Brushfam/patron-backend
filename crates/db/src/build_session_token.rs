use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use sea_orm::entity::prelude::*;

pub const TOKEN_LENGTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "build_session_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub token: String,
    pub source_code_id: i64,
    pub build_session_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::source_code::Entity",
        from = "Column::SourceCodeId",
        to = "super::source_code::Column::Id"
    )]
    SourceCode,

    #[sea_orm(
        belongs_to = "super::build_session::Entity",
        from = "Column::BuildSessionId",
        to = "super::build_session::Column::Id"
    )]
    BuildSession,
}

impl Related<super::source_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceCode.def()
    }
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BuildSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

pub fn generate_token() -> String {
    Alphanumeric.sample_string(&mut thread_rng(), TOKEN_LENGTH)
}

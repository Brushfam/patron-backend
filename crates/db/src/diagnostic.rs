use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "diagnostics")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub build_session_id: i64,
    pub file_id: i64,
    pub level: Level,
    pub start: i64,
    pub end: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, JsonSchema)]
#[sea_orm(rs_type = "i16", db_type = "Integer")]
#[serde(rename_all = "snake_case")]
pub enum Level {
    #[sea_orm(num_value = 0)]
    Error,
    #[sea_orm(num_value = 1)]
    Warning,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::FileId",
        to = "super::file::Column::Id"
    )]
    File,

    #[sea_orm(
        belongs_to = "super::build_session::Entity",
        from = "Column::BuildSessionId",
        to = "super::build_session::Column::Id"
    )]
    BuildSession,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BuildSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

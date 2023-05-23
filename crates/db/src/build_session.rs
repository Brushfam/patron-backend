use sea_orm::{entity::prelude::*, FromQueryResult};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "build_sessions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: Option<i64>,
    pub source_code_id: i64,
    pub status: Status,
    pub cargo_contract_version: String,
    pub rustc_version: String,
    pub code_hash: Option<Vec<u8>>,
    pub metadata: Option<Vec<u8>>,
    pub created_at: TimeDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize)]
#[sea_orm(rs_type = "i16", db_type = "Integer")]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[sea_orm(num_value = 0)]
    New,
    #[sea_orm(num_value = 1)]
    Failed,
    #[sea_orm(num_value = 2)]
    Completed,
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
        belongs_to = "super::source_code::Entity",
        from = "Column::SourceCodeId",
        to = "super::source_code::Column::Id"
    )]
    SourceCode,

    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Code.def()
    }
}

impl Related<super::source_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceCode.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(FromQueryResult)]
pub struct ProcessedBuildSession {
    pub id: i64,
    pub source_code_id: i64,
    pub rustc_version: String,
    pub cargo_contract_version: String,
}

#[derive(Serialize, FromQueryResult)]
pub struct BuildSessionInfo {
    pub source_code_id: i64,
    pub cargo_contract_version: String,
    pub rustc_version: String,
}

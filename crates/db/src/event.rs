use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub node_id: i64,
    pub account: Vec<u8>,
    pub event_type: EventType,
    pub body: String,
    pub block_timestamp: TimeDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize)]
#[sea_orm(rs_type = "i16", db_type = "Integer")]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    #[sea_orm(num_value = 0)]
    Instantiation,
    #[sea_orm(num_value = 1)]
    CodeHashUpdate,
    #[sea_orm(num_value = 2)]
    Termination,
}

#[derive(Serialize)]
pub enum EventBody {
    Instantiation,

    CodeHashUpdate {
        /// New code hash, stored as a hex value.
        new_code_hash: String,
    },

    Termination,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

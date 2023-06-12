//! Discovered smart contract event model.
//!
//! These events are discovered by a separate event client server (also known as a sync server).

use sea_orm::entity::prelude::*;
use serde::Serialize;

/// Event model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "events")]
pub struct Model {
    /// Unique event identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related node identifier.
    pub node_id: i64,

    /// Related smart contract account identifier.
    pub account: Vec<u8>,

    /// Type of the current event model.
    pub event_type: EventType,

    /// Raw event body value, instantiated from a JSON serialization of a [`EventBody`] enum.
    pub body: String,

    /// Timestamp of a block during which the event occured.
    pub block_timestamp: TimeDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize)]
#[sea_orm(rs_type = "i16", db_type = "Integer")]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// A contract was instantiated.
    #[sea_orm(num_value = 0)]
    Instantiation,

    /// Contract's code hash was updated.
    #[sea_orm(num_value = 1)]
    CodeHashUpdate,

    /// A contract was terminated.
    #[sea_orm(num_value = 2)]
    Termination,
}

#[derive(Serialize)]
pub enum EventBody {
    /// A contract was instantiated.
    Instantiation,

    /// Contract's code hash was updated.
    CodeHashUpdate {
        /// New code hash, stored as a hex value.
        new_code_hash: String,
    },

    /// A contract was terminated.
    Termination,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

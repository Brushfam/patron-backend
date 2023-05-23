use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Contracts::Table)
                    .col(
                        ColumnDef::new(Contracts::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Contracts::CodeHash).binary().not_null())
                    .col(ColumnDef::new(Contracts::NodeId).big_integer().not_null())
                    .col(ColumnDef::new(Contracts::Address).binary().not_null())
                    .col(ColumnDef::new(Contracts::Owner).binary())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Contracts::Table, Contracts::NodeId)
                            .to(crate::Nodes::Table, crate::Nodes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("node_id_address_contracts_idx")
                            .col(Contracts::NodeId)
                            .col(Contracts::Address)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Contracts::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Contracts {
    Table,
    Id,
    CodeHash,
    NodeId,
    Address,
    Owner,
}

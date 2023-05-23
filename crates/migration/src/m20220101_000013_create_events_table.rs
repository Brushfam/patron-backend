use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Events::Table)
                    .col(
                        ColumnDef::new(Events::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Events::NodeId).big_integer().not_null())
                    .col(ColumnDef::new(Events::Account).binary().not_null())
                    .col(ColumnDef::new(Events::EventType).small_integer().not_null())
                    .col(ColumnDef::new(Events::Body).string().not_null())
                    .col(
                        ColumnDef::new(Events::BlockTimestamp)
                            .timestamp()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Events::Table, Events::NodeId)
                            .to(crate::Nodes::Table, crate::Nodes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Events::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Events {
    Table,
    Id,
    NodeId,
    Account,
    EventType,
    Body,
    BlockTimestamp,
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SourceCodes::Table)
                    .col(
                        ColumnDef::new(SourceCodes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SourceCodes::UserId).big_integer())
                    .col(
                        ColumnDef::new(SourceCodes::ArchiveHash)
                            .binary()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(SourceCodes::CreatedAt)
                            .timestamp()
                            .not_null()
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(SourceCodes::Table, SourceCodes::UserId)
                            .to(crate::Users::Table, crate::Users::Id)
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SourceCodes::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub(crate) enum SourceCodes {
    Table,
    Id,
    UserId,
    ArchiveHash,
    CreatedAt,
}

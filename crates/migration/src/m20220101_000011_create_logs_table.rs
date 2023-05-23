use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Logs::Table)
                    .col(
                        ColumnDef::new(Logs::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Logs::BuildSessionId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Logs::Text).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Logs::Table, Logs::BuildSessionId)
                            .to(crate::BuildSessions::Table, crate::BuildSessions::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Logs::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Logs {
    Table,
    Id,
    BuildSessionId,
    Text,
}

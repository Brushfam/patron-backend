use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Diagnostics::Table)
                    .col(
                        ColumnDef::new(Diagnostics::Id)
                            .big_integer()
                            .not_null()
                            .primary_key()
                            .auto_increment(),
                    )
                    .col(
                        ColumnDef::new(Diagnostics::BuildSessionId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Diagnostics::FileId).big_integer().not_null())
                    .col(
                        ColumnDef::new(Diagnostics::Level)
                            .small_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Diagnostics::Start).big_integer().not_null())
                    .col(ColumnDef::new(Diagnostics::End).big_integer().not_null())
                    .col(ColumnDef::new(Diagnostics::Message).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Diagnostics::Table, Diagnostics::FileId)
                            .to(crate::Files::Table, crate::Files::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Diagnostics::Table, Diagnostics::BuildSessionId)
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
            .drop_table(Table::drop().table(Diagnostics::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub(crate) enum Diagnostics {
    Table,
    Id,
    BuildSessionId,
    FileId,
    Level,
    Start,
    End,
    Message,
}

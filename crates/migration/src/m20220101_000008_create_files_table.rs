use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Files::Table)
                    .col(
                        ColumnDef::new(Files::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Files::SourceCodeId).big_integer().not_null())
                    .col(ColumnDef::new(Files::Name).string().not_null())
                    .col(ColumnDef::new(Files::Text).text().not_null())
                    .index(
                        Index::create()
                            .name("source_code_id_name_files_idx")
                            .col(Files::SourceCodeId)
                            .col(Files::Name)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Files::Table, Files::SourceCodeId)
                            .to(crate::SourceCodes::Table, crate::SourceCodes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Files::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Files {
    Table,
    Id,
    SourceCodeId,
    Name,
    Text,
}

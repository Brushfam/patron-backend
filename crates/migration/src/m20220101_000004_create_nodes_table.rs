use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Nodes::Table)
                    .col(
                        ColumnDef::new(Nodes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Nodes::Name).string().not_null().unique_key())
                    .col(ColumnDef::new(Nodes::Url).string().not_null().unique_key())
                    .col(ColumnDef::new(Nodes::Schema).string().not_null())
                    .col(ColumnDef::new(Nodes::PaymentContract).binary())
                    .col(
                        ColumnDef::new(Nodes::ConfirmedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Nodes::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub(crate) enum Nodes {
    Table,
    Id,
    Name,
    Url,
    Schema,
    PaymentContract,
    ConfirmedBlock,
}

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    error::DbError,
    indexer::{CF_META_NAME, DB_META_LAST_BLOCK_IN_DB_KEY, DbResult, SimpleStorableCell},
};

#[derive(Debug)]
pub struct LastBlockCell(pub u64);

impl BorshSerialize for LastBlockCell {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        u64::serialize(&self.0, writer)
    }
}

impl BorshDeserialize for LastBlockCell {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        u64::deserialize_reader(reader).map(|val| LastBlockCell(val))
    }
}

impl SimpleStorableCell for LastBlockCell {
    const CELL_NAME: &'static str = DB_META_LAST_BLOCK_IN_DB_KEY;
    const CF_NAME: &'static str = CF_META_NAME;

    fn key_constructor() -> DbResult<Vec<u8>> {
        borsh::to_vec(&Self::CELL_NAME).map_err(|err| {
            DbError::borsh_cast_message(
                err,
                Some(format!("Failed to serialize {:?}", Self::CELL_NAME)),
            )
        })
    }

    fn value_constructor(&self) -> DbResult<Vec<u8>> {
        borsh::to_vec(&self.0).map_err(|err| {
            DbError::borsh_cast_message(err, Some("Failed to serialize last block id".to_owned()))
        })
    }
}

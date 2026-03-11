use super::*;

#[derive(Debug, Clone)]
pub struct DBMetadata {
    pub first_block_in_db: u64,
    pub last_block_in_db: u64,
    pub last_observed_l1_lib_header_in_db: [u8; 32],
    pub is_first_block_set: bool,
    pub last_breakpoint_id: u64,
}

impl RocksDBIO {
    pub fn meta_keys_list() -> DbResult<Vec<Vec<u8>>> {
        let mut keys = vec![];

        keys.push(
            borsh::to_vec(&DB_META_FIRST_BLOCK_IN_DB_KEY).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to serialize DB_META_FIRST_BLOCK_IN_DB_KEY".to_string()),
                )
            })?,
        );
        keys.push(borsh::to_vec(&DB_META_LAST_BLOCK_IN_DB_KEY).map_err(|err| {
            DbError::borsh_cast_message(
                err,
                Some("Failed to serialize DB_META_LAST_BLOCK_IN_DB_KEY".to_string()),
            )
        })?);
        keys.push(
            borsh::to_vec(&DB_META_LAST_OBSERVED_L1_LIB_HEADER_ID_IN_DB_KEY).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some(
                        "Failed to serialize DB_META_LAST_OBSERVED_L1_LIB_HEADER_ID_IN_DB_KEY"
                            .to_string(),
                    ),
                )
            })?,
        );
        keys.push(borsh::to_vec(&DB_META_FIRST_BLOCK_SET_KEY).map_err(|err| {
            DbError::borsh_cast_message(
                err,
                Some("Failed to serialize DB_META_FIRST_BLOCK_SET_KEY".to_string()),
            )
        })?);
        keys.push(borsh::to_vec(&DB_META_LAST_BREAKPOINT_ID).map_err(|err| {
            DbError::borsh_cast_message(
                err,
                Some("Failed to serialize DB_META_LAST_BREAKPOINT_ID".to_string()),
            )
        })?);

        Ok(keys)
    }

    pub fn read_meta_all(&self) -> DbResult<Option<DBMetadata>> {
        let cf_meta = self.meta_column();

        let multi_get_res = self.db.multi_get_cf(
            RocksDBIO::meta_keys_list()?
                .into_iter()
                .map(|key| (&cf_meta, key)),
        );

        let Some(first_block_in_db_raw) =
            multi_get_res[0]
                .as_ref()
                .map_err(|err| DbError::RocksDbError {
                    error: err.clone(),
                    additional_info: Some("Failed to read first_block_in_db".to_string()),
                })?
        else {
            return Ok(None);
        };
        let Some(last_block_in_db_raw) =
            multi_get_res[1]
                .as_ref()
                .map_err(|err| DbError::RocksDbError {
                    error: err.clone(),
                    additional_info: Some("Failed to read last_block_in_db".to_string()),
                })?
        else {
            return Ok(None);
        };
        let Some(last_observed_l1_lib_header_in_db_raw) =
            multi_get_res[2]
                .as_ref()
                .map_err(|err| DbError::RocksDbError {
                    error: err.clone(),
                    additional_info: Some(
                        "Failed to read last_observed_l1_lib_header_in_db".to_string(),
                    ),
                })?
        else {
            return Ok(None);
        };
        let is_first_block_set = multi_get_res[3]
            .as_ref()
            .map_err(|err| DbError::RocksDbError {
                error: err.clone(),
                additional_info: Some("Failed to read is_first_block_set".to_string()),
            })?
            .is_some();
        let Some(last_breakpoint_id_raw) =
            multi_get_res[4]
                .as_ref()
                .clone()
                .map_err(|err| DbError::RocksDbError {
                    error: err.clone(),
                    additional_info: Some("Failed to read last_breakpoint_id".to_string()),
                })?
        else {
            return Ok(None);
        };

        let first_block_in_db = borsh::from_slice::<u64>(first_block_in_db_raw).map_err(|err| {
            DbError::borsh_cast_message(err, Some("Failed to deserialize first block".to_string()))
        })?;
        let last_block_in_db = borsh::from_slice::<u64>(last_block_in_db_raw).map_err(|err| {
            DbError::borsh_cast_message(err, Some("Failed to deserialize last block".to_string()))
        })?;
        let last_observed_l1_lib_header_in_db = borsh::from_slice::<[u8; 32]>(
            last_observed_l1_lib_header_in_db_raw,
        )
        .map_err(|err| {
            DbError::borsh_cast_message(
                err,
                Some("Failed to deserialize last l1 lib header".to_string()),
            )
        })?;
        let last_breakpoint_id =
            borsh::from_slice::<u64>(last_breakpoint_id_raw).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize last breakpoint id".to_string()),
                )
            })?;

        Ok(Some(DBMetadata {
            first_block_in_db,
            last_block_in_db,
            last_observed_l1_lib_header_in_db,
            is_first_block_set,
            last_breakpoint_id,
        }))
    }

    pub fn get_block_batch(&self, before: Option<u64>, limit: u64) -> DbResult<Vec<Block>> {
        let mut seq = vec![];

        // Determine the starting block ID
        let start_block_id = if let Some(before_id) = before {
            before_id.saturating_sub(1)
        } else {
            // Get the latest block ID
            self.get_meta_last_block_in_db()?
        };

        for i in 0..limit {
            let block_id = start_block_id.saturating_sub(i);
            if block_id == 0 {
                break;
            }
            seq.push(block_id);
        }

        self.get_block_batch_seq(seq.into_iter())
    }

    /// Get block batch from a sequence
    ///
    /// Currently assumes non-decreasing sequence
    ///
    /// ToDo: Add suport of arbitrary sequences
    fn get_block_batch_seq(&self, seq: impl Iterator<Item = u64>) -> DbResult<Vec<Block>> {
        let cf_block = self.block_column();

        // Keys setup
        let mut keys = vec![];
        for block_id in seq {
            keys.push((
                &cf_block,
                borsh::to_vec(&block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block id".to_string()),
                    )
                })?,
            ));
        }

        let multi_get_res = self.db.multi_get_cf(keys);

        // Keys parsing
        let mut block_batch = vec![];
        for res in multi_get_res {
            let res = res.map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

            let block = if let Some(data) = res {
                Ok(borsh::from_slice::<Block>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize block data".to_string()),
                    )
                })?)
            } else {
                // Block not found, assuming that previous one was the last
                break;
            }?;

            block_batch.push(block);
        }

        Ok(block_batch)
    }

    /// Get block ids by txs
    ///
    /// Transactions must be sorted by time of arrival
    ///
    /// ToDo: There may be multiple transactions in one block
    /// so this method can take redundant reads.
    /// Need to update signature and implementation.
    fn get_block_ids_by_tx_vec(&self, tx_vec: &[[u8; 32]]) -> DbResult<Vec<u64>> {
        let cf_tti = self.tx_hash_to_id_column();

        // Keys setup
        let mut keys = vec![];
        for tx_hash in tx_vec {
            keys.push((
                &cf_tti,
                borsh::to_vec(tx_hash).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize tx_hash".to_string()),
                    )
                })?,
            ));
        }

        let multi_get_res = self.db.multi_get_cf(keys);

        // Keys parsing
        let mut block_id_batch = vec![];
        for res in multi_get_res {
            let res = res.map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

            let block_id = if let Some(data) = res {
                Ok(borsh::from_slice::<u64>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize block id".to_string()),
                    )
                })?)
            } else {
                // Block not found, assuming that previous one was the last
                break;
            }?;

            block_id_batch.push(block_id);
        }

        Ok(block_id_batch)
    }

    // Account

    pub(crate) fn get_acc_transaction_hashes(
        &self,
        acc_id: [u8; 32],
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<[u8; 32]>> {
        let cf_att = self.account_id_to_tx_hash_column();
        let mut tx_batch = vec![];

        // Keys preparation
        let mut keys = vec![];
        for tx_id in offset..(offset + limit) {
            let mut prefix = borsh::to_vec(&acc_id).map_err(|berr| {
                DbError::borsh_cast_message(
                    berr,
                    Some("Failed to serialize account id".to_string()),
                )
            })?;
            let suffix = borsh::to_vec(&tx_id).map_err(|berr| {
                DbError::borsh_cast_message(berr, Some("Failed to serialize tx id".to_string()))
            })?;

            prefix.extend_from_slice(&suffix);

            keys.push((&cf_att, prefix));
        }

        let multi_get_res = self.db.multi_get_cf(keys);

        for res in multi_get_res {
            let res = res.map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

            let tx_hash = if let Some(data) = res {
                Ok(borsh::from_slice::<[u8; 32]>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize tx_hash".to_string()),
                    )
                })?)
            } else {
                // Tx hash not found, assuming that previous one was the last
                break;
            }?;

            tx_batch.push(tx_hash);
        }

        Ok(tx_batch)
    }

    pub fn get_acc_transactions(
        &self,
        acc_id: [u8; 32],
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<NSSATransaction>> {
        let mut tx_batch = vec![];

        let tx_hashes = self.get_acc_transaction_hashes(acc_id, offset, limit)?;

        let associated_blocks_multi_get = self
            .get_block_batch_seq(self.get_block_ids_by_tx_vec(&tx_hashes)?.into_iter())?
            .into_iter()
            .zip(tx_hashes);

        for (block, tx_hash) in associated_blocks_multi_get {
            let transaction = block
                .body
                .transactions
                .iter()
                .find(|tx| tx.hash().0 == tx_hash)
                .ok_or(DbError::db_interaction_error(format!(
                    "Missing transaction in block {} with hash {:#?}",
                    block.header.block_id, tx_hash
                )))?;

            tx_batch.push(transaction.clone());
        }

        Ok(tx_batch)
    }
}

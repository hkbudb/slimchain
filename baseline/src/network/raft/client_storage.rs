use super::{
    message::{NewBlockRequest, NewBlockResponse},
    rpc::get_block,
};
use crate::{
    behavior::{commit_block, verify_block},
    block::{
        raft::{verify_consensus, Block},
        BlockLoaderTrait, BlockTrait,
    },
    db::{DBPtr, Transaction as DBTransaction},
};
use async_raft::{
    raft::{Entry, EntryPayload, MembershipConfig},
    storage::{CurrentSnapshotData, HardState, InitialState},
    RaftStorage,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use slimchain_chain::latest::{LatestTxCount, LatestTxCountPtr};
use slimchain_common::{
    basic::{BlockHeight, H256},
    digest::Digestible,
    error::{anyhow, ensure, Context, Result},
    iter::iter_result,
};
use slimchain_network::{
    behavior::raft::storage::fetch_leader_id,
    http::config::{NetworkConfig, NetworkRouteTable, PeerId},
};
use slimchain_tx_state::TxStateUpdate;
use std::{collections::BTreeSet, io::Cursor};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone, Serialize, Deserialize)]
struct RaftSnapshot {
    index: u64,
    term: u64,
    membership: MembershipConfig,
    latest_block: Block,
}

#[derive(Clone, Serialize, Deserialize)]
struct RaftStateMachine {
    last_applied_log: u64,
    latest_block: Block,
}

pub struct ClientNodeStorage {
    peer_id: PeerId,
    route_table: NetworkRouteTable,
    latest_tx_count: LatestTxCountPtr,
    db: DBPtr,
    raft_log: RwLock<BTreeSet<u64>>,
    raft_snapshot: RwLock<Option<RaftSnapshot>>,
    raft_sm: RwLock<RaftStateMachine>,
    miner_update: Mutex<Option<(H256, TxStateUpdate)>>,
}

impl ClientNodeStorage {
    pub fn new(db: DBPtr, net_cfg: &NetworkConfig) -> Result<Self> {
        let height = db.get_meta_object("height")?.unwrap_or_default();
        let latest_block: Block = db
            .get_block(height)
            .context("Failed to get the latest block.")?;
        let latest_tx_count = LatestTxCount::new(0);

        let last_applied_log = db.get_meta_object("raft-last-applied")?.unwrap_or_default();
        let log = db.get_meta_object("raft-log")?.unwrap_or_default();
        let last_snapshot = db.get_meta_object("raft-snapshot")?.unwrap_or_default();

        Ok(Self {
            peer_id: net_cfg.peer_id,
            route_table: net_cfg.to_route_table(),
            latest_tx_count,
            db,
            raft_log: RwLock::new(log),
            raft_snapshot: RwLock::new(last_snapshot),
            raft_sm: RwLock::new(RaftStateMachine {
                last_applied_log,
                latest_block,
            }),
            miner_update: Mutex::new(None),
        })
    }

    pub fn db(&self) -> DBPtr {
        self.db.clone()
    }

    pub async fn latest_block_height(&self) -> BlockHeight {
        let sm = self.raft_sm.read().await;
        sm.latest_block.block_height()
    }

    pub async fn get_block(&self, height: BlockHeight) -> Result<Block> {
        self.db.get_block(height)
    }

    pub fn latest_tx_count(&self) -> LatestTxCountPtr {
        self.latest_tx_count.clone()
    }

    pub async fn set_miner_update(&self, block: &Block, tx_state_update: TxStateUpdate) {
        let blk_hash = block.to_digest();
        let mut miner_update = self.miner_update.lock().await;
        *miner_update = Some((blk_hash, tx_state_update));
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn save_to_db(&self) -> Result<()> {
        let sm = self.raft_sm.read().await;
        let log = self.raft_log.read().await;
        let snapshot = self.raft_snapshot.read().await;
        let mut db_tx = DBTransaction::new();
        db_tx.insert_meta_object("height", &sm.latest_block.block_height())?;
        db_tx.insert_meta_object("raft-last-applied", &sm.last_applied_log)?;
        db_tx.insert_meta_object("raft-log", &(*log))?;
        db_tx.insert_meta_object("raft-snapshot", &(*snapshot))?;
        self.db.write_async(db_tx).await
    }

    fn read_log(&self, idx: u64) -> Result<Entry<NewBlockRequest>> {
        self.db
            .get_log_object(idx)?
            .ok_or_else(|| anyhow!("Failed to read raft log. idx={}", idx))
    }
}

#[async_trait]
impl RaftStorage<NewBlockRequest, NewBlockResponse> for ClientNodeStorage {
    type Snapshot = Cursor<Vec<u8>>;

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn get_membership_config(&self) -> Result<MembershipConfig> {
        let log = self.raft_log.read().await;
        let cfg_opt = iter_result(
            log.iter().rev().map(|idx| self.read_log(*idx)),
            |mut iter| {
                iter.find_map(|entry| match &entry.payload {
                    EntryPayload::ConfigChange(cfg) => Some(cfg.membership.clone()),
                    EntryPayload::SnapshotPointer(snap) => Some(snap.membership.clone()),
                    _ => None,
                })
            },
        )?;
        Ok(match cfg_opt {
            Some(cfg) => cfg,
            None => MembershipConfig::new_initial(self.peer_id.into()),
        })
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn get_initial_state(&self) -> Result<InitialState> {
        let membership = self.get_membership_config().await?;
        let log = self.raft_log.read().await;
        let sm = self.raft_sm.read().await;
        let hs: Option<HardState> = self.db.get_meta_object("raft-hs")?;
        let state = match hs {
            Some(hard_state) => {
                let (last_log_index, last_log_term) = match log.iter().rev().next() {
                    Some(idx) => {
                        let log_entry = self.read_log(*idx)?;
                        (log_entry.index, log_entry.term)
                    }
                    None => (0, 0),
                };
                let last_applied_log = sm.last_applied_log;
                InitialState {
                    last_log_index,
                    last_log_term,
                    last_applied_log,
                    hard_state,
                    membership,
                }
            }
            None => {
                let new = InitialState::new_initial(self.peer_id.into());
                self.save_hard_state(&new.hard_state).await?;
                new
            }
        };
        Ok(state)
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self, hs), err)]
    async fn save_hard_state(&self, hs: &HardState) -> Result<()> {
        let mut db_tx = DBTransaction::new();
        db_tx.insert_meta_object("raft-hs", hs)?;
        self.db.write_async(db_tx).await
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn get_log_entries(&self, start: u64, stop: u64) -> Result<Vec<Entry<NewBlockRequest>>> {
        if start > stop {
            error!("invalid request, start > stop");
            return Ok(vec![]);
        }
        let _log = self.raft_log.read().await;
        (start..stop).map(|idx| self.read_log(idx)).collect()
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn delete_logs_from(&self, start: u64, stop: Option<u64>) -> Result<()> {
        if let Some(stop) = stop {
            if start > stop {
                error!("invalid request, start > stop");
                return Ok(());
            }
        }

        let mut db_tx = DBTransaction::new();
        let mut log = self.raft_log.write().await;

        if let Some(stop) = stop.as_ref() {
            for key in start..*stop {
                log.remove(&key);
                db_tx.delete_log_object(key);
            }
        } else {
            for key in log.split_off(&start) {
                db_tx.delete_log_object(key);
            }
        }
        self.db.write_async(db_tx).await
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn append_entry_to_log(&self, entry: &Entry<NewBlockRequest>) -> Result<()> {
        let mut db_tx = DBTransaction::new();
        let mut log = self.raft_log.write().await;
        log.insert(entry.index);
        db_tx.insert_log_object(entry.index, entry)?;
        self.db.write_async(db_tx).await
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn replicate_to_log(&self, entries: &[Entry<NewBlockRequest>]) -> Result<()> {
        let mut db_tx = DBTransaction::new();
        let mut log = self.raft_log.write().await;
        for entry in entries {
            log.insert(entry.index);
            db_tx.insert_log_object(entry.index, entry)?;
        }
        self.db.write_async(db_tx).await
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn apply_entry_to_state_machine(
        &self,
        index: &u64,
        data: &NewBlockRequest,
    ) -> Result<NewBlockResponse> {
        let mut sm = self.raft_sm.write().await;
        let blk_proposal = &data.0;
        let blk_proposal_height = blk_proposal.block_height();
        let snapshot_height = sm.latest_block.block_height();

        if blk_proposal_height <= snapshot_height {
            return Ok(NewBlockResponse::Ok);
        } else if blk_proposal_height != snapshot_height.next_height() {
            let err = format!(
                "Invalid block height. curr: {}, proposal: {}",
                snapshot_height, blk_proposal_height
            );
            return Ok(NewBlockResponse::Err(err));
        }

        let mut state_update = TxStateUpdate::default();
        let mut miner = false;

        let mut miner_update = self.miner_update.lock().await;
        if let Some((blk_hash, update)) = miner_update.take() {
            if blk_proposal.to_digest() == blk_hash {
                state_update = update;
                miner = true;
            }
        }

        if !miner {
            match verify_block(
                &self.db,
                sm.latest_block.block_height(),
                blk_proposal,
                verify_consensus,
            )
            .await
            {
                Ok(update) => {
                    state_update = update;
                }
                Err(e) => {
                    let err = format!("Failed to import block. Error: {}", e);
                    return Ok(NewBlockResponse::Err(err));
                }
            }
        }

        if let Err(e) =
            commit_block(&self.db, blk_proposal, &state_update, &self.latest_tx_count).await
        {
            let err = format!("Failed to commit block. Error: {}", e);
            return Ok(NewBlockResponse::Err(err));
        }

        sm.last_applied_log = *index;
        sm.latest_block = blk_proposal.clone();

        Ok(NewBlockResponse::Ok)
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn replicate_to_state_machine(&self, entries: &[(&u64, &NewBlockRequest)]) -> Result<()> {
        for (index, data) in entries {
            self.apply_entry_to_state_machine(*index, *data).await?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn do_log_compaction(&self) -> Result<CurrentSnapshotData<Self::Snapshot>> {
        let sm_copy = self.raft_sm.read().await.clone();
        let last_applied_log = sm_copy.last_applied_log;
        let membership_config = {
            let log = self.raft_log.read().await;
            iter_result(log.iter().rev().map(|idx| self.read_log(*idx)), |iter| {
                iter.skip_while(|entry| entry.index > last_applied_log)
                    .find_map(|entry| match &entry.payload {
                        EntryPayload::ConfigChange(cfg) => Some(cfg.membership.clone()),
                        _ => None,
                    })
            })?
            .unwrap_or_else(|| MembershipConfig::new_initial(self.peer_id.into()))
        };

        let term;
        let snapshot_bytes: Vec<u8>;
        {
            let mut db_tx = DBTransaction::new();
            let mut log = self.raft_log.write().await;
            let mut current_snapshot = self.raft_snapshot.write().await;
            term = log
                .get(&last_applied_log)
                .and_then(|idx| self.read_log(*idx).ok())
                .map(|entry| entry.term)
                .ok_or_else(|| {
                    anyhow!(
                        "last_applied_log {} not available during log compaction",
                        last_applied_log
                    )
                })?;

            let new_log = log.split_off(&last_applied_log);
            for &idx in log.iter() {
                if idx != last_applied_log {
                    db_tx.delete_log_object(idx);
                }
            }
            *log = new_log;
            log.insert(last_applied_log);

            db_tx.insert_log_object(
                last_applied_log,
                &Entry::<NewBlockRequest>::new_snapshot_pointer(
                    last_applied_log,
                    term,
                    "".into(),
                    membership_config.clone(),
                ),
            )?;
            self.db.write_async(db_tx).await?;

            let snapshot = RaftSnapshot {
                index: last_applied_log,
                term,
                membership: membership_config.clone(),
                latest_block: sm_copy.latest_block,
            };
            snapshot_bytes = postcard::to_allocvec(&snapshot)?;
            *current_snapshot = Some(snapshot);
        };

        Ok(CurrentSnapshotData {
            term,
            index: last_applied_log,
            membership: membership_config.clone(),
            snapshot: Box::new(Cursor::new(snapshot_bytes)),
        })
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn create_snapshot(&self) -> Result<(String, Box<Self::Snapshot>)> {
        Ok((String::from(""), Box::new(Cursor::new(Vec::new()))))
    }

    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self, snapshot), err)]
    async fn finalize_snapshot_installation(
        &self,
        index: u64,
        term: u64,
        delete_through: Option<u64>,
        id: String,
        snapshot: Box<Self::Snapshot>,
    ) -> Result<()> {
        let new_snapshot: RaftSnapshot = postcard::from_bytes(snapshot.get_ref().as_slice())?;

        {
            let mut db_tx = DBTransaction::new();
            let mut log = self.raft_log.write().await;
            let membership_config =
                iter_result(log.iter().rev().map(|idx| self.read_log(*idx)), |iter| {
                    iter.skip_while(|entry| entry.index > index)
                        .find_map(|entry| match &entry.payload {
                            EntryPayload::ConfigChange(cfg) => Some(cfg.membership.clone()),
                            _ => None,
                        })
                })?
                .unwrap_or_else(|| MembershipConfig::new_initial(self.peer_id.into()));

            match &delete_through {
                Some(through) => {
                    let new_log = log.split_off(&(through + 1));
                    for &idx in log.iter() {
                        if idx != index {
                            db_tx.delete_log_object(idx);
                        }
                    }
                    *log = new_log;
                }
                None => {
                    for &idx in log.iter() {
                        if idx != index {
                            db_tx.delete_log_object(idx);
                        }
                    }
                    log.clear();
                }
            }
            log.insert(index);
            db_tx.insert_log_object(
                index,
                &Entry::<NewBlockRequest>::new_snapshot_pointer(index, term, id, membership_config),
            )?;
            self.db.write_async(db_tx).await?;
        }

        {
            let mut sm = self.raft_sm.write().await;

            let leader_id = fetch_leader_id(&self.route_table).await?;
            let leader_addr = self.route_table.peer_address(leader_id)?;

            let mut height = sm.latest_block.block_height();
            while height < new_snapshot.latest_block.block_height() {
                let block = get_block(leader_addr, height.next_height()).await?;
                let update = verify_block(&self.db, height, &block, verify_consensus).await?;
                commit_block(&self.db, &block, &update, &self.latest_tx_count).await?;

                height = height.next_height();

                if height == new_snapshot.latest_block.block_height() {
                    ensure!(block == new_snapshot.latest_block, "inconsistent block");
                }
            }

            sm.last_applied_log = new_snapshot.index;
            sm.latest_block = new_snapshot.latest_block.clone();
        }

        {
            let mut current_snapshot = self.raft_snapshot.write().await;
            *current_snapshot = Some(new_snapshot);
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn get_current_snapshot(&self) -> Result<Option<CurrentSnapshotData<Self::Snapshot>>> {
        match &*self.raft_snapshot.read().await {
            Some(snapshot) => {
                let reader = postcard::to_allocvec(&snapshot)?;
                Ok(Some(CurrentSnapshotData {
                    index: snapshot.index,
                    term: snapshot.term,
                    membership: snapshot.membership.clone(),
                    snapshot: Box::new(Cursor::new(reader)),
                }))
            }
            None => Ok(None),
        }
    }
}

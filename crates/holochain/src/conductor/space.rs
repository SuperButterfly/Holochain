//! This module contains data and functions for running operations
//! at the level of a [`DnaHash`] space.
//! Multiple [`Cell`](crate::conductor::Cell)'s could share the same space.
use std::{collections::HashMap, sync::Arc, time::Duration};

use holo_hash::{DhtOpHash, DnaHash};
use holochain_conductor_api::conductor::{ConductorConfig, DatabaseRootPath};
use holochain_p2p::{
    dht::{
        arq::{power_and_count_from_length, ArqBoundsSet},
        hash::RegionHash,
        region::{RegionBounds, RegionData},
        region_set::{RegionCoordSetLtcs, RegionSetLtcs},
        spacetime::TelescopingTimes,
        ArqBounds, ArqStrat,
    },
    dht_arc::{DhtArcRange, DhtArcSet},
    event::FetchOpDataQuery,
};
use holochain_sqlite::{
    conn::{DbSyncLevel, DbSyncStrategy},
    db::{
        DbKindAuthored, DbKindCache, DbKindConductor, DbKindDht, DbKindP2pAgents, DbKindP2pMetrics,
        DbKindWasm, DbWrite, ReadAccess,
    },
    prelude::{DatabaseError, DatabaseResult},
};
use holochain_state::{
    prelude::{from_blob, StateQueryResult},
    query::{map_sql_dht_op_common, StateQueryError},
};
use holochain_types::{
    db_cache::DhtDbQueryCache,
    dht_op::{DhtOp, DhtOpType},
};
use holochain_zome_types::{DnaDefHashed, Entry, EntryVisibility, SignedHeader, Timestamp};
use kitsune_p2p::{
    event::{TimeWindow, TimeWindowInclusive},
    KitsuneP2pConfig,
};
use rusqlite::named_params;
use tracing::instrument;

use crate::core::{
    queue_consumer::QueueConsumerMap,
    workflow::{
        countersigning_workflow::{incoming_countersigning, CountersigningWorkspace},
        incoming_dht_ops_workflow::{
            incoming_dht_ops_workflow, IncomingOpHashes, IncomingOpsBatch,
        },
    },
};

use super::{
    conductor::RwShare,
    error::ConductorResult,
    p2p_agent_store::{self, P2pBatch},
};
use std::convert::TryInto;

#[cfg(test)]
mod tests;

#[derive(Clone)]
/// This is the set of all current
/// [`DnaHash`] spaces for all cells
/// installed on this conductor.
pub struct Spaces {
    map: RwShare<HashMap<DnaHash, Space>>,
    pub(crate) db_dir: Arc<DatabaseRootPath>,
    pub(crate) db_sync_strategy: DbSyncStrategy,
    /// The map of running queue consumer workflows.
    pub(crate) queue_consumer_map: QueueConsumerMap,
    pub(crate) conductor_db: DbWrite<DbKindConductor>,
    pub(crate) wasm_db: DbWrite<DbKindWasm>,
    network_config: KitsuneP2pConfig,
}

#[derive(Clone)]
/// This is the set of data required at the
/// [`DnaHash`] space level.
/// All cells in the same [`DnaHash`] space
/// will share these.
pub struct Space {
    /// The dna hash for this space.
    pub dna_hash: Arc<DnaHash>,

    /// The caches databases. These are shared across cells.
    /// There is one per unique Dna.
    pub cache_db: DbWrite<DbKindCache>,

    /// The authored databases. These are shared across cells.
    /// There is one per unique Dna.
    pub authored_db: DbWrite<DbKindAuthored>,

    /// The dht databases. These are shared across cells.
    /// There is one per unique Dna.
    pub dht_db: DbWrite<DbKindDht>,

    /// The database for storing AgentInfoSigned
    pub p2p_agents_db: DbWrite<DbKindP2pAgents>,

    /// The database for storing p2p MetricDatum(s)
    pub p2p_metrics_db: DbWrite<DbKindP2pMetrics>,

    /// The batch sender for writes to the p2p database.
    pub p2p_batch_sender: tokio::sync::mpsc::Sender<P2pBatch>,

    /// A cache for slow database queries.
    pub dht_query_cache: DhtDbQueryCache,

    /// Countersigning workspace that is shared across this cell.
    pub countersigning_workspace: CountersigningWorkspace,

    /// Incoming op hashes that are queued for processing.
    pub incoming_op_hashes: IncomingOpHashes,

    /// Incoming ops batch for this space.
    pub incoming_ops_batch: IncomingOpsBatch,
}

#[cfg(test)]
pub struct TestSpaces {
    pub spaces: Spaces,
    pub test_spaces: HashMap<DnaHash, TestSpace>,
    pub queue_consumer_map: QueueConsumerMap,
}
#[cfg(test)]
pub struct TestSpace {
    pub space: Space,
    _temp_dir: tempfile::TempDir,
}

impl Spaces {
    /// Create a new empty set of [`DnaHash`] spaces.
    pub fn new(config: &ConductorConfig) -> ConductorResult<Self> {
        let root_db_dir = config.environment_path.clone();
        let db_sync_strategy = config.db_sync_strategy.clone();
        let db_sync_level = match db_sync_strategy {
            DbSyncStrategy::Fast => DbSyncLevel::Off,
            DbSyncStrategy::Resilient => DbSyncLevel::Normal,
        };
        let conductor_db =
            DbWrite::open_with_sync_level(root_db_dir.as_ref(), DbKindConductor, db_sync_level)?;
        let wasm_db =
            DbWrite::open_with_sync_level(root_db_dir.as_ref(), DbKindWasm, db_sync_level)?;
        Ok(Spaces {
            map: RwShare::new(HashMap::new()),
            db_dir: Arc::new(root_db_dir),
            db_sync_strategy,
            queue_consumer_map: QueueConsumerMap::new(),
            conductor_db,
            wasm_db,
            network_config: config.network.clone().unwrap_or_default(),
        })
    }

    /// Get something from every space
    pub fn get_from_spaces<R, F: FnMut(&Space) -> R>(&self, f: F) -> Vec<R> {
        self.map
            .share_ref(|spaces| spaces.values().map(f).collect())
    }

    /// Get the space if it exists or create it if it doesn't.
    pub fn get_or_create_space(&self, dna_hash: &DnaHash) -> ConductorResult<Space> {
        self.get_or_create_space_ref(dna_hash, Space::clone)
    }

    fn get_or_create_space_ref<F, R>(&self, dna_hash: &DnaHash, f: F) -> ConductorResult<R>
    where
        F: Fn(&Space) -> R,
    {
        match self.map.share_ref(|spaces| spaces.get(dna_hash).map(&f)) {
            Some(r) => Ok(r),
            None => self
                .map
                .share_mut(|spaces| match spaces.entry(dna_hash.clone()) {
                    std::collections::hash_map::Entry::Occupied(entry) => Ok(f(entry.get())),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let space = Space::new(
                            Arc::new(dna_hash.clone()),
                            &self.db_dir,
                            self.db_sync_strategy,
                        )?;

                        let r = f(&space);
                        entry.insert(space);
                        Ok(r)
                    }
                }),
        }
    }

    /// Get the cache database (this will create the space if it doesn't already exist).
    pub fn cache(&self, dna_hash: &DnaHash) -> ConductorResult<DbWrite<DbKindCache>> {
        self.get_or_create_space_ref(dna_hash, |space| space.cache_db.clone())
    }

    /// Get the authored database (this will create the space if it doesn't already exist).
    pub fn authored_db(&self, dna_hash: &DnaHash) -> ConductorResult<DbWrite<DbKindAuthored>> {
        self.get_or_create_space_ref(dna_hash, |space| space.authored_db.clone())
    }

    /// Get the dht database (this will create the space if it doesn't already exist).
    pub fn dht_db(&self, dna_hash: &DnaHash) -> ConductorResult<DbWrite<DbKindDht>> {
        self.get_or_create_space_ref(dna_hash, |space| space.dht_db.clone())
    }

    /// Get the peer database (this will create the space if it doesn't already exist).
    pub fn p2p_agents_db(&self, dna_hash: &DnaHash) -> ConductorResult<DbWrite<DbKindP2pAgents>> {
        self.get_or_create_space_ref(dna_hash, |space| space.p2p_agents_db.clone())
    }

    /// Get the peer database (this will create the space if it doesn't already exist).
    pub fn p2p_metrics_db(&self, dna_hash: &DnaHash) -> ConductorResult<DbWrite<DbKindP2pMetrics>> {
        self.get_or_create_space_ref(dna_hash, |space| space.p2p_metrics_db.clone())
    }

    /// Get the batch sender (this will create the space if it doesn't already exist).
    pub fn p2p_batch_sender(
        &self,
        dna_hash: &DnaHash,
    ) -> ConductorResult<tokio::sync::mpsc::Sender<P2pBatch>> {
        self.get_or_create_space_ref(dna_hash, |space| space.p2p_batch_sender.clone())
    }

    #[instrument(skip(self))]
    /// the network module is requesting a list of dht op hashes
    /// Get the [`DhtOpHash`]es and authored timestamps for a given time window.
    pub async fn handle_query_op_hashes(
        &self,
        dna_hash: &DnaHash,
        dht_arc_set: DhtArcSet,
        window: TimeWindow,
        max_ops: usize,
        include_limbo: bool,
    ) -> ConductorResult<Option<(Vec<DhtOpHash>, TimeWindowInclusive)>> {
        // The exclusive window bounds.
        let start = window.start;
        let end = window.end;
        let max_ops: u32 = max_ops.try_into().unwrap_or(u32::MAX);

        let db = self.dht_db(dna_hash)?;
        let include_limbo = include_limbo
            .then(|| "\n")
            .unwrap_or("AND DhtOp.when_integrated IS NOT NULL\n");

        let intervals = dht_arc_set.intervals();
        let sql = if let Some(DhtArcRange::Full) = intervals.first() {
            format!(
                "{} {} {}",
                holochain_sqlite::sql::sql_cell::FETCH_OP_HASHES_P1,
                include_limbo,
                holochain_sqlite::sql::sql_cell::FETCH_OP_HASHES_P2,
            )
        } else {
            let sql_ranges = intervals
                .into_iter()
                .filter(|i| matches!(i, &DhtArcRange::Bounded(_, _)))
                .map(|interval| match interval {
                    DhtArcRange::Bounded(start_loc, end_loc) => {
                        if start_loc <= end_loc {
                            format!(
                                "AND storage_center_loc >= {} AND storage_center_loc <= {} \n ",
                                start_loc, end_loc
                            )
                        } else {
                            format!(
                                "AND (storage_center_loc < {} OR storage_center_loc > {}) \n ",
                                end_loc, start_loc
                            )
                        }
                    }
                    _ => unreachable!(),
                })
                .collect::<String>();
            format!(
                "{} {} {} {}",
                holochain_sqlite::sql::sql_cell::FETCH_OP_HASHES_P1,
                include_limbo,
                sql_ranges,
                holochain_sqlite::sql::sql_cell::FETCH_OP_HASHES_P2,
            )
        };
        let results = db
            .async_reader(move |txn| {
                let hashes = txn
                    .prepare_cached(&sql)?
                    .query_map(
                        named_params! {
                            ":from": start,
                            ":to": end,
                            ":limit": max_ops,
                        },
                        |row| row.get("hash"),
                    )?
                    .collect::<rusqlite::Result<Vec<DhtOpHash>>>()?;
                let range = hashes.first().and_then(|s| hashes.last().map(|e| (s, e)));
                match range {
                    Some((start, end)) => {
                        let start: Timestamp = txn.query_row(
                            "SELECT authored_timestamp FROM DhtOp WHERE hash = ?",
                            [start],
                            |row| row.get(0),
                        )?;
                        let end: Timestamp = txn.query_row(
                            "SELECT authored_timestamp FROM DhtOp WHERE hash = ?",
                            [end],
                            |row| row.get(0),
                        )?;
                        DatabaseResult::Ok(Some((hashes, start..=end)))
                    }
                    None => Ok(None),
                }
            })
            .await?;

        Ok(results)
    }

    /// The network module needs info about various groupings ("regions") of ops
    ///
    /// Note that this always includes all ops regardless of integration status.
    /// This is to avoid the degenerate case of freshly joining a network, and
    /// having several new peers gossiping with you at once about the same regions.
    /// If we calculate our region hash only by integrated ops, we will experience
    /// mismatches for a large number of ops repeatedly until we have integrated
    /// those ops. Note that when *sending* ops we filter out ops in limbo.
    pub async fn handle_fetch_op_regions(
        &self,
        dna_def: &DnaDefHashed,
        dht_arc_set: DhtArcSet,
    ) -> ConductorResult<RegionSetLtcs> {
        use holo_hash::HasHash;
        let dna_hash = dna_def.as_hash();
        let sql = holochain_sqlite::sql::sql_cell::FETCH_OP_REGION;
        let topology = dna_def.topology();
        let max_chunks = ArqStrat::default().max_chunks();
        let arq_set = ArqBoundsSet::new(
            dht_arc_set
                .intervals()
                .into_iter()
                .map(|i| {
                    let len = i.length();
                    let (pow, _) = power_and_count_from_length(&topology.space, len, max_chunks);
                    ArqBounds::from_interval_rounded(&topology, pow, i)
                })
                .collect(),
        );
        let times = TelescopingTimes::historical(&topology, self.recent_threshold());
        let coords = RegionCoordSetLtcs::new(times, arq_set);
        let coords_clone = coords.clone();
        let db = self.dht_db(dna_hash)?;
        db.async_reader(move |txn| {
            let mut stmt = txn.prepare_cached(sql).map_err(DatabaseError::from)?;
            Ok(coords_clone.into_region_set(|(_, coords)| {
                let bounds = coords.to_bounds(&topology);
                let (x0, x1) = bounds.x;
                let (t0, t1) = bounds.t;
                stmt.query_row(
                    named_params! {
                        ":storage_start_loc": x0,
                        ":storage_end_loc": x1,
                        ":timestamp_min": t0,
                        ":timestamp_max": t1,
                    },
                    |row| {
                        let size: f64 = row.get("total_size")?;
                        Ok(RegionData {
                            hash: RegionHash::from_vec(row.get("xor_hash")?)
                                .expect("region hash must be 32 bytes"),
                            size: size.min(u32::MAX as f64) as u32,
                            count: row.get("count")?,
                        })
                    },
                )
            })?)
        })
        .await
    }

    #[instrument(skip(self, query))]
    /// The network module is requesting the content for dht ops
    pub async fn handle_fetch_op_data(
        &self,
        dna_hash: &DnaHash,
        query: FetchOpDataQuery,
    ) -> ConductorResult<Vec<(holo_hash::DhtOpHash, holochain_types::dht_op::DhtOp)>> {
        match query {
            FetchOpDataQuery::Hashes(op_hashes) => {
                self.handle_fetch_op_data_by_hashes(dna_hash, op_hashes)
                    .await
            }
            FetchOpDataQuery::Regions(regions) => {
                self.handle_fetch_op_data_by_regions(dna_hash, regions)
                    .await
            }
        }
    }

    #[instrument(skip(self, regions))]
    /// The network module is requesting the content for dht ops
    pub async fn handle_fetch_op_data_by_regions(
        &self,
        dna_hash: &DnaHash,
        regions: Vec<RegionBounds>,
    ) -> ConductorResult<Vec<(holo_hash::DhtOpHash, holochain_types::dht_op::DhtOp)>> {
        let sql = holochain_sqlite::sql::sql_cell::FETCH_OPS_BY_REGION;
        Ok(self
            .dht_db(dna_hash)?
            .async_reader(move |txn| {
                let mut stmt = txn.prepare_cached(sql).map_err(StateQueryError::from)?;
                StateQueryResult::Ok(
                    regions
                        .into_iter()
                        .map(|bounds| {
                            let (x0, x1) = bounds.x;
                            let (t0, t1) = bounds.t;
                            stmt.query_and_then(
                                named_params! {
                                    ":storage_start_loc": x0,
                                    ":storage_end_loc": x1,
                                    ":timestamp_min": t0,
                                    ":timestamp_max": t1,
                                },
                                |row| {
                                    let hash: DhtOpHash =
                                        row.get("hash").map_err(StateQueryError::from)?;
                                    Ok(map_sql_dht_op_common(row)?.map(|op| (hash, op)))
                                },
                            )
                            .map_err(StateQueryError::from)?
                            .collect::<Result<Vec<Option<_>>, StateQueryError>>()
                        })
                        .collect::<Result<Vec<Vec<Option<_>>>, _>>()?
                        .into_iter()
                        .flatten()
                        .filter_map(|x| x)
                        .collect(),
                )
            })
            .await?)
    }

    #[instrument(skip(self, op_hashes))]
    /// The network module is requesting the content for dht ops
    pub async fn handle_fetch_op_data_by_hashes(
        &self,
        dna_hash: &DnaHash,
        op_hashes: Vec<holo_hash::DhtOpHash>,
    ) -> ConductorResult<Vec<(holo_hash::DhtOpHash, holochain_types::dht_op::DhtOp)>> {
        const OPS_IN_MEMORY_BOUND_BYTES: usize = 3_000_000; // 3MB
                                                            // FIXME: Test this query.
        let db = self.dht_db(dna_hash)?;
        let results = db
            .async_reader(move |txn| {
                let mut out = Vec::with_capacity(op_hashes.len());
                let mut total_bytes = 0;
                for hash in op_hashes {
                    // FIXME: cache this query (make prepared statement)
                    let r = txn.query_row_and_then(
                        "
                            SELECT DhtOp.hash, DhtOp.type AS dht_type,
                            Header.blob AS header_blob, Entry.blob AS entry_blob,
                            LENGTH(Header.blob) as header_size, LENGTH(Entry.blob) as entry_size
                            FROM DHtOp
                            JOIN Header ON DhtOp.header_hash = Header.hash
                            LEFT JOIN Entry ON Header.entry_hash = Entry.hash
                            WHERE
                            DhtOp.hash = ?
                            AND
                            DhtOp.when_integrated IS NOT NULL
                        ",
                        [hash],
                        |row| {
                            let header_bytes: Option<usize> = row.get("header_size")?;
                            let entry_bytes: Option<usize> = row.get("entry_size")?;
                            let bytes = header_bytes.unwrap_or(0) + entry_bytes.unwrap_or(0);
                            let header = from_blob::<SignedHeader>(row.get("header_blob")?)?;
                            let op_type: DhtOpType = row.get("dht_type")?;
                            let hash: DhtOpHash = row.get("hash")?;
                            // Check the entry isn't private before gossiping it.
                            let mut entry: Option<Entry> = None;
                            if header
                                .0
                                .entry_type()
                                .filter(|et| *et.visibility() == EntryVisibility::Public)
                                .is_some()
                            {
                                let e: Option<Vec<u8>> = row.get("entry_blob")?;
                                entry = match e {
                                    Some(entry) => Some(from_blob::<Entry>(entry)?),
                                    None => None,
                                };
                            }
                            let op = DhtOp::from_type(op_type, header, entry)?;
                            StateQueryResult::Ok(((hash, op), bytes))
                        },
                    );
                    match r {
                        Ok((r, bytes)) => {
                            out.push(r);
                            total_bytes += bytes;
                            // pair(maackle, freesig): be sure to add this limit in the region fetch case too
                            if total_bytes > OPS_IN_MEMORY_BOUND_BYTES {
                                break;
                            }
                        }
                        Err(holochain_state::query::StateQueryError::Sql(
                            rusqlite::Error::QueryReturnedNoRows,
                        )) => (),
                        Err(e) => return Err(e),
                    }
                }
                StateQueryResult::Ok(out)
            })
            .await?;
        Ok(results)
    }

    #[instrument(skip(self, request_validation_receipt, ops))]
    /// we are receiving a "publish" event from the network
    pub async fn handle_publish(
        &self,
        dna_hash: &DnaHash,
        request_validation_receipt: bool,
        countersigning_session: bool,
        ops: Vec<holochain_types::dht_op::DhtOp>,
    ) -> ConductorResult<()> {
        use futures::StreamExt;
        let ops = futures::stream::iter(ops.into_iter().map(|op| {
            let hash = DhtOpHash::with_data_sync(&op);
            (hash, op)
        }))
        .collect()
        .await;

        // If this is a countersigning session then
        // send it to the countersigning workflow otherwise
        // send it to the incoming ops workflow.
        if countersigning_session {
            let (workspace, trigger) = self.get_or_create_space_ref(dna_hash, |space| {
                (
                    space.countersigning_workspace.clone(),
                    self.queue_consumer_map
                        .countersigning_trigger(space.dna_hash.clone()),
                )
            })?;
            let trigger = match trigger {
                Some(t) => t,
                // If the workflow has not been spawned yet we can't handle incoming messages.
                None => return Ok(()),
            };
            incoming_countersigning(ops, &workspace, trigger)?;
        } else {
            let space = self.get_or_create_space(dna_hash)?;
            let trigger = match self
                .queue_consumer_map
                .sys_validation_trigger(space.dna_hash.clone())
            {
                Some(t) => t,
                // If the workflow has not been spawned yet we can't handle incoming messages.
                // Note this is not an error because only a validation receipt is proof of a publish.
                None => return Ok(()),
            };
            incoming_dht_ops_workflow(&space, trigger, ops, request_validation_receipt).await?;
        }
        Ok(())
    }

    /// Get the recent_threshold based on the kitsune network config
    pub fn recent_threshold(&self) -> Duration {
        Duration::from_secs(
            self.network_config
                .tuning_params
                .danger_gossip_recent_threshold_secs,
        )
    }
}

impl Space {
    fn new(
        dna_hash: Arc<DnaHash>,
        root_db_dir: &DatabaseRootPath,
        db_sync_strategy: DbSyncStrategy,
    ) -> ConductorResult<Self> {
        use holochain_p2p::DnaHashExt;
        let space = dna_hash.to_kitsune();
        let db_sync_level = match db_sync_strategy {
            DbSyncStrategy::Fast => DbSyncLevel::Off,
            DbSyncStrategy::Resilient => DbSyncLevel::Normal,
        };
        let cache = DbWrite::open_with_sync_level(
            root_db_dir.as_ref(),
            DbKindCache(dna_hash.clone()),
            db_sync_level,
        )?;
        let authored_db = DbWrite::open_with_sync_level(
            root_db_dir.as_ref(),
            DbKindAuthored(dna_hash.clone()),
            DbSyncLevel::Normal,
        )?;
        let dht_db = DbWrite::open_with_sync_level(
            root_db_dir.as_ref(),
            DbKindDht(dna_hash.clone()),
            db_sync_level,
        )?;
        let p2p_agents_db = DbWrite::open_with_sync_level(
            root_db_dir.as_ref(),
            DbKindP2pAgents(space.clone()),
            db_sync_level,
        )?;
        let p2p_metrics_db = DbWrite::open_with_sync_level(
            root_db_dir.as_ref(),
            DbKindP2pMetrics(space),
            db_sync_level,
        )?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        tokio::spawn(p2p_agent_store::p2p_put_all_batch(
            p2p_agents_db.clone(),
            rx,
        ));
        let p2p_batch_sender = tx;

        let countersigning_workspace = CountersigningWorkspace::new();
        let incoming_op_hashes = IncomingOpHashes::default();
        let incoming_ops_batch = IncomingOpsBatch::default();
        let dht_query_cache = DhtDbQueryCache::new(dht_db.clone().into());
        let r = Self {
            dna_hash,
            cache_db: cache,
            authored_db,
            dht_db,
            p2p_agents_db,
            p2p_metrics_db,
            p2p_batch_sender,
            countersigning_workspace,
            incoming_op_hashes,
            incoming_ops_batch,
            dht_query_cache,
        };
        Ok(r)
    }
}

#[cfg(test)]
impl TestSpaces {
    pub fn new(dna_hashes: impl IntoIterator<Item = DnaHash>) -> Self {
        let queue_consumer_map = QueueConsumerMap::new();
        Self::with_queue_consumer(dna_hashes, queue_consumer_map)
    }

    pub fn with_queue_consumer(
        dna_hashes: impl IntoIterator<Item = DnaHash>,
        queue_consumer_map: QueueConsumerMap,
    ) -> Self {
        let mut test_spaces: HashMap<DnaHash, _> = HashMap::new();
        for hash in dna_hashes.into_iter() {
            test_spaces.insert(hash.clone(), TestSpace::new(hash));
        }
        let temp_dir = tempfile::Builder::new()
            .prefix("holochain-test-environments")
            .tempdir()
            .unwrap();
        let spaces = Spaces::new(&ConductorConfig {
            environment_path: temp_dir.path().to_path_buf().into(),
            ..Default::default()
        })
        .unwrap();
        spaces.map.share_mut(|map| {
            map.extend(
                test_spaces
                    .iter()
                    .map(|(k, v)| (k.clone(), v.space.clone())),
            );
        });
        Self {
            queue_consumer_map,
            spaces,
            test_spaces,
        }
    }
}

#[cfg(test)]
impl TestSpace {
    pub fn new(dna_hash: DnaHash) -> Self {
        let temp_dir = tempfile::Builder::new()
            .prefix("holochain-test-environments")
            .tempdir()
            .unwrap();

        Self {
            space: Space::new(
                Arc::new(dna_hash),
                &temp_dir.path().to_path_buf().into(),
                Default::default(),
            )
            .unwrap(),
            _temp_dir: temp_dir,
        }
    }
}

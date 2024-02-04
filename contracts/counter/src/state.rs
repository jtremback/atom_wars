use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CONSTANTS: Item<Constants> = Item::new("constants");

#[cw_serde]
pub struct Constants {
    pub denom: String,
    pub round_length: u64,
}

pub const LOCK_ID: Item<u64> = Item::new("lock_id");

pub const PROP_ID: Item<u64> = Item::new("prop_id");

pub const ROUND_ID: Item<u64> = Item::new("round_id");

pub const TRIBUTE_ID: Item<u64> = Item::new("tribute_id");

// LocksMap: key(sender_address, lock_id) -> {
//     amount: Coin,
//     lock_start: Timestamp,
//     lock_end: Timestamp
// }
pub const LOCKS_MAP: Map<(Addr, u64), LockEntry> = Map::new("locks_map");

#[cw_serde]
pub struct LockEntry {
    pub amount: Coin,
    pub lock_start: Timestamp,
    pub lock_end: Timestamp,
}

// PropMap: key((round_id, prop_id)) -> {
//     round_id: u64,
//     covenant_params,
//     executed: bool
// }
pub const PROP_MAP: Map<(u64, u64), Proposal> = Map::new("prop_map");

#[cw_serde]
pub struct Proposal {
    pub round_id: u64,
    pub covenant_params: String,
    pub executed: bool,
    pub power: Uint128,
}

// VoteMap: key() -> {
//     prop_id: u64,
//     sender_address: Address
//     power: Uint128
// }
pub const VOTE_MAP: Map<Addr, Vote> = Map::new("vote_map");

#[cw_serde]
pub struct Vote {
    pub prop_id: u64,
    pub power: Uint128,
}

// RoundMap: key(round_id) -> {
//     round_id: u64,
//     round_end: Timestamp
//     winners
// }
pub const ROUND_MAP: Map<u64, Round> = Map::new("round_map");

#[cw_serde]
pub struct Round {
    pub round_id: u64,
    pub round_end: Timestamp,
}

// TributeMap: key(prop_id, tribute_id) -> {
//     sender: Address,
//     amount: Coin
// }
pub const TRIBUTE_MAP: Map<(u64, u64), Tribute> = Map::new("tribute_map");

#[cw_serde]
pub struct Tribute {
    pub sender: Addr,
    pub amount: Coin,
}

pub const TALLY_MAP: Map<(u64, u64), Uint128> = Map::new("tally_map");

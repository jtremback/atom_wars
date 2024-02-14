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

// LOCKS_MAP: key(sender_address, lock_id) -> LockEntry {
//     funds: Coin,
//     lock_start: Timestamp,
//     lock_end: Timestamp
// }
pub const LOCKS_MAP: Map<(Addr, u64), LockEntry> = Map::new("locks_map");

#[cw_serde]
pub struct LockEntry {
    pub funds: Coin,
    pub lock_start: Timestamp,
    pub lock_end: Timestamp,
}

// PropMap: key(round_id, prop_id) -> Proposal {
//     round_id: u64,
//     covenant_params: String,
//     executed: bool,
//     power: Uint128
// }
pub const PROP_MAP: Map<(u64, u64), Proposal> = Map::new("prop_map");

#[cw_serde]
pub struct Proposal {
    pub round_id: u64,
    pub covenant_params: String,
    pub executed: bool,
    pub power: Uint128,
}

// VoteMap: key(round_id, sender_addr) -> Vote {
//     prop_id: u64,
//     power: Uint128,
//     tribute_claimed: bool
// }
pub const VOTE_MAP: Map<(u64, Addr), Vote> = Map::new("vote_map");

#[cw_serde]
pub struct Vote {
    pub prop_id: u64,
    pub power: Uint128,
    pub tribute_claimed: bool,
}

// RoundMap: key(round_id) -> Round {
//     round_id: u64,
//     round_end: Timestamp
// }
pub const ROUND_MAP: Map<u64, Round> = Map::new("round_map");

#[cw_serde]
pub struct Round {
    pub round_id: u64,
    pub round_end: Timestamp,
}

// TributeMap: key(round_id, prop_id, tribute_id) -> Tribute {
//     depositor: Address,
//     funds: Coin,
//     refunded: bool
// }
pub const TRIBUTE_MAP: Map<(u64, u64, u64), Tribute> = Map::new("tribute_map");
#[cw_serde]
pub struct Tribute {
    pub depositor: Addr,
    pub funds: Coin,
    pub refunded: bool,
}

// WinningProp: key(round_id) -> prop_id
pub const WINNING_PROP: Map<u64, u64> = Map::new("score_map");

// TotalPowerVoting: key(round_id) -> Uint128
pub const TOTAL_POWER_VOTING: Map<u64, Uint128> = Map::new("total_power_voting");

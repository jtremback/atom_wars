pub const TRIBUTE_ID: Item<u64> = Item::new("tribute_id");

// TRIBUTE_MAP: key(round_id, prop_id, tribute_id) -> Tribute {
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

// TributeClaims: key(sender_addr, tribute_id) -> bool
pub const TRIBUTE_CLAIMS: Map<(Addr, u64), bool> = Map::new("tribute_claims");

// MAIN TODOS:
// - Handle power scaling and attenuation <- Done
// - Add real covenant logic
// - Question: How to design voting so that people don't just wait until the end to vote?
// - Question: How to handle the case where a proposal is executed but the covenant fails?
// - Covenant Question: How to deal with someone using MEV to skew the pool ratio right before the liquidity is pulled? Streaming the liquidity pull? You'd have to set up a cron job for that.
// - Covenant Question: Can people sandwich this whole thing - covenant system has price limits - but we should allow people to retry executing the prop during the round
// - Question: How to punish people who vote for props that lose money due to IL? Adding their liquid staked positions to the prop position is a non starter mechanically. Instead we should hit their staked positions at the end if there is IL.
// - - Question: Should they also be exposed to upside?
// - - Question: At what point do you punish them for the IL? At the end of the round? Should there be failsafes to pull a position once IL breaches a threshold?

// Power scaling function: \left\{x<1:\ 2,\ 1<x<4:\ -5^{\left(x-4.5\right)}+1,\ x>4:\ 0\right\}
// fn piecewise_function(x: f64) -> f64 {
//     if x < 1.0 {
//         2.0 // For x < 1, return 2
//     } else if x > 4.0 {
//         0.0 // For x > 4, return 0
//     } else {
//         -5.0f64.powf(x - 4.5) + 1.0 // For 1 < x < 4, calculate the expression
//     }
// }

use cosmwasm_std::{
    entry_point, to_json_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdError, StdResult, Timestamp, Uint128,
};

use crate::error::ContractError;
use crate::msg::{CountResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{
    Constants, LockEntry, Proposal, Round, Tribute, Vote, CONSTANTS, LOCKS_MAP, LOCK_ID, PROP_ID,
    PROP_MAP, ROUND_ID, ROUND_MAP, TRIBUTE_CLAIMS, TRIBUTE_ID, TRIBUTE_MAP, VOTE_MAP, WINNING_PROP,
};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{MsgBurn, MsgMint};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = Constants {
        denom: msg.denom.clone(),
        round_length: msg.round_length,
    };
    CONSTANTS.save(deps.storage, &state)?;
    Ok(Response::new()
        .add_attribute("action", "initialisation")
        .add_attribute("sender", _info.sender.clone())
        .add_attribute("denom", msg.denom))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::LockTokens { lock_duration } => lock_tokens(deps, env, info, lock_duration),
        ExecuteMsg::UnlockTokens {} => unlock_tokens(deps, env, info),
        ExecuteMsg::CreateProposal { covenant_params } => {
            create_proposal(deps, env, info, covenant_params)
        }
        ExecuteMsg::Vote { proposal_id } => vote(deps, env, info, proposal_id),
        ExecuteMsg::Tally {} => end_round(deps, env, info),
        ExecuteMsg::ExecuteProposal { proposal_id } => {
            execute_proposal(deps, env, info, proposal_id)
        }
        ExecuteMsg::AddTribute {
            proposal_id,
            round_id,
        } => add_tribute(deps, env, info, round_id, proposal_id),
        ExecuteMsg::RefundTribute {
            proposal_id,
            round_id,
        } => refund_tribute(deps, env, info, round_id, round_id, proposal_id),
    }
}

// LockTokens(lock_duration):
//     Receive tokens
//     Validate against denom whitelist
//     Create entry in LocksMap
fn lock_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lock_duration: u64,
) -> Result<Response, ContractError> {
    // Validate that their lock duration (given in nanos) is either 1 month, 3 months, 6 months, or 12 months
    let one_month_in_nanos: u64 = 2629746000000000;

    if lock_duration != one_month_in_nanos
        && lock_duration != one_month_in_nanos * 3
        && lock_duration != one_month_in_nanos * 6
        && lock_duration != one_month_in_nanos * 12
    {
        return Err(ContractError::Std(StdError::generic_err(
            "Lock duration must be 1, 3, 6, or 12 months",
        )));
    }

    // Validate that sent funds are the required denom
    if info.funds.len() != 1 {
        return Err(ContractError::Std(StdError::generic_err(
            "Must send exactly one coin",
        )));
    }

    let sent_funds = info
        .funds
        .get(0)
        .ok_or_else(|| ContractError::Std(StdError::generic_err("Must send exactly one coin")))?;

    if sent_funds.denom != CONSTANTS.load(deps.storage)?.denom {
        return Err(ContractError::Std(StdError::generic_err(
            "Must send the correct denom",
        )));
    }

    // Create entry in LocksMap
    let lock_entry = LockEntry {
        funds: sent_funds.clone(),
        lock_start: env.block.time,
        lock_end: env.block.time.plus_nanos(lock_duration),
    };
    // increment lock_id
    let lock_id = LOCK_ID.load(deps.storage)?;
    LOCK_ID.save(deps.storage, &(lock_id + 1))?;

    LOCKS_MAP.save(deps.storage, (info.sender, lock_id), &lock_entry)?;

    Ok(Response::new().add_attribute("action", "lock_tokens"))
}

// UnlockTokens():
//     Validate caller
//     Validate `lock_end` < now
//     Send `amount` tokens back to caller
//     Delete entry from LocksMap
fn unlock_tokens(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // Iterate all locks for the caller and unlock them if lock_end < now
    let locks =
        LOCKS_MAP
            .prefix(info.sender.clone())
            .range(deps.storage, None, None, Order::Ascending);

    let mut sends = vec![];
    let mut to_delete = vec![];

    for lock in locks {
        let (lock_id, lock_entry) = lock?;
        if lock_entry.lock_end < env.block.time {
            // Send tokens back to caller
            sends.push(lock_entry.funds.clone());
            // Delete entry from LocksMap

            to_delete.push((info.sender.clone(), lock_id));
        }
    }

    // Delete unlocked locks
    for (addr, lock_id) in to_delete {
        LOCKS_MAP.remove(deps.storage, (addr, lock_id));
    }

    Ok(Response::new()
        .add_attribute("action", "unlock_tokens")
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: sends,
        }))
}

fn validate_covenant_params(covenant_params: String) -> Result<(), ContractError> {
    // Validate covenant_params
    Ok(())
}

// CreateProposal(covenant_params, tribute):
//     Validate covenant_params
//     Hold tribute in contract's account
//     Create in PropMap
fn create_proposal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    covenant_params: String,
) -> Result<Response, ContractError> {
    validate_covenant_params(covenant_params.clone())?;

    let round_id = ROUND_ID.load(deps.storage)?;

    // Create proposal in PropMap
    let proposal = Proposal {
        covenant_params,
        round_id: round_id,
        executed: false,
        power: Uint128::zero(),
    };

    let prop_id = PROP_ID.load(deps.storage)?;
    PROP_ID.save(deps.storage, &(prop_id + 1))?;
    PROP_MAP.save(deps.storage, (round_id, prop_id), &proposal)?;

    Ok(Response::new().add_attribute("action", "create_proposal"))
}

fn scale_lockup_power(lockup_time: u64, raw_power: Uint128) -> Uint128 {
    let one_month_in_nanos: u64 = 2629746000000000;
    let three_months_in_nanos: u64 = one_month_in_nanos * 3;
    let six_months_in_nanos: u64 = one_month_in_nanos * 6;

    let two: Uint128 = 2u16.into();

    // Scale lockup power
    // 1x if lockup is between 0 and 1 months
    // 1.5x if lockup is between 1 and 3 months
    // 2x if lockup is between 3 and 6 months
    // 4x if lockup is between 6 and 12 months
    // TODO: is there a less funky way to do Uint128 math???
    let scaled_power = match lockup_time {
        // 4x if lockup is over 6 months
        lockup_time if lockup_time > one_month_in_nanos * 6 => raw_power * two * two,
        // 2x if lockup is between 3 and 6 months
        lockup_time if lockup_time > one_month_in_nanos * 3 => raw_power * two,
        // 1.5x if lockup is between 1 and 3 months
        lockup_time if lockup_time > one_month_in_nanos => raw_power + (raw_power / two),
        // Covers 0 and 1 month which have no scaling
        _ => raw_power,
    };

    scaled_power
}

fn vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // Load the round_id
    let round_id = ROUND_ID.load(deps.storage)?;

    // Load the round
    let round = ROUND_MAP.load(deps.storage, round_id)?;

    // Get any existing vote and reverse it
    let vote = VOTE_MAP.load(deps.storage, (round_id, info.sender.clone()));
    if let Ok(vote) = vote {
        // Reverse vote
        let mut proposal = PROP_MAP.load(deps.storage, (round_id, vote.prop_id))?;
        proposal.power -= vote.power;
        PROP_MAP.save(deps.storage, (round_id, vote.prop_id), &proposal)?;
    }
    // Delete vote
    VOTE_MAP.remove(deps.storage, (round_id, info.sender.clone()));

    // Get sender's total locked power
    let mut power: Uint128 = Uint128::zero();
    let locks =
        LOCKS_MAP
            .prefix(info.sender.clone())
            .range(deps.storage, None, None, Order::Ascending);

    for lock in locks {
        let (_, lock_entry) = lock?;

        // Get the remaining lockup time at the end of this round.
        // This means that their power will be scaled the same by this function no matter when they vote in the round
        let lockup_time = lock_entry.lock_end.nanos() - round.round_end.nanos();

        // Scale power. This is what implements the different powers for different lockup times.
        let scaled_power = scale_lockup_power(lockup_time, lock_entry.funds.amount);

        power += scaled_power;
    }

    // Update proposal's power in propmap
    let mut proposal = PROP_MAP.load(deps.storage, (round_id, proposal_id))?;
    proposal.power += power;
    PROP_MAP.save(deps.storage, (round_id, proposal_id), &proposal)?;

    // Check if winning proposal has a lower score, if so, update round's winning proposal
    match WINNING_PROP.may_load(deps.storage, round_id)? {
        Some(winning_prop_id) => {
            let winning_prop = PROP_MAP.load(deps.storage, (round_id, winning_prop_id))?;
            if proposal.power > winning_prop.power {
                WINNING_PROP.save(deps.storage, round_id, &proposal_id)?;
            }
        }
        None => {
            WINNING_PROP.save(deps.storage, round_id, &proposal_id)?;
        }
    }

    // Create vote in Votemap
    let vote = Vote {
        prop_id: proposal_id,
        power,
        tribute_claimed: false,
    };
    VOTE_MAP.save(deps.storage, (round_id, info.sender), &vote)?;

    Ok(Response::new().add_attribute("action", "vote"))
}

fn end_round(_deps: DepsMut, _env: Env, _info: MessageInfo) -> Result<Response, ContractError> {
    // Check that round has ended by getting latest round and checking if round_end < now
    let round = ROUND_ID.load(_deps.storage)?;
    let round = ROUND_MAP.load(_deps.storage, round)?;

    if round.round_end > _env.block.time {
        return Err(ContractError::Std(StdError::generic_err(
            "Round has not ended yet",
        )));
    }

    // Start a new round
    let round_id = round.round_id + 1;
    let round_end = _env
        .block
        .time
        .plus_nanos(CONSTANTS.load(_deps.storage)?.round_length);
    ROUND_MAP.save(
        _deps.storage,
        round_id,
        &Round {
            round_end,
            round_id,
        },
    )?;
    ROUND_ID.save(_deps.storage, &(round_id))?;

    Ok(Response::new().add_attribute("action", "tally"))
}

fn do_covenant_stuff(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _covenant_params: String,
) -> Result<Response, ContractError> {
    // Do covenant stuff
    Ok(Response::new().add_attribute("action", "do_covenant_stuff"))
}

fn execute_proposal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // Check that the propsal won the last round
    let last_round_id = ROUND_ID.load(deps.storage)? - 1;
    let winning_prop_id = WINNING_PROP.load(deps.storage, last_round_id)?;
    if winning_prop_id != proposal_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Proposal did not win the last round",
        )));
    }

    // Check that the proposal has not already been executed
    let mut proposal = PROP_MAP.load(deps.storage, (last_round_id, proposal_id))?;
    if proposal.executed {
        return Err(ContractError::Std(StdError::generic_err(
            "Proposal already executed",
        )));
    }

    // Execute proposal
    // do_covenant_stuff(deps, env, info, proposal.clone().covenant_params);

    // Mark proposal as executed
    proposal.executed = true;
    PROP_MAP.save(deps.storage, (last_round_id, proposal_id), &proposal)?;

    Ok(Response::new().add_attribute("action", "execute_proposal"))
}

fn add_tribute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    round_id: u64,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // Check that the round is currently ongoing
    let current_round_id = ROUND_ID.load(deps.storage)?;
    if round_id != current_round_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Round is not currently ongoing",
        )));
    }

    // Check that the sender has sent funds
    if info.funds.is_empty() {
        return Err(ContractError::Std(StdError::generic_err(
            "Must send funds to add tribute",
        )));
    }

    // Check that the sender has only sent one type of coin for the tribute
    if info.funds.len() != 1 {
        return Err(ContractError::Std(StdError::generic_err(
            "Must send exactly one coin",
        )));
    }

    // Create tribute in TributeMap
    let tribute_id = TRIBUTE_ID.load(deps.storage)?;
    TRIBUTE_ID.save(deps.storage, &(tribute_id + 1))?;
    let tribute = Tribute {
        funds: info.funds[0].clone(),
        depositor: info.sender.clone(),
        refunded: false,
    };
    TRIBUTE_MAP.save(deps.storage, (round_id, proposal_id, tribute_id), &tribute)?;

    Ok(Response::new().add_attribute("action", "add_tribute"))
}

// ClaimTribute(round_id, prop_id):
//     Check that the round is ended
//     Check that the prop won
//     Look up sender's vote for the round
//     Check that the sender voted for the prop
//     Check that the sender has not already claimed the tribute
//     Divide sender's vote power by total power voting for the prop to figure out their percentage
//     Use the sender's percentage to send them the right portion of the tribute
//     Mark on the sender's vote that they claimed the tribute
fn claim_tribute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    round_id: u64,
    proposal_id: u64,
    tribute_id: u64,
) -> Result<Response, ContractError> {
    // Check that the round is ended by checking that the round_id is not the current round
    let current_round_id = ROUND_ID.load(deps.storage)?;
    if round_id == current_round_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Round has not ended yet",
        )));
    }

    // Check that the prop won
    let winning_prop_id = WINNING_PROP.load(deps.storage, round_id)?;
    if winning_prop_id != proposal_id {
        return Err(ContractError::Std(StdError::generic_err("Proposal lost")));
    }

    // Look up sender's vote for the round, error if it cannot be found
    let vote = VOTE_MAP.load(deps.storage, (round_id, info.sender.clone()))?;

    // Check that the sender voted for the prop
    if vote.prop_id != proposal_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Sender did not vote for the proposal",
        )));
    }

    // Check that the sender has not already claimed the tribute using the TRIBUTE_CLAIMS map
    if TRIBUTE_CLAIMS.may_load(deps.storage, (info.sender.clone(), tribute_id))? == Some(true) {
        return Err(ContractError::Std(StdError::generic_err(
            "Sender has already claimed the tribute",
        )));
    }

    // Divide sender's vote power by the prop's power to figure out their percentage
    let proposal = PROP_MAP.load(deps.storage, (round_id, proposal_id))?;
    // TODO: percentage needs to be a decimal type
    let percentage = vote.power / proposal.power;

    // Load the tribute and use the percentage to figure out how much of the tribute to send them
    let tribute = TRIBUTE_MAP.load(deps.storage, (round_id, proposal_id, tribute_id))?;
    let amount = Uint128::from(tribute.funds.amount * percentage);

    // Mark in the TRIBUTE_CLAIMS that the sender has claimed this tribute
    TRIBUTE_CLAIMS.save(deps.storage, (info.sender.clone(), tribute_id), &true)?;

    // Send the tribute to the sender
    Ok(Response::new()
        .add_attribute("action", "claim_tribute")
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            // TODO: amount needs to be a Coin type- take calculated amount instead of entire tribute amount
            amount: vec![Coin {
                denom: tribute.funds.denom,
                amount,
            }],
        }))
}

// RefundTribute(round_id, prop_id, tribute_id):
//     Check that the round is ended
//     Check that the prop lost
//     Check that the sender is the depositor of the tribute
//     Check that the sender has not already refunded the tribute
//     Send the tribute back to the sender
fn refund_tribute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    round_id: u64,
    proposal_id: u64,
    tribute_id: u64,
) -> Result<Response, ContractError> {
    // Check that the round is ended by checking that the round_id is not the current round
    let current_round_id = ROUND_ID.load(deps.storage)?;
    if round_id == current_round_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Round has not ended yet",
        )));
    }

    // Check that the prop lost
    let winning_prop_id = WINNING_PROP.load(deps.storage, round_id)?;
    if winning_prop_id == proposal_id {
        return Err(ContractError::Std(StdError::generic_err("Proposal won")));
    }

    // Check that the sender is the depositor of the tribute
    let mut tribute = TRIBUTE_MAP.load(deps.storage, (round_id, proposal_id, tribute_id))?;
    if tribute.depositor != info.sender {
        return Err(ContractError::Std(StdError::generic_err(
            "Sender is not the depositor of the tribute",
        )));
    }

    // Check that the sender has not already refunded the tribute
    if tribute.refunded {
        return Err(ContractError::Std(StdError::generic_err(
            "Sender has already refunded the tribute",
        )));
    }

    // Mark the tribute as refunded
    tribute.refunded = true;
    TRIBUTE_MAP.save(deps.storage, (round_id, proposal_id, tribute_id), &tribute)?;

    // Send the tribute back to the sender
    Ok(Response::new()
        .add_attribute("action", "refund_tribute")
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![tribute.funds],
        }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => query_count(deps),
    }
}

pub fn query_count(_deps: Deps) -> StdResult<Binary> {
    let constant = CONSTANTS.load(_deps.storage)?;
    to_json_binary(&(CountResponse { count: 0 }))
}

use cosmwasm_std::{
    entry_point, to_json_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Timestamp, Uint128,
};

use crate::error::ContractError;
use crate::msg::{CountResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{
    Constants, LockEntry, Proposal, Round, Tribute, Vote, CONSTANTS, LOCKS_MAP, LOCK_ID, PROP_ID,
    PROP_MAP, ROUND_ID, ROUND_MAP, TRIBUTE_ID, TRIBUTE_MAP, VOTE_MAP, WINNING_PROP,
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
        amount: sent_funds.clone(),
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

    for lock in locks {
        let (lock_id, lock_entry) = lock?;
        if lock_entry.lock_end < env.block.time {
            // Send `amount` tokens back to caller
            sends.push(lock_entry.amount.clone());
            // Delete entry from LocksMap
            LOCKS_MAP.remove(deps.storage, (info.sender.clone(), lock_id));
        }
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

fn vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // Load the proposal
    let round_id = ROUND_ID.load(deps.storage)?;
    let proposal = PROP_MAP.load(deps.storage, (round_id, proposal_id))?;

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
        power += lock_entry.amount.amount;
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
            round_id: round_id,
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
    do_covenant_stuff(deps, env, info, proposal.covenant_params);

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

    // Create tribute in TributeMap
    let tribute_id = TRIBUTE_ID.load(deps.storage)?;
    TRIBUTE_ID.save(deps.storage, &(tribute_id + 1))?;
    let tribute = Tribute {
        amount: info.funds[0].clone(),
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

    // Check that the sender has not already claimed the tribute
    if vote.tribute_claimed {
        return Err(ContractError::Std(StdError::generic_err(
            "Sender has already claimed the tribute",
        )));
    }

    // Divide sender's vote power by the prop's power to figure out their percentage
    let proposal = PROP_MAP.load(deps.storage, (round_id, proposal_id))?;
    let percentage = vote.power / proposal.power;

    // Load the tribute and use the percentage to figure out how much of the tribute to send them
    let tribute = TRIBUTE_MAP.load(deps.storage, (round_id, proposal_id, tribute_id))?;
    let amount = Uint128::from(tribute.amount.amount * percentage);

    // Mark on the sender's vote that they claimed the tribute
    let mut vote = VOTE_MAP.load(deps.storage, (round_id, info.sender.clone()))?;
    vote.tribute_claimed = true;
    VOTE_MAP.save(deps.storage, (round_id, info.sender.clone()), &vote)?;

    // Send the tribute to the sender
    Ok(Response::new()
        .add_attribute("action", "claim_tribute")
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![tribute.amount],
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
            amount: vec![tribute.amount],
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

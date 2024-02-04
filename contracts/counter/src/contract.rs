use cosmwasm_std::{
    entry_point, to_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Timestamp, Uint128,
};

use crate::error::ContractError;
use crate::msg::{CountResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{
    Constants, LockEntry, Proposal, Round, Vote, CONSTANTS, LOCKS_MAP, LOCK_ID, PROP_ID, PROP_MAP,
    ROUND_ID, ROUND_MAP, TRIBUTE_ID, VOTE_MAP,
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
        denom: msg.denom,
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
        ExecuteMsg::Tally {} => tally(deps, env, info),
        ExecuteMsg::ExecuteProposal { proposal_id } => {
            execute_proposal(deps, env, info, proposal_id)
        }
        ExecuteMsg::AddTribute { proposal_id } => add_tribute(deps, env, info, proposal_id),
        ExecuteMsg::RefundTribute { proposal_id } => refund_tribute(deps, env, info, proposal_id),
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
    let locks = LOCKS_MAP
        .prefix(info.sender)
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
    validate_covenant_params(covenant_params)?;

    // Create proposal in PropMap
    let proposal = Proposal {
        covenant_params,
        round_id: ROUND_ID.load(deps.storage)?,
        executed: false,
        power: Uint128::zero(),
    };

    let prop_id = PROP_ID.load(deps.storage)?;
    PROP_ID.save(deps.storage, &(prop_id + 1))?;
    PROP_MAP.save(deps.storage, prop_id, &proposal)?;

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
    let vote = VOTE_MAP.load(deps.storage, info.sender.clone());
    if let Ok(vote) = vote {
        // Reverse vote
        let mut proposal = PROP_MAP.load(deps.storage, (round_id, vote.prop_id))?;
        proposal.power -= vote.power;
        PROP_MAP.save(deps.storage, (round_id, vote.prop_id), &proposal)?;
    }
    // Delete vote
    VOTE_MAP.remove(deps.storage, info.sender.clone());

    // Get sender's total locked power
    let mut power: Uint128 = Uint128::zero();
    let locks = LOCKS_MAP
        .prefix(info.sender)
        .range(deps.storage, None, None, Order::Ascending);

    for lock in locks {
        let (_, lock_entry) = lock?;
        power += lock_entry.amount.amount;
    }

    // Update proposal's power in propmap
    let mut proposal = PROP_MAP.load(deps.storage, (round_id, proposal_id))?;
    proposal.power += power;
    PROP_MAP.save(deps.storage, (round_id, proposal_id), &proposal)?;

    // Create vote in Votemap
    let vote = Vote {
        prop_id: proposal_id,
        power,
    };
    VOTE_MAP.save(deps.storage, info.sender, &vote)?;

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
    // Check that the proposal won last round by iterating proposals from last round and making sure this
    // one has the highest power
    let last_round_id = ROUND_ID.load(deps.storage)? - 1;
    let mut max_power = Uint128::zero();
    let mut max_prop_id = 0;
    let proposals =
        PROP_MAP
            .prefix(last_round_id)
            .range(deps.storage, None, None, Order::Ascending);

    for proposal in proposals {
        let (prop_id, proposal) = proposal?;
        if proposal.power > max_power {
            max_power = proposal.power;
            max_prop_id = prop_id;
        }
    }

    if max_prop_id != proposal_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Proposal did not win last round",
        )));
    }

    // Execute proposal
    let mut proposal = PROP_MAP.load(deps.storage, (last_round_id, proposal_id))?;
    if proposal.executed {
        return Err(ContractError::Std(StdError::generic_err(
            "Proposal already executed",
        )));
    }
    do_covenant_stuff(deps, env, info, proposal.covenant_params);

    // Mark proposal as executed
    proposal.executed = true;
    PROP_MAP.save(deps.storage, (last_round_id, proposal_id), &proposal)?;

    Ok(Response::new().add_attribute("action", "execute_proposal"))
}

fn add_tribute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _proposal_id: u64,
) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "add_tribute"))
}

fn refund_tribute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _proposal_id: u64,
) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "refund_tribute"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => query_count(deps),
    }
}

fn try_increment(deps: DepsMut, _env: Env, _info: MessageInfo) -> Result<Response, ContractError> {
    let mut constant = CONSTANTS.load(deps.storage)?;
    constant.count += 1;
    CONSTANTS.save(deps.storage, &constant)?;
    Ok(Response::new().add_attribute("action", "increament"))
}

fn try_reset(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    count: i32,
) -> Result<Response, ContractError> {
    let mut constant = CONSTANTS.load(deps.storage)?;
    if constant.owner != info.sender {
        return Err(ContractError::Std(StdError::generic_err("Unauthorized")));
    }
    constant.count = count;
    CONSTANTS.save(deps.storage, &constant)?;
    Ok(Response::new().add_attribute("action", "COUNT reset successfully"))
}

pub fn query_count(_deps: Deps) -> StdResult<Binary> {
    let constant = CONSTANTS.load(_deps.storage)?;
    to_binary(
        &(CountResponse {
            count: constant.count,
        }),
    )
}

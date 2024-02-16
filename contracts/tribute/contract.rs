// These are just the methods that have to do with the tribute contract, extracted and pasted from the main atom wars contract
// after we decided to put tribute in its own contract. The methods are not complete and will need to be modified to work with the new contract.

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
    tribute_id: u64,
) -> Result<Response, ContractError> {
    // Check that the sender has not already claimed the tribute using the TRIBUTE_CLAIMS map
    if TRIBUTE_CLAIMS.may_load(deps.storage, (info.sender.clone(), tribute_id))? == Some(true) {
        return Err(ContractError::Std(StdError::generic_err(
            "Sender has already claimed the tribute",
        )));
    }

    // Check that the round is ended
    let current_round_id = ROUND_ID.load(deps.storage)?;
    if round_id >= current_round_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Round has not ended yet",
        )));
    }

    // Look up sender's vote for the round, error if it cannot be found
    let vote = VOTE_MAP.load(deps.storage, (round_id, info.sender.clone()))?;

    // Load the winning prop_id
    let winning_prop_id = get_winning_prop(deps.as_ref(), round_id)?;

    // Check that the sender voted for the winning proposal
    if winning_prop_id != vote.prop_id {
        return Err(ContractError::Std(StdError::generic_err(
            "Proposal did not win the last round",
        )));
    }

    // Divide sender's vote power by the prop's power to figure out their percentage
    let proposal = PROPOSAL_MAP.load(deps.storage, (round_id, vote.prop_id))?;
    // TODO: percentage needs to be a decimal type
    let percentage = vote.power / proposal.power;

    // Load the tribute and use the percentage to figure out how much of the tribute to send them
    let tribute = TRIBUTE_MAP.load(deps.storage, (round_id, vote.prop_id, tribute_id))?;
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

    // Get the winning prop for the round
    let winning_prop_id = get_winning_prop(deps.as_ref(), round_id)?;

    // Check that this prop lost
    if winning_prop_id == proposal_id {
        return Err(ContractError::Std(StdError::generic_err("Proposal won")));
    }

    // Load the tribute
    let mut tribute = TRIBUTE_MAP.load(deps.storage, (round_id, proposal_id, tribute_id))?;

    // Check that the sender is the depositor of the tribute
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

// pub fn query_winning_proposal(deps: Deps) -> StdResult<Binary> {
//     let winning_prop_id = get_winning_prop(deps)?;
//     to_json_binary(&winning_prop_id)
// }

pub fn query_count(_deps: Deps) -> StdResult<Binary> {
    let constant = CONSTANTS.load(_deps.storage)?;
    to_json_binary(&(CountResponse { count: 0 }))
}

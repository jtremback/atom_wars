# Atom Wars

## Endpoints

```
LockTokens(lock_duration):
    Receive tokens
    Validate against denom whitelist
    Create entry in LocksMap

UnlockTokens():
    Validate caller
    Validate `lock_end` < now
    Send `amount` tokens back to caller
    Delete entry from LocksMap

CreateProposal(covenant_params, tribute):
    Validate covenant_params
    Hold tribute in contract's account
    Create in PropMap

Vote(proposal_id): // This is currently written for dynamic tallying
    Validate proposal_id (is it in the current round)
    Create entry in VoteMap

Tally(): // This is currently written for dynamic tallying, but is missing pagination
    Check that the current round is over
    Iterate all VoteMap entries for current round:
        Look up sender
        Calculate sender's power
        Add to tally
    Cache the final tally
    Iterate all VoteMap entries for current round:
        Look up sender
        Calculate sender's power
        Pay out tributes proportionally (this involves an iteration)
    Create a new Round in the RoundMap, starting the new round
    Create new proposals for winners

ExecuteProposal(prop_id):
    Check if the prop_id is one of last round's winners
    Load the proposal
    Check if it has already been executed
    If the covenant_params show that this is an incumbent proposal:
        Do nothing
    If not:
        Pull liquidity out of the old proposal in that tranche
        Use the proposal's covenant_params to call the covenant factory to instantiate the covenant
        Deploy Atom into the covenant

AddTribute(prop_id):
    Validate prop_id, make sure it's in the current round
    Check sent funds and add to that prop's tribute in the PropMap

RefundTribute(prop_id):
    Check that prop_id is a losing prop from a previous round
    Iterate all tributes for that prop id:

Migrate:
    Use a migrate message only open to an admin to update params such as denom whitelist
    Set the Hub's ICA account as the admin
```

## State

```
LocksMap: key(sender_address, lock_id) -> {
    amount: Coin,
    lock_start: Timestamp,
    lock_end: Timestamp
}

PropMap: key(prop_id) -> {
    round_id: u64,
    covenant_params,
    executed: bool
}

VoteMap: key() -> {
    round_id: u64,
    prop_id: u64,
    sender_address: Address
}

RoundMap: key(round_id) -> {
    round_id: u64,
    round_end: Timestamp,
    winners
}

TributeMap: key(prop_id, tribute_id) -> {
    sender: Address,
    amount: Coin
}
```

## Notes

Options for covenant deployment/control

- Proposal author specifies a deposit address and convenant contract address to withdraw from, and code ID so that we can see it's a covenant, possibly verify emergency committee
- Have the Atom Wars contract deploy the covenant itself so that you know it's good. prop author would then have to fund the covenant after it passes
- We make a contract that implements covenants to some standard and then the prop author specifies the address and code ID of their covenant that they deploy with our special

Notes on tallying etc

- Dynamic vote tallying at the end is by far the simplest, however could lead to DOS
  - Easy mitigation is a minimum lock to at least not let it get out of hand
- How to make running tally work
  - Keep a minimal amount of data (global tally) to be processed at the end e.g. power voting for each option
  - Each time a user interacts with the voting system, modify the global tally to reflect it, but also store the information needed to reverse their influence on the global tally
  - When they interact with the voting system again, reverse the influence of their old state on the global tally, using the saved data, then recompute the influence of their new state, saving the data needed to reverse again.
  - Due to the fact that power changes continously (even with non-attenuating power), you need to calculate what their power will be when the round ends (if they don't change anything, because that would just be recalculated as summarized above), not what their power currently is.

## Compiling contracts

To compile your contracts:

```bash
wasmkit compile
```

## Running script

```bash
wasmkit run scripts/sample-script.ts
```

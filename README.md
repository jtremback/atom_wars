# Atom Wars technical spec

This is an overview of **the first version of** Atom Wars from a technical and user perspective. The code implementing most of the voting system has already been written, but there is still a substantial amount of work to do to allow the contract to handle protocol owned liquidity. This integration will be done by Timewave.

Much of this has already been covered in the original Atom Wars post, but there are some nuances and differences here since this describes the functioning of the actual contract.

# Protocol Owned Liquidity

Atom Wars is a system where Atom stakers can lock up their staked Atoms in exchange for voting power, which they can use to vote on proposals to deploy Atoms owned by the community pool into liquidity (aka market-making) positions on various decentralized exchanges.

Atom is commonly used to enter and exit positions of other Cosmos tokens. Supplying more liquidity will help to cement Atom’s role as interchain money. The competition in Atom Wars to secure these PoL spots will generate excitement around Atom, and incentivize holders to lock it up.

Some portion of this PoL will be reserved for chains using the Hub’s ICS security product, helping grow the ecosystem. Additionally, PoL proposals can offer “tribute”- an incentive for the Atom Wars voters to vote for them.

# Locking

## Collateral

The first version of Atom Wars will take stAtom as collateral. There has been some opposition to this since there is still debate over whether liquid staking is safe, and Stride (the issuer of stAtom) charges a 10% fee on staking rewards. However, Stride is a consumer chain, and a portion of this fee ultimately goes back to the Hub.

Accepting multiple liquid staking tokens would be complicated and require using an price oracle, and/or querying multiple LST providers before most actions to check how many Atoms each LST represents.

A future version of Atom Wars will let people lock up their Atoms without using a liquid staking token or unstaking from their current validator by using a technology called “LSM shares”, which effectively create a separate denomination for every delegator’s stake on every Hub validator. This is exciting, but non-trivial, since it will require the Atom Wars contract to be able to handle a potentially unlimited number of locked denominations, and it will require Atom Wars to query the Cosmos Hub in several places to validate these denominations whenever a user takes an action.

Until this technology is ready, Atom Wars will institute a cap on the number of stAtoms that can be locked in the contract. This cap will be a fraction of a percent of the total Atom supply, alleviating any security concerns.

## Lock lengths

When a user locks their stAtom, they can select to lock it for different amounts of time. The longer they lock, the more voting power they get.

| stATOM Lock | Power |
| --- | --- |
| 1 month | 1x |
| 3 months | 1.5x |
| 6 months | 2x |
| 1 year | 4x |

Power will decay as time goes by. If a user locks their stAtom for 6 months, their multiplier will stay at 2x until 3 months have passed, then drop down to 1.5x, and so on. This is to make sure that the voters with the longest remaining lockups, hence the most “skin in the game” have the most power at any given time.

![Untitled](https://prod-files-secure.s3.us-west-2.amazonaws.com/446c82d5-b3f9-476a-81d4-56294383712c/9f003de5-d362-4655-977a-42fd2b052146/Untitled.png)

# Voting and deployment of PoL

Voting happens in rounds. The length is configurable and still needs to be fully decided, but let’s say rounds are one month for this example.

During a round, anyone can submit a proposal to be executed in the next round, and voters vote on these proposals. There are several tranches, and only one proposal can win each tranche.

![Untitled](https://prod-files-secure.s3.us-west-2.amazonaws.com/446c82d5-b3f9-476a-81d4-56294383712c/1338a6d6-af5d-4330-bdf9-40660ce50347/Untitled.png)

Once the round is over, the winning proposal in each tranche is deployed using Timewave, a product which automates DeFi processes. Currently it supports liquidity provision on Astroport and Osmosis.

Proposals can be resubmitted in subsequent rounds. If a proposal wins two rounds in a row, the PoL in the proposal will not be touched. If a proposal loses, the PoL will be pulled, and redeployed to the winning proposal.

# Tribute

The Atom Wars forum post mentions “tribute”- funds that proposal creators can attach to proposals which is paid out to the winning proposal. This is not implemented within the main Atom Wars contract, but it is possible for tribute to be awarded with pluggable tribute contracts that read from the Atom Wars contract. These can be switched out permissionlessly and even customized or reinvented by proposal authors.

We will deploy an example default tribute contract which pays out tribute to anyone who voted for a proposal- but only if that proposal wins. This can be used as is by proposal authors, or used as a starting point for custom tribute contracts.

# Contract spec

This voting system is designed to allow for an unlimited number of proposals and an unlimited number of votes to be created, without being vulnerable to DOS. A naive implementation, where all votes or all proposals were processed at the end of the round could be DOSed by creating a large number of votes or proposals. This is not a problem for this implementation.

I will explain the overall principle here, with a more concrete specification below:

- In the `vote` method:
  - The information on which proposal is winning is updated each time someone votes, instead of being calculated at the end of the round.
  - This information is stored in a map called PROPS_BY_SCORE, which maps the score of a proposal to the proposal id.
  - Because of the way that maps work, a single access to PROPS_BY_SCORE can be made to get the winning proposal.
  - To enable switching votes, we store the vote in VOTE_MAP.
  - When a user votes the second time in a round, the information about their previous vote from VOTE_MAP is used to reverse the effect of their previous vote.
  - This leads to slightly higher gas costs for each vote, in exchange for a much lower gas cost at the end of the round.
- In the `get_winning_proposal` function, called by the `execute_proposal` method, and query methods:
  - We consult the PROPS_BY_SCORE map to get the proposal with the highest score, by iterating in descending order and stopping after the first item that comes out, which will be the prop with the highest score.
  - This operation has a very low fixed cost, avoiding any issues of many votes causing DOS.

The other methods are pretty trivial and easy to understand from the specification below

## Methods

- **lock_tokens(lock_duration: u64)**
  - Validate that lock_duration is either 1, 3, 6, or 12 months, in nanoseconds
  - Validate that the sent funds are in the correct denom
  - Create an entry in LOCKS_MAP
- **unlock_tokens()**
  - Iterate all lock entries for the user address
    - If lock_end < current block time
      - send lock_entry.funds to the user
      - Delete the lock entry
- **create_proposal(covenant_params: String)**
  - Validate covenant_params
  - Load the current round_id
  - Create an entry in PROPOSAL_MAP
- **vote(proposal_id: u64)**
  - Load the current round_id
  - Load the round
  - Load any existing vote for this round from the user, and reverse it:
    - Load the proposal that was voted for
    - Remove the proposal’s power from PROPS_BY_SCORE
    - Decrement the proposal’s power to reverse the effect of the vote
    - Save the proposal and save the new power in PROPS_BY_SCORE
    - Delete the old vote
  - Get the user’s current voting power:
    - Iterate all of the user’s lock entries
    - Scale the power of each lock entry by the remaining lockup time at the end of this round
    - Sum the powers of all lock entries
  - Update the proposal’s power in the proposal entry in PROPOSAL_MAP and in PROPS_BY_SCORE
  - Save the vote in VOTE_MAP
- **end_round()**
  - Load the current round_id and the round
  - Check that it has ended by comparing round_end < current block time
  - Calculate the new round’s round_end with current block time + round_length
  - Increment the current round_id
  - Save the new round in ROUND_MAP
- **execute_proposal(proposal_id: u64)**
  - Load the current round_id and subtract 1 to get the last round’s id
  - Load the proposal_id of the winning prop last round using the get_winning_proposal function
  - Check that the proposal_id parameter matches last round’s winner
  - Load the proposal
  - Check that the proposal has not already been executed
  - Execute the proposal using Timewave covenant logic
  - Mark the proposal as executed and save it

## State

- **CONSTANTS →**
  - denom: String
  - round_length: u64
- **LOCKS_MAP: key(user_address, lock_id) →**
  - funds: Coin
  - lock_start: Timestamp
  - lock_end: Timestamp
- **PROPOSAL_MAP: key(round_id, prop_id) →**
  - round_id: u64
  - covenant_params: String
  - executed: bool
  - power: Uint128
- **VOTE_MAP: key(round_id, user_address) →**
  - prop_id: u64
  - power: Uint128
- **ROUND_MAP: key(round_id) →**
  - round_id: u64
  - round_end: Timestamp
- **WINNING_PROP: key(round_id) →**
  - prop_id: u64

# Tribute contract spec

This tribute contract allows anyone to attach funds to a proposal being voted on in Atom Wars. If the proposal wins, the funds will be paid out to everyone who voted for the proposal.

## Methods

- **add_tribute(round_id: u64, proposal_id: u64)**
  - Query the Atom Wars contract to check that the round is currently ongoing
  - Check that the user has sent funds with this function call
  - Create a tribute entry in TRIBUTE_MAP
- **claim_tribute(round_id: u64, tribute_id: u64)**
  - Use the TRIBUTE_CLAIMS map to check that the user has not already claimed their share of the tribute
  - Query the Atom Wars contract to check that the round has ended
  - Query the Atom Wars contract to get the user’s vote for the round
  - Query the Atom Wars contract to get the winning proposal for the round
  - Check that the user voted for the winning proposal
  - Divide the vote’s power by the total power that voted for the proposal to get the percentage of the tribute that they are entitled to
  - Multiply the percentage by the tribute’s total funds to get the amount owed to the user
  - Mark in the TRIBUTE_CLAIMS map that the user has claimed the tribute
  - Send the funds to the user
- **refund_tribute(round_id: u64, proposal_id: u64, tribute_id: u64)**
  - Query the Atom Wars contract to check that the round has ended
  - Query the Atom Wars contract to get the winning proposal for the round
  - Check that proposal_id is NOT the winning proposal
  - Load the tribute from TRIBUTE_MAP
  - Check that the user is the depositor of the tribute
  - Check that the tribute has not already been refunded
  - Mark the tribute as refunded
  - Send the funds back to the user

## State

- **TRIBUTE_MAP: key(round_id: u64, prop_id: u64, tribute_id: u64) →**
  - depositor: Addr
  - funds: Coin
  - refunded: bool
- **TRIBUTE_CLAIMS: key(sender_addr: Addr, tribute_id: u64) →**
  - claimed: bool

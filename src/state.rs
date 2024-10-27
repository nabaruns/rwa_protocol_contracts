use cosmwasm_schema::cw_serde;

use crate::error::ContractError;
use cosmwasm_std::{Addr, Api, Coin, Decimal, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct State {
    pub num_offerings: u64,
    pub fee: Decimal,
    pub owner: Addr,
}

#[cw_serde]
pub struct Offering {
    pub amount: Uint128,
    pub contract: Addr,
    pub seller: Addr,
    pub list_price: Coin,
}

pub const STATE: Item<State> = Item::new("state");
pub const OFFERINGS: Map<&str, Offering> = Map::new("offerings");

pub fn increment_offerings(store: &mut dyn Storage) -> Result<u64, ContractError> {
    let mut num = 0;
    STATE.update(store, |mut state| -> Result<_, ContractError> {
        state.num_offerings += 1;
        num = state.num_offerings;
        Ok(state)
    })?;

    Ok(num)
}

pub fn get_fund(funds: Vec<Coin>, denom: String) -> Result<Coin, ContractError> {
    for fund in funds.into_iter() {
        if fund.denom == denom {
            return Ok(fund);
        }
    }

    Err(ContractError::InsufficientFunds {})
}

pub fn maybe_addr(api: &dyn Api, human: Option<String>) -> StdResult<Option<Addr>> {
    human.map(|x| api.addr_validate(&x)).transpose()
}

#[cw_serde]
pub struct Rental {
    pub id: String,
    pub offering_id: String,
    pub renter: Addr,
    pub start_time: u64,
    pub end_time: u64,
    pub amount: Uint128,
}

pub const RENTALS: Map<&str, Rental> = Map::new("rentals");

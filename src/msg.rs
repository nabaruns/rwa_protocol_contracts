use cw20::Cw20ReceiveMsg;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Decimal, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub fee: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    Buy {
        offering_id: String,
    },
    WithdrawRwa {
        offering_id: String,
    },
    ReceiveRwa(Cw20ReceiveMsg),
    /// only admin.
    WithdrawFees {
        amount: Uint128,
        denom: String,
    },
    /// only admin.
    ChangeFee {
        fee: Decimal,
    },
    RentRwa {
        offering_id: String,
        duration: u64,
    },
    EndRental {
        rental_id: String,
    },
    Clawback {
        rental_id: String,
    },
}

#[cw_serde]
pub struct SellRwa {
    pub list_price: Coin,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(CountResponse)]
    GetCount {},
    #[returns(FeeResponse)]
    GetFee {},
    /// With Enumerable extension.
    /// Requires pagination. Lists all offers controlled by the contract.
    /// Return type: OffersResponse.
    #[returns(OffersResponse)]
    AllOffers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(RentalResponse)]
    GetRental {
        rental_id: String,
    },
}

#[cw_serde]
pub struct CountResponse {
    pub count: u64,
}

#[cw_serde]
pub struct FeeResponse {
    pub fee: Decimal,
}

#[cw_serde]
pub struct OffersResponse {
    pub offers: Vec<Offer>,
}

#[cw_serde]
pub struct Offer {
    pub id: String,
    pub amount: Uint128,
    pub contract: Addr,
    pub seller: Addr,
    pub list_price: Coin,
}

#[cw_serde]
pub struct RentalInfo {
    pub id: String,
    pub offering_id: String,
    pub renter: Addr,
    pub start_time: u64,
    pub end_time: u64,
    pub amount: Uint128,
}

#[cw_serde]
pub struct RentalResponse {
    pub rental: RentalInfo,
}

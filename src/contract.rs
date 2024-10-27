use cosmwasm_std::{
    coin, entry_point, from_json, to_json_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20ReceiveMsg;

use crate::error::ContractError;
use crate::msg::{
    CountResponse, ExecuteMsg, FeeResponse, InstantiateMsg, Offer, OffersResponse, QueryMsg,
    RentalInfo, RentalResponse, SellRwa,
};
use crate::state::{
    get_fund, increment_offerings, Offering, Rental, State, OFFERINGS, RENTALS, STATE,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use std::ops::{Mul, Sub};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:rwa-protocol";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State {
        num_offerings: 0,
        fee: msg.fee,
        owner: info.sender,
    };
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Buy { offering_id } => execute_buy(deps, info, offering_id),
        ExecuteMsg::WithdrawRwa { offering_id } => execute_withdraw(deps, info, offering_id),
        ExecuteMsg::ReceiveRwa(msg) => execute_receive_rwa(deps, info, msg),
        ExecuteMsg::WithdrawFees { amount, denom } => {
            execute_withdraw_fees(deps, info, amount, denom)
        }
        ExecuteMsg::ChangeFee { fee } => execute_change_fee(deps, info, fee),
        ExecuteMsg::RentRwa {
            offering_id,
            duration,
        } => execute_rent_rwa(deps, env, info, offering_id, duration),
        ExecuteMsg::EndRental { rental_id } => execute_end_rental(deps, env, info, rental_id),
        ExecuteMsg::Clawback { rental_id } => execute_clawback(deps, env, info, rental_id),
    }
}

pub fn execute_buy(
    deps: DepsMut,
    info: MessageInfo,
    offering_id: String,
) -> Result<Response, ContractError> {
    // check if offering exists
    let off = OFFERINGS.load(deps.storage, &offering_id)?;

    if off.seller.eq(&info.sender) {
        return Err(ContractError::InvalidBuyer {});
    }

    // check for enough coins
    let off_fund = get_fund(info.funds.clone(), off.list_price.denom)?;
    if off_fund.amount < off.list_price.amount {
        return Err(ContractError::InsufficientFunds {});
    }

    let state = STATE.load(deps.storage)?;
    let net_amount = Decimal::one().sub(state.fee).mul(off_fund.amount);
    // create transfer msg
    let transfer_msg: CosmosMsg = BankMsg::Send {
        to_address: off.seller.clone().into(),
        amount: vec![coin(net_amount.u128(), off_fund.denom.clone())],
    }
    .into();

    // create transfer cw721 msg
    let transfer_rwa_msg = WasmMsg::Execute {
        contract_addr: off.contract.clone().into(),
        msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: off.amount,
        })?,
        funds: vec![],
    };

    OFFERINGS.remove(deps.storage, &offering_id);

    let price_string = format!("{}{}", off_fund.amount, off_fund.denom);
    let res = Response::new()
        .add_attribute("action", "buy_rwa")
        .add_attribute("buyer", info.sender)
        .add_attribute("seller", off.seller)
        .add_attribute("paid_price", price_string)
        .add_attribute("amount", off.amount)
        .add_attribute("rwa_contract", off.contract)
        .add_messages(vec![
            transfer_msg,
            cosmwasm_std::CosmosMsg::Wasm(transfer_rwa_msg),
        ]);
    Ok(res)
}

pub fn execute_withdraw(
    deps: DepsMut,
    info: MessageInfo,
    offering_id: String,
) -> Result<Response, ContractError> {
    let off = OFFERINGS.load(deps.storage, &offering_id)?;
    if off.seller.ne(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    let transfer_rwa_msg = WasmMsg::Execute {
        contract_addr: off.contract.into(),
        msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: off.seller.into(),
            amount: off.amount,
        })?,
        funds: vec![],
    };

    OFFERINGS.remove(deps.storage, &offering_id);

    let res = Response::new()
        .add_attribute("action", "withdraw_rwa")
        .add_attribute("seller", info.sender)
        .add_message(transfer_rwa_msg);
    Ok(res)
}

pub fn execute_receive_rwa(
    deps: DepsMut,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg: SellRwa = from_json(&wrapper.msg)?;
    let id = increment_offerings(deps.storage)?.to_string();

    // save Offering
    let off = Offering {
        contract: info.sender.clone(),
        amount: wrapper.amount,
        seller: deps.api.addr_validate(&wrapper.sender)?,
        list_price: msg.list_price.clone(),
    };
    OFFERINGS.save(deps.storage, &id, &off)?;

    let price_string = format!("{}{}", msg.list_price.amount, msg.list_price.denom);
    let res = Response::new()
        .add_attribute("action", "sell_rwa")
        .add_attribute("offering_id", id)
        .add_attribute("rwa_contract", info.sender)
        .add_attribute("seller", off.seller)
        .add_attribute("list_price", price_string)
        .add_attribute("amount", off.amount);
    Ok(res)
}

pub fn execute_withdraw_fees(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
    denom: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    if state.owner.ne(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    let transfer: CosmosMsg = BankMsg::Send {
        to_address: state.owner.into(),
        amount: vec![coin(amount.into(), denom)],
    }
    .into();

    Ok(Response::new().add_message(transfer))
}

pub fn execute_change_fee(
    deps: DepsMut,
    info: MessageInfo,
    fee: Decimal,
) -> Result<Response, ContractError> {
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        if state.owner.ne(&info.sender) {
            return Err(ContractError::Unauthorized {});
        }

        state.fee = fee;
        Ok(state)
    })?;

    let res = Response::new()
        .add_attribute("action", "change_fee")
        .add_attribute("fee", fee.to_string());
    Ok(res)
}

pub fn execute_rent_rwa(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    offering_id: String,
    duration: u64,
) -> Result<Response, ContractError> {
    let off = OFFERINGS.load(deps.storage, &offering_id)?;

    if off.seller == info.sender {
        return Err(ContractError::InvalidRenter {});
    }

    let rental_price = off.list_price.amount * Uint128::from(duration);
    let payment = get_fund(info.funds, off.list_price.denom.clone())?;
    if payment.amount < rental_price {
        return Err(ContractError::InsufficientFunds {});
    }

    let state = STATE.load(deps.storage)?;
    let fee_amount = rental_price * state.fee;
    let seller_amount = rental_price - fee_amount;

    let transfer_to_seller = BankMsg::Send {
        to_address: off.seller.to_string(),
        amount: vec![coin(seller_amount.u128(), off.list_price.denom.clone())],
    };

    let transfer_rwa = WasmMsg::Execute {
        contract_addr: off.contract.to_string(),
        msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount: off.amount,
        })?,
        funds: vec![],
    };

    let rental_id = increment_rentals(deps.storage)?.to_string();
    let rental = Rental {
        id: rental_id.clone(),
        offering_id: offering_id.clone(),
        renter: info.sender.clone(),
        start_time: env.block.time.seconds(),
        end_time: env.block.time.seconds() + duration,
        amount: off.amount,
    };
    RENTALS.save(deps.storage, &rental_id, &rental)?;

    Ok(Response::new()
        .add_message(transfer_to_seller)
        .add_message(transfer_rwa)
        .add_attribute("action", "rent_rwa")
        .add_attribute("rental_id", rental_id)
        .add_attribute("offering_id", offering_id)
        .add_attribute("renter", info.sender)
        .add_attribute("duration", duration.to_string()))
}

pub fn execute_end_rental(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    rental_id: String,
) -> Result<Response, ContractError> {
    let rental = RENTALS.load(deps.storage, &rental_id)?;

    if env.block.time.seconds() < rental.end_time {
        return Err(ContractError::RentalNotExpired {});
    }

    if info.sender != rental.renter {
        return Err(ContractError::Unauthorized {});
    }

    let off = OFFERINGS.load(deps.storage, &rental.offering_id)?;

    let transfer_rwa = WasmMsg::Execute {
        contract_addr: off.contract.to_string(),
        msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: off.seller.to_string(),
            amount: rental.amount,
        })?,
        funds: vec![],
    };

    RENTALS.remove(deps.storage, &rental_id);

    Ok(Response::new()
        .add_message(transfer_rwa)
        .add_attribute("action", "end_rental")
        .add_attribute("rental_id", rental_id)
        .add_attribute("renter", rental.renter))
}

pub fn execute_clawback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    rental_id: String,
) -> Result<Response, ContractError> {
    let rental = RENTALS
        .may_load(deps.storage, &rental_id)?
        .ok_or(ContractError::RentalNotFound {})?;
    let off = OFFERINGS.load(deps.storage, &rental.offering_id)?;

    if info.sender != off.seller {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.time.seconds() < rental.end_time {
        return Err(ContractError::RentalNotExpired {});
    }

    let transfer_rwa = WasmMsg::Execute {
        contract_addr: off.contract.to_string(),
        msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: off.seller.to_string(),
            amount: rental.amount,
        })?,
        funds: vec![],
    };

    RENTALS.remove(deps.storage, &rental_id);

    Ok(Response::new()
        .add_message(transfer_rwa)
        .add_attribute("action", "clawback")
        .add_attribute("rental_id", rental_id)
        .add_attribute("seller", off.seller))
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => to_json_binary(&query_count(deps)?),
        QueryMsg::GetFee {} => to_json_binary(&query_fee(deps)?),
        QueryMsg::AllOffers { start_after, limit } => {
            to_json_binary(&query_all(deps, start_after, limit)?)
        }
        QueryMsg::GetRental { rental_id } => to_json_binary(&query_rental(deps, rental_id)?),
    }
}

fn query_count(deps: Deps) -> StdResult<CountResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(CountResponse {
        count: state.num_offerings,
    })
}

fn query_fee(deps: Deps) -> StdResult<FeeResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(FeeResponse { fee: state.fee })
}

fn query_all(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<OffersResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into_bytes()));

    let offers: StdResult<Vec<Offer>> = OFFERINGS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| item.map(map_offer))
        .collect();

    Ok(OffersResponse { offers: offers? })
}

fn query_rental(deps: Deps, rental_id: String) -> StdResult<RentalResponse> {
    let rental = RENTALS.load(deps.storage, &rental_id)?;
    let off = OFFERINGS.load(deps.storage, &rental.offering_id)?;
    Ok(RentalResponse {
        rental: RentalInfo {
            id: rental.id,
            offering_id: rental.offering_id,
            renter: rental.renter,
            start_time: rental.start_time,
            end_time: rental.end_time,
            amount: off.amount,
        },
    })
}

fn map_offer((k, v): (String, Offering)) -> Offer {
    Offer {
        id: k,
        amount: v.amount,
        contract: v.contract,
        seller: v.seller,
        list_price: v.list_price,
    }
}

pub fn increment_rentals(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = RENTALS
        .keys(store, None, None, Order::Descending)
        .next()
        .map(|r| r.and_then(|id| id.parse::<u64>().map_err(StdError::invalid_utf8)))
        .transpose()?
        .unwrap_or(0)
        + 1;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{attr, coins, Decimal, SubMsg};
    use cw20::Cw20ReceiveMsg;

    fn setup(deps: DepsMut) {
        let msg = InstantiateMsg {
            fee: Decimal::percent(2),
        };
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        instantiate(deps, mock_env(), info, msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            fee: Decimal::percent(2),
        };
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }

    #[test]
    fn sell_rwa() {
        let mut deps = mock_dependencies();
        setup(deps.as_mut());

        let sell_msg = SellRwa {
            list_price: coin(1000, "earth"),
        };

        let msg = ExecuteMsg::ReceiveRwa(Cw20ReceiveMsg {
            sender: "owner".into(),
            amount: Uint128::new(100),
            msg: to_json_binary(&sell_msg).unwrap(),
        });
        let info = mock_info("rwa-token", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(0, res.messages.len());

        let msg = QueryMsg::AllOffers {
            start_after: None,
            limit: None,
        };
        let res = query(deps.as_ref(), mock_env(), msg).unwrap();
        let value: OffersResponse = from_json(&res).unwrap();

        assert_eq!(Uint128::new(100), value.offers.first().unwrap().amount);
    }

    #[test]
    fn buy_rwa() {
        let mut deps = mock_dependencies();
        setup(deps.as_mut());

        let sell_msg = SellRwa {
            list_price: coin(1000, "earth"),
        };

        let msg = ExecuteMsg::ReceiveRwa(Cw20ReceiveMsg {
            sender: "owner".into(),
            amount: Uint128::new(100),
            msg: to_json_binary(&sell_msg).unwrap(),
        });
        let info = mock_info("rwa-token", &[]);
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Now buy the RWA tokens
        let msg = ExecuteMsg::Buy {
            offering_id: "1".into(),
        };
        let info = mock_info("buyer", &coins(1000, "earth"));
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(2, res.messages.len());
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "owner".into(),
                amount: coins(980, "earth")
            }))
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "rwa-token".into(),
                msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
                    recipient: "buyer".into(),
                    amount: Uint128::new(100),
                })
                .unwrap(),
                funds: vec![],
            }))
        );

        // Check that the offering has been removed
        let msg = QueryMsg::AllOffers {
            start_after: None,
            limit: None,
        };
        let res = query(deps.as_ref(), mock_env(), msg).unwrap();
        let value: OffersResponse = from_json(&res).unwrap();
        assert_eq!(0, value.offers.len());
    }

    #[test]
    fn withdraw_fees() {
        let mut deps = mock_dependencies_with_balance(&coins(1000, "earth"));
        setup(deps.as_mut());

        let msg = ExecuteMsg::WithdrawFees {
            amount: 1000u32.into(),
            denom: "earth".into(),
        };
        let info = mock_info("anyone", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg.clone());
        match res {
            Err(ContractError::Unauthorized {}) => {}
            _ => panic!("Must return Unauthorized error"),
        }

        let info = mock_info("creator", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "creator".into(),
                amount: coins(1000, "earth")
            }))
        );
    }

    #[test]
    fn change_fee() {
        let mut deps = mock_dependencies();
        setup(deps.as_mut());

        let msg = ExecuteMsg::ChangeFee {
            fee: Decimal::percent(3),
        };
        let info = mock_info("anyone", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg.clone());
        match res {
            Err(ContractError::Unauthorized {}) => {}
            _ => panic!("Must return Unauthorized error"),
        }

        let info = mock_info("creator", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        let msg = QueryMsg::GetFee {};
        let res = query(deps.as_ref(), mock_env(), msg).unwrap();
        let value: FeeResponse = from_json(&res).unwrap();
        assert_eq!(Decimal::percent(3), value.fee);
    }

    #[test]
    fn rent_rwa() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        setup(deps.as_mut());

        // Create an offering
        let sell_msg = SellRwa {
            list_price: coin(10, "earth"),
        };
        let msg = ExecuteMsg::ReceiveRwa(Cw20ReceiveMsg {
            sender: "owner".into(),
            amount: Uint128::new(100),
            msg: to_json_binary(&sell_msg).unwrap(),
        });
        let info = mock_info("rwa-token", &[]);
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Rent the RWA
        let msg = ExecuteMsg::RentRwa {
            offering_id: "1".into(),
            duration: 30, // 30 seconds
        };
        let info = mock_info("renter", &coins(300, "earth")); // 10 * 30 = 300
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        assert_eq!(2, res.messages.len());
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "owner".into(),
                amount: coins(294, "earth") // 300 - 2% fee
            }))
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "rwa-token".into(),
                msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
                    recipient: "renter".into(),
                    amount: Uint128::new(100),
                })
                .unwrap(),
                funds: vec![],
            }))
        );

        // Check rental info
        let msg = QueryMsg::GetRental {
            rental_id: "1".into(),
        };
        let res = query(deps.as_ref(), env.clone(), msg).unwrap();
        let rental_info: RentalResponse = from_json(&res).unwrap();
        assert_eq!(rental_info.rental.renter, "renter");
        assert_eq!(rental_info.rental.amount, Uint128::new(100));
    }

    #[test]
    fn end_rental_and_clawback() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        setup(deps.as_mut());

        // List RWA
        let list_price = coin(10, "earth");
        let sell_msg = SellRwa {
            list_price: list_price.clone(),
        };
        let list_msg = ExecuteMsg::ReceiveRwa(Cw20ReceiveMsg {
            sender: "owner".into(),
            amount: Uint128::new(100),
            msg: to_json_binary(&sell_msg).unwrap(),
        });
        let list_info = mock_info("rwa-token", &[]);
        let list_res = execute(deps.as_mut(), env.clone(), list_info, list_msg).unwrap();

        // Check that the RWA was listed successfully
        assert_eq!(0, list_res.messages.len());
        assert_eq!(
            list_res.attributes,
            vec![
                attr("action", "sell_rwa"),
                attr("offering_id", "1"),
                attr("rwa_contract", "rwa-token"),
                attr("seller", "owner"),
                attr(
                    "list_price",
                    format!("{}{}", list_price.amount, list_price.denom)
                ),
                attr("amount", "100"),
            ]
        );

        // Rent the RWA
        let rent_msg = ExecuteMsg::RentRwa {
            offering_id: "1".into(),
            duration: 30, // 30 seconds
        };
        let rent_info = mock_info("renter", &coins(300, "earth")); // 10 * 30 = 300
        execute(deps.as_mut(), env.clone(), rent_info, rent_msg).unwrap();

        // Fast forward time
        env.block.time = env.block.time.plus_seconds(31);

        // End rental
        let end_msg = ExecuteMsg::EndRental {
            rental_id: "1".into(),
        };
        let end_info = mock_info("renter", &[]);
        let end_res = execute(deps.as_mut(), env.clone(), end_info, end_msg).unwrap();

        assert_eq!(1, end_res.messages.len());
        assert_eq!(
            end_res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "rwa-token".into(),
                msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
                    recipient: "owner".into(),
                    amount: Uint128::new(100),
                })
                .unwrap(),
                funds: vec![],
            }))
        );

        // Try clawback (should fail as rental is already ended)
        let clawback_msg = ExecuteMsg::Clawback {
            rental_id: "1".into(),
        };
        let clawback_info = mock_info("owner", &[]);
        let clawback_err =
            execute(deps.as_mut(), env.clone(), clawback_info, clawback_msg).unwrap_err();
        match clawback_err {
            ContractError::RentalNotFound {} => {}
            _ => panic!("Must return RentalNotFound error"),
        }
    }
}

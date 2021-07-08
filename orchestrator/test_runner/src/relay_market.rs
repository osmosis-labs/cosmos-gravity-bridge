//! This is the testing module for relay market functionality.
//! This is where relayers utilize web30 to interact with a testnet to obtain coin swap values
//! and determine whether relays should happen or not
use std::str::FromStr;

use crate::one_eth;
use crate::utils::{ValidatorKeys, send_one_eth, start_orchestrators};
use actix_web::web;
use clarity::abi::{Token, derive_method_id, encode_call};
use clarity::{Address as EthAddress, PrivateKey as EthPrivateKey, Uint256};
use deep_space::Contact;
use env_logger::{Builder, Env};
use gravity_proto::gravity::query_client::QueryClient as GravityQueryClient;
use tonic::transport::Channel;
use web30::client::Web3;
use web30::eth_wrapping;
use web30::amm::{DAI_CONTRACT_ADDRESS, WETH_CONTRACT_ADDRESS, UNISWAP_QUOTER_ADDRESS};
use web30::jsonrpc::error::Web3Error;

pub async fn relay_market_test(
    _web30: &Web3,
    _grpc_client: GravityQueryClient<Channel>,
    _contact: &Contact,
    keys: Vec<ValidatorKeys>,
    gravity_address: EthAddress,
) {
    // Start Relayer
    // Get price from uniswap
    // Test valset relay happy path
    let address =
        EthAddress::parse_and_validate("0x5A0b54D5dc17e0AadC383d2db43B0a0D3E029c4c").unwrap();
    println!("address is {}", address);
    send_one_eth(address, _web30).await;
    let amount = Uint256::from(1_000_000_000_000_000_000u64);
    let fee = Uint256::from(500u16);
    let sqrt_price_limit_x96_uint160 = Uint256::from(0u16);
    info!("fee: {}", fee);
    let weth_in_dai = _web30.get_uniswap_price(
        address.clone(),
        *WETH_CONTRACT_ADDRESS,
        *DAI_CONTRACT_ADDRESS,
        fee.clone(),
        amount.clone(),
        sqrt_price_limit_x96_uint160,
        None
    ).await;
    if weth_in_dai.is_err() {
        info!("Error getting weth in dai swap price from uniswap: {:?}", weth_in_dai.as_ref().err());
    }
    let weth_in_dai = weth_in_dai.unwrap();
    println!("{:?} weth is worth {} dai less fees", amount, weth_in_dai);
    let fee = 3000u32;
    let weth_in_dai2 = _web30.get_uniswap_price(address.clone(), *WETH_CONTRACT_ADDRESS,
        *DAI_CONTRACT_ADDRESS, fee.clone().into(), amount.clone(),
        0u32.into(), None).await;
    if weth_in_dai2.is_err() {
        info!("Error getting swap price from uniswap: {:?}", weth_in_dai2.as_ref().err());
    }
    let weth_in_dai2 = weth_in_dai2.unwrap();
    assert!(weth_in_dai2 > 1u32.into());
    println!("{:?} weth is worth {} dai less fees", amount, weth_in_dai2);
}

#[test]
pub fn relay_market_local() {
    use actix::System;
    use env_logger::{Builder, Env};
    use std::time::Duration;
    use crate::{one_eth, send_one_eth};
    Builder::from_env(Env::default().default_filter_or("debug")).init(); // Change to debug for logs
    let runner = System::new();
    let web3 = Web3::new("http://localhost:8545", crate::OPERATION_TIMEOUT);
    let caller_address =
        EthAddress::parse_and_validate("0x5A0b54D5dc17e0AadC383d2db43B0a0D3E029c4c").unwrap();
    let amount = one_eth();
    let fee = Uint256::from(500u16);
    let sqrt_price_limit_x96_uint160 = Uint256::from(0u16);

    runner.block_on(async move {
        debug!("Test");
        send_one_eth(caller_address, &web3).await;
        let price = web3
            .get_uniswap_price(
                caller_address,
                *WETH_CONTRACT_ADDRESS,
                *DAI_CONTRACT_ADDRESS,
                fee.clone(),
                amount.clone(),
                sqrt_price_limit_x96_uint160.clone(),
                None,
            )
            .await;
        if price.is_err() {
            panic!("Error getting price: {:?}", price.err());
        }
        let weth2dai = price.unwrap();
        debug!("weth->dai price is {}", weth2dai);
        assert!(weth2dai > 0u32.into());
        let price = web3
            .get_uniswap_price(
                caller_address,
                *DAI_CONTRACT_ADDRESS,
                *WETH_CONTRACT_ADDRESS,
                fee.clone(),
                weth2dai,
                sqrt_price_limit_x96_uint160,
                None,
            )
            .await;
        let dai2weth = price.unwrap();
        debug!("dai->weth price is {}", &dai2weth);
        let amount_float: f64 = (amount.to_string()).parse().unwrap();
        let dai2weth_float: f64 = (dai2weth.to_string()).parse().unwrap();
        // If we were to swap, we should get within 5% back what we originally put in to account for slippage and fees
        assert!((0.95 * amount_float) < dai2weth_float && dai2weth_float < (1.05 * amount_float));
    });
}

#[test]
pub fn relay_swap() {
    use rand::{Rng, thread_rng};
    fn genkey() -> EthPrivateKey {
        let mut rng = thread_rng();
        let key: [u8; 32] = rng.gen();
        EthPrivateKey::from_slice(&key).unwrap()
    }

    use actix::System;
    use env_logger::{Builder, Env};
    use std::time::Duration;
    use web30::client::Web3;
    use web30::eth_wrapping;
    use crate::{one_eth, send_one_eth};
    Builder::from_env(Env::default().default_filter_or("debug")).init(); // Change to debug for logs
    let runner = System::new();

    let web3 = Web3::new("http://localhost:8545", crate::OPERATION_TIMEOUT);
    let secret = genkey();
    let amount = Uint256::from(1000000000u128); // .5 eth
    let fee = Uint256::from(500u16);
    let sqrt_price_limit_x96_uint160 = Uint256::from(0u16);
    let pub_key = secret.to_public_key().unwrap();

    // let mut wtr: Vec<u8> = vec![];
    // wtr.extend(&derive_method_id(sig).unwrap());
    // let payload = wtr;

    // let args_count = get_args_count(sig).unwrap();
    // let tokens_count = get_tokens_count(&tokens);
    // println!("args count is {} tokens count is {} tokens is {:?}", args_count, tokens_count, tokens);
    let router = EthAddress::parse_and_validate("0xE592427A0AEce92De3Edee1F18E0157C05861564").unwrap();

    runner.block_on(async move {
        let deadline = web3.eth_block_number().await.unwrap() + 100u32.into();

        let tokens: Vec<Token> = vec![
            Token::Address(*WETH_CONTRACT_ADDRESS),
            Token::Address(*DAI_CONTRACT_ADDRESS),
            Token::Uint(fee.clone()),
            Token::Address(pub_key),
            Token::Uint(deadline),
            Token::Uint(amount.clone()),
            Token::Uint(0u32.into()),
            Token::Uint(sqrt_price_limit_x96_uint160.clone())
        ];
        let tokens = [Token::Struct(tokens)];
        let sig = "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))";
        println!("sig: {} tokens: {:?}", sig, tokens);
        let payload = encode_call(sig, &tokens).unwrap();
        send_one_eth(pub_key, &web3).await;
        send_one_eth(pub_key, &web3).await;
        let success = web3.wrap_eth(amount, secret).await;
        if let Ok(b) = success {
            println!("Wrapped eth: {}", b);
        } else {
            println!("Failed to wrap");
        }
        println!("WETH Balance: {}", web3.get_weth_balance(pub_key).await.unwrap());
        let result = web3.send_transaction(
            router,
            payload,
            0u32.into(),
            pub_key,
            secret,
            vec![],
        )
        .await;
        println!("result is {:?}", result);
        // TODO: Replace the above logic with the actual call below once that is solidified
        // let price = web3.swap_uniswap(
        //         secret,
        //         *WETH_CONTRACT_ADDRESS,
        //         *DAI_CONTRACT_ADDRESS,
        //         fee.clone(),
        //         amount.clone(),
        //         0u32.into(),
        //         0u32.into(),
        //         sqrt_price_limit_x96_uint160.clone(),
        //         None,
        //         None,
        //     )
        //     .await;
        // if price.is_err() {
        //     panic!("Error performing swap: {:?}", price.err());
        // }
        // debug!("price is {:?}", price)
    });
}
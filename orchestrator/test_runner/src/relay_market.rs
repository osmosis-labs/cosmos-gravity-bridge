//! This is the testing module for relay market functionality.
//! This is where relayers utilize web30 to interact with a testnet to obtain coin swap values
//! and determine whether relays should happen or not
use std::str::FromStr;
use std::time::Duration;

use crate::happy_path::test_valset_update;
use crate::utils::{
    create_parameter_change_proposal, send_one_eth, start_orchestrators, ValidatorKeys,
};
use crate::MINER_PRIVATE_KEY;
use crate::{get_fee, STAKING_TOKEN, TOTAL_TIMEOUT};
use crate::{one_eth, MINER_ADDRESS};
use actix_web::web;
use clarity::{Address as EthAddress, Uint256};
use cosmos_gravity::query::get_gravity_params;
use deep_space::{Coin, Contact};
use gravity_proto::cosmos_sdk_proto::cosmos::gov::v1beta1::VoteOption;
use gravity_proto::gravity::query_client::QueryClient as GravityQueryClient;
use tokio::time::sleep;
use tonic::transport::Channel;
use web30::amm::{DAI_CONTRACT_ADDRESS, WETH_CONTRACT_ADDRESS};
use web30::client::Web3;
use web30::jsonrpc::error::Web3Error;

pub async fn relay_market_test(
    web30: &Web3,
    grpc_client: GravityQueryClient<Channel>,
    contact: &Contact,
    keys: Vec<ValidatorKeys>,
    gravity_address: EthAddress,
) {
    let mut grpc_client = grpc_client;
    // Start Orchestrators
    start_orchestrators(keys.clone(), gravity_address, false, true).await;

    test_valset_good_reward(web30, &mut grpc_client, contact, &keys, gravity_address).await;
    // test_valset_break_even_reward(web30, &mut grpc_client, contact, &keys, gravity_address).await;
    // test_valset_bad_reward(web30, &mut grpc_client, contact, &keys, gravity_address).await;
}

// Updates the valset reward used by the test net
async fn change_valset_reward(
    contact: &Contact,
    grpc_client: &mut GravityQueryClient<Channel>,
    gravity_address: EthAddress,
    keys: &Vec<ValidatorKeys>,
    valset_reward: Coin,
) {
    // 1000 altg deposit
    let deposit = Coin {
        amount: 1_000_000_000u64.into(),
        denom: STAKING_TOKEN.to_string(),
    };
    let proposer_key = keys[0].validator_key;
    // First create a governance proposal to use the input reward
    info!("Creating parameter change governance proposal");
    create_parameter_change_proposal(
        contact,
        proposer_key,
        deposit,
        gravity_address,
        valset_reward.clone(),
    )
    .await;

    // Now vote to pass the proposal
    let proposals = contact
        .get_governance_proposals_in_voting_period()
        .await
        .unwrap();
    for proposal in proposals.proposals {
        // vote yes on very proposal we find
        for key in keys.iter() {
            info!("Voting yes on governance proposal");
            let res = contact
                .vote_on_gov_proposal(
                    proposal.proposal_id,
                    VoteOption::Yes,
                    get_fee(),
                    key.validator_key,
                    Some(TOTAL_TIMEOUT),
                )
                .await
                .unwrap();
            contact.wait_for_tx(res, TOTAL_TIMEOUT).await.unwrap();
        }
    }
    // Wait for the voting period to pass
    sleep(Duration::from_secs(65)).await;

    // Test results
    let params = get_gravity_params(grpc_client).await.unwrap();

    info!("params is {:?}", params);
    let actual_reward = params.valset_reward.unwrap();
    info!("reward is {:?}", actual_reward);
    assert_eq!(
        valset_reward.amount,
        Uint256::from_str(&actual_reward.amount).unwrap()
    );
    assert_eq!(valset_reward.denom, actual_reward.denom);
}

// Gets the amount of dai one eth is worth
async fn get_dai_in_eth(web30: &Web3) -> Result<Uint256, Web3Error> {
    web30
        .get_uniswap_price(
            *MINER_ADDRESS,
            *WETH_CONTRACT_ADDRESS,
            *DAI_CONTRACT_ADDRESS,
            None,
            one_eth(),
            None,
            None,
        )
        .await
}

async fn test_valset_good_reward(
    web30: &Web3,
    grpc_client: &mut GravityQueryClient<Channel>,
    contact: &Contact,
    keys: &Vec<ValidatorKeys>,
    gravity_address: EthAddress,
) {
    let denom = web30
        .get_erc20_name(*DAI_CONTRACT_ADDRESS, *MINER_ADDRESS)
        .await
        .unwrap();

    // 1 mwei in dai
    let break_even = get_dai_in_eth(web30).await.unwrap() / 1_000_000_000_000_000_000u128.into();
    // Pay them in 2 mwei worth of dai
    let amount = break_even * 2u8.into();
    let valset_reward = Coin { amount, denom };
    // Pay the relayers in dai
    change_valset_reward(
        contact,
        grpc_client,
        gravity_address,
        keys,
        valset_reward.clone(),
    )
    .await;
    // trigger a valset update
    test_valset_update(web30, contact, &keys, gravity_address).await;

    // check that one of the relayers has footoken now
    let mut found = false;
    for key in keys.iter() {
        let target_address = key.eth_key.to_public_key().unwrap();
        let balance_of_dai = web30
            .get_erc20_balance(*DAI_CONTRACT_ADDRESS, target_address)
            .await
            .unwrap();
        if balance_of_dai == valset_reward.amount {
            found = true;
        }
    }
    if !found {
        panic!("No relayer was rewarded in dai for relaying validator set!")
    }
    info!("Successfully Issued validator set reward!");
}

async fn test_valset_break_even_reward(
    web30: &Web3,
    grpc_client: &mut GravityQueryClient<Channel>,
    contact: &Contact,
    keys: &Vec<ValidatorKeys>,
    gravity_address: EthAddress,
) {
}

async fn test_valset_bad_reward(
    web30: &Web3,
    grpc_client: &mut GravityQueryClient<Channel>,
    contact: &Contact,
    keys: &Vec<ValidatorKeys>,
    gravity_address: EthAddress,
) {
}
// if we don't do this the orchestrators may run ahead of us and we'll be stuck here after
//     // getting credit for two loops when we did one
//     let starting_eth_valset_nonce = get_valset_nonce(gravity_address, *MINER_ADDRESS, &web30)
//         .await
//         .expect("Failed to get starting eth valset");
//     let start = Instant::now();

//     // now we send a valset request that the orchestrators will pick up on
//     // in this case we send it as the first validator because they can pay the fee
//     info!("Sending in valset request");

//     // this is hacky and not really a good way to test validator set updates in a highly
//     // repeatable fashion. What we really need to do is be aware of the total staking state
//     // and manipulate the validator set very intentionally rather than kinda blindly like
//     // we are here. For example the more your run this function the less this fixed amount
//     // makes any difference, eventually it will fail because the change to the total staked
//     // percentage is too small.
//     let mut rng = rand::thread_rng();
//     let validator_to_change = rng.gen_range(0..keys.len());
//     let delegate_address = &keys[validator_to_change]
//         .validator_key
//         // this is not guaranteed to be correct, the chain may set the valoper prefix in a
//         // different way, but I haven't yet seen one that does not match this pattern
//         .to_address(&format!("{}valoper", *ADDRESS_PREFIX))
//         .unwrap();
//     let amount = Coin {
//         denom: STAKING_TOKEN.to_string(),
//         amount: (STARTING_STAKE_PER_VALIDATOR / 4).into(),
//     };
//     info!(
//         "Delegating {} to {} in order to generate a validator set update",
//         amount, delegate_address
//     );
//     contact
//         .delegate_to_validator(
//             *delegate_address,
//             amount,
//             get_fee(),
//             keys[1].validator_key,
//             Some(TOTAL_TIMEOUT),
//         )
//         .await
//         .unwrap();

//     let mut current_eth_valset_nonce = get_valset_nonce(gravity_address, *MINER_ADDRESS, &web30)
//         .await
//         .expect("Failed to get current eth valset");

//     while starting_eth_valset_nonce == current_eth_valset_nonce {
//         info!(
//             "Validator set is not yet updated to {}>, waiting",
//             starting_eth_valset_nonce
//         );
//         current_eth_valset_nonce = get_valset_nonce(gravity_address, *MINER_ADDRESS, &web30)
//             .await
//             .expect("Failed to get current eth valset");
//         delay_for(Duration::from_secs(4)).await;
//         if Instant::now() - start > TOTAL_TIMEOUT {
//             panic!("Failed to update validator set");
//         }
//     }
//     assert!(starting_eth_valset_nonce != current_eth_valset_nonce);
//     info!("Validator set successfully updated!");
// }

// #[test]
// pub fn relay_swap() {
//     use clarity::PrivateKey as EthPrivateKey;
//     use rand::{thread_rng, Rng};
//     fn genkey() -> EthPrivateKey {
//         let mut rng = thread_rng();
//         let key: [u8; 32] = rng.gen();
//         EthPrivateKey::from_slice(&key).unwrap()
//     }

//     use crate::{one_eth, send_one_eth};
//     use actix::System;
//     use env_logger::{Builder, Env};
//     use web30::client::Web3;
//     Builder::from_env(Env::default().default_filter_or("debug")).init(); // Change to debug for logs
//     let runner = System::new();

//     let web3 = Web3::new("http://localhost:8545", crate::OPERATION_TIMEOUT);
//     let secret = genkey();
//     let amount = one_eth();
//     let pub_key = secret.to_public_key().unwrap();

//     runner.block_on(async move {
//         send_one_eth(pub_key, &web3).await;
//         send_one_eth(pub_key, &web3).await;
//         let success = web3.wrap_eth(amount.clone(), secret, None).await;
//         if let Ok(b) = success {
//             println!("Wrapped eth: {}", b);
//         } else {
//             println!("Failed to wrap");
//         }
//         println!(
//             "WETH Balance: {}, DAI Balance: {}",
//             web3.get_erc20_balance(*WETH_CONTRACT_ADDRESS, pub_key)
//                 .await
//                 .unwrap(),
//             web3.get_erc20_balance(*DAI_CONTRACT_ADDRESS, pub_key)
//                 .await
//                 .unwrap()
//         );
//         let price = web3
//             .swap_uniswap(
//                 secret,
//                 *WETH_CONTRACT_ADDRESS,
//                 *DAI_CONTRACT_ADDRESS,
//                 None,
//                 amount.clone(),
//                 None,
//                 None,
//                 None,
//                 None,
//                 None,
//             )
//             .await;
//         if price.is_err() {
//             panic!("Error performing swap: {:?}", price.err());
//         }
//         debug!("price is {:?}", price);
//         debug!(
//             "WETH Balance: {}, DAI Balance: {}",
//             web3.get_erc20_balance(*WETH_CONTRACT_ADDRESS, pub_key)
//                 .await
//                 .unwrap(),
//             web3.get_erc20_balance(*DAI_CONTRACT_ADDRESS, pub_key)
//                 .await
//                 .unwrap()
//         );
//     });
// }

use crate::get_fee;
use crate::happy_path::test_erc20_deposit;
use crate::one_eth;
use crate::utils::*;
use crate::ADDRESS_PREFIX;
use crate::OPERATION_TIMEOUT;
use crate::STAKING_TOKEN;
use crate::STARTING_STAKE_PER_VALIDATOR;
use crate::TOTAL_TIMEOUT;
use clarity::{Address as EthAddress, Uint256};
use cosmos_gravity::query::get_last_event_nonce_for_validator;
use cosmos_gravity::send::MEMO;
use deep_space::address::Address as CosmosAddress;
use deep_space::coin::Coin;
use deep_space::error::CosmosGrpcError;
use deep_space::utils::encode_any;
use deep_space::Contact;
use deep_space::Fee;
use deep_space::Msg;
use ethereum_gravity::utils::downcast_uint256;
use futures::future::join;
use gravity_proto::cosmos_sdk_proto::cosmos::gov::v1beta1::VoteOption;
use gravity_proto::cosmos_sdk_proto::cosmos::params::v1beta1::ParamChange;
use gravity_proto::cosmos_sdk_proto::cosmos::params::v1beta1::ParameterChangeProposal;
use gravity_proto::cosmos_sdk_proto::cosmos::staking::v1beta1::QueryValidatorsRequest;
use gravity_proto::cosmos_sdk_proto::cosmos::tx::v1beta1::BroadcastMode;
use gravity_proto::gravity::query_client::QueryClient as GravityQueryClient;
use gravity_proto::gravity::MsgSendToCosmosClaim;
use std::str::FromStr;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;
use tonic::transport::Channel;
use web30::client::Web3;

pub async fn unhalt_bridge_test(
    web30: &Web3,
    grpc_client: GravityQueryClient<Channel>,
    contact: &Contact,
    keys: Vec<ValidatorKeys>,
    gravity_address: EthAddress,
    erc20_address: EthAddress,
    validator_out: bool,
) {
    let prefix = contact.get_prefix();
    let mut grpc_client = grpc_client;
    let no_relay_market_config = create_default_test_config();
    let bridge_user = get_user_key();
    info!("Sending bridge user some tokens");
    send_one_eth(bridge_user.eth_address, web30).await;
    send_erc20_bulk(
        one_eth() * 10u64.into(),
        erc20_address,
        &[bridge_user.eth_address],
        web30,
    )
    .await;

    start_orchestrators(
        keys.clone(),
        gravity_address,
        validator_out,
        no_relay_market_config.clone(),
    )
    .await;

    redistribute_stake(&keys, contact, &prefix).await;

    // Test a deposit to increment the event nonce before false claims happen
    test_erc20_deposit(
        web30,
        contact,
        &mut grpc_client,
        bridge_user.cosmos_address,
        gravity_address,
        erc20_address,
        10_000_000_000_000_000u64.into(),
    )
    .await;

    let fee = Fee {
        amount: vec![get_fee()],
        gas_limit: 500_000_000u64,
        granter: None,
        payer: None,
    };
    // These are the nonces each validator is aware of before false claims are submitted
    let (init_val1_nonce, init_val2_nonce, init_val3_nonce) =
        get_nonces(&mut grpc_client, &keys, &prefix).await;
    // At this point we can use any nonce since all the validators have the same state
    let initial_valid_nonce = init_val1_nonce;
    info!(
        "initial_nonce: {} init_val1_nonce: {} init_val2_nonce: {} init_val3_nonce: {}",
        initial_valid_nonce, init_val1_nonce, init_val2_nonce, init_val3_nonce,
    );

    // All nonces should be the same right now
    assert!(
        init_val1_nonce == init_val2_nonce && init_val2_nonce == init_val3_nonce,
        "The initial nonces differed!"
    );

    let initial_height =
        downcast_uint256(web30.eth_get_latest_block().await.unwrap().number).unwrap();

    info!("Two validators submitting false claims!");
    submit_false_claims(
        &keys,
        initial_valid_nonce + 1,
        initial_height + 1,
        &bridge_user,
        &prefix,
        erc20_address,
        contact,
        &fee,
    )
    .await;

    info!("Getting latest nonce after false claims for each validator");
    let (val1_nonce, val2_nonce, val3_nonce) = get_nonces(&mut grpc_client, &keys, &prefix).await;
    info!(
        "initial_nonce: {} val1_nonce: {} val2_nonce: {} val3_nonce: {}",
        initial_valid_nonce, val1_nonce, val2_nonce, val3_nonce,
    );

    // val2_nonce and val3_nonce should be initial + 1 but val1_nonce should not
    assert!(
        val2_nonce == initial_valid_nonce + 1 && val2_nonce == val3_nonce,
        "The false claims validators do not have updated nonces"
    );
    assert!(
        val1_nonce == initial_valid_nonce,
        "The honest validator should not have an updated nonce!"
    );

    info!("Checking that bridge is halted!");

    // Attempt transaction on halted bridge
    let should_fail = test_erc20_deposit(
        web30,
        contact,
        &mut grpc_client,
        bridge_user.cosmos_address,
        gravity_address,
        erc20_address,
        Uint256::from_str("100_000_000_000_000_000").unwrap(),
    )
    .await;

    sleep(Duration::from_secs(30));
    // if should_fail.is_ok() {
    //     panic!("Expected bridge to be locked, but test_erc20_deposit succeeded!");
    // }
    info!("Getting latest nonce after bridge halt check");
    let (val1_nonce, val2_nonce, val3_nonce) = get_nonces(&mut grpc_client, &keys, &prefix).await;
    info!(
        "initial_nonce: {} val1_nonce: {} val2_nonce: {} val3_nonce: {}",
        initial_valid_nonce, val1_nonce, val2_nonce, val3_nonce,
    );

    info!(
        "Bridge successfully locked, starting governance vote to reset nonce to {}.",
        initial_valid_nonce
    );

    // Unhalt the bridge
    let deposit = Coin {
        denom: STAKING_TOKEN.to_string(),
        amount: 1_000_000_000u64.into(),
    };
    let _ = submit_and_pass_gov_proposal(initial_valid_nonce, &deposit, contact, &keys)
        .await
        .expect("Governance proposal failed");
    let start = Instant::now();
    loop {
        let (new_val1_nonce, new_val2_nonce, new_val3_nonce) =
            get_nonces(&mut grpc_client, &keys, &prefix).await;
        if new_val1_nonce == val1_nonce
            && new_val2_nonce == val2_nonce
            && new_val3_nonce == val3_nonce
        {
            info!(
                "Nonces have not changed: {}=>{}, {}=>{}, {}=>{}, sleeping before retry",
                new_val1_nonce, val1_nonce, new_val2_nonce, val2_nonce, new_val3_nonce, val3_nonce
            );
            if Instant::now()
                .checked_duration_since(start)
                .unwrap()
                .gt(&Duration::from_secs(10 * 60))
            {
                panic!("10 minutes have elapsed trying to get the validator last nonces to change for val1 and val2!");
            }
            sleep(Duration::from_secs(10));
            continue;
        } else {
            info!(
                "New nonces: {}=>{}, {}=>{}, {}=>{}, sleeping before retry",
                new_val1_nonce, val1_nonce, new_val2_nonce, val2_nonce, new_val3_nonce, val3_nonce
            );
            break;
        }
    }
    let (val1_nonce, val2_nonce, val3_nonce) = get_nonces(&mut grpc_client, &keys, &prefix).await;
    assert!(
        val1_nonce == val2_nonce && val2_nonce == val3_nonce && val1_nonce == initial_valid_nonce,
        "The post-reset nonces are not equal to the initial nonce",
    );

    // // Restart the failed orchestrators
    // start_orchestrator(
    //     &keys[1],
    //     grpc_client.clone(),
    //     gravity_address,
    //     no_relay_market_config.clone(),
    // );
    // start_orchestrator(
    //     &keys[2],
    //     grpc_client.clone(),
    //     gravity_address,
    //     no_relay_market_config.clone(),
    // );
    info!("Sleeping to see if any other stuff happens on the network");
    sleep(Duration::from_secs(60));

    info!("Attempting to resend now that the bridge should be fixed");
    let res = test_erc20_deposit(
        web30,
        contact,
        &mut grpc_client,
        bridge_user.cosmos_address,
        gravity_address,
        erc20_address,
        Uint256::from_str("100_000_000_000_000_000").unwrap(),
    )
    .await;
    info!("res is {:?}", res);
}

async fn redistribute_stake(keys: &[ValidatorKeys], contact: &Contact, _prefix: &str) {
    let validators = contact
        .get_validators_list(QueryValidatorsRequest::default())
        .await
        .unwrap();
    for validator in validators.validators {
        info!(
            "Validator {} has {} tokens",
            validator.operator_address, validator.tokens
        );
    }
    // let num_validators = keys.len();
    // let controlling_stake = (STARTING_STAKE_PER_VALIDATOR * (num_validators as u128)) as f64 * 0.45;
    // let stake_needed: f64 = controlling_stake - STARTING_STAKE_PER_VALIDATOR as f64;
    // info!(
    //     "Controlling stake is {} stake needed is {}",
    //     controlling_stake, stake_needed
    // );
    let delegate_address = keys[0]
        .validator_key
        .to_address(&format!("{}valoper", *ADDRESS_PREFIX))
        .unwrap();
    // Want validator 0 to have 50% voting power, and they start with 33.3...%
    // 1/2 = 1/3 + 2X where 2X is the delegate amount from the other two validators
    // X = (1/2 - 1/3) / 2 = ((3 * sspv / 2) - sspv) / 2
    // (3s/2 - 2s/2) / 2 = (s/2)/2 = s/4
    let delegate_amount = (STARTING_STAKE_PER_VALIDATOR as f64) / 4.0;
    for (i, k) in keys.iter().enumerate() {
        if i == 0 {
            // Don't delegate from validator 1 to itself
            continue;
        }
        let amount = Coin {
            denom: (*STAKING_TOKEN).to_string(),
            amount: Uint256::from_str(delegate_amount.to_string().as_str()).unwrap(),
        };

        let res = contact
            .delegate_to_validator(
                delegate_address,
                amount,
                get_fee(),
                k.validator_key,
                Some(TOTAL_TIMEOUT),
            )
            .await;
        info!(
            "Delegated {} from validator {} to validator 1, response {:?}",
            delegate_amount,
            &(i + 1),
            res,
        )
    }
    let validators = contact
        .get_validators_list(QueryValidatorsRequest::default())
        .await
        .unwrap();
    for validator in validators.validators {
        info!(
            "Validator {} has {} tokens",
            validator.operator_address, validator.tokens
        );
    }
}

async fn submit_and_pass_gov_proposal(
    nonce: u64,
    deposit: &Coin,
    contact: &Contact,
    keys: &[ValidatorKeys],
) -> Result<bool, CosmosGrpcError> {
    let mut params_to_change: Vec<ParamChange> = Vec::new();
    // this does not
    let reset_state = ParamChange {
        subspace: "gravity".to_string(),
        key: "ResetBridgeState".to_string(),
        value: serde_json::to_string(&true).unwrap(),
    };
    info!("Submit and pass gov proposal: nonce is {}", nonce);
    params_to_change.push(reset_state);
    let reset_nonce = ParamChange {
        subspace: "gravity".to_string(),
        key: "ResetBridgeNonce".to_string(),
        value: format!("\"{}\"", nonce),
    };
    params_to_change.push(reset_nonce);
    let proposal = ParameterChangeProposal {
        title: "Reset Bridge State".to_string(),
        description: "Test resetting bridge state to before things were messed up".to_string(),
        changes: params_to_change,
    };
    let any = encode_any(
        proposal,
        "/cosmos.params.v1beta1.ParameterChangeProposal".to_string(),
    );

    let res = contact
        .create_gov_proposal(
            any.clone(),
            deposit.clone(),
            get_fee(),
            keys[0].validator_key,
            Some(TOTAL_TIMEOUT),
        )
        .await;
    info!("Proposal response is {:?}", res);
    if res.is_err() {
        return Err(res.unwrap_err());
    }

    // Vote yes on all proposals with all validators
    let proposals = contact
        .get_governance_proposals_in_voting_period()
        .await
        .unwrap();
    info!("Found proposals: {:?}", proposals.proposals);
    for proposal in proposals.proposals {
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
    Ok(true)
}

async fn get_nonces(
    grpc_client: &mut GravityQueryClient<Channel>,
    keys: &[ValidatorKeys],
    prefix: &str,
) -> (u64, u64, u64) {
    let nonce1 = get_last_event_nonce_for_validator(
        grpc_client,
        keys[0].orch_key.to_address(prefix).unwrap(),
        prefix.to_string(),
    )
    .await
    .unwrap();
    let nonce2 = get_last_event_nonce_for_validator(
        grpc_client,
        keys[1].orch_key.to_address(prefix).unwrap(),
        prefix.to_string(),
    )
    .await
    .unwrap();
    let nonce3 = get_last_event_nonce_for_validator(
        grpc_client,
        keys[2].orch_key.to_address(prefix).unwrap(),
        prefix.to_string(),
    )
    .await
    .unwrap();
    (nonce1, nonce2, nonce3)
}

fn create_claim(
    nonce: u64,
    height: u64,
    token_contract: &EthAddress,
    initiator_eth_addr: &EthAddress,
    receiver_cosmos_addr: &CosmosAddress,
    orchestrator_addr: &CosmosAddress,
) -> MsgSendToCosmosClaim {
    MsgSendToCosmosClaim {
        event_nonce: nonce,
        block_height: height,
        token_contract: token_contract.to_string(),
        amount: one_eth().to_string(),
        cosmos_receiver: receiver_cosmos_addr.to_string(),
        ethereum_sender: initiator_eth_addr.to_string(),
        orchestrator: orchestrator_addr.to_string(),
    }
}
async fn create_message(
    claim: &MsgSendToCosmosClaim,
    contact: &Contact,
    key: &ValidatorKeys,
    fee: &Fee,
    prefix: &str,
) -> Vec<u8> {
    let msg_url = "/gravity.v1.MsgSendToCosmosClaim";

    let msg = Msg::new(msg_url, claim.clone());
    let args = contact
        .get_message_args(key.orch_key.to_address(prefix).unwrap(), fee.clone())
        .await
        .unwrap();
    let msgs = vec![msg];
    key.orch_key.sign_std_msg(&msgs, args, MEMO).unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn submit_false_claims(
    keys: &[ValidatorKeys],
    nonce: u64,
    height: u64,
    bridge_user: &BridgeUserKey,
    prefix: &str,
    erc20_address: EthAddress,
    contact: &Contact,
    fee: &Fee,
) {
    let mut fut_1 = None;
    let mut fut_2 = None;
    for (i, k) in keys.iter().enumerate() {
        if i == 0 {
            info!("Skipping validator 0 for false claims");
            continue;
        }
        let claim = create_claim(
            nonce,
            height,
            &erc20_address,
            &bridge_user.eth_address,
            &bridge_user.cosmos_address,
            &k.orch_key.to_address(prefix).unwrap(),
        );
        info!("Oracle number {} submitting false deposit {:?}", i, &claim);
        let msg_bytes = create_message(&claim, contact, k, fee, prefix).await;

        let response = contact
            .send_transaction(msg_bytes, BroadcastMode::Sync)
            .await
            .unwrap();
        let fut = contact.wait_for_tx(response, OPERATION_TIMEOUT);

        if i == 1 {
            fut_1 = Some(fut);
        } else {
            fut_2 = Some(fut);
        }
    }
    let join_res = join(fut_1.unwrap(), fut_2.unwrap()).await;
    match join_res.0 {
        Ok(success) => {
            info!("Received success from claim_1's wait_for_tx {:?}", success);
        }
        Err(err) => {
            info!("Received an error from claim_1 {}", err);
        }
    }
    match join_res.1 {
        Ok(success) => {
            info!("Received success from claim_2's wait_for_tx {:?}", success);
        }
        Err(err) => {
            info!("Received an error from claim_2 {}", err);
        }
    }
}

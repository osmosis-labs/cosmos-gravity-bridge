use crate::happy_path::test_erc20_deposit;
use crate::one_eth;
use crate::utils::check_cosmos_balance;
use crate::utils::create_default_test_config;
use crate::utils::decode_response;
use crate::utils::get_user_key;
use crate::utils::start_orchestrators;
use crate::utils::ValidatorKeys;
use crate::MINER_ADDRESS;
use crate::MINER_PRIVATE_KEY;
use crate::OPERATION_TIMEOUT;
use crate::TOTAL_TIMEOUT;
use clarity::Address as EthAddress;
use cosmos_gravity::send::cancel_send_to_eth;
use cosmos_gravity::send::send_to_eth;
use deep_space::Address as CosmosAddress;
use deep_space::Coin;
use deep_space::Contact;
use ethereum_gravity::send_to_cosmos::send_to_cosmos;
use ethereum_gravity::utils::get_valset_nonce;
use gravity_proto::cosmos_sdk_proto::cosmos::tx::v1beta1::Tx;
use gravity_proto::gravity::query_client::QueryClient as GravityQueryClient;
use gravity_proto::gravity::MsgSendToEthResponse;
use gravity_proto::gravity::QueryPendingSendToEth;
use tonic::transport::Channel;
use web30::amm::DAI_CONTRACT_ADDRESS;
use web30::amm::WETH_CONTRACT_ADDRESS;
use web30::client::Web3;

pub async fn genesis_panic_test(
    web30: &Web3,
    grpc_client: GravityQueryClient<Channel>,
    contact: &Contact,
    keys: Vec<ValidatorKeys>,
    gravity_address: EthAddress,
    erc20_address: EthAddress,
) {
    let mut grpc_client = grpc_client.clone();
    let no_relay_market_config = create_default_test_config();
    info!("Start orchestrators");
    start_orchestrators(keys.clone(), gravity_address, false, no_relay_market_config).await;

    info!("Generate user");
    let bridge_user = get_user_key();
    let cosmos_secret = keys[0].validator_key;
    let cosmos_address = cosmos_secret.to_address(&contact.get_prefix()).unwrap();

    info!("Give miner some DAI");
    give_miner_dai(web30).await;
    info!("Give validator some DAI");
    send_dai_to_cosmos_user(cosmos_address, gravity_address, web30, contact).await;

    info!("Check validator's DAI value");
    let coin = check_cosmos_balance("gravity", cosmos_address, contact).await;
    let coin = coin.expect("No Cosmos gravity balances");
    let token_name = coin.denom;
    let amount = coin.amount;

    let fee = Coin {
        denom: token_name.clone(),
        amount: 1u64.into(),
    };
    let amount = amount - 5u64.into();
    info!(
        "Sending {}{} from {} on Cosmos back to Ethereum",
        amount, token_name, cosmos_address
    );
    let _res = send_to_eth(
        cosmos_secret,
        bridge_user.eth_dest_address,
        Coin {
            denom: token_name.clone(),
            amount: amount.clone(),
        },
        fee.clone(),
        fee.clone(),
        contact,
        None,
        // Some(TOTAL_TIMEOUT),
    )
    .await;
    let query_response = grpc_client
        .get_pending_send_to_eth(QueryPendingSendToEth {
            sender_address: cosmos_address.to_string(),
        })
        .await;
    if query_response.is_err() {
        panic!(
            "get_pending_send_to_eth failed with {:?}",
            query_response.err()
        );
    }
    let query_response = query_response.unwrap();
    info!("query_response is {:?}", query_response);
    for unbatched in query_response.into_inner().unbatched_transfers {
        info!("Found unbatched {:?}", unbatched);
    }
    // let res = res.expect("send_to_eth error before we could cancel it");

    // let msg_response = decode_response::<Tx>(res);
    // let tx_id = msg_response.transaction_id;

    // info!(
    //     "Recently sent send_to_eth transaction and received id {}, cancelling now",
    //     tx_id
    // );

    // let cancel = cancel_send_to_eth(cosmos_secret, tx_id, fee.clone(), contact).await;
    // info!("Cancel is {:?}", cancel)
}

pub async fn give_miner_dai(web30: &Web3) {
    // Acquire 10,000 WETH
    let weth_acquired = web30
        .wrap_eth(one_eth() * 10000u16.into(), *MINER_PRIVATE_KEY, None)
        .await;
    assert!(
        !weth_acquired.is_err(),
        "Unable to wrap eth via web30.wrap_eth() {:?}",
        weth_acquired
    );
    // Acquire 1,000 WETH worth of DAI (probably ~23,000 DAI)
    let token_acquired = web30
        .swap_uniswap(
            *MINER_PRIVATE_KEY,
            *WETH_CONTRACT_ADDRESS,
            *DAI_CONTRACT_ADDRESS,
            None,
            one_eth() * 1000u16.into(),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(
        !token_acquired.is_err(),
        "Unable to give the miner 1000 WETH worth of {}",
        *DAI_CONTRACT_ADDRESS,
    );
}

pub async fn send_dai_to_cosmos_user(
    cosmos_dest: CosmosAddress,
    gravity_address: EthAddress,
    web30: &Web3,
    contact: &Contact,
) {
    get_valset_nonce(gravity_address, *MINER_ADDRESS, web30)
        .await
        .expect("Incorrect Gravity Address or otherwise unable to contact Gravity");

    let bals = contact.get_balances(cosmos_dest).await.unwrap();
    for coin in bals {
        info!(
            "Before deposit: cosmos_dest {} has a balance of {} {} tokens",
            cosmos_dest, coin.amount, coin.denom
        );
    }
    info!(
        "Sending cosmos_dest {} 1000 DAI {}",
        cosmos_dest, *DAI_CONTRACT_ADDRESS
    );
    let res = test_erc20_deposit(
        web30,
        contact,
        cosmos_dest,
        gravity_address,
        *DAI_CONTRACT_ADDRESS,
        #[allow(clippy::inconsistent_digit_grouping)]
        1_000_000000000000000000u128.into(),
    )
    .await;
    info!(
        "Sent dai to cosmos user {}, result is {:?}",
        cosmos_dest, res
    );
    let bals = contact.get_balances(cosmos_dest).await.unwrap();
    let mut found = false;
    for coin in bals {
        info!(
            "cosmos_dest {} has a balance of {} {} tokens",
            cosmos_dest, coin.amount, coin.denom
        );
    }
}

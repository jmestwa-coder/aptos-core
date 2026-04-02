// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

use crate::{
    genesis::enable_sync_only_mode, smoke_test_environment::SwarmBuilder,
    utils::get_on_chain_resource,
};
use aptos::common::types::GasOptions;
use aptos_config::config::{OverrideNodeConfig, PersistableConfig};
use aptos_forge::{NodeExt, Swarm, SwarmExt};
use aptos_logger::{debug, info};
use aptos_types::{
    dkg::chunky_dkg::ChunkyDKGState,
    on_chain_config::{
        FeatureFlag, Features, OnChainChunkyDKGConfig, OnChainRandomnessConfig,
    },
};
use std::{
    ops::Add,
    sync::Arc,
    time::{Duration, Instant},
};

/// Chain recovery using a local config from ChunkyDKG stall should work.
/// See `chunky_dkg_config_seqnum.move` for more details.
#[tokio::test]
async fn chunky_dkg_stall_recovery() {
    let epoch_duration_secs = 10;
    let estimated_dkg_latency_secs = 120;

    let (mut swarm, mut cli, _faucet) = SwarmBuilder::new_local(4)
        .with_num_fullnodes(0)
        .with_aptos()
        .with_init_config(Arc::new(|_, config, _| {
            config.api.failpoints_enabled = true;
            config.api.allow_encrypted_txns_submission = true;
            config.consensus.quorum_store.enable_batch_v2_tx = true;
            config.consensus.quorum_store.enable_batch_v2_rx = true;
            config.consensus.quorum_store.enable_opt_qs_v2_payload_tx = true;
            config.consensus.quorum_store.enable_opt_qs_v2_payload_rx = true;
            config
                .state_sync
                .state_sync_driver
                .enable_auto_bootstrapping = true;
            config
                .state_sync
                .state_sync_driver
                .max_connection_deadline_secs = 3;
        }))
        .with_init_genesis_config(Arc::new(move |conf| {
            conf.epoch_duration_secs = epoch_duration_secs;
            conf.consensus_config.enable_validator_txns();
            conf.randomness_config_override = Some(OnChainRandomnessConfig::default_enabled());
            conf.chunky_dkg_config_override = Some(OnChainChunkyDKGConfig::default_enabled());
            let mut features = Features::default();
            features.enable(FeatureFlag::ENCRYPTED_TRANSACTIONS);
            conf.initial_features_override = Some(features);
        }))
        .build_with_cli(0)
        .await;

    let root_addr = swarm.chain_info().root_account().address();
    let root_idx = cli.add_account_with_address_to_cli(swarm.root_key(), root_addr);

    info!("Wait for epoch 2 (proves ChunkyDKG completed for epoch transition).");
    swarm
        .wait_for_all_nodes_to_catchup_to_epoch(
            2,
            Duration::from_secs(epoch_duration_secs + estimated_dkg_latency_secs),
        )
        .await
        .expect("Epoch 2 taking too long to arrive!");

    info!("Verify ChunkyDKG completed a session before we stall.");
    let rest_client = swarm.validators().next().unwrap().rest_client();
    let dkg_state = get_on_chain_resource::<ChunkyDKGState>(&rest_client).await;
    assert!(
        dkg_state.last_completed.is_some(),
        "ChunkyDKG should have a completed session before stall"
    );
    let pre_stall_completed_epoch = dkg_state.last_completed.unwrap().target_epoch();

    info!("Halting the chain by putting every validator into sync_only mode.");
    for validator in swarm.validators_mut() {
        enable_sync_only_mode(4, validator).await;
    }

    info!("Chain should have halted.");
    let liveness_check_result = swarm
        .liveness_check(Instant::now().add(Duration::from_secs(20)))
        .await;
    info!("liveness_check_result={:?}", liveness_check_result);
    assert!(liveness_check_result.is_err());

    info!("Hot-fixing all validators with chunky_dkg_override_seq_num=1.");
    for (idx, validator) in swarm.validators_mut().enumerate() {
        info!("Stopping validator {}.", idx);
        validator.stop();
        let config_path = validator.config_path();
        let mut validator_override_config =
            OverrideNodeConfig::load_config(config_path.clone()).unwrap();
        validator_override_config
            .override_config_mut()
            .chunky_dkg_override_seq_num = 1;
        validator_override_config
            .override_config_mut()
            .consensus
            .sync_only = false;
        info!("Updating validator {} config.", idx);
        validator_override_config.save_config(config_path).unwrap();
        info!("Restarting validator {}.", idx);
        validator.start().unwrap();
        info!("Let validator {} bake for 5 secs.", idx);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let liveness_check_result = swarm
        .liveness_check(Instant::now().add(Duration::from_secs(30)))
        .await;
    assert!(
        liveness_check_result.is_ok(),
        "Chain should have resumed after override"
    );

    info!("Verify no new ChunkyDKG session completed while override is active.");
    let rest_client = swarm.validators().next().unwrap().rest_client();
    let dkg_state = get_on_chain_resource::<ChunkyDKGState>(&rest_client).await;
    let current_completed_epoch = dkg_state
        .last_completed
        .as_ref()
        .map(|s| s.target_epoch())
        .unwrap_or(0);
    assert_eq!(
        current_completed_epoch, pre_stall_completed_epoch,
        "ChunkyDKG should not have completed new sessions while disabled"
    );

    info!("Bump on-chain ChunkyDKGConfigSeqNum to re-enable ChunkyDKG.");
    let script = r#"
script {
    use aptos_framework::aptos_governance;
    use aptos_framework::chunky_dkg_config_seqnum;

    fun main(core_resources: &signer) {
        let framework_signer = aptos_governance::get_signer_testnet_only(core_resources, @0x1);
        chunky_dkg_config_seqnum::set_for_next_epoch(&framework_signer, 2);
        aptos_governance::force_end_epoch(&framework_signer);
    }
}
    "#;
    let gas_options = GasOptions {
        gas_unit_price: Some(1),
        max_gas: Some(2000000),
        expiration_secs: 60,
    };
    let txn_summary = cli
        .run_script_with_gas_options(root_idx, script, Some(gas_options))
        .await
        .expect("Txn execution error.");
    debug!("txn_summary={:?}", txn_summary);

    tokio::time::sleep(Duration::from_secs(10)).await;

    let epoch = rest_client
        .get_ledger_information()
        .await
        .unwrap()
        .into_inner()
        .epoch;
    info!(
        "Current epoch is {}. Wait until epoch {}, and ChunkyDKG should be back.",
        epoch,
        epoch + 1
    );

    // Epoch advancing proves both DKG and ChunkyDKG completed successfully
    swarm
        .wait_for_all_nodes_to_catchup_to_epoch(
            epoch + 1,
            Duration::from_secs(epoch_duration_secs + estimated_dkg_latency_secs),
        )
        .await
        .unwrap_or_else(|_| panic!("Epoch {} taking too long to arrive!", epoch + 1));

    info!("Verify ChunkyDKG completed a new session after re-enable.");
    let dkg_state = get_on_chain_resource::<ChunkyDKGState>(&rest_client).await;
    assert!(
        dkg_state.last_completed.is_some(),
        "ChunkyDKG should have a completed session after re-enable"
    );
    let post_reenable_epoch = dkg_state.last_completed.unwrap().target_epoch();
    assert!(
        post_reenable_epoch > pre_stall_completed_epoch,
        "ChunkyDKG should have completed for a newer epoch after re-enable (got {}, pre-stall was {})",
        post_reenable_epoch,
        pre_stall_completed_epoch
    );

    info!("ChunkyDKG re-enabled: completed session for epoch {} (pre-stall was {}).", post_reenable_epoch, pre_stall_completed_epoch);
}

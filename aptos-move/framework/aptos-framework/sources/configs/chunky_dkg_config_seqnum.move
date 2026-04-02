/// ChunkyDKG stall recovery utils.
///
/// When ChunkyDKG or SecretShareManager is stuck due to a bug, the chain may be stuck. Below is the recovery procedure.
/// 1. Ensure more than 2/3 stakes are stuck at the same version.
/// 1. Every validator restarts with `chunky_dkg_override_seq_num` set to `X+1` in the node config file,
///    where `X` is the current `ChunkyDKGConfigSeqNum` on chain.
/// 1. The chain should then be unblocked.
/// 1. Once the bug is fixed and the binary + framework have been patched,
///    a governance proposal is needed to set `ChunkyDKGConfigSeqNum` to be `X+2`.
module aptos_framework::chunky_dkg_config_seqnum {
    use aptos_framework::config_buffer;
    use aptos_framework::system_addresses;

    friend aptos_framework::reconfiguration_with_dkg;

    /// If this seqnum is smaller than a validator local override, the on-chain `ChunkyDKGConfig` will be ignored.
    /// Useful in a chain recovery from ChunkyDKG stall.
    struct ChunkyDKGConfigSeqNum has drop, key, store {
        seq_num: u64,
    }

    /// Update `ChunkyDKGConfigSeqNum`.
    /// Used when re-enabling ChunkyDKG after an emergency disable via local override.
    public fun set_for_next_epoch(framework: &signer, seq_num: u64) {
        system_addresses::assert_aptos_framework(framework);
        config_buffer::upsert(ChunkyDKGConfigSeqNum { seq_num });
    }

    /// Initialize the configuration. Used in genesis or governance.
    public fun initialize(framework: &signer) {
        system_addresses::assert_aptos_framework(framework);
        if (!exists<ChunkyDKGConfigSeqNum>(@aptos_framework)) {
            move_to(framework, ChunkyDKGConfigSeqNum { seq_num: 0 })
        }
    }

    /// Only used in reconfigurations to apply the pending `ChunkyDKGConfigSeqNum`, if there is any.
    public(friend) fun on_new_epoch(framework: &signer) acquires ChunkyDKGConfigSeqNum {
        system_addresses::assert_aptos_framework(framework);
        if (config_buffer::does_exist<ChunkyDKGConfigSeqNum>()) {
            let new_config = config_buffer::extract_v2<ChunkyDKGConfigSeqNum>();
            if (exists<ChunkyDKGConfigSeqNum>(@aptos_framework)) {
                *borrow_global_mut<ChunkyDKGConfigSeqNum>(@aptos_framework) = new_config;
            } else {
                move_to(framework, new_config);
            }
        }
    }
}

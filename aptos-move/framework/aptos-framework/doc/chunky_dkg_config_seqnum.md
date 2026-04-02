
<a id="0x1_chunky_dkg_config_seqnum"></a>

# Module `0x1::chunky_dkg_config_seqnum`

ChunkyDKG stall recovery utils.

When ChunkyDKG or SecretShareManager is stuck due to a bug, the chain may be stuck. Below is the recovery procedure.
1. Ensure more than 2/3 stakes are stuck at the same version.
1. Every validator restarts with <code>chunky_dkg_override_seq_num</code> set to <code>X+1</code> in the node config file,
where <code>X</code> is the current <code><a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a></code> on chain.
1. The chain should then be unblocked.
1. Once the bug is fixed and the binary + framework have been patched,
a governance proposal is needed to set <code><a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a></code> to be <code>X+2</code>.


-  [Resource `ChunkyDKGConfigSeqNum`](#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum)
-  [Function `set_for_next_epoch`](#0x1_chunky_dkg_config_seqnum_set_for_next_epoch)
-  [Function `initialize`](#0x1_chunky_dkg_config_seqnum_initialize)
-  [Function `on_new_epoch`](#0x1_chunky_dkg_config_seqnum_on_new_epoch)


<pre><code><b>use</b> <a href="config_buffer.md#0x1_config_buffer">0x1::config_buffer</a>;
<b>use</b> <a href="system_addresses.md#0x1_system_addresses">0x1::system_addresses</a>;
</code></pre>



<a id="0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum"></a>

## Resource `ChunkyDKGConfigSeqNum`

If this seqnum is smaller than a validator local override, the on-chain <code>ChunkyDKGConfig</code> will be ignored.
Useful in a chain recovery from ChunkyDKG stall.


<pre><code><b>struct</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a> <b>has</b> drop, store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>seq_num: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a id="0x1_chunky_dkg_config_seqnum_set_for_next_epoch"></a>

## Function `set_for_next_epoch`

Update <code><a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a></code>.
Used when re-enabling ChunkyDKG after an emergency disable via local override.


<pre><code><b>public</b> <b>fun</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_set_for_next_epoch">set_for_next_epoch</a>(framework: &<a href="../../aptos-stdlib/../move-stdlib/doc/signer.md#0x1_signer">signer</a>, seq_num: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_set_for_next_epoch">set_for_next_epoch</a>(framework: &<a href="../../aptos-stdlib/../move-stdlib/doc/signer.md#0x1_signer">signer</a>, seq_num: u64) {
    <a href="system_addresses.md#0x1_system_addresses_assert_aptos_framework">system_addresses::assert_aptos_framework</a>(framework);
    <a href="config_buffer.md#0x1_config_buffer_upsert">config_buffer::upsert</a>(<a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a> { seq_num });
}
</code></pre>



</details>

<a id="0x1_chunky_dkg_config_seqnum_initialize"></a>

## Function `initialize`

Initialize the configuration. Used in genesis or governance.


<pre><code><b>public</b> <b>fun</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_initialize">initialize</a>(framework: &<a href="../../aptos-stdlib/../move-stdlib/doc/signer.md#0x1_signer">signer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_initialize">initialize</a>(framework: &<a href="../../aptos-stdlib/../move-stdlib/doc/signer.md#0x1_signer">signer</a>) {
    <a href="system_addresses.md#0x1_system_addresses_assert_aptos_framework">system_addresses::assert_aptos_framework</a>(framework);
    <b>if</b> (!<b>exists</b>&lt;<a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a>&gt;(@aptos_framework)) {
        <b>move_to</b>(framework, <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a> { seq_num: 0 })
    }
}
</code></pre>



</details>

<a id="0x1_chunky_dkg_config_seqnum_on_new_epoch"></a>

## Function `on_new_epoch`

Only used in reconfigurations to apply the pending <code><a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a></code>, if there is any.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_on_new_epoch">on_new_epoch</a>(framework: &<a href="../../aptos-stdlib/../move-stdlib/doc/signer.md#0x1_signer">signer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_on_new_epoch">on_new_epoch</a>(framework: &<a href="../../aptos-stdlib/../move-stdlib/doc/signer.md#0x1_signer">signer</a>) <b>acquires</b> <a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a> {
    <a href="system_addresses.md#0x1_system_addresses_assert_aptos_framework">system_addresses::assert_aptos_framework</a>(framework);
    <b>if</b> (<a href="config_buffer.md#0x1_config_buffer_does_exist">config_buffer::does_exist</a>&lt;<a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a>&gt;()) {
        <b>let</b> new_config = <a href="config_buffer.md#0x1_config_buffer_extract_v2">config_buffer::extract_v2</a>&lt;<a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a>&gt;();
        <b>if</b> (<b>exists</b>&lt;<a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a>&gt;(@aptos_framework)) {
            *<b>borrow_global_mut</b>&lt;<a href="chunky_dkg_config_seqnum.md#0x1_chunky_dkg_config_seqnum_ChunkyDKGConfigSeqNum">ChunkyDKGConfigSeqNum</a>&gt;(@aptos_framework) = new_config;
        } <b>else</b> {
            <b>move_to</b>(framework, new_config);
        }
    }
}
</code></pre>



</details>


[move-book]: https://aptos.dev/move/book/SUMMARY

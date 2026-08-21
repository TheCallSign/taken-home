Assuming Dispute / Resolve / Chargeback is client scoped. As in, only the client
that initiated the money movement can dispute / resolve / chargeback it.

Since the dispute workflow back-references transaction ID's, we need to keep track of them.
Assuming that disputes and related are client scoped, transaction history can be client scoped. No TTL on old transactions that we keep history for, in case of disputes. Each entry costs 24 bytes + slot overhead so there is a fair bit of headroom for large datasets.

Issues involving the contents of the CSV like invalid transaction IDs and a tx
that isn't under dispute being resolved are ignored. I am taking this approach
for other errors that aren't fatal.

A record that fails to deserialize (bad amount, bad client id, unknown
transaction type, wrong field count) is skipped with a warning on stderr rather
than aborting the run, so one bad row can't discard the whole ledger.

Only deposits can be disputed, the spec states that for disputes: "available
decreases by the amount disputed", which doesn't make sense for withdrawals. If it is needed, it is simple enough to add support.

Scaling

Mutabality is scoped to the per client history for most transactions. Adding a new client would require updating the shared map of `Client`s. Path to multi-threaded implentation is primaryly wrapping the clients map in a `RwLock`, and each client in it's own `Mutex`.

The primary interaction with the parser is an iterator. This lays the groundwork to async streams: The iterator produces a future that is driven to completion by a task. `Client`s could be seen as actors.

Most of the tests have been written with AI assistance.

The 14 integration tests drive the compiled binary end-to-end, covering every transaction type plus the ugly inputs: unfunded withdrawals, out-of-order and cross-client disputes, duplicate/finalized dispute
actions, frozen accounts, missing amounts, malformed rows, and a 20k-row mixed workload checked against exact balances.

Every end-to-end test parses the engine's output CSV back into accounts and asserts `available + held == total` per client, alongside the expected balances.

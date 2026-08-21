use crate::transaction::{TransactionId, TxnState};
use fnv::FnvHashMap;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

mod decimal_trim {
    use rust_decimal::Decimal;
    use serde::Serializer;

    pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
        // trim trailing zeros before serializing.
        let d = d.normalize();
        s.collect_str(&d)
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientId(u16);

#[derive(Serialize)]
pub struct Client {
    #[serde(rename = "client")]
    id: ClientId,
    #[serde(serialize_with = "decimal_trim::serialize")]
    available: Decimal,
    #[serde(serialize_with = "decimal_trim::serialize")]
    held: Decimal,
    #[serde(serialize_with = "decimal_trim::serialize")]
    total: Decimal,
    locked: bool,
    #[serde(skip)]
    movement_history: FnvHashMap<TransactionId, (Decimal, TxnState)>,
}

impl Client {
    pub fn new(id: ClientId) -> Self {
        Self {
            id,
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            total: Decimal::ZERO,
            locked: false,
            movement_history: FnvHashMap::default(),
        }
    }

    /// A deposit is a credit to the client's asset account, meaning it should increase the available and
    /// total funds of the client account
    pub fn deposit(&mut self, txn_id: TransactionId, amount: Decimal) {
        if self.movement_history.contains_key(&txn_id) {
            // Ignore duplicate transaction IDs
            return;
        }

        self.movement_history
            .insert(txn_id, (amount, TxnState::Uncontested));
        self.available += amount;
        self.total += amount;
    }

    /// A withdraw is a debit to the client's asset account, meaning it should decrease the available and
    /// total funds of the client account
    pub fn withdraw(&mut self, amount: Decimal) {
        if self.available >= amount {
            self.available -= amount;
            self.total -= amount;
        }
    }

    /// A dispute represents a client's claim that a transaction was erroneous and should be reversed.
    /// The transaction shouldn't be reversed yet but the associated funds should be held. This means
    /// that the clients available funds should decrease by the amount disputed, their held funds should
    /// increase by the amount disputed, while their total funds should remain the same.
    pub fn dispute(&mut self, transaction_id: TransactionId) {
        let previous_txn = self.movement_history.get_mut(&transaction_id);

        // Ignore other states or an invalid txn_id. Can't dispute a transaction twice.
        if let Some((amount, state @ TxnState::Uncontested)) = previous_txn {
            self.available -= *amount;
            self.held += *amount;
            *state = TxnState::Disputed;
        }
    }

    /// A resolve represents a resolution to a dispute, releasing the associated held funds. Funds that
    /// were previously disputed are no longer disputed. This means that the clients held funds should
    /// decrease by the amount no longer disputed, their available funds should increase by the amount
    /// no longer disputed, and their total funds should remain the same.
    pub fn resolve(&mut self, transaction_id: TransactionId) {
        let previous_txn = self.movement_history.get_mut(&transaction_id);

        // Ignore other states or an invalid txn_id. Can't resolve a uncontested or finalized transaction.
        if let Some((amount, state @ TxnState::Disputed)) = previous_txn {
            self.held -= *amount;
            self.available += *amount;
            *state = TxnState::Finalized;
        }
    }

    /// A chargeback is the final state of a dispute and represents the client reversing a transaction.
    /// Funds that were held have now been withdrawn. This means that the clients held funds and total
    /// funds should decrease by the amount previously disputed. If a chargeback occurs the client's
    /// account should be immediately frozen.
    pub fn chargeback(&mut self, transaction_id: TransactionId) {
        let previous_txn = self.movement_history.get_mut(&transaction_id);

        // Ignore other states or an invalid txn_id. Can't chargeback a uncontested or finalized transaction.
        if let Some((amount, state @ TxnState::Disputed)) = previous_txn {
            self.held -= *amount;
            self.total -= *amount;
            self.locked = true;
            *state = TxnState::Finalized;
        }
    }

    /// Check if the client is locked (frozen) due to a chargeback.
    /// A locked client cannot perform any further transactions.
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Check if the client has no deposits.
    pub fn has_no_deposits(&self) -> bool {
        self.movement_history.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::ClientId;
    use crate::{
        ledger::Ledger,
        records,
        transaction::{TransactionId, TransactionRecord, TxType},
    };
    use rust_decimal::Decimal;

    #[test]
    fn test_deposit() {
        let mut ledger = Ledger::new();
        let transaction = TransactionRecord {
            tx_type: TxType::Deposit,
            id: TransactionId(1),
            client_id: ClientId(1),
            amount: Some(Decimal::new(100000, 4)), // $10.00
        };
        ledger.process_transaction(transaction).unwrap();
        let client = ledger.clients.get(&ClientId(1)).unwrap();
        assert_eq!(client.available, Decimal::new(100000, 4));
        assert_eq!(client.total, Decimal::new(100000, 4));
    }

    #[test]
    fn malformed_csv_records_are_skipped_and_the_rest_still_process() {
        let input = "type,client,tx,amount\n\
                     deposit,1,1,10.0\n\
                     deposit,1,2,not-a-number\n\
                     deposit,1,3,5.0";
        let mut reader = csv::Reader::from_reader(input.as_bytes());
        let ledger = Ledger::from_iter(records(&mut reader))
            .expect("a malformed record must not fail the run");

        let client = ledger.clients.get(&ClientId(1)).unwrap();
        assert_eq!(client.available, Decimal::new(150000, 4));
        assert_eq!(client.total, Decimal::new(150000, 4));
    }
}

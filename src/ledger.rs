use fnv::FnvHashMap;

use crate::{
    LedgerError, Result,
    client::{Client, ClientId},
    transaction::{TransactionRecord, TxType},
};
use rust_decimal::Decimal;

#[derive(Default)]
pub struct Ledger {
    pub(crate) clients: FnvHashMap<ClientId, Client>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process_transaction(&mut self, txn_record: TransactionRecord) -> Result<()> {
        let client = self
            .clients
            .entry(txn_record.client_id)
            .or_insert_with(|| Client::new(txn_record.client_id));

        if client.locked() {
            // If a client's account is locked, any transaction should be ignored
            return Ok(());
        }
        let TransactionRecord {
            tx_type,
            id: txn_id,
            client_id,
            amount,
        } = txn_record;

        // Rescale the amount to 4 decimal places if it exists
        let amount = amount.map(|a| a.round_dp(4));

        match (tx_type, amount) {
            (TxType::Deposit, Some(a)) if a > Decimal::ZERO => client.deposit(txn_id, a),
            (TxType::Withdrawal, Some(a)) if a > Decimal::ZERO => client.withdraw(a),
            (TxType::Dispute, None) => {
                client.dispute(txn_id);
            }
            (TxType::Resolve, None) => {
                client.resolve(txn_id);
            }
            (TxType::Chargeback, None) => {
                client.chargeback(txn_id);
            }
            _ => {
                // Ignore invalid transactions
                // Including disputes/resolves/chargebacks that have amounts
                // and deposits/withdrawals that are negative or zero.
            }
        }

        // Remove the client from the ledger if
        // 1. The operation above failed
        // 2. There was no previous successful operation.
        if client.has_no_deposits() {
            self.clients.remove(&client_id);
        }

        Ok(())
    }

    pub fn from_iter<I: IntoIterator<Item = Result<TransactionRecord>>>(iter: I) -> Result<Self> {
        let mut ledger = Ledger::new();
        for record in iter {
            match record {
                Ok(transaction) => ledger.process_transaction(transaction)?,
                // Skipped rather than fatal so one bad row can't discard the whole ledger.
                Err(LedgerError::DeserializeTransaction { source, .. }) => {
                    eprintln!("warning: skipping malformed record: {source}")
                }
                Err(error) => return Err(error),
            }
        }
        Ok(ledger)
    }

    pub fn write_csv<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        let mut wtr = csv::Writer::from_writer(writer);
        for client in self.clients.values() {
            wtr.serialize(client)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

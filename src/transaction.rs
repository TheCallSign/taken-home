use rust_decimal::Decimal;
use serde::Deserialize;

use crate::client::ClientId;

pub(crate) enum TxnState {
    Uncontested,
    Disputed,
    Finalized,
}

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TxType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TransactionId(pub(crate) u32);

#[derive(Deserialize, Debug, Copy, Clone)]
pub struct TransactionRecord {
    #[serde(rename = "type")]
    pub(crate) tx_type: TxType,
    #[serde(rename = "tx")]
    pub(crate) id: TransactionId,
    #[serde(rename = "client")]
    pub(crate) client_id: ClientId,
    pub(crate) amount: Option<Decimal>,
}
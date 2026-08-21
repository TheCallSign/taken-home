use thiserror::Error;

pub type Result<T> = std::result::Result<T, LedgerError>;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("usage: {program} <transactions.csv>")]
    InvalidArguments { program: String },

    #[error("failed to deserialize CSV transaction")]
    DeserializeTransaction {
        #[source]
        source: csv::Error,
    },

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#![deny(clippy::all)]
mod client;
mod error;
mod ledger;
mod transaction;

use std::{error::Error, fs::File, io};
use crate::transaction::TransactionRecord;

use crate::ledger::Ledger;
use error::{LedgerError, Result};

pub fn records<R: io::Read>(r: &mut csv::Reader<R>) -> impl Iterator<Item = Result<TransactionRecord>> + '_ {
    r.deserialize().map(|source| source.map_err(|e| LedgerError::DeserializeTransaction { source: e }))
}

fn run() -> Result<()> {
    let mut args = std::env::args_os();
    let program = args
        .next()
        .unwrap_or_else(|| "coding-take-home".into())
        .to_string_lossy()
        .into_owned();

    let path = match (args.next(), args.next()) {
        (Some(path), None) => path,
        _ => return Err(LedgerError::InvalidArguments { program }),
    };

    let file = File::open(path)?;
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(io::BufReader::new(file));
    let ledger = Ledger::from_iter(records(&mut reader))?;

    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    ledger.write_csv(&mut output)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if let Some(source) = error.source() {
                eprintln!("source: {source}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn baseline() {
        let input = r#"type,client,tx,amount
deposit,1,1,1.0
deposit,2,2,2.0
deposit,1,3,2.0
withdrawal,1,4,1.5
withdrawal,2,5,1.0"#;
        let mut reader = csv::Reader::from_reader(input.as_bytes());
        let ledger =
            Ledger::from_iter(reader.deserialize().map(|r| r.map_err(LedgerError::from))).unwrap();

        let expected = r#"client,available,held,total,locked
1,1.5,0,1.5,false
2,1,0,1,false"#;

        let mut output = vec![];
        ledger.write_csv(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(
            output.trim() == expected.trim(),
            "Output did not match expected. Output: {}",
            output
        );
    }
}

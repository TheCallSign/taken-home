use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_INPUT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, PartialEq)]
struct Account {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

struct InputFile(PathBuf);

impl InputFile {
    fn new(contents: &str) -> Self {
        let id = NEXT_INPUT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "payments-engine-test-{}-{id}.csv",
            std::process::id()
        ));
        fs::write(&path, contents).expect("failed to write test input");
        Self(path)
    }
}

impl Drop for InputFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn run(input: &str) -> Output {
    let input = InputFile::new(input);
    Command::new(env!("CARGO_BIN_EXE_coding-take-home"))
        .arg(&input.0)
        .output()
        .expect("failed to run payments engine")
}

fn parse_successful_output(output: Output) -> HashMap<u16, Account> {
    assert!(
        output.status.success(),
        "engine failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful run wrote to stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    parse_accounts(&output.stdout)
}

fn parse_accounts(stdout: &[u8]) -> HashMap<u16, Account> {
    let mut accounts = HashMap::new();
    let mut reader = csv::Reader::from_reader(stdout);
    for result in reader.deserialize() {
        let account: Account = result.expect("stdout was not valid account CSV");
        let client = account.client;
        assert!(
            accounts.insert(client, account).is_none(),
            "client {client} appeared more than once"
        );
    }
    accounts
}

fn run_successfully(input: &str) -> HashMap<u16, Account> {
    parse_successful_output(run(input))
}

fn decimal(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn assert_account(
    accounts: &HashMap<u16, Account>,
    client: u16,
    available: &str,
    held: &str,
    total: &str,
    locked: bool,
) {
    let actual = accounts
        .get(&client)
        .unwrap_or_else(|| panic!("missing client {client}"));
    assert_eq!(actual.available, decimal(available), "available");
    assert_eq!(actual.held, decimal(held), "held");
    assert_eq!(actual.total, decimal(total), "total");
    assert_eq!(actual.locked, locked, "locked");
    assert_eq!(
        actual.available + actual.held,
        actual.total,
        "available + held must equal total"
    );
}

#[test]
fn implements_the_documented_cli_example_and_ignores_an_unfunded_withdrawal() {
    let accounts = run_successfully(
        "type, client, tx, amount\n\
         deposit, 1, 1, 1.0\n\
         deposit, 2, 2, 2.0\n\
         deposit, 1, 3, 2.0\n\
         withdrawal, 1, 4, 1.5\n\
         withdrawal, 2, 5, 3.0\n",
    );

    assert_eq!(accounts.len(), 2);
    assert_account(&accounts, 1, "1.5", "0", "1.5", false);
    assert_account(&accounts, 2, "2", "0", "2", false);
}

#[test]
fn accepts_whitespace_and_preserves_four_decimal_place_arithmetic() {
    let accounts = run_successfully(
        "type, client, tx, amount\n\
           deposit , 7 , 4294967295 , 1.2345\n\
           withdrawal , 7 , 1 , 0.0004\n\
           deposit, 65535, 2, 0.0001\n",
    );

    assert_eq!(accounts.len(), 2);
    assert_account(&accounts, 7, "1.2341", "0", "1.2341", false);
    assert_account(&accounts, u16::MAX, "0.0001", "0", "0.0001", false);
}

#[test]
fn a_dispute_moves_the_original_deposit_from_available_to_held() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5.0000\n\
         dispute,1,10,\n",
    );

    assert_account(&accounts, 1, "0", "5", "5", false);
}

#[test]
fn resolving_a_dispute_releases_the_held_funds() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5\n\
         deposit,1,11,2\n\
         dispute,1,10,\n\
         withdrawal,1,12,3\n\
         resolve,1,10,\n\
         withdrawal,1,13,6\n",
    );

    // The first withdrawal is rejected while 5 is held. The second succeeds
    // after resolution releases those funds.
    assert_account(&accounts, 1, "1", "0", "1", false);
}

#[test]
fn a_chargeback_removes_held_funds_and_locks_the_account() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5\n\
         deposit,1,11,2\n\
         dispute,1,10,\n\
         chargeback,1,10,\n",
    );

    assert_account(&accounts, 1, "2", "0", "2", true);
}

#[test]
fn invalid_or_out_of_order_dispute_actions_are_ignored() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5\n\
         resolve,1,10,\n\
         chargeback,1,10,\n\
         dispute,1,999,\n\
         resolve,1,999,\n\
         chargeback,1,999,\n",
    );

    assert_account(&accounts, 1, "5", "0", "5", false);
}

#[test]
fn dispute_actions_are_scoped_to_the_client_that_owns_the_transaction() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5\n\
         deposit,2,20,3\n\
         dispute,2,10,\n",
    );

    assert_account(&accounts, 1, "5", "0", "5", false);
    assert_account(&accounts, 2, "3", "0", "3", false);
}

#[test]
fn repeated_or_finalized_dispute_actions_do_not_apply_twice() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5\n\
         dispute,1,10,\n\
         dispute,1,10,\n\
         withdrawal,1,11,1\n\
         resolve,1,10,\n\
         resolve,1,10,\n\
         dispute,1,10,\n\
         chargeback,1,10,\n\
         withdrawal,1,12,5\n",
    );

    assert_account(&accounts, 1, "0", "0", "0", false);
}

#[test]
fn transactions_after_a_chargeback_do_not_change_a_frozen_account() {
    let accounts = run_successfully(
        "type,client,tx,amount\n\
         deposit,1,10,5\n\
         dispute,1,10,\n\
         chargeback,1,10,\n\
         deposit,1,11,100\n\
         withdrawal,1,12,1\n",
    );

    assert_account(&accounts, 1, "0", "0", "0", true);
}

#[test]
fn a_header_only_input_produces_zero_accounts() {
    let accounts = run_successfully("type, client, tx, amount\n");
    assert!(accounts.is_empty());
}

#[test]
fn nonfatal_edge_cases_are_ignored_or_applied_correctly() {
    let accounts = run_successfully(
        "type, client, tx, amount\n\
         deposit, 1, 1, 5.0000\n\
         withdrawal, 1, 2, 1.5000\n\
         resolve, 1, 1,\n\
         chargeback, 1, 1,\n\
         dispute, 1, 999,\n\
         resolve, 1, 999,\n\
         chargeback, 1, 999,\n\
         dispute, 1, 1,\n\
         dispute, 1, 1,\n\
         resolve, 1, 1,\n\
         withdrawal, 1, 3, 5.0000\n\
         deposit, 2, 4294967295, 1.2345\n\
         dispute, 1, 4294967295,\n",
    );
    assert_eq!(accounts.len(), 2);
    assert_account(&accounts, 1, "3.5", "0", "3.5", false);
    assert_account(&accounts, 2, "1.2345", "0", "1.2345", false);
}

#[test]
fn a_row_with_a_missing_amount_is_ignored() {
    let accounts = run_successfully(
        "type, client, tx, amount\n\
         deposit, 2, 1, 2.0000\n\
         deposit, 9, 2,\n\
         deposit, 9, 3, 5.0000\n",
    );
    assert_eq!(accounts.len(), 2);
    assert_account(&accounts, 2, "2", "0", "2", false);
    assert_account(&accounts, 9, "5", "0", "5", false);
}

#[test]
fn a_malformed_record_is_skipped_and_the_rest_still_process() {
    let output = run("type, client, tx, amount\n\
                      deposit, nope, 1, 1.0000\n\
                      deposit, 1, 2, 3.0000\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "engine failed:\n{stderr}");
    assert!(
        stderr.contains("skipping malformed record") && stderr.contains("record 1"),
        "stderr did not report the skipped record: {stderr}"
    );

    let accounts = parse_accounts(&output.stdout);
    assert_eq!(accounts.len(), 1);
    assert_account(&accounts, 1, "3", "0", "3", false);
}

/// 500 clients over ~20k rows. Clients 1..=490 deposit and withdraw `ROUNDS` times then
/// dispute-and-resolve their first deposit, netting `ROUNDS * 0.5`. Clients 491..=500 charge back
/// their first deposit, which locks the account and makes the trailing deposit a no-op.
fn large_mixed_workload() -> String {
    const CLIENTS: u16 = 500;
    const CHARGEBACK_CLIENTS: u16 = 10;
    const ROUNDS: u32 = 20;

    let mut input = String::from("type, client, tx, amount\n");
    let mut tx: u32 = 0;
    let mut next_tx = move || {
        tx += 1;
        tx
    };

    for client in 1..=CLIENTS {
        if client > CLIENTS - CHARGEBACK_CLIENTS {
            let disputed = next_tx();
            writeln!(input, "deposit, {client}, {disputed}, 1.0000").unwrap();
            writeln!(input, "deposit, {client}, {}, 2.0000", next_tx()).unwrap();
            writeln!(input, "dispute, {client}, {disputed},").unwrap();
            writeln!(input, "chargeback, {client}, {disputed},").unwrap();
            writeln!(input, "deposit, {client}, {}, 5.0000", next_tx()).unwrap();
            continue;
        }

        let disputed = next_tx();
        writeln!(input, "deposit, {client}, {disputed}, 1.0000").unwrap();
        writeln!(input, "withdrawal, {client}, {}, 0.5000", next_tx()).unwrap();
        for _ in 1..ROUNDS {
            writeln!(input, "deposit, {client}, {}, 1.0000", next_tx()).unwrap();
            writeln!(input, "withdrawal, {client}, {}, 0.5000", next_tx()).unwrap();
        }
        writeln!(input, "dispute, {client}, {disputed},").unwrap();
        writeln!(input, "resolve, {client}, {disputed},").unwrap();
    }
    input
}

#[test]
fn a_large_mixed_workload_processes_consistently() {
    let accounts = run_successfully(&large_mixed_workload());
    assert_eq!(accounts.len(), 500);

    assert_account(&accounts, 1, "10", "0", "10", false);
    assert_account(&accounts, 490, "10", "0", "10", false);
    assert_account(&accounts, 500, "2", "0", "2", true);

    let total: Decimal = accounts.values().map(|account| account.total).sum();
    assert_eq!(total, decimal("4920"));
}

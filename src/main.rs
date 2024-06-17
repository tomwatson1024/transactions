use clap::Parser;
use std::path::PathBuf;

mod amount;
mod client;
mod clients;
mod transaction;

use amount::Amount;
use transaction::{load_transactions, TransactionId};

#[derive(Parser)]
struct Args {
    file_path: PathBuf,
}

fn main() {
    let args = Args::parse();
    summarize_transactions(
        std::fs::File::open(args.file_path).expect("failed to open file"),
        std::io::stdout(),
    );
}

fn summarize_transactions(input: impl std::io::Read, output: impl std::io::Write) {
    let mut clients = clients::Clients::new();
    for (index, transaction) in load_transactions(input).enumerate() {
        let transaction = transaction
            .unwrap_or_else(|e| panic!("invalid transaction at line {}: {}", index + 1, e));
        if clients.process_transaction(transaction).is_err() {
            // In a real system, we'd want to do something with these errors,
            // e.g. reporting them to the client.
        }
    }
    clients.write(output).expect("failed to write clients");
}

#[cfg(test)]
mod tests {
    use super::*;

    // High-level test covering a vertial slice of the whole program to make
    // sure everything fits together.
    #[test]
    fn test_summarize_transactions() {
        let input = "type, client, tx, amount
deposit, 7, 1001, 1.0
deposit, 8, 1002, 2.0
deposit, 7, 1003, 2.0
withdrawal, 7, 1004, 1.5
withdrawal, 8, 1005, 3.0
deposit, 7, 1006, 1.0
dispute, 7, 1006
deposit, 8, 1007, 1.0
dispute, 8, 1007
chargeback, 8, 1007
";
        let mut buf = Vec::new();
        summarize_transactions(input.as_bytes(), &mut buf);
        let actual = String::from_utf8(buf).unwrap();
        assert_eq!(
            actual,
            "client,available,held,total,locked
7,1.5000,1.0000,2.5000,false
8,2.0000,0.0000,2.0000,true
"
        );
    }
}

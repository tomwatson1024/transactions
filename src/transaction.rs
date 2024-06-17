use crate::Amount;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientId(u16);

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Transaction {
    pub client_id: ClientId,
    pub data: TransactionData,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransactionData {
    Deposit {
        transaction_id: TransactionId,
        amount: Amount,
    },
    Withdrawal {
        transaction_id: TransactionId,
        amount: Amount,
    },
    Dispute {
        transaction_id: TransactionId,
    },
    Resolve {
        transaction_id: TransactionId,
    },
    Chargeback {
        transaction_id: TransactionId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct TransactionId(u32);

#[cfg(test)]
impl TransactionId {
    pub fn new(value: u32) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Deposit {
    pub transaction_id: TransactionId,
    pub amount: Amount,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Withdrawal {
    pub transaction_id: TransactionId,
    pub amount: Amount,
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("missing amount")]
    MissingAmount,
}

pub fn load_transactions<R: std::io::Read>(
    reader: R,
) -> impl Iterator<Item = Result<Transaction, TransactionError>> {
    csv::ReaderBuilder::new()
        // 'dispute', 'resolve', and 'chargeback' transactions do not have an
        // amount, the fourth field.
        .flexible(true)
        // The parser must be able to handle leading and trailing whitespace.
        .trim(csv::Trim::All)
        .from_reader(reader)
        .into_deserialize::<Row>()
        .map(|r| match r {
            Ok(row) => {
                let transaction: Transaction = row.try_into()?;
                Ok(transaction)
            }
            Err(e) => Err(TransactionError::Csv(e)),
        })
}

// We can't just deserialize directly into `Transaction` because the csv crate
// doesn't support enum variants with data - see
// https://docs.rs/csv/latest/csv/struct.Reader.html#rules. Instead, deserialize
// into an intermediate type then convert.
#[derive(Deserialize)]
struct Row {
    #[serde(rename = "type")]
    type_: TransactionType,
    client: ClientId,
    tx: TransactionId,
    amount: Option<Amount>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl TryFrom<Row> for Transaction {
    type Error = TransactionError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(Transaction {
            client_id: row.client,
            data: match row.type_ {
                TransactionType::Deposit => TransactionData::Deposit {
                    transaction_id: row.tx,
                    amount: row.amount.ok_or(TransactionError::MissingAmount)?,
                },
                TransactionType::Withdrawal => TransactionData::Withdrawal {
                    transaction_id: row.tx,
                    amount: row.amount.ok_or(TransactionError::MissingAmount)?,
                },
                TransactionType::Dispute => TransactionData::Dispute {
                    transaction_id: row.tx,
                },
                TransactionType::Resolve => TransactionData::Resolve {
                    transaction_id: row.tx,
                },
                TransactionType::Chargeback => TransactionData::Chargeback {
                    transaction_id: row.tx,
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_transaction(data: &str) -> Result<Transaction, TransactionError> {
        let data = format!("type, client, tx, amount\n{}", data);
        let transactions: Vec<_> = load_transactions(data.as_bytes()).collect();
        assert_eq!(transactions.len(), 1);
        transactions.into_iter().next().unwrap()
    }

    #[test]
    fn test_parse_deposit() {
        assert_eq!(
            load_transaction("deposit, 1, 2, 3.0").unwrap(),
            Transaction {
                client_id: ClientId(1),
                data: TransactionData::Deposit {
                    transaction_id: TransactionId(2),
                    amount: Amount::try_from("3.0").unwrap(),
                },
            }
        );
    }

    #[test]
    fn test_parse_withdrawal() {
        assert_eq!(
            load_transaction("withdrawal, 1, 2, 3.0").unwrap(),
            Transaction {
                client_id: ClientId(1),
                data: TransactionData::Withdrawal {
                    transaction_id: TransactionId(2),
                    amount: Amount::try_from("3.0").unwrap(),
                },
            }
        );
    }

    #[test]
    fn test_parse_dispute() {
        assert_eq!(
            load_transaction("dispute, 1, 2").unwrap(),
            Transaction {
                client_id: ClientId(1),
                data: TransactionData::Dispute {
                    transaction_id: TransactionId(2),
                },
            }
        );
    }

    #[test]
    fn test_parse_resolve() {
        assert_eq!(
            load_transaction("resolve, 1, 2").unwrap(),
            Transaction {
                client_id: ClientId(1),
                data: TransactionData::Resolve {
                    transaction_id: TransactionId(2),
                },
            }
        );
    }

    #[test]
    fn test_parse_chargeback() {
        assert_eq!(
            load_transaction("chargeback, 1, 2").unwrap(),
            Transaction {
                client_id: ClientId(1),
                data: TransactionData::Chargeback {
                    transaction_id: TransactionId(2),
                },
            }
        );
    }

    #[test]
    fn test_parse_multiple() {
        // Don't include spaces after the commas, to test that the parser can
        // handle that too.
        let data = "type,client,tx,amount\n\
                    deposit,1,2,3.0\n\
                    withdrawal,4,5,6.0\n\
                    dispute,7,8\n\
                    resolve,9,10\n\
                    chargeback,11,12\n";
        let transactions: Vec<_> = load_transactions(data.as_bytes())
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            transactions,
            vec![
                Transaction {
                    client_id: ClientId(1),
                    data: TransactionData::Deposit {
                        transaction_id: TransactionId(2),
                        amount: Amount::try_from("3.0").unwrap(),
                    },
                },
                Transaction {
                    client_id: ClientId(4),
                    data: TransactionData::Withdrawal {
                        transaction_id: TransactionId(5),
                        amount: Amount::try_from("6.0").unwrap(),
                    },
                },
                Transaction {
                    client_id: ClientId(7),
                    data: TransactionData::Dispute {
                        transaction_id: TransactionId(8),
                    },
                },
                Transaction {
                    client_id: ClientId(9),
                    data: TransactionData::Resolve {
                        transaction_id: TransactionId(10),
                    },
                },
                Transaction {
                    client_id: ClientId(11),
                    data: TransactionData::Chargeback {
                        transaction_id: TransactionId(12),
                    },
                },
            ]
        );
    }
}

use serde::Serialize;
use std::collections::HashMap;

use crate::client::{Client, ClientError};
use crate::transaction::{ClientId, Transaction, TransactionData};
use crate::Amount;

pub struct Clients {
    clients: HashMap<ClientId, Client>,
}

impl Clients {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    pub fn process_transaction(&mut self, transaction: Transaction) -> Result<(), ClientError> {
        let client = self.clients.entry(transaction.client_id).or_default();
        match transaction.data {
            TransactionData::Deposit {
                transaction_id,
                amount,
            } => client.deposit(transaction_id, amount),

            TransactionData::Withdrawal { amount, .. } => client.withdraw(amount),
            TransactionData::Dispute { transaction_id } => client.dispute(transaction_id),
            TransactionData::Resolve { transaction_id } => client.resolve(transaction_id),
            TransactionData::Chargeback { transaction_id } => client.chargeback(transaction_id),
        }
    }

    pub fn write(&self, writer: impl std::io::Write) -> Result<(), csv::Error> {
        #[derive(Serialize)]
        struct Row {
            client: ClientId,
            available: Amount,
            held: Amount,
            total: Amount,
            locked: bool,
        }

        // HashMaps aren't ordered. Print the clients in a stable order to make
        // testing easier.
        let mut client_ids: Vec<_> = self.clients.iter().collect();
        client_ids.sort_by_key(|(id, _)| **id);

        let mut writer = csv::Writer::from_writer(writer);
        for (id, client) in client_ids {
            writer.serialize(Row {
                client: *id,
                available: client.available(),
                held: client.held(),
                total: client.total(),
                locked: client.locked(),
            })?
        }
        Ok(writer.flush()?)
    }
}

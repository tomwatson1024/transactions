use crate::{Amount, TransactionId};
use std::collections::{hash_map::Entry, HashMap};

struct Deposit {
    amount: Amount,
    disputed: bool,
}

impl Deposit {
    fn new(amount: Amount) -> Self {
        Self {
            amount,
            disputed: false,
        }
    }
}

#[derive(Default)]
pub struct Client {
    // Assumption: Only deposits can be disputed, not withdrawals. This
    // approach could be extended to allow disputing withdrawals as well, at
    // the cost of having to keep track of them.
    // In a real system we'd want to limit the size of this HashMap by limiting
    // the number of transactions that can be disputed. For example, we might
    // only keep the last 100 transactions.
    deposits: HashMap<TransactionId, Deposit>,

    available: Amount,

    // Invariant: total = available + held
    // where held is the sum of the disputed deposits.
    //
    // This is somewhat duplicating state, since we could calculate the total
    // from available and the deposits HashMap. However, this lets us avoid
    // recalculating the total every time we need it.
    total: Amount,

    locked: bool,
}

// These are all errors we'd expect to report to the client, _not_ e.g. logic
// errors. You could imagine e.g. displaying an error message to the client in
// the UI.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("would overflow")]
    Overflow,
    #[error("insufficient funds")]
    InsufficientFunds,
    #[error("unknown transaction ID")]
    UnknownTransactionId,
    #[error("duplicate transaction ID")]
    DuplicateTransactionId,
    #[error("already disputed")]
    AlreadyDisputed,
    #[error("not disputed")]
    NotDisputed,
    #[error("account locked")]
    Locked,
}

impl Client {
    pub fn deposit(
        &mut self,
        transaction_id: TransactionId,
        amount: Amount,
    ) -> Result<(), ClientError> {
        if self.locked {
            return Err(ClientError::Locked);
        }
        // Don't allow the total funds - available and held - to overflow. This
        // allows us to freely transfer funds between available and held without
        // worrying about overflow.
        // Don't immediately update `self.total` because we might still decide
        // to return an error and we don't want to leave the client in an
        // inconsistent state.
        let total = self
            .total
            .checked_add(amount)
            .ok_or(ClientError::Overflow)?;
        let entry = match self.deposits.entry(transaction_id) {
            // We rely on transaction ID uniqueness to match disputes to
            // deposits.
            Entry::Occupied(_) => return Err(ClientError::DuplicateTransactionId),
            Entry::Vacant(entry) => entry,
        };

        // Since available <= total, this isn't going to overflow.
        self.available = self.available.checked_add(amount).unwrap();
        self.total = total;
        entry.insert(Deposit::new(amount));
        Ok(())
    }

    pub fn withdraw(&mut self, amount: Amount) -> Result<(), ClientError> {
        if self.locked {
            return Err(ClientError::Locked);
        }
        self.available = self
            .available
            .checked_sub(amount)
            .ok_or(ClientError::InsufficientFunds)?;
        // This can't fail because available <= total and we've already
        // successfully reduced available.
        self.total = self.total.checked_sub(amount).unwrap();
        Ok(())
    }

    pub fn dispute(&mut self, transaction_id: TransactionId) -> Result<(), ClientError> {
        if self.locked {
            return Err(ClientError::Locked);
        }
        let deposit = self
            .deposits
            .get_mut(&transaction_id)
            .ok_or(ClientError::UnknownTransactionId)?;
        if deposit.disputed {
            return Err(ClientError::AlreadyDisputed);
        }
        // Assumption: A dispute can't be opened for an amount greater than the
        // available balance.
        // Assuming the funds are available, a dispute triggers the funds to be
        // "held" until the dispute is resolved, decreasing the available
        // balance but not the total.
        self.available = self
            .available
            .checked_sub(deposit.amount)
            .ok_or(ClientError::InsufficientFunds)?;
        deposit.disputed = true;
        Ok(())
    }

    pub fn resolve(&mut self, transaction_id: TransactionId) -> Result<(), ClientError> {
        if self.locked {
            return Err(ClientError::Locked);
        }
        let deposit = self
            .deposits
            .get_mut(&transaction_id)
            .ok_or(ClientError::UnknownTransactionId)?;
        if !deposit.disputed {
            return Err(ClientError::NotDisputed);
        }
        // Resolving a dispute releases the held funds back to the available
        // balance. It does not affect the total.
        // This can't fail because total = available + held, total doesn't
        // overflow, and deposit.amount is part of the held balance.
        self.available = self.available.checked_add(deposit.amount).unwrap();
        deposit.disputed = false;
        Ok(())
    }

    pub fn chargeback(&mut self, transaction_id: TransactionId) -> Result<(), ClientError> {
        if self.locked {
            return Err(ClientError::Locked);
        }
        let entry = match self.deposits.entry(transaction_id) {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(_) => return Err(ClientError::UnknownTransactionId),
        };
        let deposit = entry.get();
        // Assumption: A dispute must be opened before attempting a chargeback.
        if !deposit.disputed {
            return Err(ClientError::NotDisputed);
        };

        // A chargeback causes the held funds to be returned to the client,
        // decreasing the total balance. It does not affect the available
        // balance.
        // This can't fail because total >= held, and deposit.amount is part of
        // the held balance.
        self.total = self.total.checked_sub(deposit.amount).unwrap();

        // We could mark the transaction as "charged back", but it's easier to
        // just remove it - we don't currently have any requirement to keep
        // track of the transaction after it's been charged back.
        entry.remove();

        // A chargeback should cause the account to be locked, preventing any
        // further transactions.
        self.locked = true;
        Ok(())
    }

    pub fn available(&self) -> Amount {
        self.available
    }

    pub fn held(&self) -> Amount {
        // This can't fail because total >= available.
        self.total.checked_sub(self.available).unwrap()
    }

    pub fn total(&self) -> Amount {
        self.total
    }

    pub fn locked(&self) -> bool {
        self.locked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_client(client: &Client, available: &str, held: &str, total: &str, locked: bool) {
        assert_eq!(client.available(), Amount::try_from(available).unwrap());
        assert_eq!(client.held(), Amount::try_from(held).unwrap());
        assert_eq!(client.total(), Amount::try_from(total).unwrap());
        assert_eq!(client.locked(), locked);

        // Check the Client invariant.
        let actual_held = client
            .deposits
            .values()
            .filter(|d| d.disputed)
            .map(|d| d.amount)
            .fold(Amount::default(), |acc, x| acc.checked_add(x).unwrap());
        assert_eq!(client.held(), actual_held);
        assert_eq!(
            client.total(),
            client.available().checked_add(client.held()).unwrap()
        );
    }

    #[test]
    fn test_deposit() {
        // A deposit should increase the available and total funds.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);
    }

    #[test]
    fn test_deposit_duplicate_transaction_id() {
        // A deposit with a duplicate transaction ID should fail.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        assert_eq!(
            client.deposit(TransactionId::new(1), Amount::try_from("2.0").unwrap()),
            Err(ClientError::DuplicateTransactionId)
        );
    }

    #[test]
    fn test_withdrawal() {
        // A withdrawal should decrease the available and total funds.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("2.0").unwrap())
            .unwrap();
        check_client(&client, "2.0", "0.0", "2.0", false);

        client.withdraw(Amount::try_from("1.0").unwrap()).unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);
    }

    #[test]
    fn test_withdrawal_insufficient_funds() {
        // A withdrawal should fail if there are insufficient funds, and the
        // total amount of funds should not change.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);

        assert_eq!(
            client.withdraw(Amount::try_from("2.0").unwrap()),
            Err(ClientError::InsufficientFunds)
        );
        check_client(&client, "1.0", "0.0", "1.0", false);
    }

    #[test]
    fn test_dispute() {
        // A dispute should "hold" funds. The available balance should decrease,
        // the held balance should increase, and the total balance should not
        // change.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        client
            .deposit(TransactionId::new(2), Amount::try_from("2.0").unwrap())
            .unwrap();
        check_client(&client, "3.0", "0.0", "3.0", false);

        client.dispute(TransactionId::new(1)).unwrap();
        check_client(&client, "2.0", "1.0", "3.0", false);
    }

    #[test]
    fn test_dispute_unknown_transaction_id() {
        // Disputing an unknown transaction ID should report an error to the
        // caller - it's up to them whether to ignore it or do something else.
        let mut client = Client::default();
        check_client(&client, "0.0", "0.0", "0.0", false);
        assert_eq!(
            client.dispute(TransactionId::new(1)),
            Err(ClientError::UnknownTransactionId)
        );
        // The client should be unchanged.
        check_client(&client, "0.0", "0.0", "0.0", false);
    }

    #[test]
    fn test_dispute_already_disputed() {
        // Disputing an already disputed transaction should report an error to
        // the caller.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);
        client.dispute(TransactionId::new(1)).unwrap();
        check_client(&client, "0.0", "1.0", "1.0", false);
        assert_eq!(
            client.dispute(TransactionId::new(1)),
            Err(ClientError::AlreadyDisputed)
        );
        // The client should be unchanged.
        check_client(&client, "0.0", "1.0", "1.0", false);
    }

    #[test]
    fn test_dispute_insufficient_funds() {
        // A deposit can't be disputed if the funds aren't available to hold.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("2.0").unwrap())
            .unwrap();
        client
            .deposit(TransactionId::new(2), Amount::try_from("3.0").unwrap())
            .unwrap();
        client.withdraw(Amount::try_from("4.0").unwrap()).unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);
        assert_eq!(
            client.dispute(TransactionId::new(1)),
            Err(ClientError::InsufficientFunds)
        );
        // The client should be unchanged.
        check_client(&client, "1.0", "0.0", "1.0", false);
    }

    #[test]
    fn test_resolve() {
        // A resolve should release held funds. The available balance should
        // increase, the held balance should decrease, and the total balance
        // should not change.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        client
            .deposit(TransactionId::new(2), Amount::try_from("2.0").unwrap())
            .unwrap();
        check_client(&client, "3.0", "0.0", "3.0", false);

        client.dispute(TransactionId::new(1)).unwrap();
        check_client(&client, "2.0", "1.0", "3.0", false);

        client.resolve(TransactionId::new(1)).unwrap();
        check_client(&client, "3.0", "0.0", "3.0", false);
    }

    #[test]
    fn test_resolve_unknown_transaction_id() {
        let mut client = Client::default();
        assert_eq!(
            client.resolve(TransactionId::new(1)),
            Err(ClientError::UnknownTransactionId)
        );
    }

    #[test]
    fn test_resolve_not_disputed() {
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);
        assert_eq!(
            client.resolve(TransactionId::new(1)),
            Err(ClientError::NotDisputed)
        );
        // The client should be unchanged.
        check_client(&client, "1.0", "0.0", "1.0", false);
    }

    #[test]
    fn test_chargeback() {
        // A chargeback should release held funds to the client, decreasing the
        // held funds and total funds. The account should be frozen to prevent
        // further transactions.
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        client
            .deposit(TransactionId::new(2), Amount::try_from("2.0").unwrap())
            .unwrap();
        check_client(&client, "3.0", "0.0", "3.0", false);

        client.dispute(TransactionId::new(1)).unwrap();
        check_client(&client, "2.0", "1.0", "3.0", false);

        client.chargeback(TransactionId::new(1)).unwrap();
        check_client(&client, "2.0", "0.0", "2.0", true);

        // The account is frozen; any further transactions should fail without
        // changing client state.

        assert_eq!(
            client.deposit(TransactionId::new(3), Amount::try_from("1.0").unwrap()),
            Err(ClientError::Locked)
        );
        check_client(&client, "2.0", "0.0", "2.0", true);

        assert_eq!(
            client.withdraw(Amount::try_from("1.0").unwrap()),
            Err(ClientError::Locked)
        );
        check_client(&client, "2.0", "0.0", "2.0", true);

        assert_eq!(
            client.dispute(TransactionId::new(2)),
            Err(ClientError::Locked)
        );
        check_client(&client, "2.0", "0.0", "2.0", true);

        assert_eq!(
            client.resolve(TransactionId::new(2)),
            Err(ClientError::Locked)
        );
        check_client(&client, "2.0", "0.0", "2.0", true);

        assert_eq!(
            client.chargeback(TransactionId::new(2)),
            Err(ClientError::Locked)
        );
        check_client(&client, "2.0", "0.0", "2.0", true);
    }

    #[test]
    fn test_chargeback_unknown_transaction_id() {
        let mut client = Client::default();
        assert_eq!(
            client.chargeback(TransactionId::new(1)),
            Err(ClientError::UnknownTransactionId)
        );
    }

    #[test]
    fn test_chargeback_not_disputed() {
        let mut client = Client::default();
        client
            .deposit(TransactionId::new(1), Amount::try_from("1.0").unwrap())
            .unwrap();
        check_client(&client, "1.0", "0.0", "1.0", false);
        assert_eq!(
            client.chargeback(TransactionId::new(1)),
            Err(ClientError::NotDisputed)
        );
        // The client should be unchanged.
        check_client(&client, "1.0", "0.0", "1.0", false);
    }

    #[test]
    fn test_deposit_overflow() {
        // A deposit that would cause the total funds to overflow should fail,
        // even if the available funds wouldn't overflow.
        let mut client = Client::default();

        let amount = {
            let mut s = u64::MAX.to_string();
            s.insert(s.len() - 4, '.');
            Amount::try_from(s.as_str()).unwrap()
        };
        client.deposit(TransactionId::new(1), amount).unwrap();
        client.dispute(TransactionId::new(1)).unwrap();
        assert_eq!(
            client.deposit(TransactionId::new(2), Amount::try_from("1.0").unwrap()),
            Err(ClientError::Overflow)
        );
    }
}

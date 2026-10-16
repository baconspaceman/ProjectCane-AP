//! Retry orchestration over a native client's existing check debt.
use std::collections::HashSet;

pub trait CheckTransport {
    type Error;
    fn is_checked(&self, location: i64) -> bool;
    fn send_checks(&mut self, locations: &[i64]) -> Result<(), Self::Error>;
}

/// A failed send retains debt. Retire already accepted checks before retrying, exactly as native
/// clients did before migration. Callers retain save/refusal gating and diagnostic policy.
pub fn flush<T: CheckTransport>(
    pending: &mut HashSet<i64>,
    transport: &mut T,
) -> Result<(), T::Error> {
    pending.retain(|&id| !transport.is_checked(id));
    let mut batch: Vec<_> = pending.iter().copied().collect();
    batch.sort_unstable();
    if batch.is_empty() {
        return Ok(());
    }
    transport.send_checks(&batch)?;
    for id in batch {
        pending.remove(&id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Transport {
        accepted: HashSet<i64>,
        fail: bool,
        sent: Vec<Vec<i64>>,
    }
    impl CheckTransport for Transport {
        type Error = ();
        fn is_checked(&self, id: i64) -> bool {
            self.accepted.contains(&id)
        }
        fn send_checks(&mut self, ids: &[i64]) -> Result<(), ()> {
            self.sent.push(ids.to_vec());
            if self.fail {
                Err(())
            } else {
                self.accepted.extend(ids);
                Ok(())
            }
        }
    }
    #[test]
    fn death_edge_retry_and_reconnect_keep_existing_debt_semantics() {
        let mut debt = HashSet::from([20, 10]);
        let mut transport = Transport {
            accepted: HashSet::from([10]),
            fail: true,
            sent: vec![],
        };
        assert!(flush(&mut debt, &mut transport).is_err());
        assert_eq!(debt, HashSet::from([20]));
        transport.fail = false;
        flush(&mut debt, &mut transport).unwrap();
        assert!(debt.is_empty());
        assert_eq!(transport.sent, [vec![20], vec![20]]);
        debt.insert(20);
        flush(&mut debt, &mut transport).unwrap();
        assert!(debt.is_empty());
        assert_eq!(transport.sent.len(), 2);
    }
}

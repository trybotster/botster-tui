//! Per-route output credit on the Unix connection (S13).
//!
//! The Hub sends a terminal frame only against credit this client granted.
//! A route holds no credit until the Hub demands it for its head frame; the
//! client grants whole frames, first come first served across routes, out of
//! its single pending budget. A route's pool counts what it granted and has
//! not yet dequeued, so frames still queued for the application stay charged.
//!
//! - A demand for an unknown or retired (route, generation) is stale and
//!   ignored. The Hub sends a route's first demand only after its Attach
//!   response, so the route is known by then.
//! - A frame dequeued without pool to cover it is a protocol violation.
//! - `RETURN` gives back part of a pool; `CLOSED` ends the generation and
//!   releases whatever its pool still holds (a grant that crossed the close
//!   included).
//!
//! Input is credited the other way: the Attach response opens a route's input
//! window, `INPUT_CREDIT` returns units as Core consumes frames, and a frame
//! beyond credit waits here in order until credit returns.
//!
//! The ledger only accounts; the caller owns the budget and the socket. Every
//! mutator returns what the caller must release to the budget or send.

use std::collections::{BTreeMap, VecDeque};

/// One route generation, as the Hub names it.
pub type RouteKey = (String, u64);

/// An output grant to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub route: String,
    pub generation: u64,
    pub items: u32,
    pub bytes: u64,
}

/// Budget to give back after a mutation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Release {
    pub items: u64,
    pub bytes: u64,
}

/// Why the connection must close.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreditViolation {
    /// A second demand arrived while the route's first was still unserved.
    SecondDemand(RouteKey),
    /// A terminal frame arrived that no grant covered.
    UncoveredFrame(RouteKey),
    /// The Hub returned more than the route's pool held.
    ReturnExceedsPool(RouteKey),
}

#[derive(Debug, Default)]
struct RouteCredit {
    pool_items: u64,
    pool_bytes: u64,
    demand_pending: bool,
    /// Input frames the Hub will still accept on this route.
    input_credit: u32,
    /// Encoded input frames waiting, in order, for input credit.
    queued_input: VecDeque<Vec<u8>>,
}

#[derive(Debug)]
struct Demand {
    key: RouteKey,
    bytes: u64,
}

#[derive(Debug, Default)]
pub struct CreditLedger {
    routes: BTreeMap<RouteKey, RouteCredit>,
    demands: VecDeque<Demand>,
}

impl CreditLedger {
    /// A route generation the Hub attached, with its initial input window;
    /// demands for it are now accepted.
    pub fn attach(&mut self, route: &str, generation: u64, input_credit: u32) {
        self.routes
            .entry((route.to_string(), generation))
            .or_default()
            .input_credit = input_credit;
    }

    /// Send `frame` on the route now if input credit allows, else queue it
    /// behind earlier queued frames. Returns the frames to write now, in
    /// order, or `None` when the route is not attached.
    pub fn send_input(
        &mut self,
        route: &str,
        generation: u64,
        frame: Vec<u8>,
    ) -> Option<Vec<Vec<u8>>> {
        let credit = self.routes.get_mut(&(route.to_string(), generation))?;
        credit.queued_input.push_back(frame);
        Some(Self::release_input(credit))
    }

    /// The Hub returned `items` input units; returns the queued frames that
    /// may now be written, in order. A return for a closed route is stale.
    pub fn input_returned(&mut self, route: &str, generation: u64, items: u32) -> Vec<Vec<u8>> {
        let Some(credit) = self.routes.get_mut(&(route.to_string(), generation)) else {
            return Vec::new();
        };
        credit.input_credit = credit.input_credit.saturating_add(items);
        Self::release_input(credit)
    }

    fn release_input(credit: &mut RouteCredit) -> Vec<Vec<u8>> {
        let mut ready = Vec::new();
        while credit.input_credit > 0 {
            let Some(frame) = credit.queued_input.pop_front() else {
                break;
            };
            credit.input_credit -= 1;
            ready.push(frame);
        }
        ready
    }

    /// The Hub needs credit for `bytes` (one item) on the route's head frame.
    /// Queues the demand; the caller then serves the queue.
    pub fn demand(
        &mut self,
        route: &str,
        generation: u64,
        bytes: u64,
    ) -> Result<(), CreditViolation> {
        let key = (route.to_string(), generation);
        let Some(credit) = self.routes.get_mut(&key) else {
            // Stale: an unknown or retired generation.
            return Ok(());
        };
        if credit.demand_pending {
            return Err(CreditViolation::SecondDemand(key));
        }
        credit.demand_pending = true;
        self.demands.push_back(Demand { key, bytes });
        Ok(())
    }

    /// Grant demands in arrival order while `reserve` accepts each whole
    /// frame. The head blocks the queue until it fits, so a large frame is
    /// never starved by smaller ones behind it.
    pub fn serve(&mut self, mut reserve: impl FnMut(u64) -> bool) -> Vec<Grant> {
        let mut grants = Vec::new();
        while let Some(head) = self.demands.front() {
            let Some(credit) = self.routes.get_mut(&head.key) else {
                self.demands.pop_front();
                continue;
            };
            if !reserve(head.bytes) {
                break;
            }
            credit.pool_items += 1;
            credit.pool_bytes += head.bytes;
            credit.demand_pending = false;
            let head = self.demands.pop_front().expect("head exists");
            grants.push(Grant {
                route: head.key.0,
                generation: head.key.1,
                items: 1,
                bytes: head.bytes,
            });
        }
        grants
    }

    /// The application dequeued a frame of `bytes`; its charge leaves the
    /// route's pool and returns to the budget.
    pub fn dequeued(
        &mut self,
        route: &str,
        generation: u64,
        bytes: u64,
    ) -> Result<Release, CreditViolation> {
        let key = (route.to_string(), generation);
        let Some(credit) = self.routes.get_mut(&key) else {
            return Err(CreditViolation::UncoveredFrame(key));
        };
        if credit.pool_items == 0 || credit.pool_bytes < bytes {
            return Err(CreditViolation::UncoveredFrame(key));
        }
        credit.pool_items -= 1;
        credit.pool_bytes -= bytes;
        Ok(Release { items: 1, bytes })
    }

    /// The Hub gave back part of a pool it no longer needs.
    pub fn returned(
        &mut self,
        route: &str,
        generation: u64,
        items: u64,
        bytes: u64,
    ) -> Result<Release, CreditViolation> {
        let key = (route.to_string(), generation);
        let Some(credit) = self.routes.get_mut(&key) else {
            // A return for a closed generation was already settled at CLOSED.
            return Ok(Release::default());
        };
        if credit.pool_items < items || credit.pool_bytes < bytes {
            return Err(CreditViolation::ReturnExceedsPool(key));
        }
        credit.pool_items -= items;
        credit.pool_bytes -= bytes;
        Ok(Release { items, bytes })
    }

    /// The Hub ended the generation after its last frame: release what its
    /// pool still holds and forget it, with any demand it still had queued.
    pub fn closed(&mut self, route: &str, generation: u64) -> Release {
        let key = (route.to_string(), generation);
        self.demands.retain(|demand| demand.key != key);
        self.routes
            .remove(&key)
            .map(|credit| Release {
                items: credit.pool_items,
                bytes: credit.pool_bytes,
            })
            .unwrap_or_default()
    }

    /// The connection ended: release every pool and forget every route.
    pub fn reset(&mut self) -> Release {
        self.demands.clear();
        let mut release = Release::default();
        for credit in std::mem::take(&mut self.routes).into_values() {
            release.items += credit.pool_items;
            release.bytes += credit.pool_bytes;
        }
        release
    }

    #[cfg(test)]
    pub fn pool(&self, route: &str, generation: u64) -> Option<(u64, u64)> {
        self.routes
            .get(&(route.to_string(), generation))
            .map(|credit| (credit.pool_items, credit.pool_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A budget of `bytes` free bytes for the tests.
    fn budget(bytes: u64) -> impl FnMut(u64) -> bool {
        let mut free = bytes;
        move |need| {
            if need <= free {
                free -= need;
                true
            } else {
                false
            }
        }
    }

    #[test]
    fn a_demand_for_an_unknown_generation_is_stale() {
        let mut ledger = CreditLedger::default();
        ledger.attach("a", 1, 64);
        assert_eq!(ledger.demand("a", 2, 10), Ok(()));
        assert_eq!(ledger.demand("b", 1, 10), Ok(()));
        assert!(ledger.serve(budget(1_000)).is_empty());
    }

    #[test]
    fn grants_are_whole_frames_in_demand_order_and_the_head_blocks() {
        let mut ledger = CreditLedger::default();
        ledger.attach("big", 1, 64);
        ledger.attach("small", 1, 64);
        ledger.demand("big", 1, 100).expect("demand");
        ledger.demand("small", 1, 10).expect("demand");
        // 50 bytes free: the big head does not fit, and the small demand
        // behind it waits too.
        assert!(ledger.serve(budget(50)).is_empty());
        let grants = ledger.serve(budget(110));
        assert_eq!(
            grants,
            vec![
                Grant {
                    route: "big".to_string(),
                    generation: 1,
                    items: 1,
                    bytes: 100
                },
                Grant {
                    route: "small".to_string(),
                    generation: 1,
                    items: 1,
                    bytes: 10
                },
            ]
        );
        assert_eq!(ledger.pool("big", 1), Some((1, 100)));
    }

    #[test]
    fn a_second_unserved_demand_on_a_route_is_a_violation() {
        let mut ledger = CreditLedger::default();
        ledger.attach("a", 1, 64);
        ledger.demand("a", 1, 10).expect("demand");
        assert_eq!(
            ledger.demand("a", 1, 10),
            Err(CreditViolation::SecondDemand(("a".to_string(), 1)))
        );
        // Once served, the route may demand again (the next head frame).
        assert_eq!(ledger.serve(budget(10)).len(), 1);
        assert_eq!(ledger.demand("a", 1, 20), Ok(()));
    }

    #[test]
    fn dequeued_frames_leave_the_pool_and_uncovered_frames_are_violations() {
        let mut ledger = CreditLedger::default();
        ledger.attach("a", 1, 64);
        ledger.demand("a", 1, 10).expect("demand");
        ledger.serve(budget(10));
        assert_eq!(
            ledger.dequeued("a", 1, 10),
            Ok(Release {
                items: 1,
                bytes: 10
            })
        );
        assert_eq!(ledger.pool("a", 1), Some((0, 0)));
        assert_eq!(
            ledger.dequeued("a", 1, 10),
            Err(CreditViolation::UncoveredFrame(("a".to_string(), 1)))
        );
    }

    #[test]
    fn return_and_close_release_exactly_the_pool() {
        let mut ledger = CreditLedger::default();
        ledger.attach("a", 1, 64);
        for bytes in [10, 20] {
            ledger.demand("a", 1, bytes).expect("demand");
            ledger.serve(budget(bytes));
        }
        assert_eq!(ledger.pool("a", 1), Some((2, 30)));
        assert_eq!(
            ledger.returned("a", 1, 1, 10),
            Ok(Release {
                items: 1,
                bytes: 10
            })
        );
        assert_eq!(
            ledger.returned("a", 1, 5, 5),
            Err(CreditViolation::ReturnExceedsPool(("a".to_string(), 1)))
        );
        // A grant that crossed the close stays in the pool until CLOSED.
        assert_eq!(
            ledger.closed("a", 1),
            Release {
                items: 1,
                bytes: 20
            }
        );
        assert_eq!(ledger.pool("a", 1), None);
        assert_eq!(ledger.closed("a", 1), Release::default());
        assert_eq!(ledger.returned("a", 1, 1, 1), Ok(Release::default()));
    }

    #[test]
    fn close_drops_the_generations_queued_demand() {
        let mut ledger = CreditLedger::default();
        ledger.attach("a", 1, 64);
        ledger.attach("b", 1, 64);
        ledger.demand("a", 1, 100).expect("demand");
        ledger.demand("b", 1, 10).expect("demand");
        ledger.closed("a", 1);
        let grants = ledger.serve(budget(10));
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].route, "b");
    }

    #[test]
    fn input_beyond_credit_waits_in_order_until_credit_returns() {
        let mut ledger = CreditLedger::default();
        assert_eq!(ledger.send_input("a", 1, vec![0]), None, "not attached");
        ledger.attach("a", 1, 2);
        assert_eq!(ledger.send_input("a", 1, vec![1]), Some(vec![vec![1]]));
        assert_eq!(ledger.send_input("a", 1, vec![2]), Some(vec![vec![2]]));
        assert_eq!(
            ledger.send_input("a", 1, vec![3]),
            Some(Vec::new()),
            "no credit"
        );
        assert_eq!(ledger.send_input("a", 1, vec![4]), Some(Vec::new()));
        assert_eq!(ledger.input_returned("a", 1, 1), vec![vec![3]]);
        assert_eq!(ledger.input_returned("a", 1, 5), vec![vec![4]]);
        // Credit left over serves the next frame at once.
        assert_eq!(ledger.send_input("a", 1, vec![5]), Some(vec![vec![5]]));
        ledger.closed("a", 1);
        assert!(
            ledger.input_returned("a", 1, 1).is_empty(),
            "stale after close"
        );
    }

    #[test]
    fn reset_releases_every_pool() {
        let mut ledger = CreditLedger::default();
        for route in ["a", "b"] {
            ledger.attach(route, 1, 64);
            ledger.demand(route, 1, 10).expect("demand");
        }
        ledger.serve(budget(20));
        assert_eq!(
            ledger.reset(),
            Release {
                items: 2,
                bytes: 20
            }
        );
        assert_eq!(ledger.pool("a", 1), None);
    }
}

use soroban_sdk::Vec;

use super::common::setup;
use crate::types::{Loan, Member, SeedDocEntry, SeedNameEntry, SeedStakeEntry, SeedYieldEntry};
use crate::{storage::ProposalKind, Error};

#[test]
fn seed_entrypoints_check_admin_before_seeding_lock() {
    let s = setup(1);
    let outsider = soroban_sdk::Address::generate(&s.env);

    // Lock the seeding window as the admin.
    s.client.lock_seeding(&s.admin);

    let members: Vec<Member> = Vec::new(&s.env);
    let stakes: Vec<SeedStakeEntry> = Vec::new(&s.env);
    let yields: Vec<SeedYieldEntry> = Vec::new(&s.env);
    let loans: Vec<Loan> = Vec::new(&s.env);
    let names: Vec<SeedNameEntry> = Vec::new(&s.env);
    let docs: Vec<SeedDocEntry> = Vec::new(&s.env);

    // Authorization must be checked first, even when seeding is locked.
    assert_eq!(
        s.client.try_seed_members(&outsider, &members),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        s.client.try_seed_stakes(&outsider, &stakes),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        s.client.try_seed_pending_yield(&outsider, &yields),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        s.client.try_seed_loans(&outsider, &loans),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        s.client.try_seed_names(&outsider, &names),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        s.client.try_seed_docs(&outsider, &docs),
        Err(Ok(Error::NotAdmin))
    );

    // Once authorized, the lock error is independently reachable.
    assert_eq!(
        s.client.try_seed_members(&s.admin, &members),
        Err(Ok(Error::SeedingLocked))
    );
    assert_eq!(
        s.client.try_seed_stakes(&s.admin, &stakes),
        Err(Ok(Error::SeedingLocked))
    );
    assert_eq!(
        s.client.try_seed_pending_yield(&s.admin, &yields),
        Err(Ok(Error::SeedingLocked))
    );
    assert_eq!(
        s.client.try_seed_loans(&s.admin, &loans),
        Err(Ok(Error::SeedingLocked))
    );
    assert_eq!(
        s.client.try_seed_names(&s.admin, &names),
        Err(Ok(Error::SeedingLocked))
    );
    assert_eq!(
        s.client.try_seed_docs(&s.admin, &docs),
        Err(Ok(Error::SeedingLocked))
    );

    // Keep ProposalKind referenced here to make it easy to add a populated
    // docs case to this test later.
    let _ = ProposalKind::Loan;
}

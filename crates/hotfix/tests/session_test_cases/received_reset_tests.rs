//! Tests for handling inbound Reset messages.
//!
//! These tests are only concerned with true resets,
//! that is `SequenceReset` messages without the `GapFillFlag` set.
//!
//! These correspond to the test cases in
//! [Scenario 11](https://www.fixtrading.org/standards/fix-session-testcases-online/#scenario-11-receive-sequence-reset-reset).
use tokio::test;

use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;

/// Tests that the session correctly processes an inbound SequenceReset message
/// with `NewSeqNo` higher than the current target sequence number.
///
/// It should set the target sequence number to the new value.
#[test]
async fn test_receive_reset_with_new_seq_number_higher_than_current() {
    const NEW_SEQ_NO: u64 = 10;
    let (mut session, mut counterparty) = given_an_active_session().await;

    when(&mut counterparty)
        .sends_sequence_reset(1, NEW_SEQ_NO)
        .await;
    then(&mut session)
        .target_sequence_number_reaches(NEW_SEQ_NO - 1)
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

//! Tests for handling inbound Reset messages.
//!
//! These tests are only concerned with true resets,
//! that is `SequenceReset` messages without the `GapFillFlag` set.
//!
//! These correspond to the test cases in
//! [Scenario 11](https://www.fixtrading.org/standards/fix-session-testcases-online/#scenario-11-receive-sequence-reset-reset).
use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix_message::fix44::MsgType;
use tokio::test;

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

/// Tests that the reset is processed even when the sequence number is off.
///
/// For example, a sequence number too low would normally result in a termination
/// of the session, but for non-gap fill resets, it is happily accepted.
#[test]
async fn test_sequence_number_is_ignored_in_resets() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // the counterparty sends a message, but due to a failure, it's lost
    let sequence_number = counterparty.next_target_sequence_number();
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut session)
        .target_sequence_number_reaches(sequence_number)
        .await;
    counterparty.delete_last_message_from_store();

    // the counterparty sends a reset with the sequence number being the same
    // as the previous message's
    // the new sequence number is higher than what we currently have
    let new_sequence_number = sequence_number + 10;
    when(&mut counterparty)
        .sends_sequence_reset(sequence_number, new_sequence_number)
        .await;
    then(&mut session)
        .target_sequence_number_reaches(new_sequence_number - 1)
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

#[test]
async fn test_reset_moving_sequence_number_back_is_rejected() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // the counterparty sends a valid message
    let sequence_number = counterparty.next_target_sequence_number();
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut session)
        .target_sequence_number_reaches(sequence_number)
        .await;

    // the counterparty then tries to reset our sequence number back to a value
    // lower than what we think it should be
    when(&mut counterparty)
        .sends_sequence_reset(sequence_number + 1, 1)
        .await;

    // which gets rejected
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Reject))
        .await;

    // but the session remains active, and we're able to process subsequent messages
    let sequence_number = counterparty.next_target_sequence_number();
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut session)
        .target_sequence_number_reaches(sequence_number)
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

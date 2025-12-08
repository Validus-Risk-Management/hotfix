//! Tests for handling inbound gap fill messages.
//!
//! These tests are only concerned with gap fills,
//! that is `SequenceReset` messages with the `GapFillFlag` set to `Y`.
//!
//! These correspond to the test cases in
//! [Scenario 10](https://www.fixtrading.org/standards/fix-session-testcases-online/#scenario-10-receive-sequence-reset-gap-fill).
use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
use hotfix_message::fix44::MsgType;

/// Tests that the session correctly processes an inbound SequenceReset-GapFill message.
#[tokio::test]
async fn test_correct_inbound_sequence_reset_with_gap_fill() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // the counterparty previously sent admin messages (e.g., heartbeats) which we missed
    // we'll simulate this by having them skip sequence numbers 2 and 3
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;

    // the counterparty now sends a business message with sequence number 4
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we detect the gap and transition to AwaitingResend state
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 4,
            attempts: 1,
        })
        .await;

    // we send a ResendRequest to the counterparty
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;

    // the counterparty responds with a SequenceReset-GapFill for messages 2-3
    // indicating that messages 2 and 3 were admin messages that don't need to be resent
    // NewSeqNo=4 means the next message after the gap is sequence number 4
    when(&mut counterparty).sends_gap_fill(2, 4).await;

    // the counterparty also needs to resend message 4 (the business message)
    when(&mut counterparty).resends_message(4).await;

    // the session should process the gap fill and the resent message, then transition back to Active
    then(&mut session).status_changes_to(Status::Active).await;

    // verify that our target sequence number has been updated correctly
    // we should now expect sequence number 5 (after receiving 1=logon, 2-3=gap filled, 4=resent)
    then(&mut session).target_sequence_number_reaches(4).await;

    finally(&session, &mut counterparty).disconnect().await;
}

/// Tests that the session issues a new resend request when the incoming sequence reset's
/// sequence number is too high.
#[tokio::test]
async fn test_sequence_reset_with_sequence_number_too_high_during_resend() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // the counterparty previously sent admin messages (e.g., heartbeats) which we missed
    // we'll simulate this by having them skip sequence numbers 2 and 3
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;

    // the counterparty now sends a business message with sequence number 4
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we detect the gap and transition to AwaitingResend state
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 4,
            attempts: 1,
        })
        .await;

    // we send a ResendRequest to the counterparty for messages 2-4
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;

    // the counterparty responds with a SequenceReset-GapFill, but with an INCORRECT sequence number
    // instead of starting at sequence 2 (the beginning of the gap), it incorrectly starts at 3
    when(&mut counterparty).sends_gap_fill(3, 5).await;

    // the session rejects this invalid gap fill by detecting the sequence number mismatch
    // and requesting another resend for the still-missing message 2
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;

    // verify the session is still in AwaitingResend state, now with updated parameters
    // and incremented attempts (from 1 to 2)
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 3,
            attempts: 2,
        })
        .await;

    // now send the correct gap fill and resend to complete the recovery
    when(&mut counterparty).sends_gap_fill(2, 3).await;
    when(&mut counterparty).resends_message(3).await;
    then(&mut session).status_changes_to(Status::Active).await;

    finally(&session, &mut counterparty).disconnect().await;
}

/// Tests that the session ignores a SequenceReset-GapFill with a sequence number too low.
///
/// This is only true if the sequence reset is otherwise valid with the `PossDupFlag`
/// set to `Y`.
#[tokio::test]
async fn test_reject_sequence_reset_with_sequence_number_too_low_is_ignored_during_resend() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // the counterparty previously sent admin messages (e.g., heartbeats) which we missed
    // we'll simulate this by having them skip sequence numbers 2 and 3
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;

    // the counterparty now sends a business message with sequence number 4
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we detect the gap and transition to AwaitingResend state
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 4,
            attempts: 1,
        })
        .await;

    // we send a ResendRequest to the counterparty for messages 2-4
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;

    // the counterparty responds with a SequenceReset-GapFill, but with an incorrect sequence number
    // instead of starting at sequence 2 (the beginning of the gap), it incorrectly starts at 1
    when(&mut counterparty).sends_gap_fill(1, 4).await;

    // the session ignores this invalid gap fill (logged as PossDup with seq too low)
    // we verify this by checking the state hasn't changed
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 4,
            attempts: 1,
        })
        .await;

    // now send the correct gap fill and resend to complete the recovery
    when(&mut counterparty).sends_gap_fill(2, 4).await;
    when(&mut counterparty).resends_message(4).await;
    then(&mut session).status_changes_to(Status::Active).await;

    finally(&session, &mut counterparty).disconnect().await;
}

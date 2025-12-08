use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
use hotfix_message::fix44::MsgType;

/// Tests that the session correctly processes an inbound SequenceReset-GapFill message.
///
/// This test verifies the workflow where:
/// 1. An active session is established
/// 2. The counterparty previously sent admin messages (e.g., heartbeats) that we missed
/// 3. The counterparty sends a business message with a higher sequence number
/// 4. We detect the gap and send a ResendRequest
/// 5. The counterparty responds with a SequenceReset-GapFill to skip the admin messages
/// 6. We process the gap fill and update our target sequence number correctly
/// 7. The session transitions back to Active status
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

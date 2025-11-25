use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::setup::given_an_active_session;
use hotfix::session::Status;
use hotfix_message::Part;
use hotfix_message::fix44::{MsgType, RESET_SEQ_NUM_FLAG};

/// Tests that we can request the session to reset sequence numbers once.
///
/// This test verifies the workflow where:
/// 1. We have an active session with sequence numbers > 1
/// 2. We request sequence numbers to be reset on next logon as an override
/// 3. We disconnect
/// 4. We reconnect
/// 5. Sequence numbers are reset to 1
#[tokio::test]
async fn test_reset_sequence_numbers_once() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // a message is sent to increment sequence numbers
    when(&session)
        .sends_message(crate::common::test_messages::TestMessage::dummy_execution_report())
        .await;
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ExecutionReport))
        .await;

    // verify sequence numbers are greater than 1
    let session_info = session.session_handle().get_session_info().await.unwrap();
    assert!(
        session_info.next_sender_seq_number > 2,
        "sequence numbers should be incremented"
    );

    // reset on next logon is requested
    session
        .session_handle()
        .request_reset_on_next_logon()
        .await
        .expect("reset request to succeed");

    // the counterparty is disconnected
    when(&session).requests_disconnect().await;
    then(&mut counterparty).gets_disconnected().await;

    // a new connection is established to the counterparty
    when(&mut counterparty).gets_reconnected(true).await;

    // session should send logon with ResetSeqNumFlag=Y
    then(&mut counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::Logon);
            let reset_flag = msg.get::<&str>(RESET_SEQ_NUM_FLAG);
            assert_eq!(reset_flag, Ok("Y"), "ResetSeqNumFlag should be Y");
        })
        .await;

    // counterparty responds with logon
    when(&mut counterparty).sends_logon().await;
    then(&mut session).status_changes_to(Status::Active).await;

    // verify sequence numbers were reset
    let session_info = session.session_handle().get_session_info().await.unwrap();
    assert_eq!(
        session_info.next_sender_seq_number, 2,
        "sender sequence number should be 2 (after the logon)"
    );
    assert_eq!(
        session_info.next_target_seq_number, 2,
        "target sequence number should be 2 (after receiving logon)"
    );

    when(&session).requests_disconnect().await;
    then(&mut counterparty).gets_disconnected().await;
}

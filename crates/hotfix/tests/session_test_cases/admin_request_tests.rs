use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;
use hotfix::session::{SetNextTargetSeqNumError, Status};
use hotfix_message::Part;
use hotfix_message::fix44::{MsgType, RESET_SEQ_NUM_FLAG};
use std::num::NonZeroU64;

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
    finally(&session, &mut counterparty).disconnect().await;

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

    finally(&session, &mut counterparty).disconnect().await;
}

/// Happy path: while `Disconnected`, setting the next expected target sequence
/// number succeeds and the new value is visible via session info.
#[tokio::test]
async fn test_set_next_target_seq_num_while_disconnected() {
    let session = crate::common::setup::given_a_disconnected_session();

    let new_target = NonZeroU64::new(42).expect("42 is non-zero");
    session
        .session_handle()
        .set_next_target_seq_num(new_target)
        .await
        .expect("set_next_target_seq_num to succeed");

    let info = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");
    assert_eq!(info.next_target_seq_number, 42);
}

/// Rejection: while `Active`, SetNextTargetSeqNum is refused and the store is
/// untouched.
#[tokio::test]
async fn test_set_next_target_seq_num_rejected_while_active() {
    let (session, mut counterparty) = crate::common::setup::given_an_active_session().await;

    let info_before = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");

    let result = session
        .session_handle()
        .set_next_target_seq_num(NonZeroU64::new(42).expect("42 is non-zero"))
        .await;

    assert!(
        matches!(
            result,
            Err(SetNextTargetSeqNumError::InvalidState {
                current: Status::Active
            })
        ),
        "expected InvalidState{{ current: Active }}, got: {result:?}"
    );

    let info_after = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");
    assert_eq!(
        info_after.next_target_seq_number, info_before.next_target_seq_number,
        "target sequence number should not change on rejection"
    );

    crate::common::cleanup::finally(&session, &mut counterparty)
        .disconnect()
        .await;
}

/// Rejection: while `AwaitingLogon` (we've sent our Logon, peer hasn't responded),
/// SetNextTargetSeqNum is refused and the store is untouched.
#[tokio::test]
async fn test_set_next_target_seq_num_rejected_while_awaiting_logon() {
    let (session, mut counterparty) = crate::common::setup::given_a_connected_session().await;

    // wait for our outbound Logon so we're deterministically in AwaitingLogon
    crate::common::assertions::then(&mut counterparty)
        .receives(|msg| {
            crate::common::assertions::assert_msg_type(msg, hotfix_message::fix44::MsgType::Logon)
        })
        .await;

    let info_before = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");

    let result = session
        .session_handle()
        .set_next_target_seq_num(NonZeroU64::new(42).expect("42 is non-zero"))
        .await;

    assert!(
        matches!(
            result,
            Err(SetNextTargetSeqNumError::InvalidState {
                current: Status::AwaitingLogon
            })
        ),
        "expected InvalidState{{ current: AwaitingLogon }}, got: {result:?}"
    );

    let info_after = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");
    assert_eq!(
        info_after.next_target_seq_number,
        info_before.next_target_seq_number
    );

    crate::common::cleanup::finally(&session, &mut counterparty)
        .disconnect()
        .await;
}

/// Rejection: while `AwaitingLogout` (we've sent our Logout and are waiting
/// for the peer's reply), SetNextTargetSeqNum is refused and the store is
/// untouched.
#[tokio::test]
async fn test_set_next_target_seq_num_rejected_while_awaiting_logout() {
    use crate::common::actions::when;
    use crate::common::assertions::{assert_msg_type, then};
    use hotfix::message::logout::Logout;
    use hotfix_message::fix44::MsgType;

    let (session, mut counterparty) = crate::common::setup::given_an_active_session().await;

    // initiate logout from our side — we stay in AwaitingLogout until the peer replies
    when(&session).requests_disconnect().await;
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;

    let info_before = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");

    let result = session
        .session_handle()
        .set_next_target_seq_num(NonZeroU64::new(42).expect("42 is non-zero"))
        .await;

    assert!(
        matches!(
            result,
            Err(SetNextTargetSeqNumError::InvalidState {
                current: Status::AwaitingLogout
            })
        ),
        "expected InvalidState{{ current: AwaitingLogout }}, got: {result:?}"
    );

    let info_after = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");
    assert_eq!(
        info_after.next_target_seq_number,
        info_before.next_target_seq_number
    );

    // let the peer reply so the session cleans up (do NOT call finally().disconnect()
    // — we're already in logout).
    when(&mut counterparty)
        .sends_message(Logout::default())
        .await;
    then(&mut counterparty).gets_disconnected().await;
}

/// Rejection: while `AwaitingResend` (we detected a gap and asked the peer to
/// resend), SetNextTargetSeqNum is refused and the store is untouched.
#[tokio::test]
async fn test_set_next_target_seq_num_rejected_while_awaiting_resend() {
    use crate::common::actions::when;
    use crate::common::test_messages::TestMessage;

    let (mut session, mut counterparty) = crate::common::setup::given_an_active_session().await;

    // create a gap so the session transitions to AwaitingResend
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    crate::common::assertions::then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 3,
            attempts: 1,
        })
        .await;
    crate::common::assertions::then(&mut counterparty)
        .receives(|msg| {
            crate::common::assertions::assert_msg_type(
                msg,
                hotfix_message::fix44::MsgType::ResendRequest,
            )
        })
        .await;

    let info_before = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");

    let result = session
        .session_handle()
        .set_next_target_seq_num(NonZeroU64::new(42).expect("42 is non-zero"))
        .await;

    assert!(
        matches!(
            result,
            Err(SetNextTargetSeqNumError::InvalidState {
                current: Status::AwaitingResend { .. }
            })
        ),
        "expected InvalidState{{ current: AwaitingResend{{..}} }}, got: {result:?}"
    );

    let info_after = session
        .session_handle()
        .get_session_info()
        .await
        .expect("session info");
    assert_eq!(
        info_after.next_target_seq_number,
        info_before.next_target_seq_number
    );

    crate::common::cleanup::finally(&session, &mut counterparty)
        .disconnect()
        .await;
}

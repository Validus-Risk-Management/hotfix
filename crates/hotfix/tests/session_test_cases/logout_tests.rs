use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::setup::{LOGOUT_TIMEOUT, given_an_active_session};
use hotfix::message::logout::Logout;
use hotfix_message::fix44::MsgType;
use std::time::Duration;

/// Test a successful logout flow where we initiate the logout:
/// 1. Establish an active session
/// 2. We send a logout message
/// 3. Counterparty responds with a logout acknowledgement
/// 4. Verify that the connection is cleanly disconnected
///
/// This test ensures the proper FIX protocol logout sequence where
/// the session initiates the logout.
#[tokio::test]
async fn test_happy_logout_initiated_by_us() {
    let (session, mut counterparty) = given_an_active_session().await;

    // when we send a logout message
    when(&session).requests_disconnect().await;

    // then the counterparty receives a logout message
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;

    // and when the counterparty acknowledges the logout
    when(&mut counterparty)
        .sends_message(Logout::default())
        .await;

    // then disconnection occurs
    then(&mut counterparty).gets_disconnected().await;
}

/// Test a successful logout flow where the counterparty initiates the logout:
/// 1. Establish an active session
/// 2. Counterparty sends a logout message
/// 3. Verify that the session responds with a logout acknowledgement
/// 4. Verify that the connection is cleanly disconnected
///
/// This test ensures the proper FIX protocol logout sequence where
/// the session responds to a counterparty-initiated logout.
#[tokio::test]
async fn test_happy_logout_initiated_by_counterparty() {
    let (_session, mut counterparty) = given_an_active_session().await;

    // when the counterparty initiates logout
    when(&mut counterparty)
        .sends_message(Logout::default())
        .await;

    // then our session responds with logout acknowledgement
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;

    // then disconnection occurs
    then(&mut counterparty).gets_disconnected().await;
}

/// Test a logout flow where we initiate the logout and the counterparty does not respond:
/// 1. Establish an active session
/// 2. We send a logout message
/// 3. Counterparty does not respond within the logout timeout period
/// 4. Verify that the connection is cleanly disconnected regardless
#[tokio::test(start_paused = true)]
async fn test_logout_timeout_is_handled() {
    let (session, mut counterparty) = given_an_active_session().await;

    // when we send a logout message
    when(&session).requests_disconnect().await;

    // then the counterparty receives a logout message
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;

    // when enough time elapses to exceed the allowed logout timeout
    when(Duration::from_secs(LOGOUT_TIMEOUT)).elapses().await;

    // then disconnection occurs
    then(&mut counterparty).gets_disconnected().await;
}

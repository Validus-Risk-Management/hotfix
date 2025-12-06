use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::setup::given_an_active_session;
use hotfix::message::logout::Logout;
use hotfix_message::fix44::MsgType;

/// Tests successful logout flow when counterparty initiates logout:
/// 1. Establishes an active session
/// 2. Counterparty sends a logout message
/// 3. Verifies that session responds with a logout acknowledgment
/// 4. Verifies that the connection is cleanly disconnected
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

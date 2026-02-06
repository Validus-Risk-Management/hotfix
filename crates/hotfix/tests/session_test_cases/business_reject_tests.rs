use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::cleanup::finally;
use crate::common::fakes::FakeApplication;
use crate::common::setup::given_an_active_session_with_app;
use crate::common::test_messages::TestMessage;
use hotfix::application::{BusinessRejectReason, InboundDecision};
use hotfix_message::Part;
use hotfix_message::fix44::{MSG_TYPE, REF_MSG_TYPE, REF_SEQ_NUM, TEXT};

/// Tests that when the application returns InboundDecision::Reject,
/// the session sends a Business Message Reject (MsgType "j") back to the counterparty.
#[tokio::test]
async fn test_inbound_reject_sends_business_message_reject() {
    let (message_tx, message_rx) = tokio::sync::mpsc::unbounded_channel();
    let app = FakeApplication::builder(message_tx)
        .inbound_decision_fn(|_| InboundDecision::Reject {
            reason: BusinessRejectReason::NotAuthorized,
            text: Some("Not authorized for this message".to_string()),
        })
        .build();
    let (session, mut counterparty) = given_an_active_session_with_app(app, message_rx).await;

    // counterparty sends an execution report
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // session should respond with a Business Message Reject (MsgType "j")
    then(&mut counterparty)
        .receives(|msg| {
            let msg_type: &str = msg.header().get(MSG_TYPE).unwrap();
            assert_eq!(msg_type, "j");

            // RefMsgType should be the original message type ("8" for ExecutionReport)
            let ref_msg_type: &str = msg.get(REF_MSG_TYPE).unwrap();
            assert_eq!(ref_msg_type, "8");

            // BusinessRejectReason (tag 380) should be 6 (NotAuthorized)
            let reason: u32 = msg.get(BUSINESS_REJECT_REASON).unwrap();
            assert_eq!(reason, 6);

            // RefSeqNum should be the sequence number of the rejected message
            let ref_seq_num: u64 = msg.get(REF_SEQ_NUM).unwrap();
            assert!(ref_seq_num > 0);

            // Text should contain our reject reason
            let text: &str = msg.get(TEXT).unwrap();
            assert_eq!(text, "Not authorized for this message");
        })
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

/// Tests that when the application returns InboundDecision::Reject without text,
/// the Business Message Reject is sent without the Text field.
#[tokio::test]
async fn test_inbound_reject_without_text() {
    let (message_tx, message_rx) = tokio::sync::mpsc::unbounded_channel();
    let app = FakeApplication::builder(message_tx)
        .inbound_decision_fn(|_| InboundDecision::Reject {
            reason: BusinessRejectReason::UnsupportedMessageType,
            text: None,
        })
        .build();
    let (session, mut counterparty) = given_an_active_session_with_app(app, message_rx).await;

    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    then(&mut counterparty)
        .receives(|msg| {
            let msg_type: &str = msg.header().get(MSG_TYPE).unwrap();
            assert_eq!(msg_type, "j");

            let reason: u32 = msg.get(BUSINESS_REJECT_REASON).unwrap();
            assert_eq!(reason, 3);

            // Text field should not be present
            assert!(msg.get::<&str>(TEXT).is_err());
        })
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

/// Field definition for BusinessRejectReason (tag 380), used for assertions only.
const BUSINESS_REJECT_REASON: &hotfix_message::HardCodedFixFieldDefinition =
    &hotfix_message::HardCodedFixFieldDefinition {
        name: "BusinessRejectReason",
        tag: 380,
        data_type: hotfix_message::dict::FixDatatype::Int,
        location: hotfix_message::dict::FieldLocation::Body,
    };

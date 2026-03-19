use crate::message::logout::Logout;
use crate::message::reject::Reject;
use crate::message::verification::verify_message;
use crate::message::verification_error::{CompIdType, MessageVerificationError};
use crate::session::ctx::{SessionCtx, TransitionResult};
use crate::session::outbound;
use crate::session::state::SessionState;
use crate::transport::writer::WriterRef;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{MSG_SEQ_NUM, SessionRejectReason};
use hotfix_store::MessageStore;
use tracing::error;
use tracing::warn;

pub(crate) fn verify_message_with_ctx<A, S: MessageStore>(
    ctx: &SessionCtx<A, S>,
    message: &Message,
    check_too_high: bool,
    check_too_low: bool,
) -> Result<(), MessageVerificationError> {
    let expected_seq_number = if check_too_high || check_too_low {
        Some(ctx.store.next_target_seq_number())
    } else {
        None
    };
    verify_message(
        message,
        &ctx.config,
        expected_seq_number,
        check_too_high,
        check_too_low,
    )
}

pub(crate) async fn handle_sending_time_accuracy_problem<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    msg_seq_num: u64,
    text: &str,
) {
    let reject = Reject::new(msg_seq_num)
        .session_reject_reason(SessionRejectReason::SendingtimeAccuracyProblem)
        .text(text);
    if let Err(err) = outbound::send_message(ctx, writer, reject).await {
        error!("failed to send reject for time accuracy problem: {err}");
    }
    if let Err(err) = ctx.store.increment_target_seq_number().await {
        error!("failed to increment target seq number: {:?}", err);
    }
}

pub(crate) async fn handle_incorrect_begin_string<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    received_begin_string: String,
) -> TransitionResult {
    let logout = Logout::with_reason(format!(
        "beginString={received_begin_string} is not supported"
    ));
    match ctx.prepare_message(logout).await {
        Ok(prepared) => writer.send_raw_message(prepared.raw).await,
        Err(err) => warn!("failed to send logout for incorrect begin string: {err}"),
    }
    writer.disconnect().await;
    TransitionResult::TransitionTo(SessionState::new_disconnected(
        true,
        "incorrect begin string",
    ))
}

pub(crate) async fn handle_incorrect_comp_id<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    received_comp_id: String,
    comp_id_type: CompIdType,
    msg_seq_num: u64,
) -> TransitionResult {
    error!("rejecting message with incorrect comp ID: {received_comp_id} (type: {comp_id_type:?})");
    let reject = Reject::new(msg_seq_num)
        .session_reject_reason(SessionRejectReason::ValueIsIncorrect)
        .text(&format!("invalid comp ID {received_comp_id}"));
    if let Err(err) = outbound::send_message(ctx, writer, reject).await {
        error!("failed to send reject message with invalid comp ID: {err}");
    }
    let logout = Logout::with_reason("incorrect comp ID received".to_string());
    match ctx.prepare_message(logout).await {
        Ok(prepared) => writer.send_raw_message(prepared.raw).await,
        Err(err) => warn!("failed to send logout for incorrect comp ID: {err}"),
    }
    writer.disconnect().await;
    TransitionResult::TransitionTo(SessionState::new_disconnected(true, "incorrect comp ID"))
}

pub(crate) async fn handle_sequence_number_too_low<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    expected: u64,
    actual: u64,
    possible_duplicate: bool,
) -> TransitionResult {
    if possible_duplicate {
        warn!(
            "sequence number too low (expected {expected}, actual {actual}, but counterparty indicated it's poss duplicate, ignoring"
        );
        return TransitionResult::Stay;
    }
    error!(
        "we expected {expected} sequence number, but target sent lower ({actual}), terminating..."
    );
    let reason = format!("sequence number too low (actual {actual}, expected {expected})");
    let logout = Logout::with_reason(reason.clone());
    match ctx.prepare_message(logout).await {
        Ok(prepared) => writer.send_raw_message(prepared.raw).await,
        Err(err) => warn!("failed to send logout for sequence number too low: {err}"),
    }
    writer.disconnect().await;
    TransitionResult::TransitionTo(SessionState::new_disconnected(false, &reason))
}

pub(crate) async fn handle_invalid_msg_type<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    message: &Message,
    msg_type: &str,
) {
    match message.header().get(MSG_SEQ_NUM) {
        Ok(msg_seq_num) => {
            let reject = Reject::new(msg_seq_num)
                .session_reject_reason(SessionRejectReason::InvalidMsgtype)
                .text(&format!("invalid message type {msg_type}"));
            if let Err(err) = outbound::send_message(ctx, writer, reject).await {
                error!("failed to send reject message for invalid msgtype: {err}");
            }

            #[allow(clippy::collapsible_if)]
            if let Ok(seq_num) = message.header().get::<u64>(MSG_SEQ_NUM)
                && ctx.store.next_target_seq_number() == seq_num
            {
                if let Err(err) = ctx.store.increment_target_seq_number().await {
                    error!("failed to increment target seq number: {:?}", err);
                }
            }
        }
        Err(err) => {
            error!("failed to get message seq num: {:?}", err);
        }
    }
}

pub(crate) async fn handle_original_sending_time_missing<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    msg_seq_num: u64,
) {
    let reject = Reject::new(msg_seq_num)
        .session_reject_reason(SessionRejectReason::RequiredTagMissing)
        .text("original sending time is required");
    if let Err(err) = outbound::send_message(ctx, writer, reject).await {
        error!("failed to send reject for time missing tag: {err}");
    }
    if let Err(err) = ctx.store.increment_target_seq_number().await {
        error!("failed to increment target seq number: {:?}", err);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::test_utils::{
        FakeMessageStore, create_test_ctx, create_writer, extract_field, extract_msg_type,
    };
    use crate::transport::writer::WriterMessage;

    #[tokio::test]
    async fn handle_incorrect_begin_string_returns_transition_to_disconnected() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        let result = handle_incorrect_begin_string(&mut ctx, &writer, "FIX.4.0".to_string()).await;

        assert!(matches!(
            result,
            TransitionResult::TransitionTo(SessionState::Disconnected(_))
        ));

        // Should send a Logout containing the bad begin string, then disconnect
        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("5"));
                let text = extract_field(raw.as_bytes(), 58).expect("expected Text(58) field");
                assert!(
                    text.contains("FIX.4.0"),
                    "logout text should mention the bad begin string, got: {text}"
                );
            }
            _ => panic!("expected SendMessage, got {msg:?}"),
        }
        assert!(matches!(
            rx.recv().await.unwrap(),
            WriterMessage::Disconnect
        ));

        // Sender seq number should have been incremented for the logout
        assert_eq!(ctx.store.next_sender_seq, 2);
    }

    #[tokio::test]
    async fn handle_incorrect_comp_id_returns_transition_to_disconnected() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        let result = handle_incorrect_comp_id(
            &mut ctx,
            &writer,
            "BAD_COMP".to_string(),
            CompIdType::Sender,
            1,
        )
        .await;

        assert!(matches!(
            result,
            TransitionResult::TransitionTo(SessionState::Disconnected(_))
        ));

        // First message: Reject (35=3) mentioning the bad comp ID
        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("3"));
                let text = extract_field(raw.as_bytes(), 58).expect("expected Text(58) field");
                assert!(
                    text.contains("BAD_COMP"),
                    "reject text should mention the bad comp ID, got: {text}"
                );
            }
            _ => panic!("expected SendMessage(Reject), got {msg:?}"),
        }

        // Second message: Logout (35=5)
        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("5"));
            }
            _ => panic!("expected SendMessage(Logout), got {msg:?}"),
        }

        // Third: Disconnect
        assert!(matches!(
            rx.recv().await.unwrap(),
            WriterMessage::Disconnect
        ));

        // Sender seq incremented twice (reject + logout)
        assert_eq!(ctx.store.next_sender_seq, 3);
    }

    #[tokio::test]
    async fn handle_sequence_number_too_low_possible_duplicate_returns_stay() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        let result = handle_sequence_number_too_low(&mut ctx, &writer, 5, 1, true).await;

        assert!(matches!(result, TransitionResult::Stay));

        // No messages should have been sent
        assert!(rx.try_recv().is_err());

        // Store should be untouched
        assert_eq!(ctx.store.next_sender_seq, 1);
        assert_eq!(ctx.store.next_target_seq, 1);
    }

    #[tokio::test]
    async fn handle_sequence_number_too_low_returns_transition_to_disconnected_without_reconnect() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        let result = handle_sequence_number_too_low(&mut ctx, &writer, 5, 1, false).await;

        match result {
            TransitionResult::TransitionTo(state) => {
                assert!(!state.should_reconnect());
            }
            TransitionResult::Stay => panic!("expected TransitionTo(Disconnected)"),
        }

        // Should send a Logout mentioning the sequence mismatch, then disconnect
        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("5"));
                let text = extract_field(raw.as_bytes(), 58).expect("expected Text(58) field");
                assert!(
                    text.contains("5") && text.contains("1"),
                    "logout text should mention expected/actual seq nums, got: {text}"
                );
            }
            _ => panic!("expected SendMessage(Logout), got {msg:?}"),
        }
        assert!(matches!(
            rx.recv().await.unwrap(),
            WriterMessage::Disconnect
        ));

        assert_eq!(ctx.store.next_sender_seq, 2);
    }

    #[tokio::test]
    async fn handle_sending_time_accuracy_problem_sends_reject() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        handle_sending_time_accuracy_problem(&mut ctx, &writer, 42, "bad time").await;

        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("3"));
                let text = extract_field(raw.as_bytes(), 58).expect("expected Text(58) field");
                assert!(
                    text.contains("bad time"),
                    "reject text should contain the provided text, got: {text}"
                );
            }
            _ => panic!("expected SendMessage(Reject), got {msg:?}"),
        }

        // Target seq number should have been incremented
        assert_eq!(ctx.store.next_target_seq, 2);
        // Sender seq number should have been incremented for the outbound reject
        assert_eq!(ctx.store.next_sender_seq, 2);
    }

    #[tokio::test]
    async fn handle_original_sending_time_missing_sends_reject() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        handle_original_sending_time_missing(&mut ctx, &writer, 7).await;

        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("3"));
                let text = extract_field(raw.as_bytes(), 58).expect("expected Text(58) field");
                assert!(
                    text.contains("original sending time"),
                    "reject text should mention original sending time, got: {text}"
                );
            }
            _ => panic!("expected SendMessage(Reject), got {msg:?}"),
        }

        // Both sender and target seq numbers should have been incremented
        assert_eq!(ctx.store.next_sender_seq, 2);
        assert_eq!(ctx.store.next_target_seq, 2);
    }

    #[tokio::test]
    async fn handle_invalid_msg_type_sends_reject_for_message_with_seq_num() {
        let mut ctx = create_test_ctx(FakeMessageStore::new());
        let (writer, mut rx) = create_writer();

        let mut message = Message::new("FIX.4.4", "ZZ");
        message.header_mut().set(MSG_SEQ_NUM, 1u64);

        handle_invalid_msg_type(&mut ctx, &writer, &message, "ZZ").await;

        let msg = rx.recv().await.unwrap();
        match &msg {
            WriterMessage::SendMessage(raw) => {
                assert_eq!(extract_msg_type(raw.as_bytes()).as_deref(), Some("3"));
                let text = extract_field(raw.as_bytes(), 58).expect("expected Text(58) field");
                assert!(
                    text.contains("ZZ"),
                    "reject text should mention the invalid msg type, got: {text}"
                );
            }
            _ => panic!("expected SendMessage(Reject), got {msg:?}"),
        }

        // Sender seq incremented for the reject, target seq incremented because msg seq matched
        assert_eq!(ctx.store.next_sender_seq, 2);
        assert_eq!(ctx.store.next_target_seq, 2);
    }
}

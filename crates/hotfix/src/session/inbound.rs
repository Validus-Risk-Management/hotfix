use crate::message::verification::verify_message;
use crate::message::verification_error::MessageVerificationError;
use crate::session::ctx::SessionCtx;
use hotfix_message::message::Message;
use hotfix_store::MessageStore;

pub(crate) fn verify_message_with_ctx<A, S>(
    ctx: &SessionCtx<A, S>,
    message: &Message,
    check_too_high: bool,
    check_too_low: bool,
) -> Result<(), MessageVerificationError>
where
    S: MessageStore,
{
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

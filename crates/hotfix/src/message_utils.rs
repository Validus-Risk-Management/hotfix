// TODO: should these be part of message generation?

static ADMIN_TYPES: [&str; 7] = ["A", "0", "1", "2", "3", "4", "5"];

pub fn is_admin(message_type: &str) -> bool {
    ADMIN_TYPES.contains(&message_type)
}

//! Pinned TurbineChatHandler channel/type adjustment, independent of delivery policy.
use bace_gameplay_api::social::SocialError;
pub fn adjust_turbine_channel(
    dispatch: u32,
    channel: u32,
    chat_type: u32,
) -> Result<(u32, u32), SocialError> {
    match dispatch {
        1 => {
            let channel = match chat_type {
                1..=5 | 10 => chat_type,
                6..=9 => 6,
                _ => 2,
            };
            Ok((channel, channel))
        }
        2 => Ok((
            channel,
            match channel {
                1 | 11.. => 1,
                10 => 10,
                6..=9 => 6,
                2..=5 => channel,
                _ => chat_type,
            },
        )),
        _ => Err(SocialError::Invalid),
    }
}

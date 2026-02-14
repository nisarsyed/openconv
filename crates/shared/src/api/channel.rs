use crate::ids::{ChannelId, GuildId};
use serde::{Deserialize, Serialize};

/// Request to create a new channel in a guild.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: String,
}

/// Channel details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelResponse {
    pub id: ChannelId,
    pub guild_id: GuildId,
    pub name: String,
    pub channel_type: String,
    pub position: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_response_channel_type() {
        let resp = ChannelResponse {
            id: ChannelId::new(),
            guild_id: GuildId::new(),
            name: "general".into(),
            channel_type: "text".into(),
            position: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: ChannelResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel_type, "text");
    }
}

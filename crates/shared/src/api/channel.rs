use crate::ids::{ChannelId, GuildId};
use serde::{Deserialize, Serialize};

/// Request to create a new channel in a guild.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: String,
}

/// Request to update an existing channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
}

/// Request to reorder channels within a guild.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ReorderChannelsRequest {
    pub channels: Vec<ChannelPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChannelPosition {
    pub channel_id: ChannelId,
    pub position: i32,
}

/// Channel details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChannelResponse {
    pub id: ChannelId,
    pub guild_id: GuildId,
    pub name: String,
    pub channel_type: String,
    pub position: i32,
    pub topic: Option<String>,
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
            topic: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: ChannelResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel_type, "text");
    }

    #[test]
    fn update_channel_request_serde() {
        let req = UpdateChannelRequest {
            name: Some("new-name".into()),
            topic: Some("A topic".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: UpdateChannelRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name.unwrap(), "new-name");
        assert_eq!(back.topic.unwrap(), "A topic");
    }

    #[test]
    fn reorder_channels_request_serde() {
        let req = ReorderChannelsRequest {
            channels: vec![
                ChannelPosition {
                    channel_id: ChannelId::new(),
                    position: 0,
                },
                ChannelPosition {
                    channel_id: ChannelId::new(),
                    position: 1,
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ReorderChannelsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channels.len(), 2);
    }
}

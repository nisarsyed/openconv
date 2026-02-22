use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "OpenConv API",
        version = "0.1.0",
        description = "End-to-end encrypted messaging platform API"
    ),
    paths(
        // Health
        crate::handlers::health::liveness,
        crate::handlers::health::readiness,
        // Auth
        crate::handlers::auth::register_start,
        crate::handlers::auth::register_verify,
        crate::handlers::auth::register_complete,
        crate::handlers::auth::challenge,
        crate::handlers::auth::login_verify,
        crate::handlers::auth::refresh,
        crate::handlers::auth::logout,
        crate::handlers::auth::logout_all,
        crate::handlers::auth::list_devices,
        crate::handlers::auth::revoke_device,
        crate::handlers::auth::recover_start,
        crate::handlers::auth::recover_verify,
        crate::handlers::auth::recover_complete,
        // Users
        crate::handlers::users::get_me,
        crate::handlers::users::update_me,
        crate::handlers::users::get_user,
        crate::handlers::users::search_users,
        crate::handlers::users::get_prekeys,
        crate::handlers::users::upload_prekeys,
        // Guilds
        crate::handlers::guilds::create_guild,
        crate::handlers::guilds::list_guilds,
        crate::handlers::guilds::get_guild,
        crate::handlers::guilds::update_guild,
        crate::handlers::guilds::delete_guild,
        crate::handlers::guilds::restore_guild,
        crate::handlers::guilds::leave_guild,
        crate::handlers::guilds::kick_member,
        crate::handlers::guilds::list_members,
        // Channels
        crate::handlers::channels::create_channel,
        crate::handlers::channels::list_channels,
        crate::handlers::channels::get_channel,
        crate::handlers::channels::update_channel,
        crate::handlers::channels::delete_channel,
        crate::handlers::channels::reorder_channels,
        // Roles
        crate::handlers::roles::create_role,
        crate::handlers::roles::list_roles,
        crate::handlers::roles::update_role,
        crate::handlers::roles::delete_role,
        crate::handlers::roles::assign_role,
        crate::handlers::roles::remove_role,
        // Invites
        crate::handlers::invites::create_invite,
        crate::handlers::invites::list_invites,
        crate::handlers::invites::revoke_invite,
        crate::handlers::invites::get_invite_info,
        crate::handlers::invites::accept_invite,
        // DM Channels
        crate::handlers::dm_channels::create,
        crate::handlers::dm_channels::list,
        crate::handlers::dm_channels::get_one,
        crate::handlers::dm_channels::add_member,
        crate::handlers::dm_channels::leave,
        crate::handlers::dm_channels::messages,
        // Messages
        crate::handlers::messages::guild_messages,
        // Files
        crate::handlers::files::upload,
        crate::handlers::files::upload_dm,
        crate::handlers::files::download,
        crate::handlers::files::meta,
        // WebSocket
        crate::handlers::ws::create_ws_ticket,
        crate::handlers::ws::ws_upgrade,
    ),
    components(schemas(
        // Error
        crate::error::ErrorResponse,
        // IDs
        openconv_shared::ids::UserId,
        openconv_shared::ids::GuildId,
        openconv_shared::ids::ChannelId,
        openconv_shared::ids::MessageId,
        openconv_shared::ids::RoleId,
        openconv_shared::ids::FileId,
        openconv_shared::ids::DmChannelId,
        openconv_shared::ids::DeviceId,
        // Auth
        openconv_shared::api::auth::RegisterStartRequest,
        openconv_shared::api::auth::RegisterStartResponse,
        openconv_shared::api::auth::RegisterVerifyRequest,
        openconv_shared::api::auth::RegisterVerifyResponse,
        openconv_shared::api::auth::RegisterCompleteRequest,
        openconv_shared::api::auth::RegisterResponse,
        openconv_shared::api::auth::LoginChallengeRequest,
        openconv_shared::api::auth::LoginChallengeResponse,
        openconv_shared::api::auth::LoginVerifyRequest,
        openconv_shared::api::auth::LoginVerifyResponse,
        openconv_shared::api::auth::RefreshRequest,
        openconv_shared::api::auth::RefreshResponse,
        openconv_shared::api::auth::RecoverStartRequest,
        openconv_shared::api::auth::RecoverStartResponse,
        openconv_shared::api::auth::RecoverVerifyRequest,
        openconv_shared::api::auth::RecoverVerifyResponse,
        openconv_shared::api::auth::RecoverCompleteRequest,
        openconv_shared::api::auth::RecoverCompleteResponse,
        openconv_shared::api::auth::DeviceInfo,
        openconv_shared::api::auth::DevicesListResponse,
        // Guild
        openconv_shared::api::guild::CreateGuildRequest,
        openconv_shared::api::guild::UpdateGuildRequest,
        openconv_shared::api::guild::GuildResponse,
        openconv_shared::api::guild::GuildListResponse,
        openconv_shared::api::guild::GuildMemberResponse,
        openconv_shared::api::guild::RoleSummary,
        // Channel
        openconv_shared::api::channel::CreateChannelRequest,
        openconv_shared::api::channel::UpdateChannelRequest,
        openconv_shared::api::channel::ReorderChannelsRequest,
        openconv_shared::api::channel::ChannelPosition,
        openconv_shared::api::channel::ChannelResponse,
        // Role
        openconv_shared::api::role::CreateRoleRequest,
        openconv_shared::api::role::UpdateRoleRequest,
        openconv_shared::api::role::RoleResponse,
        // Invite
        openconv_shared::api::invite::CreateInviteRequest,
        openconv_shared::api::invite::InviteResponse,
        openconv_shared::api::invite::InviteInfoResponse,
        // DM Channel
        openconv_shared::api::dm_channel::CreateDmChannelRequest,
        openconv_shared::api::dm_channel::DmChannelResponse,
        openconv_shared::api::dm_channel::AddDmMemberRequest,
        // File
        openconv_shared::api::file::FileResponse,
        openconv_shared::api::file::FileMetaResponse,
        crate::handlers::files::FileUploadBody,
        // Message
        openconv_shared::api::message::SendMessageRequest,
        openconv_shared::api::message::MessageResponse,
        openconv_shared::api::message::MessageHistoryQuery,
        openconv_shared::api::message::MessageHistoryResponse,
        // WS
        openconv_shared::api::ws::PresenceStatus,
        openconv_shared::api::ws::ClientMessage,
        openconv_shared::api::ws::ServerMessage,
        // Server-local
        crate::handlers::users::UserProfileResponse,
        crate::handlers::users::PublicProfileResponse,
        crate::handlers::users::UpdateProfileRequest,
        crate::handlers::users::SearchUsersQuery,
        crate::handlers::users::SearchUsersResponse,
        crate::handlers::users::UploadPreKeysRequest,
        crate::handlers::users::PreKeyBundleResponse,
        crate::handlers::dm_channels::MessageQuery,
        crate::handlers::dm_channels::MessagePage,
        crate::handlers::dm_channels::MessageResponse,
        crate::handlers::ws::WsQueryParams,
        crate::handlers::ws::TicketResponse,
    )),
    tags(
        (name = "Health", description = "Health check endpoints"),
        (name = "Auth", description = "Authentication and registration"),
        (name = "Users", description = "User profiles and pre-keys"),
        (name = "Guilds", description = "Guild management and membership"),
        (name = "Channels", description = "Channel CRUD and reordering"),
        (name = "Roles", description = "Role management and assignment"),
        (name = "Invites", description = "Guild invite management"),
        (name = "DM Channels", description = "Direct message channels"),
        (name = "Messages", description = "Message history"),
        (name = "Files", description = "Encrypted file upload and download"),
        (name = "WebSocket", description = "WebSocket ticket and upgrade"),
    ),
    modifiers(&BearerAuth),
)]
pub struct ApiDoc;

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

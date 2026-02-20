import { useState } from "react";
import { useParams } from "react-router";
import { useAppStore } from "../../store";
import { SettingsLayout } from "./SettingsLayout";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { Avatar } from "../ui/Avatar";
import { Badge } from "../ui/Badge";
import { Select } from "../ui/Select";

function OverviewSection({ guildId }: { guildId: string }) {
  const guild = useAppStore((s) => s.guildsById[guildId]);
  const updateGuild = useAppStore((s) => s.updateGuild);

  const [name, setName] = useState(guild?.name ?? "");

  if (!guild)
    return <div className="text-[var(--text-muted)]">Guild not found</div>;

  const hasChanges = name !== guild.name;

  function handleSave() {
    if (!hasChanges || !name.trim()) return;
    updateGuild(guildId, { name });
    setName(name.trim());
  }

  return (
    <div>
      <h2 className="mb-6 text-xl font-bold text-[var(--text-primary)]">
        Overview
      </h2>

      <div className="mb-6 flex items-center gap-4">
        <Avatar src={guild.iconUrl} name={guild.name} size="lg" />
        <div className="text-sm text-[var(--text-muted)]">
          Icon editing coming soon
        </div>
      </div>

      <div className="max-w-md space-y-4">
        <Input
          label="Guild Name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />

        <Button onClick={handleSave} disabled={!hasChanges || !name.trim()}>
          Save Changes
        </Button>
      </div>
    </div>
  );
}

function RolesSection({ guildId }: { guildId: string }) {
  const roleIds = useAppStore((s) => s.roleIdsByGuild[guildId] ?? []);
  const rolesById = useAppStore((s) => s.rolesById);

  const roles = roleIds.map((id) => rolesById[id]).filter(Boolean);

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-xl font-bold text-[var(--text-primary)]">Roles</h2>
        <Button disabled title="Coming in Section 09">
          Create Role
        </Button>
      </div>

      <div className="space-y-2">
        {roles.map((role) => (
          <div
            key={role.id}
            className="flex items-center gap-3 rounded bg-[var(--bg-secondary)] px-3 py-2"
          >
            <div
              className="h-3 w-3 rounded-full"
              style={{ backgroundColor: role.color }}
            />
            <span className="text-sm font-medium text-[var(--text-primary)]">
              {role.name}
            </span>
            <span className="text-xs text-[var(--text-muted)]">
              Position: {role.position}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ChannelsSection({ guildId }: { guildId: string }) {
  const channelIds = useAppStore((s) => s.channelIdsByGuild[guildId] ?? []);
  const channelsById = useAppStore((s) => s.channelsById);
  const deleteChannel = useAppStore((s) => s.deleteChannel);

  const channels = channelIds.map((id) => channelsById[id]).filter(Boolean);

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-xl font-bold text-[var(--text-primary)]">
          Channels
        </h2>
        <Button disabled title="Coming in Section 09">
          Create Channel
        </Button>
      </div>

      <div className="space-y-2">
        {channels.map((ch) => (
          <div
            key={ch.id}
            className="flex items-center gap-3 rounded bg-[var(--bg-secondary)] px-3 py-2"
          >
            <span className="text-[var(--text-muted)]">
              {ch.channelType === "text" ? "#" : "🔊"}
            </span>
            <span className="flex-1 text-sm text-[var(--text-primary)]">
              {ch.name}
            </span>
            {ch.category && (
              <span className="text-xs text-[var(--text-muted)]">
                {ch.category}
              </span>
            )}
            <Button
              variant="danger"
              size="sm"
              onClick={() => {
                if (
                  window.confirm(`Delete #${ch.name}? This cannot be undone.`)
                ) {
                  deleteChannel(ch.id);
                }
              }}
            >
              Delete
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}

function MembersSection({ guildId }: { guildId: string }) {
  const memberKeys = useAppStore((s) => s.memberIdsByGuild[guildId] ?? []);
  const membersById = useAppStore((s) => s.membersById);
  const usersById = useAppStore((s) => s.usersById);
  const rolesById = useAppStore((s) => s.rolesById);
  const roleIds = useAppStore((s) => s.roleIdsByGuild[guildId] ?? []);
  const updateMemberRole = useAppStore((s) => s.updateMemberRole);

  const roles = roleIds.map((id) => rolesById[id]).filter(Boolean);
  const roleOptions = roles.map((r) => ({ value: r.id, label: r.name }));

  return (
    <div>
      <h2 className="mb-6 text-xl font-bold text-[var(--text-primary)]">
        Members
      </h2>

      <div className="space-y-2">
        {memberKeys.map((key) => {
          const member = membersById[key];
          if (!member) return null;
          const user = usersById[member.userId];
          if (!user) return null;

          const memberRoles = member.roles
            .map((rid) => rolesById[rid])
            .filter(Boolean);

          return (
            <div
              key={key}
              className="flex items-center gap-3 rounded bg-[var(--bg-secondary)] px-3 py-2"
            >
              <Avatar src={user.avatarUrl} name={user.displayName} size="sm" />
              <span className="flex-1 text-sm font-medium text-[var(--text-primary)]">
                {user.displayName}
              </span>
              <div className="flex gap-1">
                {memberRoles.map((r) => (
                  <Badge key={r.id} color={r.color}>
                    {r.name}
                  </Badge>
                ))}
              </div>
              <Select
                options={roleOptions}
                value={member.roles[0] ?? ""}
                onChange={(e) => updateMemberRole(key, e.target.value)}
                aria-label={`Role for ${user.displayName}`}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function GuildSettings() {
  const { guildId } = useParams<{ guildId: string }>();

  if (!guildId) return null;

  const sections = [
    {
      id: "overview",
      label: "Overview",
      content: <OverviewSection guildId={guildId} />,
    },
    {
      id: "roles",
      label: "Roles",
      content: <RolesSection guildId={guildId} />,
    },
    {
      id: "channels",
      label: "Channels",
      content: <ChannelsSection guildId={guildId} />,
    },
    {
      id: "members",
      label: "Members",
      content: <MembersSection guildId={guildId} />,
    },
  ];

  return <SettingsLayout sections={sections} />;
}
